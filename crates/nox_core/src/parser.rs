use crate::{
    BinaryOp, Diagnostic, Expr, ExprKind, MatchCase, MatchCaseValue, Module, Param, RecordField,
    Span, Stmt, Token, TokenKind, Type, UnaryOp, Value,
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
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            current: 0,
            diagnostics: Vec::new(),
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

    fn binding_declaration(&mut self, exported: bool, is_const: bool) -> Result<Stmt, Diagnostic> {
        let start = self.previous().span;
        let kind_label = if is_const { "constant" } else { "variable" };
        let (name, _) = self.consume_identifier(&format!("expected {kind_label} name"))?;
        self.consume(
            &TokenKind::Colon,
            &format!("expected ':' after {kind_label} name"),
        )?;
        let ty = self.parse_type()?;
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
            name,
            ty,
            initializer,
            exported,
            is_const,
            span: start.join(semicolon.span),
        })
    }

    fn function_declaration(&mut self, exported: bool) -> Result<Stmt, Diagnostic> {
        let start = self.previous().span;
        let (name, _) = self.consume_identifier("expected function name")?;
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
        Ok(Stmt::Function {
            name,
            params,
            return_type,
            body,
            exported,
            span: start.join(end),
        })
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
        if self.match_kind(&TokenKind::LeftBracket) {
            let element = self.parse_named_array_element_type()?;
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
            let value = self.parse_named_array_element_type()?;
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

    fn parse_named_array_element_type(&mut self) -> Result<Type, Diagnostic> {
        let token = self.advance().clone();
        let ty = self.type_from_token(token.clone())?;
        if matches!(ty, Type::Array(_) | Type::Function { .. }) {
            return Err(Diagnostic::new(
                "array element type must be a named v0 type",
                token.span,
            ));
        }
        Ok(ty)
    }

    fn type_from_token(&self, token: Token) -> Result<Type, Diagnostic> {
        match token.kind {
            TokenKind::Null => Ok(Type::Null),
            TokenKind::Identifier(name) => match name.as_str() {
                "bool" => Ok(Type::Bool),
                "int" => Ok(Type::Int),
                "float" => Ok(Type::Float),
                "str" => Ok(Type::Str),
                _ => Ok(Type::Record(name)),
            },
            _ => Err(Diagnostic::new("expected type name", token.span)),
        }
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
            let value = self.match_case_value(token.clone())?;
            self.consume(&TokenKind::FatArrow, "expected '=>' after match case")?;
            self.consume(&TokenKind::LeftBrace, "expected '{' before match case body")?;
            let (body, end) = self.block_statements()?;
            cases.push(MatchCase {
                value,
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

    fn match_case_value(&mut self, token: Token) -> Result<MatchCaseValue, Diagnostic> {
        match token.kind {
            TokenKind::Int(value) => Ok(MatchCaseValue::Int(value)),
            TokenKind::String(value) => Ok(MatchCaseValue::Str(value)),
            TokenKind::Identifier(name) if name == "none" => Ok(MatchCaseValue::None),
            TokenKind::Identifier(name) if name == "some" => {
                self.consume(&TokenKind::LeftParen, "expected '(' after 'some' match case")?;
                let (payload, _) =
                    self.consume_identifier("expected payload name in 'some' match case")?;
                self.consume(&TokenKind::RightParen, "expected ')' after 'some' payload")?;
                Ok(MatchCaseValue::Some(payload))
            }
            TokenKind::Identifier(name) if name == "ok" => {
                self.consume(&TokenKind::LeftParen, "expected '(' after 'ok' match case")?;
                let (payload, _) =
                    self.consume_identifier("expected payload name in 'ok' match case")?;
                self.consume(&TokenKind::RightParen, "expected ')' after 'ok' payload")?;
                Ok(MatchCaseValue::Ok(payload))
            }
            TokenKind::Identifier(name) if name == "err" => {
                self.consume(&TokenKind::LeftParen, "expected '(' after 'err' match case")?;
                let (payload, _) =
                    self.consume_identifier("expected payload name in 'err' match case")?;
                self.consume(&TokenKind::RightParen, "expected ')' after 'err' payload")?;
                Ok(MatchCaseValue::Err(payload))
            }
            _ => Err(Diagnostic::new(
                "expected int literal, string literal, '_', some(name), none, ok(name), or err(name) in match case",
                token.span,
            )),
        }
    }

    fn while_statement(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.previous().span;
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
        let mut statements = Vec::new();
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
    }

    fn expression_statement(&mut self) -> Result<Stmt, Diagnostic> {
        let expression = self.expression()?;
        let semicolon = self.consume(&TokenKind::Semicolon, "expected ';' after expression")?;
        let span = expression.span.join(semicolon.span);
        Ok(Stmt::Expression { expression, span })
    }

    fn expression(&mut self) -> Result<Expr, Diagnostic> {
        self.assignment()
    }

    fn assignment(&mut self) -> Result<Expr, Diagnostic> {
        let expr = self.logical_or()?;
        if self.match_kind(&TokenKind::Equal) {
            let equals = self.previous().span;
            let value = self.assignment()?;
            let span = expr.span.join(value.span);
            if let ExprKind::Variable(name) = expr.kind {
                return Ok(Expr {
                    kind: ExprKind::Assign {
                        name,
                        value: Box::new(value),
                    },
                    span,
                });
            }
            return Err(Diagnostic::new("invalid assignment target", equals));
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
        let mut expr = self.equality()?;
        while self.match_kind(&TokenKind::AndAnd) {
            expr = self.binary_expr(expr, BinaryOp::And, Self::equality)?;
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
        let mut expr = self.term()?;
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
        if self.match_kind(&TokenKind::Bang) || self.match_kind(&TokenKind::Minus) {
            let token = self.previous().clone();
            let right = self.unary()?;
            let span = token.span.join(right.span);
            let op = match token.kind {
                TokenKind::Bang => UnaryOp::Not,
                TokenKind::Minus => UnaryOp::Negate,
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
            TokenKind::Identifier(name) => {
                if self.check(&TokenKind::LeftBrace) {
                    return self.record_literal(token.span, name);
                }
                ExprKind::Variable(name)
            }
            TokenKind::LeftBracket => return self.array_literal(token.span),
            TokenKind::LeftBrace => return self.map_literal(token.span),
            TokenKind::LeftParen => {
                let expr = self.expression()?;
                self.consume(&TokenKind::RightParen, "expected ')' after expression")?;
                return Ok(expr);
            }
            _ => return Err(Diagnostic::new("expected expression", token.span)),
        };
        Ok(Expr {
            kind,
            span: token.span,
        })
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
                let key = self.expression()?;
                self.consume(&TokenKind::Colon, "expected ':' after map key")?;
                let value = self.expression()?;
                entries.push((key, value));
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
                elements.push(self.expression()?);
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
        | Stmt::Function { span, .. }
        | Stmt::Record { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::If { span, .. }
        | Stmt::Match { span, .. }
        | Stmt::While { span, .. }
        | Stmt::For { span, .. }
        | Stmt::Block { span, .. }
        | Stmt::Break { span, .. }
        | Stmt::Continue { span, .. }
        | Stmt::Expression { span, .. } => *span,
    }
}
