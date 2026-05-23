use crate::{
    lexer::lex, ArrayElement, BinaryOp, BindingTarget, ConstraintMarker, Diagnostic, EnumVariant,
    Expr, ExprKind, MapEntry, MatchCase, MatchCaseValue, Module, Param, RecordField, Span, Stmt,
    StringInterpolationPart, Token, TokenKind, TokenStringInterpolationPart, Type, UnaryOp, Value,
};

pub(crate) fn parse(tokens: Vec<Token>) -> Result<Module, Diagnostic> {
    parse_all(tokens).map_err(|mut diagnostics| diagnostics.remove(0))
}

pub(crate) fn parse_all(tokens: Vec<Token>) -> Result<Module, Vec<Diagnostic>> {
    Parser::new(tokens).parse()
}

struct Parser {
    tokens: Vec<Token>,
    current: usize,
    diagnostics: Vec<Diagnostic>,
    type_params: Vec<std::collections::HashSet<String>>,
    suppress_trailing_record_literal: bool,
    expression_depth: usize,
    block_depth: usize,
    unary_depth: usize,
}

const MAX_EXPRESSION_DEPTH: usize = 128;
const MAX_BLOCK_DEPTH: usize = 128;
const MAX_UNARY_DEPTH: usize = 128;

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            current: 0,
            diagnostics: Vec::new(),
            type_params: Vec::new(),
            suppress_trailing_record_literal: false,
            expression_depth: 0,
            block_depth: 0,
            unary_depth: 0,
        }
    }

    fn parse(mut self) -> Result<Module, Vec<Diagnostic>> {
        let mut statements = Vec::new();
        while !self.is_at_end() {
            match self.declaration() {
                Ok(statement) => statements.push(statement),
                Err(err) => {
                    self.diagnostics.push(err);
                    self.synchronize();
                }
            }
        }
        if self.diagnostics.is_empty() {
            Ok(Module { statements })
        } else {
            Err(self.diagnostics)
        }
    }

    fn declaration(&mut self) -> Result<Stmt, Diagnostic> {
        if self.match_kind(&TokenKind::Import) {
            return self.import_declaration();
        }
        if self.match_kind(&TokenKind::Export) {
            return self.export_declaration();
        }
        if self.match_kind(&TokenKind::Let) {
            return self.let_declaration(false);
        }
        if self.match_kind(&TokenKind::Const) {
            return self.const_declaration(false);
        }
        if self.match_kind(&TokenKind::Type) {
            return self.type_alias_declaration(false);
        }
        if self.match_kind(&TokenKind::Enum) {
            return self.enum_declaration(false);
        }
        if self.match_kind(&TokenKind::Fn) {
            return self.function_declaration(false);
        }
        if self.match_kind(&TokenKind::Record) {
            return self.record_declaration(false);
        }
        self.statement()
    }

    fn export_declaration(&mut self) -> Result<Stmt, Diagnostic> {
        if self.match_kind(&TokenKind::Let) {
            return self.let_declaration(true);
        }
        if self.match_kind(&TokenKind::Const) {
            return self.const_declaration(true);
        }
        if self.match_kind(&TokenKind::Type) {
            return self.type_alias_declaration(true);
        }
        if self.match_kind(&TokenKind::Enum) {
            return self.enum_declaration(true);
        }
        if self.match_kind(&TokenKind::Fn) {
            return self.function_declaration(true);
        }
        if self.match_kind(&TokenKind::Record) {
            return self.record_declaration(true);
        }
        Err(Diagnostic::new(
            "expected declaration after 'export'",
            self.peek_token().span,
        ))
    }

    fn import_declaration(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.previous().span;
        let token = self.advance().clone();
        let TokenKind::String(path) = token.kind else {
            return Err(Diagnostic::new("expected import path string", token.span));
        };
        let alias = if self.match_kind(&TokenKind::As) {
            let (name, _) = self.consume_identifier("expected import namespace after 'as'")?;
            Some(name)
        } else {
            None
        };
        let semicolon = self.consume(&TokenKind::Semicolon, "expected ';' after import")?;
        Ok(Stmt::Import {
            path,
            alias,
            span: start.join(semicolon.span),
        })
    }

    fn let_declaration(&mut self, exported: bool) -> Result<Stmt, Diagnostic> {
        self.binding_declaration(exported, false)
    }

    fn const_declaration(&mut self, exported: bool) -> Result<Stmt, Diagnostic> {
        self.binding_declaration(exported, true)
    }

    fn type_alias_declaration(&mut self, exported: bool) -> Result<Stmt, Diagnostic> {
        let start = self.previous().span;
        let (name, _) = self.consume_identifier("expected type alias name")?;
        self.consume(&TokenKind::Equal, "expected '=' after type alias name")?;
        let ty = self.parse_type()?;
        let semicolon = self.consume(&TokenKind::Semicolon, "expected ';' after type alias")?;
        Ok(Stmt::TypeAlias {
            name,
            ty,
            exported,
            span: start.join(semicolon.span),
        })
    }

    fn enum_declaration(&mut self, exported: bool) -> Result<Stmt, Diagnostic> {
        let start = self.previous().span;
        let (name, _) = self.consume_identifier("expected enum name")?;
        self.consume(&TokenKind::LeftBrace, "expected '{' before enum body")?;
        let mut variants = Vec::new();
        if !self.check(&TokenKind::RightBrace) {
            loop {
                let (variant_name, variant_span) =
                    self.consume_identifier("expected enum variant name")?;
                let payload = if self.match_kind(&TokenKind::LeftParen) {
                    let ty = self.parse_type()?;
                    self.consume(
                        &TokenKind::RightParen,
                        "expected ')' after enum variant payload type",
                    )?;
                    Some(ty)
                } else {
                    None
                };
                variants.push(EnumVariant {
                    name: variant_name,
                    payload,
                    span: variant_span,
                });
                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
                if self.check(&TokenKind::RightBrace) {
                    break;
                }
            }
        }
        let end = self.consume(&TokenKind::RightBrace, "expected '}' after enum body")?;
        Ok(Stmt::Enum {
            name,
            variants,
            exported,
            span: start.join(end.span),
        })
    }

    fn binding_declaration(&mut self, exported: bool, is_const: bool) -> Result<Stmt, Diagnostic> {
        let start = self.previous().span;
        if !exported && !is_const && self.has_else_before_statement_semicolon() {
            return self.let_else_declaration(start);
        }
        let kind_label = if is_const { "constant" } else { "variable" };
        let (target, ty) = if self.match_kind(&TokenKind::LeftParen) {
            if exported {
                return Err(Diagnostic::new(
                    "exported destructuring declarations are not supported",
                    start,
                ));
            }
            (self.tuple_binding_target(start)?, None)
        } else if self.match_kind(&TokenKind::LeftBrace) {
            if exported {
                return Err(Diagnostic::new(
                    "exported destructuring declarations are not supported",
                    start,
                ));
            }
            (self.record_binding_target(start)?, None)
        } else {
            let (name, span) = self.consume_identifier(&format!("expected {kind_label} name"))?;
            self.consume(
                &TokenKind::Colon,
                &format!("expected ':' after {kind_label} name"),
            )?;
            (BindingTarget::Name { name, span }, Some(self.parse_type()?))
        };
        self.consume(
            &TokenKind::Equal,
            &format!("expected '=' after {kind_label} name"),
        )?;
        let initializer = self.expression()?;
        let semicolon = self.consume(
            &TokenKind::Semicolon,
            &format!("expected ';' after {kind_label}"),
        )?;
        Ok(Stmt::Let {
            target,
            ty,
            initializer,
            exported,
            is_const,
            span: start.join(semicolon.span),
        })
    }

    fn let_else_declaration(&mut self, start: Span) -> Result<Stmt, Diagnostic> {
        let pattern = self.let_pattern()?;
        self.consume(&TokenKind::Equal, "expected '=' after let pattern")?;
        let value = self.expression()?;
        self.consume(&TokenKind::Else, "expected 'else' after let pattern value")?;
        self.consume(&TokenKind::LeftBrace, "expected '{' before let-else branch")?;
        let (else_branch, else_end) = self.block_statements()?;
        let semicolon = self.consume(&TokenKind::Semicolon, "expected ';' after let-else")?;
        Ok(Stmt::LetElse {
            pattern,
            value,
            else_branch,
            span: start.join(else_end).join(semicolon.span),
        })
    }

    fn has_else_before_statement_semicolon(&self) -> bool {
        let mut paren_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut bracket_depth = 0usize;
        for token in self.tokens.iter().skip(self.current) {
            match &token.kind {
                TokenKind::LeftParen => paren_depth += 1,
                TokenKind::RightParen => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::LeftBrace => brace_depth += 1,
                TokenKind::RightBrace => brace_depth = brace_depth.saturating_sub(1),
                TokenKind::LeftBracket => bracket_depth += 1,
                TokenKind::RightBracket => bracket_depth = bracket_depth.saturating_sub(1),
                TokenKind::Else if paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 => {
                    return true;
                }
                TokenKind::Semicolon
                    if paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 =>
                {
                    return false;
                }
                TokenKind::Eof => return false,
                _ => {}
            }
        }
        false
    }

    fn tuple_binding_target(&mut self, start: Span) -> Result<BindingTarget, Diagnostic> {
        let mut names = Vec::new();
        if self.check(&TokenKind::RightParen) {
            return Err(
                Diagnostic::new("tuple destructuring requires at least two names", start)
                    .with_code("tuple.arity-mismatch"),
            );
        }
        loop {
            let (name, _) = self.consume_identifier("expected name in tuple destructuring")?;
            names.push(name);
            if !self.match_kind(&TokenKind::Comma) {
                break;
            }
            if self.check(&TokenKind::RightParen) {
                break;
            }
        }
        let end = self.consume(
            &TokenKind::RightParen,
            "expected ')' after tuple destructuring",
        )?;
        if names.len() < 2 {
            return Err(Diagnostic::new(
                "tuple destructuring requires at least two names",
                start.join(end.span),
            )
            .with_code("tuple.arity-mismatch"));
        }
        Ok(BindingTarget::Tuple {
            names,
            span: start.join(end.span),
        })
    }

    fn record_binding_target(&mut self, start: Span) -> Result<BindingTarget, Diagnostic> {
        let mut names = Vec::new();
        if self.check(&TokenKind::RightBrace) {
            return Err(Diagnostic::new(
                "record destructuring requires at least one field",
                start,
            ));
        }
        loop {
            let (name, _) =
                self.consume_identifier("expected field name in record destructuring")?;
            names.push(name);
            if !self.match_kind(&TokenKind::Comma) {
                break;
            }
            if self.check(&TokenKind::RightBrace) {
                break;
            }
        }
        let end = self.consume(
            &TokenKind::RightBrace,
            "expected '}' after record destructuring",
        )?;
        Ok(BindingTarget::Record {
            names,
            span: start.join(end.span),
        })
    }

    fn function_declaration(&mut self, exported: bool) -> Result<Stmt, Diagnostic> {
        let start = self.previous().span;
        let (name, _) = self.consume_identifier("expected function name")?;
        let (type_params, type_param_constraints) = self.function_type_params()?;
        self.type_params.push(
            type_params
                .iter()
                .cloned()
                .collect::<std::collections::HashSet<_>>(),
        );
        let result = self.function_signature_and_body();
        self.type_params.pop();
        let (params, return_type, body, end) = result?;
        Ok(Stmt::Function {
            name,
            type_params,
            type_param_constraints,
            params,
            return_type,
            body,
            exported,
            span: start.join(end),
        })
    }

    fn function_signature_and_body(
        &mut self,
    ) -> Result<(Vec<Param>, Type, Vec<Stmt>, Span), Diagnostic> {
        self.consume(&TokenKind::LeftParen, "expected '(' after function name")?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                let (name, _) = self.consume_identifier("expected parameter name")?;
                self.consume(&TokenKind::Colon, "expected ':' after parameter name")?;
                let ty = self.parse_type()?;
                params.push(Param { name, ty });
                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.consume(&TokenKind::RightParen, "expected ')' after parameters")?;
        self.consume(&TokenKind::Arrow, "expected '->' before return type")?;
        let return_type = self.parse_type()?;
        self.consume(&TokenKind::LeftBrace, "expected '{' before function body")?;
        let (body, end) = self.block_statements()?;
        Ok((params, return_type, body, end))
    }

    fn function_type_params(
        &mut self,
    ) -> Result<(Vec<String>, Vec<Vec<ConstraintMarker>>), Diagnostic> {
        if !self.match_kind(&TokenKind::Less) {
            return Ok((Vec::new(), Vec::new()));
        }
        let mut params = Vec::new();
        let mut constraints: Vec<Vec<ConstraintMarker>> = Vec::new();
        loop {
            let (name, span) = self.consume_identifier("expected generic type parameter name")?;
            if params.iter().any(|param| param == &name) {
                return Err(Diagnostic::new(
                    format!("duplicate generic type parameter '{name}'"),
                    span,
                ));
            }
            let mut param_constraints: Vec<ConstraintMarker> = Vec::new();
            if self.match_kind(&TokenKind::Colon) {
                loop {
                    let (marker_name, marker_span) =
                        self.consume_identifier("expected constraint marker name")?;
                    let Some(marker) = ConstraintMarker::parse(&marker_name) else {
                        let known: Vec<&str> =
                            ConstraintMarker::all().iter().map(|m| m.as_str()).collect();
                        return Err(Diagnostic::new(
                            format!(
                                "unknown constraint marker '{marker_name}'; known markers: {}",
                                known.join(", ")
                            ),
                            marker_span,
                        )
                        .with_code("generic.constraint-unknown"));
                    };
                    if !param_constraints.contains(&marker) {
                        param_constraints.push(marker);
                    }
                    if !self.match_kind(&TokenKind::Plus) {
                        break;
                    }
                }
            }
            params.push(name);
            constraints.push(param_constraints);
            if !self.match_kind(&TokenKind::Comma) {
                break;
            }
        }
        self.consume(
            &TokenKind::Greater,
            "expected '>' after generic type parameters",
        )?;
        Ok((params, constraints))
    }

    fn record_declaration(&mut self, exported: bool) -> Result<Stmt, Diagnostic> {
        let start = self.previous().span;
        let (name, _) = self.consume_identifier("expected record name")?;
        self.consume(&TokenKind::LeftBrace, "expected '{' before record body")?;
        let mut fields = Vec::new();
        if !self.check(&TokenKind::RightBrace) {
            loop {
                let (name, span) = self.consume_identifier("expected record field name")?;
                self.consume(&TokenKind::Colon, "expected ':' after record field name")?;
                let ty = self.parse_type()?;
                fields.push(RecordField { name, ty, span });
                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
                if self.check(&TokenKind::RightBrace) {
                    break;
                }
            }
        }
        let end = self.consume(&TokenKind::RightBrace, "expected '}' after record body")?;
        Ok(Stmt::Record {
            name,
            fields,
            exported,
            span: start.join(end.span),
        })
    }

    fn parse_type(&mut self) -> Result<Type, Diagnostic> {
        if self.match_kind(&TokenKind::Fn) {
            self.consume(&TokenKind::LeftParen, "expected '(' after 'fn' type")?;
            let mut params = Vec::new();
            if !self.check(&TokenKind::RightParen) {
                loop {
                    params.push(self.parse_type()?);
                    if !self.match_kind(&TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.consume(
                &TokenKind::RightParen,
                "expected ')' after fn parameter types",
            )?;
            self.consume(&TokenKind::Arrow, "expected '->' after fn parameter list")?;
            let return_type = self.parse_type()?;
            return Ok(Type::Function {
                type_params: Vec::new(),
                params,
                return_type: Box::new(return_type),
            });
        }
        if self.match_kind(&TokenKind::LeftParen) {
            let start = self.previous().span;
            let mut elements = Vec::new();
            if self.check(&TokenKind::RightParen) {
                return Err(Diagnostic::new("tuple type cannot be empty", start));
            }
            loop {
                elements.push(self.parse_type()?);
                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
                if self.check(&TokenKind::RightParen) {
                    break;
                }
            }
            let end = self.consume(&TokenKind::RightParen, "expected ')' after tuple type")?;
            if elements.len() < 2 {
                return Err(Diagnostic::new(
                    "tuple type requires at least two elements",
                    start.join(end.span),
                )
                .with_code("tuple.arity-mismatch"));
            }
            return Ok(Type::Tuple(elements));
        }
        if self.match_kind(&TokenKind::LeftBracket) {
            let element = self.parse_container_element_type()?;
            self.consume(&TokenKind::RightBracket, "expected ']' after array type")?;
            return Ok(Type::Array(Box::new(element)));
        }
        if self.check_identifier("map") {
            self.advance();
            self.consume(&TokenKind::LeftBracket, "expected '[' after map type")?;
            let key = self.advance().clone();
            let key_type = self.type_from_token(key.clone())?;
            if key_type != Type::Str {
                return Err(Diagnostic::new("map key type must be str", key.span));
            }
            self.consume(&TokenKind::Comma, "expected ',' after map key type")?;
            let value = self.parse_container_element_type()?;
            self.consume(&TokenKind::RightBracket, "expected ']' after map type")?;
            return Ok(Type::Map(Box::new(value)));
        }
        if self.check_identifier("option") {
            self.advance();
            self.consume(&TokenKind::LeftBracket, "expected '[' after option type")?;
            let value = self.parse_type()?;
            self.consume(&TokenKind::RightBracket, "expected ']' after option type")?;
            return Ok(Type::Option(Box::new(value)));
        }
        if self.check_identifier("result") {
            self.advance();
            self.consume(&TokenKind::LeftBracket, "expected '[' after result type")?;
            let ok = self.parse_type()?;
            self.consume(&TokenKind::Comma, "expected ',' after result ok type")?;
            let err = self.parse_type()?;
            self.consume(&TokenKind::RightBracket, "expected ']' after result type")?;
            return Ok(Type::Result {
                ok: Box::new(ok),
                err: Box::new(err),
            });
        }
        let token = self.advance().clone();
        self.type_from_token(token)
    }

    fn parse_container_element_type(&mut self) -> Result<Type, Diagnostic> {
        self.parse_type()
    }

    fn type_from_token(&self, token: Token) -> Result<Type, Diagnostic> {
        match token.kind {
            TokenKind::Null => Ok(Type::Null),
            TokenKind::Identifier(name) => match name.as_str() {
                "bool" => Ok(Type::Bool),
                "int" => Ok(Type::Int),
                "float" => Ok(Type::Float),
                "str" => Ok(Type::Str),
                "json" => Ok(Type::Json),
                _ if self.generic_type_param_is_active(&name) => Ok(Type::Generic(name)),
                _ => Ok(Type::Record(name)),
            },
            _ => Err(Diagnostic::new("expected type name", token.span)),
        }
    }

    fn generic_type_param_is_active(&self, name: &str) -> bool {
        self.type_params
            .iter()
            .rev()
            .any(|params| params.contains(name))
    }

    fn statement(&mut self) -> Result<Stmt, Diagnostic> {
        if self.match_kind(&TokenKind::Return) {
            return self.return_statement();
        }
        if self.match_kind(&TokenKind::If) {
            return self.if_statement();
        }
        if self.match_kind(&TokenKind::Match) {
            return self.match_statement();
        }
        if self.match_kind(&TokenKind::While) {
            return self.while_statement();
        }
        if self.match_kind(&TokenKind::For) {
            return self.for_statement();
        }
        if self.match_kind(&TokenKind::Break) {
            let keyword = self.previous().span;
            let semicolon = self.consume(&TokenKind::Semicolon, "expected ';' after 'break'")?;
            return Ok(Stmt::Break {
                span: keyword.join(semicolon.span),
            });
        }
        if self.match_kind(&TokenKind::Continue) {
            let keyword = self.previous().span;
            let semicolon = self.consume(&TokenKind::Semicolon, "expected ';' after 'continue'")?;
            return Ok(Stmt::Continue {
                span: keyword.join(semicolon.span),
            });
        }
        if self.match_kind(&TokenKind::LeftBrace) {
            let start = self.previous().span;
            let (statements, end) = self.block_statements()?;
            return Ok(Stmt::Block {
                statements,
                span: start.join(end),
            });
        }
        self.expression_statement()
    }

    fn return_statement(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.previous().span;
        let value = self.expression()?;
        let semicolon = self.consume(&TokenKind::Semicolon, "expected ';' after return value")?;
        Ok(Stmt::Return {
            value,
            span: start.join(semicolon.span),
        })
    }

    fn if_statement(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.previous().span;
        if self.match_kind(&TokenKind::Let) {
            return self.if_let_statement(start);
        }
        self.consume(&TokenKind::LeftParen, "expected '(' after 'if'")?;
        let condition = self.expression()?;
        self.consume(&TokenKind::RightParen, "expected ')' after if condition")?;
        self.consume(&TokenKind::LeftBrace, "expected '{' before if branch")?;
        let (then_branch, then_end) = self.block_statements()?;
        let mut else_branch = Vec::new();
        let mut end = then_end;
        if self.match_kind(&TokenKind::Else) {
            if self.match_kind(&TokenKind::If) {
                let branch = self.if_statement()?;
                end = statement_span(&branch);
                else_branch = vec![branch];
            } else {
                self.consume(&TokenKind::LeftBrace, "expected '{' before else branch")?;
                let (branch, else_end) = self.block_statements()?;
                else_branch = branch;
                end = else_end;
            }
        }
        Ok(Stmt::If {
            condition,
            then_branch,
            else_branch,
            span: start.join(end),
        })
    }

    fn if_let_statement(&mut self, start: Span) -> Result<Stmt, Diagnostic> {
        let pattern = self.let_pattern()?;
        self.consume(&TokenKind::Equal, "expected '=' after if-let pattern")?;
        let value = self.let_condition_value()?;
        self.consume(&TokenKind::LeftBrace, "expected '{' before if-let branch")?;
        let (then_branch, then_end) = self.block_statements()?;
        let mut else_branch = Vec::new();
        let mut end = then_end;
        if self.match_kind(&TokenKind::Else) {
            if self.match_kind(&TokenKind::If) {
                let branch = self.if_statement()?;
                end = statement_span(&branch);
                else_branch = vec![branch];
            } else {
                self.consume(&TokenKind::LeftBrace, "expected '{' before else branch")?;
                let (branch, else_end) = self.block_statements()?;
                else_branch = branch;
                end = else_end;
            }
        }
        Ok(Stmt::IfLet {
            pattern,
            value,
            then_branch,
            else_branch,
            span: start.join(end),
        })
    }

    fn match_statement(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.previous().span;
        self.consume(&TokenKind::LeftParen, "expected '(' after 'match'")?;
        let value = self.expression()?;
        self.consume(&TokenKind::RightParen, "expected ')' after match value")?;
        self.consume(&TokenKind::LeftBrace, "expected '{' before match cases")?;
        let mut cases = Vec::new();
        let mut default = None;
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            if self.check_identifier("_") {
                if default.is_some() {
                    return Err(Diagnostic::new(
                        "match default case can only appear once",
                        self.peek_token().span,
                    ));
                }
                self.advance();
                self.consume(&TokenKind::FatArrow, "expected '=>' after match default")?;
                self.consume(
                    &TokenKind::LeftBrace,
                    "expected '{' before match default body",
                )?;
                let (body, _) = self.block_statements()?;
                default = Some(body);
                continue;
            }

            let token = self.advance().clone();
            let pattern = self.match_case_value(token.clone(), false)?;
            self.consume(&TokenKind::FatArrow, "expected '=>' after match case")?;
            self.consume(&TokenKind::LeftBrace, "expected '{' before match case body")?;
            let (body, end) = self.block_statements()?;
            cases.push(MatchCase {
                pattern,
                body,
                span: token.span.join(end),
            });
        }
        let end = self
            .consume(&TokenKind::RightBrace, "expected '}' after match cases")?
            .span;
        Ok(Stmt::Match {
            value,
            cases,
            default,
            span: start.join(end),
        })
    }

    fn let_pattern(&mut self) -> Result<MatchCaseValue, Diagnostic> {
        let token = self.advance().clone();
        self.match_case_value(token, false)
    }

    fn match_case_value(
        &mut self,
        token: Token,
        allow_binding: bool,
    ) -> Result<MatchCaseValue, Diagnostic> {
        match token.kind {
            TokenKind::Int(value) => {
                if self.match_kind(&TokenKind::DotDot) {
                    let (end, _) =
                        self.consume_int("expected int literal after '..' in match range")?;
                    return Ok(MatchCaseValue::IntRange { start: value, end });
                }
                Ok(MatchCaseValue::Int(value))
            }
            TokenKind::Float(value) => Ok(MatchCaseValue::Float(value)),
            TokenKind::String(value) => Ok(MatchCaseValue::Str(value)),
            TokenKind::Identifier(name) if name == "none" => Ok(MatchCaseValue::None),
            TokenKind::Identifier(name) if name == "some" => {
                self.consume(&TokenKind::LeftParen, "expected '(' after 'some' match case")?;
                let payload = self.match_case_payload("some")?;
                self.consume(&TokenKind::RightParen, "expected ')' after 'some' payload")?;
                Ok(MatchCaseValue::Some(Box::new(payload)))
            }
            TokenKind::Identifier(name) if name == "ok" => {
                self.consume(&TokenKind::LeftParen, "expected '(' after 'ok' match case")?;
                let payload = self.match_case_payload("ok")?;
                self.consume(&TokenKind::RightParen, "expected ')' after 'ok' payload")?;
                Ok(MatchCaseValue::Ok(Box::new(payload)))
            }
            TokenKind::Identifier(name) if name == "err" => {
                self.consume(&TokenKind::LeftParen, "expected '(' after 'err' match case")?;
                let payload = self.match_case_payload("err")?;
                self.consume(&TokenKind::RightParen, "expected ')' after 'err' payload")?;
                Ok(MatchCaseValue::Err(Box::new(payload)))
            }
            TokenKind::Identifier(name) if self.match_kind(&TokenKind::LeftParen) => {
                let payload = if self.check(&TokenKind::RightParen) {
                    None
                } else {
                    Some(Box::new(self.match_case_payload(&name)?))
                };
                self.consume(
                    &TokenKind::RightParen,
                    "expected ')' after enum variant payload",
                )?;
                Ok(MatchCaseValue::EnumVariant { name, payload })
            }
            TokenKind::Identifier(name) if !allow_binding => {
                Ok(MatchCaseValue::EnumVariant {
                    name,
                    payload: None,
                })
            }
            TokenKind::Identifier(name) if allow_binding => {
                if matches!(name.as_str(), "some" | "ok" | "err") {
                    return Err(Diagnostic::new(
                        "expected '(' after match constructor",
                        token.span,
                    ));
                }
                Ok(MatchCaseValue::Bind(name))
            }
            _ => Err(Diagnostic::new(
                "expected number literal, string literal, range, '_', some(pattern), none, ok(pattern), or err(pattern) in match case",
                token.span,
            )),
        }
    }

    fn match_case_payload(&mut self, constructor: &str) -> Result<MatchCaseValue, Diagnostic> {
        if self.check(&TokenKind::RightParen) {
            return Err(Diagnostic::new(
                format!("expected payload pattern in '{constructor}' match case"),
                self.peek_token().span,
            ));
        }
        let token = self.advance().clone();
        self.match_case_value(token, true)
    }

    fn while_statement(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.previous().span;
        if self.match_kind(&TokenKind::Let) {
            return self.while_let_statement(start);
        }
        self.consume(&TokenKind::LeftParen, "expected '(' after 'while'")?;
        let condition = self.expression()?;
        self.consume(&TokenKind::RightParen, "expected ')' after while condition")?;
        self.consume(&TokenKind::LeftBrace, "expected '{' before while body")?;
        let (body, end) = self.block_statements()?;
        Ok(Stmt::While {
            condition,
            body,
            span: start.join(end),
        })
    }

    fn while_let_statement(&mut self, start: Span) -> Result<Stmt, Diagnostic> {
        let pattern = self.let_pattern()?;
        self.consume(&TokenKind::Equal, "expected '=' after while-let pattern")?;
        let value = self.let_condition_value()?;
        self.consume(&TokenKind::LeftBrace, "expected '{' before while-let body")?;
        let (body, end) = self.block_statements()?;
        Ok(Stmt::WhileLet {
            pattern,
            value,
            body,
            span: start.join(end),
        })
    }

    fn let_condition_value(&mut self) -> Result<Expr, Diagnostic> {
        let previous = self.suppress_trailing_record_literal;
        self.suppress_trailing_record_literal = true;
        let value = self.expression();
        self.suppress_trailing_record_literal = previous;
        value
    }

    fn for_statement(&mut self) -> Result<Stmt, Diagnostic> {
        let start_span = self.previous().span;
        let (name, _) = self.consume_identifier("expected loop variable name")?;
        self.consume(&TokenKind::In, "expected 'in' after loop variable")?;
        let start = self.expression()?;
        self.consume(&TokenKind::DotDot, "expected '..' in for range")?;
        let end_expr = self.expression()?;
        self.consume(&TokenKind::LeftBrace, "expected '{' before for body")?;
        let (body, end_span) = self.block_statements()?;
        Ok(Stmt::For {
            name,
            start,
            end: end_expr,
            body,
            span: start_span.join(end_span),
        })
    }

    fn block_statements(&mut self) -> Result<(Vec<Stmt>, Span), Diagnostic> {
        if self.block_depth >= MAX_BLOCK_DEPTH {
            return Err(
                Diagnostic::new("block nesting is too deep", self.peek_token().span)
                    .with_code("parse.nesting-depth"),
            );
        }
        self.block_depth += 1;
        let mut statements = Vec::new();
        let result = (|| {
            while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                match self.declaration() {
                    Ok(statement) => statements.push(statement),
                    Err(err) => {
                        self.diagnostics.push(err);
                        self.synchronize();
                    }
                }
            }
            let end = self
                .consume(&TokenKind::RightBrace, "expected '}' after block")?
                .span;
            Ok((statements, end))
        })();
        self.block_depth -= 1;
        result
    }

    fn expression_statement(&mut self) -> Result<Stmt, Diagnostic> {
        let expression = self.expression()?;
        let semicolon = self.consume(&TokenKind::Semicolon, "expected ';' after expression")?;
        let span = expression.span.join(semicolon.span);
        Ok(Stmt::Expression { expression, span })
    }

    fn expression(&mut self) -> Result<Expr, Diagnostic> {
        if self.expression_depth >= MAX_EXPRESSION_DEPTH {
            return Err(
                Diagnostic::new("expression nesting is too deep", self.peek_token().span)
                    .with_code("parse.nesting-depth"),
            );
        }
        self.expression_depth += 1;
        let result = self.assignment();
        self.expression_depth -= 1;
        result
    }

    fn assignment(&mut self) -> Result<Expr, Diagnostic> {
        let expr = self.logical_or()?;
        if self.match_kind(&TokenKind::Equal) {
            let equals = self.previous().span;
            let value = self.assignment()?;
            let span = expr.span.join(value.span);
            match expr.kind {
                ExprKind::Variable(name) => {
                    return Ok(Expr {
                        kind: ExprKind::Assign {
                            name,
                            value: Box::new(value),
                        },
                        span,
                    });
                }
                ExprKind::Index { array, index } => {
                    return Ok(Expr {
                        kind: ExprKind::IndexAssign {
                            container: array,
                            index,
                            value: Box::new(value),
                        },
                        span,
                    });
                }
                _ => {}
            }
            return Err(Diagnostic::new("invalid assignment target", equals)
                .with_code("type.assign-target"));
        }
        Ok(expr)
    }

    fn logical_or(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.logical_and()?;
        while self.match_kind(&TokenKind::OrOr) {
            expr = self.binary_expr(expr, BinaryOp::Or, Self::logical_and)?;
        }
        Ok(expr)
    }

    fn logical_and(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.bitwise_or()?;
        while self.match_kind(&TokenKind::AndAnd) {
            expr = self.binary_expr(expr, BinaryOp::And, Self::bitwise_or)?;
        }
        Ok(expr)
    }

    fn bitwise_or(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.bitwise_xor()?;
        while self.match_kind(&TokenKind::Pipe) {
            expr = self.binary_expr(expr, BinaryOp::BitOr, Self::bitwise_xor)?;
        }
        Ok(expr)
    }

    fn bitwise_xor(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.bitwise_and()?;
        while self.match_kind(&TokenKind::Caret) {
            expr = self.binary_expr(expr, BinaryOp::BitXor, Self::bitwise_and)?;
        }
        Ok(expr)
    }

    fn bitwise_and(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.equality()?;
        while self.match_kind(&TokenKind::Ampersand) {
            expr = self.binary_expr(expr, BinaryOp::BitAnd, Self::equality)?;
        }
        Ok(expr)
    }

    fn equality(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.comparison()?;
        while self.match_kind(&TokenKind::BangEqual) || self.match_kind(&TokenKind::EqualEqual) {
            let op = match self.previous().kind {
                TokenKind::BangEqual => BinaryOp::NotEqual,
                TokenKind::EqualEqual => BinaryOp::Equal,
                _ => unreachable!(),
            };
            expr = self.binary_expr(expr, op, Self::comparison)?;
        }
        Ok(expr)
    }

    fn comparison(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.shift()?;
        while self.match_kind(&TokenKind::Greater)
            || self.match_kind(&TokenKind::GreaterEqual)
            || self.match_kind(&TokenKind::Less)
            || self.match_kind(&TokenKind::LessEqual)
        {
            let op = match self.previous().kind {
                TokenKind::Greater => BinaryOp::Greater,
                TokenKind::GreaterEqual => BinaryOp::GreaterEqual,
                TokenKind::Less => BinaryOp::Less,
                TokenKind::LessEqual => BinaryOp::LessEqual,
                _ => unreachable!(),
            };
            expr = self.binary_expr(expr, op, Self::shift)?;
        }
        Ok(expr)
    }

    fn shift(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.term()?;
        while self.match_kind(&TokenKind::LeftShift) || self.match_kind(&TokenKind::RightShift) {
            let op = match self.previous().kind {
                TokenKind::LeftShift => BinaryOp::ShiftLeft,
                TokenKind::RightShift => BinaryOp::ShiftRight,
                _ => unreachable!(),
            };
            expr = self.binary_expr(expr, op, Self::term)?;
        }
        Ok(expr)
    }

    fn term(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.factor()?;
        while self.match_kind(&TokenKind::Minus) || self.match_kind(&TokenKind::Plus) {
            let op = match self.previous().kind {
                TokenKind::Minus => BinaryOp::Subtract,
                TokenKind::Plus => BinaryOp::Add,
                _ => unreachable!(),
            };
            expr = self.binary_expr(expr, op, Self::factor)?;
        }
        Ok(expr)
    }

    fn factor(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.unary()?;
        while self.match_kind(&TokenKind::Slash) || self.match_kind(&TokenKind::Star) {
            let op = match self.previous().kind {
                TokenKind::Slash => BinaryOp::Divide,
                TokenKind::Star => BinaryOp::Multiply,
                _ => unreachable!(),
            };
            expr = self.binary_expr(expr, op, Self::unary)?;
        }
        Ok(expr)
    }

    fn unary(&mut self) -> Result<Expr, Diagnostic> {
        if self.match_kind(&TokenKind::Bang)
            || self.match_kind(&TokenKind::Minus)
            || self.match_kind(&TokenKind::Tilde)
        {
            if self.unary_depth >= MAX_UNARY_DEPTH {
                return Err(Diagnostic::new(
                    "expression nesting is too deep",
                    self.previous().span,
                )
                .with_code("parse.nesting-depth"));
            }
            let token = self.previous().clone();
            self.unary_depth += 1;
            let right = self.unary();
            self.unary_depth -= 1;
            let right = right?;
            let span = token.span.join(right.span);
            let op = match token.kind {
                TokenKind::Bang => UnaryOp::Not,
                TokenKind::Minus => UnaryOp::Negate,
                TokenKind::Tilde => UnaryOp::BitNot,
                _ => unreachable!(),
            };
            return Ok(Expr {
                kind: ExprKind::Unary {
                    op,
                    right: Box::new(right),
                },
                span,
            });
        }
        self.call()
    }

    fn call(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.primary()?;
        loop {
            if self.match_kind(&TokenKind::LeftParen) {
                let mut args = Vec::new();
                if !self.check(&TokenKind::RightParen) {
                    loop {
                        args.push(self.expression()?);
                        if !self.match_kind(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                let paren = self.consume(&TokenKind::RightParen, "expected ')' after arguments")?;
                let span = expr.span.join(paren.span);
                expr = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                        paren_span: paren.span,
                    },
                    span,
                };
                continue;
            }
            if self.match_kind(&TokenKind::LeftBracket) {
                let index = self.expression()?;
                let bracket = self.consume(&TokenKind::RightBracket, "expected ']' after index")?;
                let span = expr.span.join(bracket.span);
                expr = Expr {
                    kind: ExprKind::Index {
                        array: Box::new(expr),
                        index: Box::new(index),
                    },
                    span,
                };
                continue;
            }
            if self.match_kind(&TokenKind::Dot) {
                let (name, field_span) =
                    self.consume_identifier("expected field name after '.'")?;
                let span = expr.span.join(field_span);
                expr = Expr {
                    kind: ExprKind::Field {
                        receiver: Box::new(expr),
                        name,
                        span: field_span,
                    },
                    span,
                };
                continue;
            }
            if self.match_kind(&TokenKind::Question) {
                let question = self.previous().span;
                let span = expr.span.join(question);
                expr = Expr {
                    kind: ExprKind::Question {
                        value: Box::new(expr),
                    },
                    span,
                };
                continue;
            }
            break;
        }
        Ok(expr)
    }

    fn primary(&mut self) -> Result<Expr, Diagnostic> {
        if self.is_at_end() {
            return Err(Diagnostic::new(
                "expected expression",
                self.peek_token().span,
            ));
        }
        let token = self.advance().clone();
        let kind = match token.kind {
            TokenKind::False => ExprKind::Literal(Value::Bool(false)),
            TokenKind::True => ExprKind::Literal(Value::Bool(true)),
            TokenKind::Null => ExprKind::Literal(Value::Null),
            TokenKind::Int(value) => ExprKind::Literal(Value::Int(value)),
            TokenKind::Float(value) => ExprKind::Literal(Value::Float(value)),
            TokenKind::String(value) => ExprKind::Literal(Value::string(value)),
            TokenKind::InterpolatedString(parts) => {
                ExprKind::StringInterpolation(self.parse_interpolated_string_parts(parts)?)
            }
            TokenKind::Identifier(name) => {
                if !self.suppress_trailing_record_literal && self.check(&TokenKind::LeftBrace) {
                    return self.record_literal(token.span, name);
                }
                ExprKind::Variable(name)
            }
            TokenKind::Fn => return self.lambda_literal(token.span),
            TokenKind::Reserved(name) => {
                return Err(Diagnostic::new(
                    format!("'{name}' is a reserved keyword and cannot be used as an identifier"),
                    token.span,
                )
                .with_code("parse.reserved-keyword"));
            }
            TokenKind::LeftBracket => return self.array_literal(token.span),
            TokenKind::LeftBrace => return self.map_literal(token.span),
            TokenKind::LeftParen => {
                let first = self.expression()?;
                if self.match_kind(&TokenKind::Comma) {
                    return self.tuple_literal(token.span, first);
                }
                self.consume(&TokenKind::RightParen, "expected ')' after expression")?;
                return Ok(first);
            }
            _ => return Err(Diagnostic::new("expected expression", token.span)),
        };
        Ok(Expr {
            kind,
            span: token.span,
        })
    }

    fn lambda_literal(&mut self, start: Span) -> Result<Expr, Diagnostic> {
        let (params, return_type, body, end) = self.function_signature_and_body()?;
        Ok(Expr {
            kind: ExprKind::FunctionLiteral {
                params,
                return_type,
                body,
            },
            span: start.join(end),
        })
    }

    fn tuple_literal(&mut self, start: Span, first: Expr) -> Result<Expr, Diagnostic> {
        let mut elements = vec![first];
        if !self.check(&TokenKind::RightParen) {
            loop {
                elements.push(self.expression()?);
                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
                if self.check(&TokenKind::RightParen) {
                    break;
                }
            }
        }
        let end = self.consume(&TokenKind::RightParen, "expected ')' after tuple literal")?;
        if elements.len() < 2 {
            return Err(Diagnostic::new(
                "tuple literal requires at least two elements",
                start.join(end.span),
            )
            .with_code("tuple.arity-mismatch"));
        }
        Ok(Expr {
            kind: ExprKind::TupleLiteral { elements },
            span: start.join(end.span),
        })
    }

    fn parse_interpolated_string_parts(
        &self,
        parts: Vec<TokenStringInterpolationPart>,
    ) -> Result<Vec<StringInterpolationPart>, Diagnostic> {
        parts
            .into_iter()
            .map(|part| {
                let expression = part
                    .expression
                    .as_deref()
                    .map(|source| self.parse_interpolation_expression(source, part.span))
                    .transpose()?;
                Ok(StringInterpolationPart {
                    text: part.text,
                    expression,
                    span: part.span,
                })
            })
            .collect()
    }

    fn parse_interpolation_expression(
        &self,
        source: &str,
        source_span: Span,
    ) -> Result<Expr, Diagnostic> {
        let mut tokens = lex(source).map_err(|err| {
            Diagnostic::new(err.message, source_span).with_code("string.interpolation")
        })?;
        for token in &mut tokens {
            token.span.start += source_span.start;
            token.span.end += source_span.start;
        }
        let mut parser = Parser::new(tokens);
        let expr = parser.expression().map_err(|err| {
            Diagnostic::new(err.message, source_span).with_code("string.interpolation")
        })?;
        if !parser.is_at_end() {
            return Err(Diagnostic::new(
                "expected expression inside string interpolation",
                source_span,
            )
            .with_code("string.interpolation"));
        }
        Ok(expr)
    }

    fn record_literal(&mut self, start: Span, name: String) -> Result<Expr, Diagnostic> {
        self.consume(&TokenKind::LeftBrace, "expected '{' after record name")?;
        let mut fields = Vec::new();
        if !self.check(&TokenKind::RightBrace) {
            loop {
                let (field_name, field_span) =
                    self.consume_identifier("expected record field name")?;
                self.consume(&TokenKind::Colon, "expected ':' after record field name")?;
                let value = self.expression()?;
                fields.push((field_name, value, field_span));
                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
                if self.check(&TokenKind::RightBrace) {
                    break;
                }
            }
        }
        let end = self.consume(&TokenKind::RightBrace, "expected '}' after record literal")?;
        Ok(Expr {
            kind: ExprKind::RecordLiteral { name, fields },
            span: start.join(end.span),
        })
    }

    fn map_literal(&mut self, start: Span) -> Result<Expr, Diagnostic> {
        let mut entries = Vec::new();
        if !self.check(&TokenKind::RightBrace) {
            loop {
                if self.match_kind(&TokenKind::Ellipsis) {
                    entries.push(MapEntry::Spread(self.expression()?));
                } else {
                    let key = self.expression()?;
                    self.consume(&TokenKind::Colon, "expected ':' after map key")?;
                    let value = self.expression()?;
                    entries.push(MapEntry::Entry { key, value });
                }
                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
                if self.check(&TokenKind::RightBrace) {
                    break;
                }
            }
        }
        let end = self.consume(&TokenKind::RightBrace, "expected '}' after map literal")?;
        Ok(Expr {
            kind: ExprKind::MapLiteral { entries },
            span: start.join(end.span),
        })
    }

    fn array_literal(&mut self, start: Span) -> Result<Expr, Diagnostic> {
        let mut elements = Vec::new();
        if !self.check(&TokenKind::RightBracket) {
            loop {
                let element = if self.match_kind(&TokenKind::Ellipsis) {
                    ArrayElement::Spread(self.expression()?)
                } else {
                    ArrayElement::Expr(self.expression()?)
                };
                elements.push(element);
                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
                if self.check(&TokenKind::RightBracket) {
                    break;
                }
            }
        }
        let end = self.consume(&TokenKind::RightBracket, "expected ']' after array literal")?;
        Ok(Expr {
            kind: ExprKind::ArrayLiteral { elements },
            span: start.join(end.span),
        })
    }

    fn binary_expr(
        &mut self,
        left: Expr,
        op: BinaryOp,
        parse_right: fn(&mut Self) -> Result<Expr, Diagnostic>,
    ) -> Result<Expr, Diagnostic> {
        let right = parse_right(self)?;
        let span = left.span.join(right.span);
        Ok(Expr {
            kind: ExprKind::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            },
            span,
        })
    }

    fn consume(&mut self, kind: &TokenKind, message: &str) -> Result<Token, Diagnostic> {
        if self.check(kind) {
            return Ok(self.advance().clone());
        }
        Err(Diagnostic::new(message, self.peek_token().span).with_code("parse.expected-token"))
    }

    fn consume_identifier(&mut self, message: &str) -> Result<(String, Span), Diagnostic> {
        let token = self.peek_token().clone();
        match token.kind {
            TokenKind::Identifier(name) => Ok((name, token.span)),
            TokenKind::Reserved(name) => Err(Diagnostic::new(
                format!("'{name}' is a reserved keyword and cannot be used as an identifier"),
                token.span,
            )
            .with_code("parse.reserved-keyword")),
            _ => Err(Diagnostic::new(message, token.span)),
        }
        .inspect(|_| {
            self.advance();
        })
    }

    fn consume_int(&mut self, message: &str) -> Result<(i64, Span), Diagnostic> {
        let token = self.peek_token().clone();
        match token.kind {
            TokenKind::Int(value) => Ok((value, token.span)),
            _ => Err(Diagnostic::new(message, token.span)),
        }
        .inspect(|_| {
            self.advance();
        })
    }

    fn match_kind(&mut self, kind: &TokenKind) -> bool {
        if !self.check(kind) {
            return false;
        }
        self.advance();
        true
    }

    fn check(&self, kind: &TokenKind) -> bool {
        if self.is_at_end() {
            return matches!(kind, TokenKind::Eof);
        }
        std::mem::discriminant(&self.peek_token().kind) == std::mem::discriminant(kind)
    }

    fn check_identifier(&self, expected: &str) -> bool {
        matches!(&self.peek_token().kind, TokenKind::Identifier(name) if name == expected)
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek_token().kind, TokenKind::Eof)
    }

    fn peek_token(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }

    fn synchronize(&mut self) {
        if self.is_at_end() {
            return;
        }
        self.advance();
        while !self.is_at_end() {
            if matches!(
                self.previous().kind,
                TokenKind::Semicolon | TokenKind::RightBrace
            ) {
                return;
            }
            if matches!(
                self.peek_token().kind,
                TokenKind::Let
                    | TokenKind::Type
                    | TokenKind::Enum
                    | TokenKind::Fn
                    | TokenKind::Return
                    | TokenKind::If
                    | TokenKind::Match
                    | TokenKind::While
                    | TokenKind::For
                    | TokenKind::Import
                    | TokenKind::Export
                    | TokenKind::Record
            ) {
                return;
            }
            self.advance();
        }
    }
}

fn statement_span(statement: &Stmt) -> Span {
    match statement {
        Stmt::Import { span, .. }
        | Stmt::Let { span, .. }
        | Stmt::TypeAlias { span, .. }
        | Stmt::Enum { span, .. }
        | Stmt::Function { span, .. }
        | Stmt::Record { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::If { span, .. }
        | Stmt::IfLet { span, .. }
        | Stmt::Match { span, .. }
        | Stmt::LetElse { span, .. }
        | Stmt::While { span, .. }
        | Stmt::WhileLet { span, .. }
        | Stmt::For { span, .. }
        | Stmt::Block { span, .. }
        | Stmt::Break { span, .. }
        | Stmt::Continue { span, .. }
        | Stmt::Expression { span, .. } => *span,
    }
}
