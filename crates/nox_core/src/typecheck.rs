use std::collections::{HashMap, HashSet};

use crate::{
    type_implements_marker, ArrayElement, BinaryOp, ConstraintMarker, Diagnostic, EnumVariant,
    Expr, ExprKind, HostFunction, MapEntry, MatchCaseValue, Module, RecordField, Span, Stmt, Type,
    UnaryOp, Value,
};

#[derive(Debug, Clone)]
struct RecordSchema {
    fields: Vec<RecordField>,
    field_types: HashMap<String, Type>,
}

#[derive(Debug, Clone)]
struct EnumSchema {
    variants: Vec<EnumVariant>,
    variant_payloads: HashMap<String, Option<Type>>,
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
    enums: HashMap<String, EnumSchema>,
    type_aliases: HashMap<String, Type>,
    type_alias_spans: HashMap<String, Span>,
    hover_offset: Option<usize>,
    hover_type: Option<(Span, Type)>,
    loop_depth: usize,
    expression_depth: usize,
    function_type_params: Vec<HashSet<String>>,
    function_constraints: HashMap<String, Vec<(String, Vec<ConstraintMarker>)>>,
    active_constraints: Vec<HashMap<String, Vec<ConstraintMarker>>>,
}

const MAX_TYPECHECK_EXPRESSION_DEPTH: usize = 128;

impl TypeChecker {
    pub(crate) fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            returns: Vec::new(),
            records: HashMap::new(),
            enums: HashMap::new(),
            type_aliases: HashMap::new(),
            type_alias_spans: HashMap::new(),
            hover_offset: None,
            hover_type: None,
            loop_depth: 0,
            expression_depth: 0,
            function_type_params: Vec::new(),
            function_constraints: HashMap::new(),
            active_constraints: Vec::new(),
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
        self.collect_type_aliases(&module.statements)?;
        self.collect_records(&module.statements)?;
        self.collect_enums(&module.statements)?;
        self.resolve_all_type_aliases()?;
        self.normalize_record_schemas()?;
        self.normalize_enum_schemas()?;
        self.validate_record_declarations(&module.statements)?;
        self.validate_enum_declarations(&module.statements)?;
        self.check_statements(&module.statements).map(|_| ())
    }

    pub(crate) fn check_module_all(mut self, module: &Module) -> Result<(), Vec<Diagnostic>> {
        self.validate_top_level_declaration_names(&module.statements)
            .map_err(|err| vec![err])?;
        self.collect_type_aliases(&module.statements)
            .map_err(|err| vec![err])?;
        self.collect_records(&module.statements)
            .map_err(|err| vec![err])?;
        self.collect_enums(&module.statements)
            .map_err(|err| vec![err])?;
        self.resolve_all_type_aliases().map_err(|err| vec![err])?;
        self.normalize_record_schemas().map_err(|err| vec![err])?;
        self.normalize_enum_schemas().map_err(|err| vec![err])?;
        self.validate_record_declarations(&module.statements)
            .map_err(|err| vec![err])?;
        self.validate_enum_declarations(&module.statements)
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
        self.collect_type_aliases(&module.statements)?;
        self.collect_records(&module.statements)?;
        self.collect_enums(&module.statements)?;
        self.resolve_all_type_aliases()?;
        self.normalize_record_schemas()?;
        self.normalize_enum_schemas()?;
        self.validate_record_declarations(&module.statements)?;
        self.validate_enum_declarations(&module.statements)?;
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

    fn collect_type_aliases(&mut self, statements: &[Stmt]) -> Result<(), Diagnostic> {
        for statement in statements {
            if let Stmt::TypeAlias { name, ty, span, .. } = statement {
                self.type_aliases.insert(name.clone(), ty.clone());
                self.type_alias_spans.insert(name.clone(), *span);
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

    fn collect_enums(&mut self, statements: &[Stmt]) -> Result<(), Diagnostic> {
        for statement in statements {
            if let Stmt::Enum {
                name,
                variants,
                span,
                ..
            } = statement
            {
                if self.enums.contains_key(name) {
                    return Err(Diagnostic::new(
                        format!("enum '{name}' is already defined"),
                        *span,
                    ));
                }
                let mut variant_payloads = HashMap::new();
                for variant in variants {
                    if variant_payloads
                        .insert(variant.name.clone(), variant.payload.clone())
                        .is_some()
                    {
                        return Err(Diagnostic::new(
                            format!("duplicate enum variant '{}'", variant.name),
                            variant.span,
                        ));
                    }
                }
                self.enums.insert(
                    name.clone(),
                    EnumSchema {
                        variants: variants.clone(),
                        variant_payloads,
                    },
                );
            }
        }
        Ok(())
    }

    fn resolve_all_type_aliases(&mut self) -> Result<(), Diagnostic> {
        let names = self.type_aliases.keys().cloned().collect::<Vec<_>>();
        let mut resolved = HashMap::new();
        for name in names {
            let ty = self.resolve_alias_name(&name, &mut Vec::new())?;
            resolved.insert(name, ty);
        }
        self.type_aliases = resolved;
        Ok(())
    }

    fn resolve_alias_name(&self, name: &str, stack: &mut Vec<String>) -> Result<Type, Diagnostic> {
        if stack.iter().any(|entry| entry == name) {
            let span = self
                .type_alias_spans
                .get(name)
                .copied()
                .unwrap_or(Span { start: 0, end: 0 });
            return Err(Diagnostic::new(format!("cyclic type alias '{name}'"), span)
                .with_code("type-alias.cyclic"));
        }
        let Some(raw) = self.type_aliases.get(name) else {
            return Ok(Type::Record(name.to_string()));
        };
        stack.push(name.to_string());
        let resolved = self.resolve_type_aliases(raw, stack);
        stack.pop();
        resolved
    }

    fn resolve_type_aliases(&self, ty: &Type, stack: &mut Vec<String>) -> Result<Type, Diagnostic> {
        match ty {
            Type::Array(element) => Ok(Type::Array(Box::new(
                self.resolve_type_aliases(element, stack)?,
            ))),
            Type::Tuple(elements) => elements
                .iter()
                .map(|element| self.resolve_type_aliases(element, stack))
                .collect::<Result<Vec<_>, _>>()
                .map(Type::Tuple),
            Type::Map(value) => Ok(Type::Map(Box::new(
                self.resolve_type_aliases(value, stack)?,
            ))),
            Type::Option(value) => Ok(Type::Option(Box::new(
                self.resolve_type_aliases(value, stack)?,
            ))),
            Type::Result { ok, err } => Ok(Type::Result {
                ok: Box::new(self.resolve_type_aliases(ok, stack)?),
                err: Box::new(self.resolve_type_aliases(err, stack)?),
            }),
            Type::Record(name) if self.type_aliases.contains_key(name) => {
                self.resolve_alias_name(name, stack)
            }
            Type::Enum(name) if self.type_aliases.contains_key(name) => {
                self.resolve_alias_name(name, stack)
            }
            Type::Function {
                type_params,
                params,
                return_type,
            } => Ok(Type::Function {
                type_params: type_params.clone(),
                params: params
                    .iter()
                    .map(|param| self.resolve_type_aliases(param, stack))
                    .collect::<Result<Vec<_>, _>>()?,
                return_type: Box::new(self.resolve_type_aliases(return_type, stack)?),
            }),
            Type::Generic(_) => Ok(ty.clone()),
            other => Ok(other.clone()),
        }
    }

    fn normalize_record_schemas(&mut self) -> Result<(), Diagnostic> {
        let names = self.records.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let Some(schema) = self.records.get(&name).cloned() else {
                continue;
            };
            let fields = schema
                .fields
                .into_iter()
                .map(|field| {
                    Ok(RecordField {
                        name: field.name,
                        ty: self.resolve_type(&field.ty, field.span)?,
                        span: field.span,
                    })
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            let field_types = fields
                .iter()
                .map(|field| (field.name.clone(), field.ty.clone()))
                .collect();
            self.records.insert(
                name,
                RecordSchema {
                    fields,
                    field_types,
                },
            );
        }
        Ok(())
    }

    fn normalize_enum_schemas(&mut self) -> Result<(), Diagnostic> {
        let names = self.enums.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let Some(schema) = self.enums.get(&name).cloned() else {
                continue;
            };
            let variants = schema
                .variants
                .into_iter()
                .map(|variant| {
                    Ok(EnumVariant {
                        name: variant.name,
                        payload: variant
                            .payload
                            .map(|ty| self.resolve_type(&ty, variant.span))
                            .transpose()?,
                        span: variant.span,
                    })
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            let variant_payloads = variants
                .iter()
                .map(|variant| (variant.name.clone(), variant.payload.clone()))
                .collect();
            self.enums.insert(
                name,
                EnumSchema {
                    variants,
                    variant_payloads,
                },
            );
        }
        Ok(())
    }

    fn validate_record_declarations(&self, statements: &[Stmt]) -> Result<(), Diagnostic> {
        for statement in statements {
            if let Stmt::Record { fields, .. } = statement {
                for field in fields {
                    self.resolve_type(&field.ty, field.span)?;
                }
            }
        }
        Ok(())
    }

    fn validate_enum_declarations(&self, statements: &[Stmt]) -> Result<(), Diagnostic> {
        for statement in statements {
            if let Stmt::Enum { variants, .. } = statement {
                for variant in variants {
                    if let Some(payload) = &variant.payload {
                        self.resolve_type(payload, variant.span)?;
                    }
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
                type_params,
                params,
                return_type,
                span,
                ..
            } = statement
            {
                let resolved_params = params
                    .iter()
                    .map(|param| self.resolve_type_in_function(&param.ty, type_params, *span))
                    .collect::<Result<Vec<_>, _>>()?;
                let resolved_return =
                    self.resolve_type_in_function(return_type, type_params, *span)?;
                self.define(
                    name.clone(),
                    Type::Function {
                        type_params: type_params.clone(),
                        params: resolved_params,
                        return_type: Box::new(resolved_return),
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

    fn check_block_with_bindings(
        &mut self,
        statements: &[Stmt],
        bindings: &[(String, Type)],
    ) -> Result<bool, Diagnostic> {
        self.scopes.push(HashMap::new());
        for (name, ty) in bindings {
            self.define(name.clone(), ty.clone());
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
                target,
                ty,
                initializer,
                exported: _,
                is_const,
                span: _,
            } => {
                let expected = ty
                    .as_ref()
                    .map(|ty| self.resolve_type(ty, initializer.span))
                    .transpose()?;
                if let Some(ty) = &expected {
                    self.validate_type(ty, initializer.span)?;
                }
                let actual = self.check_expr_with_expected(initializer, expected.as_ref())?;
                if let Some(expected) = &expected {
                    self.expect_type(expected, &actual, initializer.span)?;
                }
                self.define_binding_target(target, &actual, *is_const, initializer.span)?;
                Ok(false)
            }
            Stmt::LetElse {
                pattern,
                value,
                else_branch,
                span,
            } => {
                let matched = self.check_expr(value)?;
                let bindings = self.check_match_pattern(&matched, pattern, *span)?;
                let else_returns = self.check_block(else_branch)?;
                if !else_returns {
                    return Err(Diagnostic::new(
                        "let-else branch must return before pattern bindings are used",
                        *span,
                    )
                    .with_code("control-flow.let-else-fallthrough"));
                }
                for (name, ty) in bindings {
                    self.define(name, ty);
                }
                Ok(false)
            }
            Stmt::Function {
                name,
                type_params,
                type_param_constraints,
                params,
                return_type,
                body,
                exported: _,
                span,
            } => {
                let resolved_params = params
                    .iter()
                    .map(|param| self.resolve_type_in_function(&param.ty, type_params, *span))
                    .collect::<Result<Vec<_>, _>>()?;
                let return_type = self.resolve_type_in_function(return_type, type_params, *span)?;
                let function_type = Type::Function {
                    type_params: type_params.clone(),
                    params: resolved_params.clone(),
                    return_type: Box::new(return_type.clone()),
                };
                self.define(name.clone(), function_type);

                let mut active_scope: HashMap<String, Vec<ConstraintMarker>> = HashMap::new();
                if !type_params.is_empty() && !type_param_constraints.is_empty() {
                    let constraints: Vec<(String, Vec<ConstraintMarker>)> = type_params
                        .iter()
                        .zip(type_param_constraints.iter())
                        .map(|(name, markers)| (name.clone(), markers.clone()))
                        .collect();
                    if constraints.iter().any(|(_, m)| !m.is_empty()) {
                        self.function_constraints
                            .insert(name.clone(), constraints.clone());
                    }
                    for (name, markers) in constraints {
                        active_scope.insert(name, markers);
                    }
                }
                self.active_constraints.push(active_scope);

                self.function_type_params
                    .push(type_params.iter().cloned().collect());
                self.scopes.push(HashMap::new());
                for (param, ty) in params.iter().zip(resolved_params) {
                    self.define(param.name.clone(), ty);
                }
                let return_type_for_message = return_type.clone();
                self.returns.push(return_type);
                let result = self.check_statements(body);
                self.returns.pop();
                self.scopes.pop();
                self.function_type_params.pop();
                self.active_constraints.pop();
                let has_return = result?;

                if !has_return {
                    return Err(Diagnostic::new(
                        format!("function '{name}' must return {return_type_for_message}"),
                        *span,
                    ));
                }
                Ok(false)
            }
            Stmt::TypeAlias { .. } | Stmt::Enum { .. } | Stmt::Record { .. } => Ok(false),
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
            Stmt::IfLet {
                pattern,
                value,
                then_branch,
                else_branch,
                span,
            } => {
                let matched = self.check_expr(value)?;
                let bindings = self.check_match_pattern(&matched, pattern, *span)?;
                let then_returns = self.check_block_with_bindings(then_branch, &bindings)?;
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
            Stmt::WhileLet {
                pattern,
                value,
                body,
                span,
            } => {
                let matched = self.check_expr(value)?;
                let bindings = self.check_match_pattern(&matched, pattern, *span)?;
                self.loop_depth += 1;
                let result = self.check_block_with_bindings(body, &bindings);
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
        if self.expression_depth >= MAX_TYPECHECK_EXPRESSION_DEPTH {
            return Err(Diagnostic::new("expression nesting is too deep", expr.span)
                .with_code("type.nesting-depth"));
        }
        self.expression_depth += 1;
        let result = (|| {
            let ty = match &expr.kind {
                ExprKind::Literal(Value::Null) => Ok(Type::Null),
                ExprKind::Literal(Value::Bool(_)) => Ok(Type::Bool),
                ExprKind::Literal(Value::Int(_)) => Ok(Type::Int),
                ExprKind::Literal(Value::Float(_)) => Ok(Type::Float),
                ExprKind::Literal(Value::String(_)) => Ok(Type::Str),
                ExprKind::Literal(Value::Json(_)) => Ok(Type::Json),
                ExprKind::Literal(Value::Tuple(_)) => {
                    Err(Diagnostic::new("tuple value cannot be literal", expr.span))
                }
                ExprKind::StringInterpolation(parts) => {
                    for part in parts {
                        if let Some(expression) = &part.expression {
                            let ty = self.check_expr(expression)?;
                            if !matches!(
                                ty,
                                Type::Null | Type::Bool | Type::Int | Type::Float | Type::Str
                            ) {
                                return Err(Diagnostic::new(
                                    format!("string interpolation cannot stringify {ty}"),
                                    expression.span,
                                )
                                .with_code("string.interpolation"));
                            }
                        }
                    }
                    Ok(Type::Str)
                }
                ExprKind::Question { value } => self.check_question_expr(expr.span, value),
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
                ExprKind::Literal(Value::Enum(_)) => {
                    Err(Diagnostic::new("enum value cannot be literal", expr.span))
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
                        let suggestion = self.suggest_variable(name);
                        Diagnostic::new(
                            append_did_you_mean(format!("undefined variable '{name}'"), suggestion),
                            expr.span,
                        )
                    })
                }
                ExprKind::Assign { name, value } => {
                    let binding = self.lookup_binding(name).ok_or_else(|| {
                        let suggestion = self.suggest_variable(name);
                        Diagnostic::new(
                            append_did_you_mean(format!("undefined variable '{name}'"), suggestion),
                            expr.span,
                        )
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
                        UnaryOp::BitNot => {
                            self.expect_bitwise_int(&right, expr.span)?;
                            Ok(Type::Int)
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
                    if let ExprKind::Field {
                        receiver,
                        name,
                        span,
                    } = &callee.kind
                    {
                        if let ExprKind::Variable(enum_name) = &receiver.kind {
                            if self.enums.contains_key(enum_name) {
                                return self.check_enum_constructor(
                                    enum_name,
                                    name,
                                    expr.span,
                                    args,
                                    *paren_span,
                                );
                            }
                        }
                        return self.check_record_method_call(
                            expr.span,
                            receiver,
                            name,
                            *span,
                            args,
                            *paren_span,
                        );
                    }
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
                        if is_std_json_from_json_name(name) {
                            return self.check_json_from_json_call(expr.span, args, expected);
                        }
                        if name == "len" {
                            return self.check_len_call(expr.span, args);
                        }
                        if name == "contains" {
                            return self.check_contains_call(expr.span, args);
                        }
                        if name == "map_has" {
                            return self.check_contains_call(expr.span, args);
                        }
                        if name == "map_keys" {
                            return self.check_map_keys_call(expr.span, args);
                        }
                        if name == "map_values" {
                            return self.check_map_values_call(expr.span, args);
                        }
                        if name == "map_size" {
                            return self.check_map_size_call(expr.span, args);
                        }
                        if name == "map_get" {
                            return self.check_map_get_call(expr.span, args);
                        }
                    }
                    let callee_name = if let ExprKind::Variable(name) = &callee.kind {
                        Some(name.clone())
                    } else {
                        None
                    };
                    let callee = self.check_expr(callee)?;
                    let Type::Function {
                        type_params,
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
                    let mut bindings = HashMap::new();
                    for (expected, arg) in params.iter().zip(args) {
                        let actual = self.check_expr_with_expected(
                            arg,
                            self.concrete_expected_type(expected).as_ref(),
                        )?;
                        self.unify_call_type(
                            expected,
                            &actual,
                            &type_params,
                            arg.span,
                            &mut bindings,
                        )?;
                    }
                    let result = self.instantiate_return_type(
                        &return_type,
                        expected,
                        &type_params,
                        &bindings,
                        expr.span,
                    )?;
                    if let Some(name) = callee_name {
                        self.verify_constraints(&name, &bindings, expr.span)?;
                    }
                    Ok(result)
                }
                ExprKind::ArrayLiteral { elements } => {
                    self.check_array_literal(expr.span, elements, expected)
                }
                ExprKind::TupleLiteral { elements } => {
                    self.check_tuple_literal(expr.span, elements, expected)
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
                ExprKind::FunctionLiteral {
                    params,
                    return_type,
                    body,
                } => {
                    let resolved_params = params
                        .iter()
                        .map(|param| {
                            self.validate_type(&param.ty, expr.span)
                                .map(|_| param.ty.clone())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    self.validate_type(return_type, expr.span)?;
                    let function_type = Type::Function {
                        type_params: Vec::new(),
                        params: resolved_params.clone(),
                        return_type: Box::new(return_type.clone()),
                    };

                    self.function_type_params.push(HashSet::new());
                    self.scopes.push(HashMap::new());
                    for (param, ty) in params.iter().zip(resolved_params.iter()) {
                        self.define(param.name.clone(), ty.clone());
                    }
                    self.returns.push(return_type.clone());
                    let result = self.check_statements(body);
                    self.returns.pop();
                    self.scopes.pop();
                    self.function_type_params.pop();
                    let has_return = result?;
                    if !has_return {
                        return Err(Diagnostic::new(
                            format!("lambda must return {return_type}"),
                            expr.span,
                        ));
                    }
                    Ok(function_type)
                }
                ExprKind::IndexAssign {
                    container,
                    index,
                    value,
                } => {
                    let container_type = self.check_expr(container)?;
                    let index_type = self.check_expr(index)?;
                    match container_type {
                        Type::Array(element) => {
                            self.expect_type(&Type::Int, &index_type, index.span)?;
                            let actual = self.check_expr_with_expected(value, Some(&element))?;
                            self.expect_type(&element, &actual, value.span)?;
                            Ok(Type::Null)
                        }
                        Type::Map(map_value) => {
                            self.expect_type(&Type::Str, &index_type, index.span)?;
                            let actual = self.check_expr_with_expected(value, Some(&map_value))?;
                            self.expect_type(&map_value, &actual, value.span)?;
                            Ok(Type::Null)
                        }
                        _ => Err(Diagnostic::new(
                            "indexed assignment target is not an array or map",
                            expr.span,
                        )
                        .with_code("type.assign-target")),
                    }
                }
                ExprKind::Field {
                    receiver,
                    name,
                    span,
                } => {
                    if let ExprKind::Variable(enum_name) = &receiver.kind {
                        if self.enums.contains_key(enum_name) {
                            return self.check_enum_constructor(
                                enum_name,
                                name,
                                expr.span,
                                &[],
                                *span,
                            );
                        }
                    }
                    let receiver = self.check_expr(receiver)?;
                    let Type::Record(record_name) = receiver else {
                        return Err(Diagnostic::new(
                            "field access requires a record value",
                            expr.span,
                        ));
                    };
                    let schema = self.record_schema(&record_name, expr.span)?;
                    let field_ty = schema.field_types.get(name).cloned();
                    field_ty.ok_or_else(|| {
                        let suggestion = self.suggest_record_field(&record_name, name);
                        Diagnostic::new(
                            append_did_you_mean(
                                format!("record '{record_name}' has no field '{name}'"),
                                suggestion,
                            ),
                            *span,
                        )
                    })
                }
            }?;
            Ok(ty)
        })();
        self.expression_depth -= 1;

        let ty = result?;
        self.record_hover_type(expr.span, ty.clone());
        Ok(ty)
    }

    fn check_array_literal(
        &mut self,
        span: Span,
        elements: &[ArrayElement],
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

        let (first_type, first_span) = self.check_array_element(first, expected_element)?;
        let element_type = if let Some(expected) = expected_element {
            self.expect_type(expected, &first_type, first_span)?;
            expected.clone()
        } else {
            first_type
        };

        for element in elements.iter().skip(1) {
            let (actual, span) = self.check_array_element(element, Some(&element_type))?;
            self.expect_type(&element_type, &actual, span)?;
        }

        Ok(Type::Array(Box::new(element_type)))
    }

    fn check_array_element(
        &mut self,
        element: &ArrayElement,
        expected_element: Option<&Type>,
    ) -> Result<(Type, Span), Diagnostic> {
        match element {
            ArrayElement::Expr(value) => Ok((
                self.check_expr_with_expected(value, expected_element)?,
                value.span,
            )),
            ArrayElement::Spread(value) => {
                let expected = expected_element.map(|ty| Type::Array(Box::new(ty.clone())));
                let actual = self.check_expr_with_expected(value, expected.as_ref())?;
                let Type::Array(element) = actual else {
                    return Err(Diagnostic::new(
                        format!("array spread expects array, got {actual}"),
                        value.span,
                    )
                    .with_code("type.spread-mismatch"));
                };
                Ok((*element, value.span))
            }
        }
    }

    fn check_tuple_literal(
        &mut self,
        span: Span,
        elements: &[Expr],
        expected: Option<&Type>,
    ) -> Result<Type, Diagnostic> {
        let expected_elements = match expected {
            Some(Type::Tuple(elements)) => Some(elements.as_slice()),
            Some(other) => {
                return Err(
                    Diagnostic::new(format!("expected {other}, got tuple"), span)
                        .with_code("tuple.element-type-mismatch"),
                );
            }
            None => None,
        };

        if elements.len() < 2 {
            return Err(
                Diagnostic::new("tuple literal requires at least two elements", span)
                    .with_code("tuple.arity-mismatch"),
            );
        }
        if let Some(expected) = expected_elements {
            if expected.len() != elements.len() {
                return Err(Diagnostic::new(
                    format!(
                        "tuple arity mismatch: expected {} elements, got {}",
                        expected.len(),
                        elements.len()
                    ),
                    span,
                )
                .with_code("tuple.arity-mismatch"));
            }
            for (expected, element) in expected.iter().zip(elements) {
                let actual = self.check_expr_with_expected(element, Some(expected))?;
                self.expect_type(expected, &actual, element.span)
                    .map_err(|mut diagnostic| {
                        diagnostic.code = "tuple.element-type-mismatch";
                        diagnostic
                    })?;
            }
            Ok(Type::Tuple(expected.to_vec()))
        } else {
            elements
                .iter()
                .map(|element| self.check_expr(element))
                .collect::<Result<Vec<_>, _>>()
                .map(Type::Tuple)
        }
    }

    fn check_map_literal(
        &mut self,
        span: Span,
        entries: &[MapEntry],
        expected: Option<&Type>,
    ) -> Result<Type, Diagnostic> {
        let expected_value = match expected {
            Some(Type::Map(value)) => Some(value.as_ref()),
            Some(other) => {
                return Err(Diagnostic::new(format!("expected {other}, got map"), span));
            }
            None => None,
        };

        let Some(first) = entries.first() else {
            if let Some(value) = expected_value {
                return Ok(Type::Map(Box::new(value.clone())));
            }
            return Err(Diagnostic::new(
                "empty map literal needs an expected type",
                span,
            ));
        };

        let (first_value_type, first_span) = self.check_map_entry(first, expected_value)?;
        let value_type = if let Some(expected) = expected_value {
            self.expect_type(expected, &first_value_type, first_span)?;
            expected.clone()
        } else {
            first_value_type
        };

        for entry in entries.iter().skip(1) {
            let (actual, span) = self.check_map_entry(entry, Some(&value_type))?;
            self.expect_type(&value_type, &actual, span)?;
        }

        Ok(Type::Map(Box::new(value_type)))
    }

    fn check_map_entry(
        &mut self,
        entry: &MapEntry,
        expected_value: Option<&Type>,
    ) -> Result<(Type, Span), Diagnostic> {
        match entry {
            MapEntry::Entry { key, value } => {
                let key_type = self.check_expr(key)?;
                self.expect_type(&Type::Str, &key_type, key.span)?;
                Ok((
                    self.check_expr_with_expected(value, expected_value)?,
                    value.span,
                ))
            }
            MapEntry::Spread(value) => {
                let expected = expected_value.map(|ty| Type::Map(Box::new(ty.clone())));
                let actual = self.check_expr_with_expected(value, expected.as_ref())?;
                let Type::Map(value_type) = actual else {
                    return Err(Diagnostic::new(
                        format!("map spread expects map, got {actual}"),
                        value.span,
                    )
                    .with_code("type.spread-mismatch"));
                };
                Ok((*value_type, value.span))
            }
        }
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
                let suggestion = self.suggest_record_field(name, field_name);
                Diagnostic::new(
                    append_did_you_mean(
                        format!("record '{name}' has no field '{field_name}'"),
                        suggestion,
                    ),
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

    fn check_question_expr(&mut self, span: Span, value: &Expr) -> Result<Type, Diagnostic> {
        let Some(return_type) = self.returns.last().cloned() else {
            return Err(
                Diagnostic::new("'?' can only be used inside a function", span)
                    .with_code("result.question-mark.mismatch"),
            );
        };
        let inner = self.check_expr(value)?;
        match inner {
            Type::Option(payload) => {
                let Type::Option(return_payload) = return_type else {
                    return Err(Diagnostic::new(
                        format!("'?' on option requires enclosing function to return option, got {return_type}"),
                        span,
                    )
                    .with_code("result.question-mark.mismatch"));
                };
                let _ = return_payload;
                Ok(*payload)
            }
            Type::Result { ok, err } => {
                let Type::Result {
                    ok: return_ok,
                    err: return_err,
                } = return_type
                else {
                    return Err(Diagnostic::new(
                        format!("'?' on result requires enclosing function to return result, got {return_type}"),
                        span,
                    )
                    .with_code("result.question-mark.mismatch"));
                };
                self.expect_type(&return_err, &err, value.span)
                    .map_err(|mut diagnostic| {
                        diagnostic.code = "result.question-mark.mismatch";
                        diagnostic.message = format!(
                            "'?' error type mismatch: enclosing function returns result[{return_ok}, {return_err}], expression is result[{ok}, {err}]"
                        );
                        diagnostic
                    })?;
                Ok(*ok)
            }
            other => Err(Diagnostic::new(
                format!("'?' expects option or result, got {other}"),
                span,
            )
            .with_code("result.question-mark.mismatch")),
        }
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
            Type::Int | Type::Float | Type::Str => {
                self.check_literal_match_statement(matched, cases, default, span)
            }
            Type::Option(_) => {
                if default.is_some() {
                    return Err(Diagnostic::new(
                        "option match does not accept '_' default case",
                        span,
                    ));
                }
                let mut all_cases_return = true;
                for case in cases {
                    if !matches!(case.pattern, MatchCaseValue::Some(_) | MatchCaseValue::None) {
                        return Err(Diagnostic::new(
                            "option match only accepts some(name) and none cases",
                            case.span,
                        ));
                    }
                    let bindings = self.check_match_pattern(matched, &case.pattern, case.span)?;
                    all_cases_return &= self.check_block_with_bindings(&case.body, &bindings)?;
                }
                if !patterns_cover_type(cases.iter().map(|case| &case.pattern), matched) {
                    return Err(
                        Diagnostic::new("option match must cover some and none", span)
                            .with_code("match.non-exhaustive"),
                    );
                }
                Ok(all_cases_return)
            }
            Type::Result { .. } => {
                if default.is_some() {
                    return Err(Diagnostic::new(
                        "result match does not accept '_' default case",
                        span,
                    ));
                }
                let mut all_cases_return = true;
                for case in cases {
                    if !matches!(case.pattern, MatchCaseValue::Ok(_) | MatchCaseValue::Err(_)) {
                        return Err(Diagnostic::new(
                            "result match only accepts ok(name) and err(name) cases",
                            case.span,
                        ));
                    }
                    let bindings = self.check_match_pattern(matched, &case.pattern, case.span)?;
                    all_cases_return &= self.check_block_with_bindings(&case.body, &bindings)?;
                }
                if !patterns_cover_type(cases.iter().map(|case| &case.pattern), matched) {
                    return Err(Diagnostic::new("result match must cover ok and err", span)
                        .with_code("match.non-exhaustive"));
                }
                Ok(all_cases_return)
            }
            Type::Enum(name) => {
                if default.is_some() {
                    return Err(Diagnostic::new(
                        "enum match does not accept '_' default case",
                        span,
                    ));
                }
                let schema = self.enum_schema(name, span)?.clone();
                let mut all_cases_return = true;
                for case in cases {
                    if !matches!(case.pattern, MatchCaseValue::EnumVariant { .. }) {
                        return Err(Diagnostic::new(
                            "enum match only accepts enum variant cases",
                            case.span,
                        ));
                    }
                    let bindings = self.check_match_pattern(matched, &case.pattern, case.span)?;
                    all_cases_return &= self.check_block_with_bindings(&case.body, &bindings)?;
                }
                if !enum_patterns_cover_schema(cases.iter().map(|case| &case.pattern), &schema) {
                    return Err(Diagnostic::new(
                        format!("enum match must cover all variants of {name}"),
                        span,
                    )
                    .with_code("match.non-exhaustive"));
                }
                Ok(all_cases_return)
            }
            _ => Err(Diagnostic::new(
                format!(
                    "match value must be int, float, str, option, result, or enum, got {matched}"
                ),
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
            return Err(Diagnostic::new("match requires '_' default case", span)
                .with_code("match.non-exhaustive"));
        };

        let mut seen = HashSet::new();
        let mut all_cases_return = true;
        for case in cases {
            let case_type = match_case_type(&case.pattern).ok_or_else(|| {
                Diagnostic::new(
                    "literal match only accepts number, range, and str cases",
                    case.span,
                )
            })?;
            self.expect_type(matched, &case_type, case.span)?;
            if !seen.insert(match_pattern_key(&case.pattern)) {
                return Err(Diagnostic::new("duplicate match case", case.span));
            }
            all_cases_return &= self.check_block(&case.body)?;
        }
        let default_returns = self.check_block(default)?;
        Ok(all_cases_return && default_returns)
    }

    fn check_match_pattern(
        &self,
        matched: &Type,
        pattern: &MatchCaseValue,
        span: Span,
    ) -> Result<Vec<(String, Type)>, Diagnostic> {
        let mut bindings = Vec::new();
        self.collect_match_pattern_bindings(matched, pattern, span, &mut bindings)?;
        Ok(bindings)
    }

    fn collect_match_pattern_bindings(
        &self,
        matched: &Type,
        pattern: &MatchCaseValue,
        span: Span,
        bindings: &mut Vec<(String, Type)>,
    ) -> Result<(), Diagnostic> {
        match pattern {
            MatchCaseValue::Bind(name) => {
                bindings.push((name.clone(), matched.clone()));
                Ok(())
            }
            MatchCaseValue::Int(_) => expect_pattern_type(matched, &Type::Int, span),
            MatchCaseValue::Float(_) => expect_pattern_type(matched, &Type::Float, span),
            MatchCaseValue::Str(_) => expect_pattern_type(matched, &Type::Str, span),
            MatchCaseValue::IntRange { start, end } => {
                if start >= end {
                    return Err(Diagnostic::new(
                        "match range start must be less than end",
                        span,
                    ));
                }
                expect_pattern_type(matched, &Type::Int, span)
            }
            MatchCaseValue::None => {
                if matches!(matched, Type::Option(_)) {
                    Ok(())
                } else {
                    Err(Diagnostic::new(
                        format!("none pattern requires option, got {matched}"),
                        span,
                    ))
                }
            }
            MatchCaseValue::Some(inner) => {
                let Type::Option(payload) = matched else {
                    return Err(Diagnostic::new(
                        format!("some(pattern) requires option, got {matched}"),
                        span,
                    ));
                };
                self.collect_match_pattern_bindings(payload, inner, span, bindings)
            }
            MatchCaseValue::Ok(inner) => {
                let Type::Result { ok, .. } = matched else {
                    return Err(Diagnostic::new(
                        format!("ok(pattern) requires result, got {matched}"),
                        span,
                    ));
                };
                self.collect_match_pattern_bindings(ok, inner, span, bindings)
            }
            MatchCaseValue::Err(inner) => {
                let Type::Result { err, .. } = matched else {
                    return Err(Diagnostic::new(
                        format!("err(pattern) requires result, got {matched}"),
                        span,
                    ));
                };
                self.collect_match_pattern_bindings(err, inner, span, bindings)
            }
            MatchCaseValue::EnumVariant { name, payload } => {
                let Type::Enum(enum_name) = matched else {
                    return Err(Diagnostic::new(
                        format!("enum variant pattern requires enum, got {matched}"),
                        span,
                    ));
                };
                let schema = self.enum_schema(enum_name, span)?;
                let expected_payload = schema.variant_payloads.get(name).ok_or_else(|| {
                    let suggestion = self.suggest_enum_variant(enum_name, name);
                    Diagnostic::new(
                        append_did_you_mean(
                            format!("enum '{enum_name}' has no variant '{name}'"),
                            suggestion,
                        ),
                        span,
                    )
                    .with_code("enum.variant-not-found")
                })?;
                match (expected_payload, payload) {
                    (Some(expected), Some(payload)) => {
                        self.collect_match_pattern_bindings(expected, payload, span, bindings)
                    }
                    (None, None) => Ok(()),
                    (Some(_), None) => Err(Diagnostic::new(
                        format!("enum variant '{name}' requires a payload pattern"),
                        span,
                    )),
                    (None, Some(_)) => Err(Diagnostic::new(
                        format!("enum variant '{name}' does not carry a payload"),
                        span,
                    )),
                }
            }
        }
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

    fn check_map_keys_call(&mut self, span: Span, args: &[Expr]) -> Result<Type, Diagnostic> {
        let _ = self.check_map_unary_call(span, args)?;
        Ok(Type::Array(Box::new(Type::Str)))
    }

    fn check_map_values_call(&mut self, span: Span, args: &[Expr]) -> Result<Type, Diagnostic> {
        let value = self.check_map_unary_call(span, args)?;
        Ok(Type::Array(value))
    }

    fn check_map_size_call(&mut self, span: Span, args: &[Expr]) -> Result<Type, Diagnostic> {
        let _ = self.check_map_unary_call(span, args)?;
        Ok(Type::Int)
    }

    fn check_map_unary_call(&mut self, span: Span, args: &[Expr]) -> Result<Box<Type>, Diagnostic> {
        if args.len() != 1 {
            return Err(Diagnostic::new(
                format!("expected 1 arguments but got {}", args.len()),
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
        Ok(value)
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

    fn check_record_method_call(
        &mut self,
        span: Span,
        receiver: &Expr,
        name: &str,
        name_span: Span,
        args: &[Expr],
        paren_span: Span,
    ) -> Result<Type, Diagnostic> {
        let receiver_type = self.check_expr(receiver)?;
        let Type::Record(record_name) = &receiver_type else {
            return Err(
                Diagnostic::new("method call requires a record value", receiver.span)
                    .with_code("record.method-not-found"),
            );
        };
        let Some(method_type) = self.lookup(name) else {
            return Err(Diagnostic::new(
                format!("record '{record_name}' has no method '{name}'"),
                name_span,
            )
            .with_code("record.method-not-found"));
        };
        let Type::Function {
            type_params,
            params,
            return_type,
        } = method_type
        else {
            return Err(Diagnostic::new(
                format!("record '{record_name}' has no method '{name}'"),
                name_span,
            )
            .with_code("record.method-not-found"));
        };
        let Some(first_param) = params.first() else {
            return Err(Diagnostic::new(
                format!("method '{name}' must accept {record_name} as its first parameter"),
                name_span,
            )
            .with_code("record.method-not-found"));
        };
        if first_param != &receiver_type {
            return Err(Diagnostic::new(
                format!("method '{name}' first parameter must be {record_name}, got {first_param}"),
                receiver.span,
            )
            .with_code("record.method-not-found"));
        }
        let expected_arg_count = params.len().saturating_sub(1);
        if args.len() != expected_arg_count {
            return Err(Diagnostic::new(
                format!(
                    "expected {expected_arg_count} arguments but got {}",
                    args.len()
                ),
                paren_span,
            ));
        }
        let mut bindings = HashMap::new();
        for (expected, arg) in params.iter().skip(1).zip(args) {
            let actual =
                self.check_expr_with_expected(arg, self.concrete_expected_type(expected).as_ref())?;
            self.unify_call_type(expected, &actual, &type_params, arg.span, &mut bindings)?;
        }
        let return_type =
            self.instantiate_return_type(&return_type, None, &type_params, &bindings, span)?;
        self.record_hover_type(span, return_type.clone());
        Ok(return_type)
    }

    fn check_enum_constructor(
        &mut self,
        enum_name: &str,
        variant_name: &str,
        span: Span,
        args: &[Expr],
        arg_span: Span,
    ) -> Result<Type, Diagnostic> {
        let schema = self.enum_schema(enum_name, span)?;
        let payload = schema
            .variant_payloads
            .get(variant_name)
            .ok_or_else(|| {
                let suggestion = self.suggest_enum_variant(enum_name, variant_name);
                Diagnostic::new(
                    append_did_you_mean(
                        format!("enum '{enum_name}' has no variant '{variant_name}'"),
                        suggestion,
                    ),
                    span,
                )
                .with_code("enum.variant-not-found")
            })?
            .clone();
        match payload {
            Some(expected) => {
                if args.len() != 1 {
                    return Err(Diagnostic::new(
                        format!("expected 1 arguments but got {}", args.len()),
                        arg_span,
                    ));
                }
                let actual = self.check_expr_with_expected(&args[0], Some(&expected))?;
                self.expect_type(&expected, &actual, args[0].span)?;
            }
            None => {
                if !args.is_empty() {
                    return Err(Diagnostic::new(
                        format!("expected 0 arguments but got {}", args.len()),
                        arg_span,
                    ));
                }
            }
        }
        Ok(Type::Enum(enum_name.to_string()))
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
            BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::ShiftLeft
            | BinaryOp::ShiftRight => {
                self.expect_bitwise_int(left, span)?;
                self.expect_bitwise_int(right, span)?;
                Ok(Type::Int)
            }
            BinaryOp::Greater | BinaryOp::GreaterEqual | BinaryOp::Less | BinaryOp::LessEqual => {
                self.expect_same_numeric(left, right, span)
                    .map(|_| Type::Bool)
            }
            BinaryOp::Equal | BinaryOp::NotEqual
                if matches!(
                    left,
                    Type::Json | Type::Array(_) | Type::Map(_) | Type::Record(_) | Type::Enum(_)
                ) =>
            {
                Err(Diagnostic::new("container equality is not supported", span))
            }
            BinaryOp::Equal | BinaryOp::NotEqual => {
                self.expect_type(left, right, span)?;
                Ok(Type::Bool)
            }
        }
    }

    fn check_json_from_json_call(
        &mut self,
        span: Span,
        args: &[Expr],
        expected: Option<&Type>,
    ) -> Result<Type, Diagnostic> {
        if args.len() != 1 {
            return Err(Diagnostic::new(
                format!("expected 1 arguments but got {}", args.len()),
                span,
            ));
        }
        let actual = self.check_expr_with_expected(&args[0], Some(&Type::Json))?;
        self.expect_type(&Type::Json, &actual, args[0].span)?;
        let Some(Type::Result { ok, err }) = expected else {
            return Err(Diagnostic::new(
                "json.from_json requires expected result[T, str] type",
                span,
            )
            .with_code("generic.infer-failed"));
        };
        self.expect_type(&Type::Str, err, span)?;
        Ok(Type::Result {
            ok: ok.clone(),
            err: Box::new(Type::Str),
        })
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

    fn concrete_expected_type(&self, expected: &Type) -> Option<Type> {
        match expected {
            Type::Generic(_) => None,
            Type::Array(element) => self
                .concrete_expected_type(element)
                .map(|element| Type::Array(Box::new(element))),
            Type::Tuple(elements) => elements
                .iter()
                .map(|element| self.concrete_expected_type(element))
                .collect::<Option<Vec<_>>>()
                .map(Type::Tuple),
            Type::Map(value) => self
                .concrete_expected_type(value)
                .map(|value| Type::Map(Box::new(value))),
            Type::Option(value) => self
                .concrete_expected_type(value)
                .map(|value| Type::Option(Box::new(value))),
            Type::Result { ok, err } => Some(Type::Result {
                ok: Box::new(self.concrete_expected_type(ok)?),
                err: Box::new(self.concrete_expected_type(err)?),
            }),
            Type::Function { .. } => None,
            other => Some(other.clone()),
        }
    }

    fn verify_constraints(
        &self,
        function_name: &str,
        bindings: &HashMap<String, Type>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some(constraints) = self.function_constraints.get(function_name) else {
            return Ok(());
        };
        for (param_name, markers) in constraints {
            if markers.is_empty() {
                continue;
            }
            let Some(bound) = bindings.get(param_name) else {
                continue;
            };
            for marker in markers {
                if !self.type_satisfies_marker(bound, *marker) {
                    return Err(Diagnostic::new(
                        format!(
                            "type '{bound}' does not implement constraint '{}' required by generic parameter '{param_name}' of function '{function_name}'",
                            marker.as_str()
                        ),
                        span,
                    )
                    .with_code("generic.constraint-unsatisfied"));
                }
            }
        }
        Ok(())
    }

    fn type_satisfies_marker(&self, ty: &Type, marker: ConstraintMarker) -> bool {
        if let Type::Generic(name) = ty {
            if let Some(active) = self.active_constraints.last() {
                if let Some(markers) = active.get(name) {
                    if markers.contains(&marker) {
                        return true;
                    }
                }
            }
            return false;
        }
        if type_implements_marker(ty, marker) {
            return true;
        }
        match ty {
            Type::Array(element) | Type::Option(element) => {
                self.type_satisfies_marker(element, marker)
            }
            Type::Map(value) => self.type_satisfies_marker(value, marker),
            Type::Tuple(elements) => elements
                .iter()
                .all(|element| self.type_satisfies_marker(element, marker)),
            Type::Result { ok, err } => {
                self.type_satisfies_marker(ok, marker) && self.type_satisfies_marker(err, marker)
            }
            _ => false,
        }
    }

    fn unify_call_type(
        &self,
        expected: &Type,
        actual: &Type,
        type_params: &[String],
        span: Span,
        bindings: &mut HashMap<String, Type>,
    ) -> Result<(), Diagnostic> {
        match expected {
            Type::Generic(name) if type_params.iter().any(|param| param == name) => {
                if let Some(bound) = bindings.get(name) {
                    if bound == actual {
                        Ok(())
                    } else {
                        Err(Diagnostic::new(
                            format!(
                                "could not infer generic type '{name}': expected {bound}, got {actual}"
                            ),
                            span,
                        )
                        .with_code("generic.infer-failed"))
                    }
                } else {
                    bindings.insert(name.clone(), actual.clone());
                    Ok(())
                }
            }
            Type::Array(expected) => match actual {
                Type::Array(actual) => {
                    self.unify_call_type(expected, actual, type_params, span, bindings)
                }
                _ => self.expect_type(&Type::Array(expected.clone()), actual, span),
            },
            Type::Tuple(expected) => match actual {
                Type::Tuple(actual) if expected.len() == actual.len() => {
                    for (expected, actual) in expected.iter().zip(actual) {
                        self.unify_call_type(expected, actual, type_params, span, bindings)?;
                    }
                    Ok(())
                }
                _ => self.expect_type(&Type::Tuple(expected.clone()), actual, span),
            },
            Type::Map(expected) => match actual {
                Type::Map(actual) => {
                    self.unify_call_type(expected, actual, type_params, span, bindings)
                }
                _ => self.expect_type(expected, actual, span),
            },
            Type::Option(expected) => match actual {
                Type::Option(actual) => {
                    self.unify_call_type(expected, actual, type_params, span, bindings)
                }
                _ => self.expect_type(expected, actual, span),
            },
            Type::Result { ok, err } => match actual {
                Type::Result {
                    ok: actual_ok,
                    err: actual_err,
                } => {
                    self.unify_call_type(ok, actual_ok, type_params, span, bindings)?;
                    self.unify_call_type(err, actual_err, type_params, span, bindings)
                }
                _ => self.expect_type(expected, actual, span),
            },
            Type::Function {
                params: expected_params,
                return_type: expected_return,
                ..
            } => match actual {
                Type::Function {
                    params: actual_params,
                    return_type: actual_return,
                    ..
                } if expected_params.len() == actual_params.len() => {
                    for (expected_param, actual_param) in
                        expected_params.iter().zip(actual_params.iter())
                    {
                        self.unify_call_type(
                            expected_param,
                            actual_param,
                            type_params,
                            span,
                            bindings,
                        )?;
                    }
                    self.unify_call_type(
                        expected_return,
                        actual_return,
                        type_params,
                        span,
                        bindings,
                    )
                }
                _ => self.expect_type(expected, actual, span),
            },
            _ => self.expect_type(expected, actual, span),
        }
    }

    fn instantiate_return_type(
        &self,
        return_type: &Type,
        expected: Option<&Type>,
        type_params: &[String],
        bindings: &HashMap<String, Type>,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        if type_params.is_empty() {
            return Ok(return_type.clone());
        }
        let mut bindings = bindings.clone();
        if let Some(expected) = expected {
            self.unify_call_type(return_type, expected, type_params, span, &mut bindings)?;
        }
        for type_param in type_params {
            if !bindings.contains_key(type_param) {
                return Err(Diagnostic::new(
                    format!("could not infer generic type '{type_param}'"),
                    span,
                )
                .with_code("generic.infer-failed"));
            }
        }
        self.substitute_generic_type(return_type, &bindings, span)
    }

    fn substitute_generic_type(
        &self,
        ty: &Type,
        bindings: &HashMap<String, Type>,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        match ty {
            Type::Generic(name) => bindings.get(name).cloned().ok_or_else(|| {
                Diagnostic::new(format!("could not infer generic type '{name}'"), span)
                    .with_code("generic.infer-failed")
            }),
            Type::Array(element) => Ok(Type::Array(Box::new(
                self.substitute_generic_type(element, bindings, span)?,
            ))),
            Type::Tuple(elements) => elements
                .iter()
                .map(|element| self.substitute_generic_type(element, bindings, span))
                .collect::<Result<Vec<_>, _>>()
                .map(Type::Tuple),
            Type::Map(value) => Ok(Type::Map(Box::new(
                self.substitute_generic_type(value, bindings, span)?,
            ))),
            Type::Option(value) => Ok(Type::Option(Box::new(
                self.substitute_generic_type(value, bindings, span)?,
            ))),
            Type::Result { ok, err } => Ok(Type::Result {
                ok: Box::new(self.substitute_generic_type(ok, bindings, span)?),
                err: Box::new(self.substitute_generic_type(err, bindings, span)?),
            }),
            Type::Function { .. } => Ok(ty.clone()),
            other => Ok(other.clone()),
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

    fn expect_bitwise_int(&self, actual: &Type, span: Span) -> Result<(), Diagnostic> {
        if actual == &Type::Int {
            Ok(())
        } else {
            Err(
                Diagnostic::new(format!("bitwise operator expects int, got {actual}"), span)
                    .with_code("type.bitwise-non-int"),
            )
        }
    }

    fn resolve_type(&self, ty: &Type, span: Span) -> Result<Type, Diagnostic> {
        let resolved = self.resolve_type_aliases(ty, &mut Vec::new())?;
        let resolved = self.resolve_named_type(&resolved);
        self.validate_type(&resolved, span)?;
        Ok(resolved)
    }

    fn resolve_type_in_function(
        &self,
        ty: &Type,
        type_params: &[String],
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let generic_names = type_params.iter().cloned().collect::<HashSet<_>>();
        let resolved = self.resolve_type_aliases(ty, &mut Vec::new())?;
        let resolved = self.mark_generic_types(&resolved, &generic_names);
        let resolved = self.resolve_named_type(&resolved);
        self.validate_type_with_generics(&resolved, span, &generic_names)?;
        Ok(resolved)
    }

    fn mark_generic_types(&self, ty: &Type, type_params: &HashSet<String>) -> Type {
        match ty {
            Type::Array(element) => {
                Type::Array(Box::new(self.mark_generic_types(element, type_params)))
            }
            Type::Tuple(elements) => Type::Tuple(
                elements
                    .iter()
                    .map(|element| self.mark_generic_types(element, type_params))
                    .collect(),
            ),
            Type::Map(value) => Type::Map(Box::new(self.mark_generic_types(value, type_params))),
            Type::Option(value) => {
                Type::Option(Box::new(self.mark_generic_types(value, type_params)))
            }
            Type::Result { ok, err } => Type::Result {
                ok: Box::new(self.mark_generic_types(ok, type_params)),
                err: Box::new(self.mark_generic_types(err, type_params)),
            },
            Type::Record(name) if type_params.contains(name) => Type::Generic(name.clone()),
            Type::Function {
                type_params: nested_type_params,
                params,
                return_type,
            } => Type::Function {
                type_params: nested_type_params.clone(),
                params: params
                    .iter()
                    .map(|param| self.mark_generic_types(param, type_params))
                    .collect(),
                return_type: Box::new(self.mark_generic_types(return_type, type_params)),
            },
            other => other.clone(),
        }
    }

    fn resolve_named_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Array(element) => Type::Array(Box::new(self.resolve_named_type(element))),
            Type::Tuple(elements) => Type::Tuple(
                elements
                    .iter()
                    .map(|element| self.resolve_named_type(element))
                    .collect(),
            ),
            Type::Map(value) => Type::Map(Box::new(self.resolve_named_type(value))),
            Type::Option(value) => Type::Option(Box::new(self.resolve_named_type(value))),
            Type::Result { ok, err } => Type::Result {
                ok: Box::new(self.resolve_named_type(ok)),
                err: Box::new(self.resolve_named_type(err)),
            },
            Type::Record(name) if self.enums.contains_key(name) => Type::Enum(name.clone()),
            Type::Function {
                type_params,
                params,
                return_type,
            } => Type::Function {
                type_params: type_params.clone(),
                params: params
                    .iter()
                    .map(|param| self.resolve_named_type(param))
                    .collect(),
                return_type: Box::new(self.resolve_named_type(return_type)),
            },
            other => other.clone(),
        }
    }

    fn validate_type(&self, ty: &Type, span: Span) -> Result<(), Diagnostic> {
        if contains_generic_type(ty) {
            if let Some(type_params) = self.function_type_params.last() {
                return self.validate_type_with_generics(ty, span, type_params);
            }
        }
        match ty {
            Type::Null | Type::Bool | Type::Int | Type::Float | Type::Str | Type::Json => Ok(()),
            Type::Array(element) | Type::Map(element) => self.validate_type(element, span),
            Type::Tuple(elements) => {
                if elements.len() < 2 {
                    return Err(
                        Diagnostic::new("tuple type requires at least two elements", span)
                            .with_code("tuple.arity-mismatch"),
                    );
                }
                for element in elements {
                    self.validate_type(element, span)?;
                }
                Ok(())
            }
            Type::Option(value) => self.validate_type(value, span),
            Type::Result { ok, err } => {
                self.validate_type(ok, span)?;
                self.validate_type(err, span)
            }
            Type::Record(name) => self.record_schema(name, span).map(|_| ()),
            Type::Enum(name) => self.enum_schema(name, span).map(|_| ()),
            Type::Function {
                type_params,
                params,
                return_type,
            } => {
                let type_params = type_params.iter().cloned().collect::<HashSet<_>>();
                for param in params {
                    self.validate_type_with_generics(param, span, &type_params)?;
                }
                self.validate_type_with_generics(return_type, span, &type_params)
            }
            Type::Generic(name) => {
                let suggestion = self.suggest_type(name);
                Err(Diagnostic::new(
                    append_did_you_mean(format!("unknown type '{name}'"), suggestion),
                    span,
                ))
            }
        }
    }

    fn validate_type_with_generics(
        &self,
        ty: &Type,
        span: Span,
        type_params: &HashSet<String>,
    ) -> Result<(), Diagnostic> {
        match ty {
            Type::Generic(name) if type_params.contains(name) => Ok(()),
            Type::Generic(name) => {
                let suggestion = self.suggest_type(name);
                Err(Diagnostic::new(
                    append_did_you_mean(format!("unknown type '{name}'"), suggestion),
                    span,
                ))
            }
            Type::Array(element) | Type::Map(element) => {
                self.validate_type_with_generics(element, span, type_params)
            }
            Type::Tuple(elements) => {
                if elements.len() < 2 {
                    return Err(
                        Diagnostic::new("tuple type requires at least two elements", span)
                            .with_code("tuple.arity-mismatch"),
                    );
                }
                for element in elements {
                    self.validate_type_with_generics(element, span, type_params)?;
                }
                Ok(())
            }
            Type::Option(value) => self.validate_type_with_generics(value, span, type_params),
            Type::Result { ok, err } => {
                self.validate_type_with_generics(ok, span, type_params)?;
                self.validate_type_with_generics(err, span, type_params)
            }
            Type::Function {
                type_params: nested_type_params,
                params,
                return_type,
            } => {
                let mut nested = type_params.clone();
                nested.extend(nested_type_params.iter().cloned());
                for param in params {
                    self.validate_type_with_generics(param, span, &nested)?;
                }
                self.validate_type_with_generics(return_type, span, &nested)
            }
            other => self.validate_type(other, span),
        }
    }

    fn record_schema(&self, name: &str, span: Span) -> Result<&RecordSchema, Diagnostic> {
        self.records.get(name).ok_or_else(|| {
            let suggestion = self.suggest_type(name);
            Diagnostic::new(
                append_did_you_mean(format!("unknown type '{name}'"), suggestion),
                span,
            )
        })
    }

    fn enum_schema(&self, name: &str, span: Span) -> Result<&EnumSchema, Diagnostic> {
        self.enums.get(name).ok_or_else(|| {
            let suggestion = self.suggest_type(name);
            Diagnostic::new(
                append_did_you_mean(format!("unknown type '{name}'"), suggestion),
                span,
            )
        })
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

    fn define_binding_target(
        &mut self,
        target: &crate::BindingTarget,
        actual: &Type,
        is_const: bool,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match target {
            crate::BindingTarget::Name { name, .. } => {
                if is_const {
                    self.define_const(name.clone(), actual.clone());
                } else {
                    self.define(name.clone(), actual.clone());
                }
                Ok(())
            }
            crate::BindingTarget::Tuple { names, .. } => {
                let Type::Tuple(elements) = actual else {
                    return Err(Diagnostic::new(
                        format!("tuple destructuring requires tuple value, got {actual}"),
                        span,
                    )
                    .with_code("tuple.arity-mismatch"));
                };
                if names.len() != elements.len() {
                    return Err(Diagnostic::new(
                        format!(
                            "tuple arity mismatch: expected {} names, got {} values",
                            names.len(),
                            elements.len()
                        ),
                        span,
                    )
                    .with_code("tuple.arity-mismatch"));
                }
                for (name, ty) in names.iter().zip(elements) {
                    if is_const {
                        self.define_const(name.clone(), ty.clone());
                    } else {
                        self.define(name.clone(), ty.clone());
                    }
                }
                Ok(())
            }
            crate::BindingTarget::Record { names, .. } => {
                let Type::Record(record_name) = actual else {
                    return Err(Diagnostic::new(
                        format!("record destructuring requires record value, got {actual}"),
                        span,
                    ));
                };
                let schema = self.record_schema(record_name, span)?.clone();
                for name in names {
                    let ty = schema.field_types.get(name).cloned().ok_or_else(|| {
                        let suggestion = self.suggest_record_field(record_name, name);
                        Diagnostic::new(
                            append_did_you_mean(
                                format!("record '{record_name}' has no field '{name}'"),
                                suggestion,
                            ),
                            span,
                        )
                    })?;
                    if is_const {
                        self.define_const(name.clone(), ty);
                    } else {
                        self.define(name.clone(), ty);
                    }
                }
                Ok(())
            }
        }
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

    fn suggest_variable(&self, name: &str) -> Option<String> {
        suggest_similar(
            name,
            self.scopes
                .iter()
                .flat_map(|s| s.keys().map(|k| k.as_str())),
        )
    }

    fn suggest_record_field(&self, record_name: &str, field: &str) -> Option<String> {
        let schema = self.records.get(record_name)?;
        suggest_similar(field, schema.field_types.keys().map(|k| k.as_str()))
    }

    fn suggest_enum_variant(&self, enum_name: &str, variant: &str) -> Option<String> {
        let schema = self.enums.get(enum_name)?;
        suggest_similar(variant, schema.variant_payloads.keys().map(|k| k.as_str()))
    }

    fn suggest_type(&self, name: &str) -> Option<String> {
        let candidates = self
            .records
            .keys()
            .map(|k| k.as_str())
            .chain(self.enums.keys().map(|k| k.as_str()))
            .chain(self.type_aliases.keys().map(|k| k.as_str()));
        suggest_similar(name, candidates)
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
        Stmt::Let { target, span, .. } => target.single_name().map(|name| (name, *span)),
        Stmt::Function { name, span, .. }
        | Stmt::Record { name, span, .. }
        | Stmt::Enum { name, span, .. }
        | Stmt::TypeAlias { name, span, .. } => Some((name, *span)),
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
        MatchCaseValue::Float(_) => Some(Type::Float),
        MatchCaseValue::Str(_) => Some(Type::Str),
        MatchCaseValue::IntRange { .. } => Some(Type::Int),
        MatchCaseValue::Bind(_)
        | MatchCaseValue::Some(_)
        | MatchCaseValue::None
        | MatchCaseValue::Ok(_)
        | MatchCaseValue::Err(_)
        | MatchCaseValue::EnumVariant { .. } => None,
    }
}

fn expect_pattern_type(actual: &Type, expected: &Type, span: Span) -> Result<(), Diagnostic> {
    if actual == expected {
        Ok(())
    } else {
        Err(Diagnostic::new(
            format!("match pattern expects {expected}, got {actual}"),
            span,
        )
        .with_code("type.mismatch"))
    }
}

fn patterns_cover_type<'a>(
    mut patterns: impl Iterator<Item = &'a MatchCaseValue>,
    ty: &Type,
) -> bool {
    match ty {
        Type::Option(payload) => {
            let mut saw_none = false;
            let mut some_covers_payload = false;
            let mut some_patterns = Vec::new();
            for pattern in patterns {
                match pattern {
                    MatchCaseValue::None => saw_none = true,
                    MatchCaseValue::Some(inner) => {
                        if matches!(inner.as_ref(), MatchCaseValue::Bind(_)) {
                            some_covers_payload = true;
                        } else {
                            some_patterns.push(inner.as_ref());
                        }
                    }
                    _ => {}
                }
            }
            saw_none
                && (some_covers_payload || patterns_cover_type(some_patterns.into_iter(), payload))
        }
        Type::Result { ok, err } => {
            let mut ok_patterns = Vec::new();
            let mut err_patterns = Vec::new();
            for pattern in patterns {
                match pattern {
                    MatchCaseValue::Ok(inner) => ok_patterns.push(inner.as_ref()),
                    MatchCaseValue::Err(inner) => err_patterns.push(inner.as_ref()),
                    _ => {}
                }
            }
            patterns_cover_type(ok_patterns.into_iter(), ok)
                && patterns_cover_type(err_patterns.into_iter(), err)
        }
        Type::Enum(_) => false,
        _ => patterns.any(|pattern| matches!(pattern, MatchCaseValue::Bind(_))),
    }
}

fn contains_generic_type(ty: &Type) -> bool {
    match ty {
        Type::Generic(_) => true,
        Type::Array(element) | Type::Map(element) | Type::Option(element) => {
            contains_generic_type(element)
        }
        Type::Tuple(elements) => elements.iter().any(contains_generic_type),
        Type::Result { ok, err } => contains_generic_type(ok) || contains_generic_type(err),
        Type::Function {
            params,
            return_type,
            ..
        } => params.iter().any(contains_generic_type) || contains_generic_type(return_type),
        _ => false,
    }
}

fn is_std_json_from_json_name(name: &str) -> bool {
    name == "$import$std$json$nox$from_json" || name == "__nox_std_json_from_json"
}

fn enum_patterns_cover_schema<'a>(
    patterns: impl Iterator<Item = &'a MatchCaseValue>,
    schema: &EnumSchema,
) -> bool {
    let mut covered = HashSet::new();
    for pattern in patterns {
        if let MatchCaseValue::EnumVariant { name, .. } = pattern {
            covered.insert(name);
        }
    }
    schema
        .variants
        .iter()
        .all(|variant| covered.contains(&variant.name))
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let (n, m) = (a_chars.len(), b_chars.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr: Vec<usize> = vec![0; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

fn suggest_similar<I, S>(target: &str, candidates: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let target_lower = target.to_lowercase();
    let max_dist = if target.chars().count() <= 3 { 1 } else { 2 };
    let mut best: Option<(usize, String)> = None;
    for cand in candidates {
        let s = cand.as_ref();
        if s == target {
            continue;
        }
        let dist = levenshtein(&s.to_lowercase(), &target_lower);
        if dist <= max_dist && best.as_ref().is_none_or(|(d, _)| dist < *d) {
            best = Some((dist, s.to_string()));
        }
    }
    best.map(|(_, s)| s)
}

fn append_did_you_mean(message: String, suggestion: Option<String>) -> String {
    match suggestion {
        Some(s) => format!("{message}, did you mean '{s}'?"),
        None => message,
    }
}

fn match_pattern_key(pattern: &MatchCaseValue) -> String {
    match pattern {
        MatchCaseValue::Int(value) => format!("int:{value}"),
        MatchCaseValue::Float(value) => format!("float:{value:?}"),
        MatchCaseValue::Str(value) => format!("str:{value:?}"),
        MatchCaseValue::IntRange { start, end } => format!("range:{start}..{end}"),
        MatchCaseValue::Bind(_) => "bind".to_string(),
        MatchCaseValue::Some(inner) => format!("some({})", match_pattern_key(inner)),
        MatchCaseValue::None => "none".to_string(),
        MatchCaseValue::Ok(inner) => format!("ok({})", match_pattern_key(inner)),
        MatchCaseValue::Err(inner) => format!("err({})", match_pattern_key(inner)),
        MatchCaseValue::EnumVariant { name, payload } => match payload {
            Some(payload) => format!("{name}({})", match_pattern_key(payload)),
            None => name.clone(),
        },
    }
}
