use std::collections::{HashMap, HashSet};

use crate::{
    BinaryOp, Diagnostic, Expr, ExprKind, HostFunction, MatchCaseValue, Module, RecordField, Span,
    Stmt, Type, UnaryOp, Value,
};

#[derive(Debug, Clone)]
struct RecordSchema {
    fields: Vec<RecordField>,
    field_types: HashMap<String, Type>,
}

#[derive(Debug, Clone)]
struct Binding {
    ty: Type,
    is_const: bool,
}

pub(crate) struct TypeChecker {
    scopes: Vec<HashMap<String, Binding>>,
    returns: Vec<Type>,
    records: HashMap<String, RecordSchema>,
    hover_offset: Option<usize>,
    hover_type: Option<(Span, Type)>,
    loop_depth: usize,
}

impl TypeChecker {
    pub(crate) fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            returns: Vec::new(),
            records: HashMap::new(),
            hover_offset: None,
            hover_type: None,
            loop_depth: 0,
        }
    }

    pub(crate) fn new_with_hosts(host_functions: &HashMap<String, HostFunction>) -> Self {
        let mut checker = Self::new();
        for (name, host) in host_functions {
            checker.define(name.clone(), host.function.signature_type());
        }
        checker
    }

    pub(crate) fn new_hover(host_functions: &HashMap<String, HostFunction>, offset: usize) -> Self {
        let mut checker = Self::new_with_hosts(host_functions);
        checker.hover_offset = Some(offset);
        checker
    }

    pub(crate) fn check_module(mut self, module: &Module) -> Result<(), Diagnostic> {
        self.validate_top_level_declaration_names(&module.statements)?;
        self.collect_records(&module.statements)?;
        self.validate_record_declarations(&module.statements)?;
        self.check_statements(&module.statements).map(|_| ())
    }

    pub(crate) fn check_module_all(mut self, module: &Module) -> Result<(), Vec<Diagnostic>> {
        self.validate_top_level_declaration_names(&module.statements)
            .map_err(|err| vec![err])?;
        self.collect_records(&module.statements)
            .map_err(|err| vec![err])?;
        self.validate_record_declarations(&module.statements)
            .map_err(|err| vec![err])?;
        self.predeclare_functions(&module.statements)
            .map_err(|err| vec![err])?;

        let mut diagnostics = Vec::new();
        for statement in &module.statements {
            match self.check_statement(statement) {
                Ok(_) => {}
                Err(err) => diagnostics.push(err),
            }
        }

        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(diagnostics)
        }
    }

    pub(crate) fn hover_type(mut self, module: &Module) -> Result<Option<Type>, Diagnostic> {
        self.validate_top_level_declaration_names(&module.statements)?;
        self.collect_records(&module.statements)?;
        self.validate_record_declarations(&module.statements)?;
        self.check_statements(&module.statements)?;
        Ok(self.hover_type.map(|(_, ty)| ty))
    }

    fn validate_top_level_declaration_names(&self, statements: &[Stmt]) -> Result<(), Diagnostic> {
        let mut names = HashMap::new();
        for statement in statements {
            let Some((name, span)) = top_level_declaration(statement) else {
                continue;
            };
            if is_internal_import_name(name) {
                continue;
            }
            if is_option_result_constructor_name(name) {
                return Err(Diagnostic::new(
                    format!("name '{name}' is reserved for option/result construction"),
                    span,
                ));
            }
            if names.insert(name, span).is_some() {
                return Err(Diagnostic::new(format!("name '{name}' redeclared"), span)
                    .with_code("module.name-conflict"));
            }
        }
        Ok(())
    }

    fn collect_records(&mut self, statements: &[Stmt]) -> Result<(), Diagnostic> {
        for statement in statements {
            if let Stmt::Record {
                name, fields, span, ..
            } = statement
            {
                if self.records.contains_key(name) {
                    return Err(Diagnostic::new(
                        format!("record '{name}' is already defined"),
                        *span,
                    ));
                }
                let mut field_types = HashMap::new();
                for field in fields {
                    if field_types
                        .insert(field.name.clone(), field.ty.clone())
                        .is_some()
                    {
                        return Err(Diagnostic::new(
                            format!("duplicate field '{}'", field.name),
                            field.span,
                        ));
                    }
                }
                self.records.insert(
                    name.clone(),
                    RecordSchema {
                        fields: fields.clone(),
                        field_types,
                    },
                );
            }
        }
        Ok(())
    }

    fn validate_record_declarations(&self, statements: &[Stmt]) -> Result<(), Diagnostic> {
        for statement in statements {
            if let Stmt::Record { fields, .. } = statement {
                for field in fields {
                    self.validate_type(&field.ty, field.span)?;
                }
            }
        }
        Ok(())
    }

    fn check_statements(&mut self, statements: &[Stmt]) -> Result<bool, Diagnostic> {
        self.predeclare_functions(statements)?;
        let mut returned = false;
        for statement in statements {
            if self.check_statement(statement)? {
                returned = true;
            }
        }
        Ok(returned)
    }

    fn predeclare_functions(&mut self, statements: &[Stmt]) -> Result<(), Diagnostic> {
        for statement in statements {
            if let Stmt::Function {
                name,
                params,
                return_type,
                span,
                ..
            } = statement
            {
                for param in params {
                    self.validate_type(&param.ty, *span)?;
                }
                self.validate_type(return_type, *span)?;
                self.define(
                    name.clone(),
                    Type::Function {
                        params: params.iter().map(|param| param.ty.clone()).collect(),
                        return_type: Box::new(return_type.clone()),
                    },
                );
            }
        }
        Ok(())
    }

    fn check_block(&mut self, statements: &[Stmt]) -> Result<bool, Diagnostic> {
        self.scopes.push(HashMap::new());
        let result = self.check_statements(statements);
        self.scopes.pop();
        result
    }

    fn check_block_with_binding(
        &mut self,
        statements: &[Stmt],
        binding: Option<(&str, Type)>,
    ) -> Result<bool, Diagnostic> {
        self.scopes.push(HashMap::new());
        if let Some((name, ty)) = binding {
            self.define(name.to_string(), ty);
        }
        let result = self.check_statements(statements);
        self.scopes.pop();
        result
    }

    fn check_statement(&mut self, statement: &Stmt) -> Result<bool, Diagnostic> {
        match statement {
            Stmt::Import { span, .. } => Err(Diagnostic::new(
                "unresolved import; call Engine::eval with a module loader",
                *span,
            )),
            Stmt::Let {
                name,
                ty,
                initializer,
                exported: _,
                is_const,
                span: _,
            } => {
                self.validate_type(ty, initializer.span)?;
                let actual = self.check_expr_with_expected(initializer, Some(ty))?;
                self.expect_type(ty, &actual, initializer.span)?;
                if *is_const {
                    self.define_const(name.clone(), ty.clone());
                } else {
                    self.define(name.clone(), ty.clone());
                }
                Ok(false)
            }
            Stmt::Function {
                name,
                params,
                return_type,
                body,
                exported: _,
                span,
            } => {
                for param in params {
                    self.validate_type(&param.ty, *span)?;
                }
                self.validate_type(return_type, *span)?;
                let function_type = Type::Function {
                    params: params.iter().map(|param| param.ty.clone()).collect(),
                    return_type: Box::new(return_type.clone()),
                };
                self.define(name.clone(), function_type);

                self.scopes.push(HashMap::new());
                for param in params {
                    self.define(param.name.clone(), param.ty.clone());
                }
                self.returns.push(return_type.clone());
                let result = self.check_statements(body);
                self.returns.pop();
                self.scopes.pop();
                let has_return = result?;

                if !has_return {
                    return Err(Diagnostic::new(
                        format!("function '{name}' must return {return_type}"),
                        *span,
                    ));
                }
                Ok(false)
            }
            Stmt::Record { .. } => Ok(false),
            Stmt::Return { value, span } => {
                let Some(expected) = self.returns.last().cloned() else {
                    return Err(Diagnostic::new("return outside function", *span));
                };
                let actual = self.check_expr_with_expected(value, Some(&expected))?;
                self.expect_type(&expected, &actual, value.span)?;
                Ok(true)
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                span: _,
            } => {
                let actual = self.check_expr(condition)?;
                self.expect_type(&Type::Bool, &actual, condition.span)?;
                let then_returns = self.check_block(then_branch)?;
                let else_returns = self.check_block(else_branch)?;
                Ok(then_returns && else_returns && !else_branch.is_empty())
            }
            Stmt::Match {
                value,
                cases,
                default,
                span,
            } => {
                let matched = self.check_expr(value)?;
                self.check_match_statement(&matched, cases, default.as_deref(), *span, value.span)
            }
            Stmt::While {
                condition,
                body,
                span: _,
            } => {
                let actual = self.check_expr(condition)?;
                self.expect_type(&Type::Bool, &actual, condition.span)?;
                self.loop_depth += 1;
                let result = self.check_block(body);
                self.loop_depth -= 1;
                result?;
                Ok(false)
            }
            Stmt::For {
                name,
                start,
                end,
                body,
                span: _,
            } => {
                let actual = self.check_expr(start)?;
                self.expect_type(&Type::Int, &actual, start.span)?;
                let actual = self.check_expr(end)?;
                self.expect_type(&Type::Int, &actual, end.span)?;
                self.scopes.push(HashMap::new());
                self.define(name.clone(), Type::Int);
                self.loop_depth += 1;
                let result = self.check_block(body);
                self.loop_depth -= 1;
                self.scopes.pop();
                result?;
                Ok(false)
            }
            Stmt::Block { statements, .. } => self.check_block(statements),
            Stmt::Break { span } => {
                if self.loop_depth == 0 {
                    return Err(Diagnostic::new(
                        "'break' is only allowed inside a 'while' or 'for' loop",
                        *span,
                    ));
                }
                Ok(false)
            }
            Stmt::Continue { span } => {
                if self.loop_depth == 0 {
                    return Err(Diagnostic::new(
                        "'continue' is only allowed inside a 'while' or 'for' loop",
                        *span,
                    ));
                }
                Ok(false)
            }
            Stmt::Expression { expression, .. } => {
                self.check_expr(expression)?;
                Ok(false)
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr) -> Result<Type, Diagnostic> {
        self.check_expr_with_expected(expr, None)
    }

    fn check_expr_with_expected(
        &mut self,
        expr: &Expr,
        expected: Option<&Type>,
    ) -> Result<Type, Diagnostic> {
        let ty = match &expr.kind {
            ExprKind::Literal(Value::Null) => Ok(Type::Null),
            ExprKind::Literal(Value::Bool(_)) => Ok(Type::Bool),
            ExprKind::Literal(Value::Int(_)) => Ok(Type::Int),
            ExprKind::Literal(Value::Float(_)) => Ok(Type::Float),
            ExprKind::Literal(Value::String(_)) => Ok(Type::Str),
            ExprKind::Literal(Value::Array(_)) => {
                Err(Diagnostic::new("array value cannot be literal", expr.span))
            }
            ExprKind::Literal(Value::Map(_)) => {
                Err(Diagnostic::new("map value cannot be literal", expr.span))
            }
            ExprKind::Literal(Value::Option(_)) => {
                Err(Diagnostic::new("option value cannot be literal", expr.span))
            }
            ExprKind::Literal(Value::Result(_)) => {
                Err(Diagnostic::new("result value cannot be literal", expr.span))
            }
            ExprKind::Literal(Value::Record(_)) => {
                Err(Diagnostic::new("record value cannot be literal", expr.span))
            }
            ExprKind::Literal(Value::Function(_)) => Err(Diagnostic::new(
                "function value cannot be literal",
                expr.span,
            )),
            ExprKind::Variable(name) => {
                if name == "none" {
                    return self.check_none(expr.span, expected);
                }
                self.lookup(name).ok_or_else(|| {
                    Diagnostic::new(format!("undefined variable '{name}'"), expr.span)
                })
            }
            ExprKind::Assign { name, value } => {
                let binding = self.lookup_binding(name).ok_or_else(|| {
                    Diagnostic::new(format!("undefined variable '{name}'"), expr.span)
                })?;
                if binding.is_const {
                    return Err(Diagnostic::new(
                        format!("cannot assign to constant '{name}'"),
                        expr.span,
                    ));
                }
                let actual = self.check_expr_with_expected(value, Some(&binding.ty))?;
                self.expect_type(&binding.ty, &actual, value.span)?;
                Ok(binding.ty)
            }
            ExprKind::Unary { op, right } => {
                let right = self.check_expr(right)?;
                match op {
                    UnaryOp::Not => {
                        self.expect_type(&Type::Bool, &right, expr.span)?;
                        Ok(Type::Bool)
                    }
                    UnaryOp::Negate => {
                        if right.is_numeric() {
                            Ok(right)
                        } else {
                            Err(Diagnostic::new(
                                format!("unary '-' expects int or float, got {right}"),
                                expr.span,
                            ))
                        }
                    }
                }
            }
            ExprKind::Binary { left, op, right } => {
                let left = self.check_expr(left)?;
                let right = self.check_expr(right)?;
                self.check_binary(expr.span, &left, *op, &right)
            }
            ExprKind::Call {
                callee,
                args,
                paren_span,
            } => {
                if let ExprKind::Variable(name) = &callee.kind {
                    if matches!(name.as_str(), "some" | "ok" | "err") {
                        return self.check_option_result_constructor(
                            name,
                            expr.span,
                            args,
                            expected,
                            *paren_span,
                        );
                    }
                    if name == "len" {
                        return self.check_len_call(expr.span, args);
                    }
                    if name == "contains" {
                        return self.check_contains_call(expr.span, args);
                    }
                    if name == "map_get" {
                        return self.check_map_get_call(expr.span, args);
                    }
                }
                let callee = self.check_expr(callee)?;
                let Type::Function {
                    params,
                    return_type,
                } = callee
                else {
                    return Err(Diagnostic::new("called value is not a function", expr.span));
                };
                if args.len() != params.len() {
                    return Err(Diagnostic::new(
                        format!("expected {} arguments but got {}", params.len(), args.len()),
                        *paren_span,
                    ));
                }
                for (expected, arg) in params.iter().zip(args) {
                    let actual = self.check_expr_with_expected(arg, Some(expected))?;
                    self.expect_type(expected, &actual, arg.span)?;
                }
                Ok(*return_type)
            }
            ExprKind::ArrayLiteral { elements } => {
                self.check_array_literal(expr.span, elements, expected)
            }
            ExprKind::MapLiteral { entries } => {
                self.check_map_literal(expr.span, entries, expected)
            }
            ExprKind::RecordLiteral { name, fields } => {
                self.check_record_literal(expr.span, name, fields, expected)
            }
            ExprKind::Index { array, index } => {
                let indexed = self.check_expr(array)?;
                let index_type = self.check_expr(index)?;
                match indexed {
                    Type::Array(element) => {
                        self.expect_type(&Type::Int, &index_type, index.span)?;
                        Ok(*element)
                    }
                    Type::Map(value) => {
                        self.expect_type(&Type::Str, &index_type, index.span)?;
                        Ok(*value)
                    }
                    _ => Err(Diagnostic::new(
                        "indexed value is not an array or map",
                        expr.span,
                    )),
                }
            }
            ExprKind::Field {
                receiver,
                name,
                span,
            } => {
                let receiver = self.check_expr(receiver)?;
                let Type::Record(record_name) = receiver else {
                    return Err(Diagnostic::new(
                        "field access requires a record value",
                        expr.span,
                    ));
                };
                let schema = self.record_schema(&record_name, expr.span)?;
                schema.field_types.get(name).cloned().ok_or_else(|| {
                    Diagnostic::new(
                        format!("record '{record_name}' has no field '{name}'"),
                        *span,
                    )
                })
            }
        }?;
        self.record_hover_type(expr.span, ty.clone());
        Ok(ty)
    }

    fn check_array_literal(
        &mut self,
        span: Span,
        elements: &[Expr],
        expected: Option<&Type>,
    ) -> Result<Type, Diagnostic> {
        let expected_element = match expected {
            Some(Type::Array(element)) => Some(element.as_ref()),
            Some(other) => {
                return Err(Diagnostic::new(
                    format!("expected {other}, got array"),
                    span,
                ));
            }
            None => None,
        };

        let Some(first) = elements.first() else {
            if let Some(element) = expected_element {
                return Ok(Type::Array(Box::new(element.clone())));
            }
            return Err(Diagnostic::new(
                "empty array literal needs an expected type",
                span,
            ));
        };

        let element_type = if let Some(expected) = expected_element {
            let actual = self.check_expr_with_expected(first, Some(expected))?;
            self.expect_type(expected, &actual, first.span)?;
            expected.clone()
        } else {
            self.check_expr(first)?
        };

        for element in elements.iter().skip(1) {
            let actual = self.check_expr_with_expected(element, Some(&element_type))?;
            self.expect_type(&element_type, &actual, element.span)?;
        }

        Ok(Type::Array(Box::new(element_type)))
    }

    fn check_map_literal(
        &mut self,
        span: Span,
        entries: &[(Expr, Expr)],
        expected: Option<&Type>,
    ) -> Result<Type, Diagnostic> {
        let expected_value = match expected {
            Some(Type::Map(value)) => Some(value.as_ref()),
            Some(other) => {
                return Err(Diagnostic::new(format!("expected {other}, got map"), span));
            }
            None => None,
        };

        let Some((first_key, first_value)) = entries.first() else {
            if let Some(value) = expected_value {
                return Ok(Type::Map(Box::new(value.clone())));
            }
            return Err(Diagnostic::new(
                "empty map literal needs an expected type",
                span,
            ));
        };

        let key_type = self.check_expr(first_key)?;
        self.expect_type(&Type::Str, &key_type, first_key.span)?;

        let value_type = if let Some(expected) = expected_value {
            let actual = self.check_expr_with_expected(first_value, Some(expected))?;
            self.expect_type(expected, &actual, first_value.span)?;
            expected.clone()
        } else {
            self.check_expr(first_value)?
        };

        for (key, value) in entries.iter().skip(1) {
            let key_type = self.check_expr(key)?;
            self.expect_type(&Type::Str, &key_type, key.span)?;
            let actual = self.check_expr_with_expected(value, Some(&value_type))?;
            self.expect_type(&value_type, &actual, value.span)?;
        }

        Ok(Type::Map(Box::new(value_type)))
    }

    fn check_record_literal(
        &mut self,
        span: Span,
        name: &str,
        fields: &[(String, Expr, Span)],
        expected: Option<&Type>,
    ) -> Result<Type, Diagnostic> {
        let expected_type = Type::Record(name.to_string());
        if let Some(expected) = expected {
            self.expect_type(expected, &expected_type, span)?;
        }

        let schema = self.record_schema(name, span)?.clone();
        let mut seen = HashSet::new();
        for (field_name, value, field_span) in fields {
            if !seen.insert(field_name.clone()) {
                return Err(Diagnostic::new(
                    format!("duplicate field '{field_name}'"),
                    *field_span,
                ));
            }
            let expected = schema.field_types.get(field_name).ok_or_else(|| {
                Diagnostic::new(
                    format!("record '{name}' has no field '{field_name}'"),
                    *field_span,
                )
            })?;
            let actual = self.check_expr_with_expected(value, Some(expected))?;
            self.expect_type(expected, &actual, value.span)?;
        }

        for field in &schema.fields {
            if !seen.contains(&field.name) {
                return Err(Diagnostic::new(
                    format!("missing field '{}'", field.name),
                    span,
                ));
            }
        }

        Ok(expected_type)
    }

    fn check_none(&mut self, span: Span, expected: Option<&Type>) -> Result<Type, Diagnostic> {
        let Some(Type::Option(payload)) = expected else {
            return Err(Diagnostic::new(
                "'none' requires expected option type",
                span,
            ));
        };
        Ok(Type::Option(payload.clone()))
    }

    fn check_option_result_constructor(
        &mut self,
        name: &str,
        span: Span,
        args: &[Expr],
        expected: Option<&Type>,
        paren_span: Span,
    ) -> Result<Type, Diagnostic> {
        match name {
            "some" => {
                if args.len() != 1 {
                    return Err(Diagnostic::new(
                        format!("expected 1 arguments but got {}", args.len()),
                        paren_span,
                    ));
                }
                if let Some(Type::Option(payload)) = expected {
                    let actual = self.check_expr_with_expected(&args[0], Some(payload))?;
                    self.expect_type(payload, &actual, args[0].span)?;
                    Ok(Type::Option(payload.clone()))
                } else {
                    let payload = self.check_expr(&args[0])?;
                    Ok(Type::Option(Box::new(payload)))
                }
            }
            "ok" => {
                if args.len() != 1 {
                    return Err(Diagnostic::new(
                        format!("expected 1 arguments but got {}", args.len()),
                        paren_span,
                    ));
                }
                let Some(Type::Result { ok, err }) = expected else {
                    return Err(Diagnostic::new("'ok' requires expected result type", span));
                };
                let actual = self.check_expr_with_expected(&args[0], Some(ok))?;
                self.expect_type(ok, &actual, args[0].span)?;
                Ok(Type::Result {
                    ok: ok.clone(),
                    err: err.clone(),
                })
            }
            "err" => {
                if args.len() != 1 {
                    return Err(Diagnostic::new(
                        format!("expected 1 arguments but got {}", args.len()),
                        paren_span,
                    ));
                }
                let Some(Type::Result { ok, err }) = expected else {
                    return Err(Diagnostic::new("'err' requires expected result type", span));
                };
                let actual = self.check_expr_with_expected(&args[0], Some(err))?;
                self.expect_type(err, &actual, args[0].span)?;
                Ok(Type::Result {
                    ok: ok.clone(),
                    err: err.clone(),
                })
            }
            _ => unreachable!("only option/result constructors are routed here"),
        }
    }

    fn check_match_statement(
        &mut self,
        matched: &Type,
        cases: &[crate::MatchCase],
        default: Option<&[Stmt]>,
        span: Span,
        value_span: Span,
    ) -> Result<bool, Diagnostic> {
        match matched {
            Type::Int | Type::Str => {
                self.check_literal_match_statement(matched, cases, default, span)
            }
            Type::Option(payload) => {
                if default.is_some() {
                    return Err(Diagnostic::new(
                        "option match does not accept '_' default case",
                        span,
                    ));
                }
                let mut saw_some = false;
                let mut saw_none = false;
                let mut all_cases_return = true;
                for case in cases {
                    match &case.value {
                        MatchCaseValue::Some(name) => {
                            if saw_some {
                                return Err(Diagnostic::new("duplicate match case", case.span));
                            }
                            saw_some = true;
                            all_cases_return &= self.check_block_with_binding(
                                &case.body,
                                Some((name, payload.as_ref().clone())),
                            )?;
                        }
                        MatchCaseValue::None => {
                            if saw_none {
                                return Err(Diagnostic::new("duplicate match case", case.span));
                            }
                            saw_none = true;
                            all_cases_return &= self.check_block(&case.body)?;
                        }
                        _ => {
                            return Err(Diagnostic::new(
                                "option match only accepts some(name) and none cases",
                                case.span,
                            ));
                        }
                    }
                }
                if !saw_some || !saw_none {
                    return Err(Diagnostic::new(
                        "option match must cover some and none",
                        span,
                    ));
                }
                Ok(all_cases_return)
            }
            Type::Result { ok, err } => {
                if default.is_some() {
                    return Err(Diagnostic::new(
                        "result match does not accept '_' default case",
                        span,
                    ));
                }
                let mut saw_ok = false;
                let mut saw_err = false;
                let mut all_cases_return = true;
                for case in cases {
                    match &case.value {
                        MatchCaseValue::Ok(name) => {
                            if saw_ok {
                                return Err(Diagnostic::new("duplicate match case", case.span));
                            }
                            saw_ok = true;
                            all_cases_return &= self.check_block_with_binding(
                                &case.body,
                                Some((name, ok.as_ref().clone())),
                            )?;
                        }
                        MatchCaseValue::Err(name) => {
                            if saw_err {
                                return Err(Diagnostic::new("duplicate match case", case.span));
                            }
                            saw_err = true;
                            all_cases_return &= self.check_block_with_binding(
                                &case.body,
                                Some((name, err.as_ref().clone())),
                            )?;
                        }
                        _ => {
                            return Err(Diagnostic::new(
                                "result match only accepts ok(name) and err(name) cases",
                                case.span,
                            ));
                        }
                    }
                }
                if !saw_ok || !saw_err {
                    return Err(Diagnostic::new("result match must cover ok and err", span));
                }
                Ok(all_cases_return)
            }
            _ => Err(Diagnostic::new(
                format!("match value must be int, str, option, or result, got {matched}"),
                value_span,
            )),
        }
    }

    fn check_literal_match_statement(
        &mut self,
        matched: &Type,
        cases: &[crate::MatchCase],
        default: Option<&[Stmt]>,
        span: Span,
    ) -> Result<bool, Diagnostic> {
        let Some(default) = default else {
            return Err(Diagnostic::new("match requires '_' default case", span));
        };

        let mut seen = HashSet::new();
        let mut all_cases_return = true;
        for case in cases {
            let case_type = match_case_type(&case.value).ok_or_else(|| {
                Diagnostic::new("literal match only accepts int and str cases", case.span)
            })?;
            self.expect_type(matched, &case_type, case.span)?;
            if !seen.insert(case.value.clone()) {
                return Err(Diagnostic::new("duplicate match case", case.span));
            }
            all_cases_return &= self.check_block(&case.body)?;
        }
        let default_returns = self.check_block(default)?;
        Ok(all_cases_return && default_returns)
    }

    fn check_len_call(&mut self, span: Span, args: &[Expr]) -> Result<Type, Diagnostic> {
        if args.len() != 1 {
            return Err(Diagnostic::new(
                format!("expected 1 arguments but got {}", args.len()),
                span,
            ));
        }
        let actual = self.check_expr(&args[0])?;
        if !matches!(actual, Type::Array(_) | Type::Str) {
            return Err(Diagnostic::new(
                format!("expected array or str, got {actual}"),
                args[0].span,
            ));
        }
        Ok(Type::Int)
    }

    fn check_contains_call(&mut self, span: Span, args: &[Expr]) -> Result<Type, Diagnostic> {
        if args.len() != 2 {
            return Err(Diagnostic::new(
                format!("expected 2 arguments but got {}", args.len()),
                span,
            ));
        }
        let map_type = self.check_expr(&args[0])?;
        if !matches!(map_type, Type::Map(_)) {
            return Err(Diagnostic::new(
                format!("expected map, got {map_type}"),
                args[0].span,
            ));
        }
        let key_type = self.check_expr(&args[1])?;
        self.expect_type(&Type::Str, &key_type, args[1].span)?;
        Ok(Type::Bool)
    }

    fn check_map_get_call(&mut self, span: Span, args: &[Expr]) -> Result<Type, Diagnostic> {
        if args.len() != 2 {
            return Err(Diagnostic::new(
                format!("expected 2 arguments but got {}", args.len()),
                span,
            ));
        }
        let map_type = self.check_expr(&args[0])?;
        let Type::Map(value) = map_type else {
            return Err(Diagnostic::new(
                format!("expected map, got {map_type}"),
                args[0].span,
            ));
        };
        let key_type = self.check_expr(&args[1])?;
        self.expect_type(&Type::Str, &key_type, args[1].span)?;
        Ok(Type::Option(value))
    }

    fn check_binary(
        &self,
        span: Span,
        left: &Type,
        op: BinaryOp,
        right: &Type,
    ) -> Result<Type, Diagnostic> {
        match op {
            BinaryOp::And | BinaryOp::Or => {
                self.expect_type(&Type::Bool, left, span)?;
                self.expect_type(&Type::Bool, right, span)?;
                Ok(Type::Bool)
            }
            BinaryOp::RangeLessThan => {
                self.expect_type(&Type::Int, left, span)?;
                self.expect_type(&Type::Int, right, span)?;
                Ok(Type::Bool)
            }
            BinaryOp::Add => match (left, right) {
                (Type::Int, Type::Int) => Ok(Type::Int),
                (Type::Float, Type::Float) => Ok(Type::Float),
                (Type::Str, Type::Str) => Ok(Type::Str),
                _ => Err(Diagnostic::new(
                    format!("'+' is not defined for {left} and {right}"),
                    span,
                )),
            },
            BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide => {
                self.expect_same_numeric(left, right, span)
            }
            BinaryOp::Greater | BinaryOp::GreaterEqual | BinaryOp::Less | BinaryOp::LessEqual => {
                self.expect_same_numeric(left, right, span)
                    .map(|_| Type::Bool)
            }
            BinaryOp::Equal | BinaryOp::NotEqual
                if matches!(left, Type::Array(_) | Type::Map(_) | Type::Record(_)) =>
            {
                Err(Diagnostic::new("container equality is not supported", span))
            }
            BinaryOp::Equal | BinaryOp::NotEqual => {
                self.expect_type(left, right, span)?;
                Ok(Type::Bool)
            }
        }
    }

    fn expect_type(&self, expected: &Type, actual: &Type, span: Span) -> Result<(), Diagnostic> {
        if expected == actual {
            Ok(())
        } else {
            Err(
                Diagnostic::new(format!("expected {expected}, got {actual}"), span)
                    .with_code("type.mismatch"),
            )
        }
    }

    fn expect_same_numeric(
        &self,
        left: &Type,
        right: &Type,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        match (left, right) {
            (Type::Int, Type::Int) => Ok(Type::Int),
            (Type::Float, Type::Float) => Ok(Type::Float),
            _ => Err(Diagnostic::new(
                format!("operator expects matching numeric types, got {left} and {right}"),
                span,
            )),
        }
    }

    fn validate_type(&self, ty: &Type, span: Span) -> Result<(), Diagnostic> {
        match ty {
            Type::Null | Type::Bool | Type::Int | Type::Float | Type::Str => Ok(()),
            Type::Array(element) | Type::Map(element) => self.validate_type(element, span),
            Type::Option(value) => self.validate_type(value, span),
            Type::Result { ok, err } => {
                self.validate_type(ok, span)?;
                self.validate_type(err, span)
            }
            Type::Record(name) => self.record_schema(name, span).map(|_| ()),
            Type::Function {
                params,
                return_type,
            } => {
                for param in params {
                    self.validate_type(param, span)?;
                }
                self.validate_type(return_type, span)
            }
        }
    }

    fn record_schema(&self, name: &str, span: Span) -> Result<&RecordSchema, Diagnostic> {
        self.records
            .get(name)
            .ok_or_else(|| Diagnostic::new(format!("unknown type '{name}'"), span))
    }

    fn define(&mut self, name: String, ty: Type) {
        self.define_binding(
            name,
            Binding {
                ty,
                is_const: false,
            },
        );
    }

    fn define_const(&mut self, name: String, ty: Type) {
        self.define_binding(name, Binding { ty, is_const: true });
    }

    fn define_binding(&mut self, name: String, binding: Binding) {
        self.scopes
            .last_mut()
            .expect("type checker always has a scope")
            .insert(name, binding);
    }

    fn lookup(&self, name: &str) -> Option<Type> {
        self.lookup_binding(name).map(|binding| binding.ty)
    }

    fn lookup_binding(&self, name: &str) -> Option<Binding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn record_hover_type(&mut self, span: Span, ty: Type) {
        let Some(offset) = self.hover_offset else {
            return;
        };
        if offset < span.start || offset >= span.end {
            return;
        }
        let current_width = self
            .hover_type
            .as_ref()
            .map(|(span, _)| span.end.saturating_sub(span.start))
            .unwrap_or(usize::MAX);
        let width = span.end.saturating_sub(span.start);
        if width <= current_width {
            self.hover_type = Some((span, ty));
        }
    }
}

fn top_level_declaration(statement: &Stmt) -> Option<(&str, Span)> {
    match statement {
        Stmt::Let { name, span, .. }
        | Stmt::Function { name, span, .. }
        | Stmt::Record { name, span, .. } => Some((name, *span)),
        _ => None,
    }
}

fn is_internal_import_name(name: &str) -> bool {
    name.starts_with("$import$")
}

fn is_option_result_constructor_name(name: &str) -> bool {
    matches!(name, "some" | "none" | "ok" | "err")
}

fn match_case_type(value: &MatchCaseValue) -> Option<Type> {
    match value {
        MatchCaseValue::Int(_) => Some(Type::Int),
        MatchCaseValue::Str(_) => Some(Type::Str),
        MatchCaseValue::Some(_)
        | MatchCaseValue::None
        | MatchCaseValue::Ok(_)
        | MatchCaseValue::Err(_) => None,
    }
}
