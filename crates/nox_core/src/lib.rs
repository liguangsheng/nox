use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    rc::{Rc, Weak},
    time::Duration,
};

mod bytecode;
mod ffi;
mod heap;
mod lexer;
mod parser;
mod typecheck;
mod vm;

#[cfg(fuzzing)]
pub mod fuzzing {
    use super::{compile, lex, parse_all, verify, Diagnostic, TypeChecker};

    pub fn parse_source(source: &str) -> Result<(), Vec<Diagnostic>> {
        let tokens = lex(source).map_err(|diagnostic| vec![diagnostic])?;
        parse_all(tokens).map(|_| ())
    }

    pub fn typecheck_source(source: &str) -> Result<(), Vec<Diagnostic>> {
        let tokens = lex(source).map_err(|diagnostic| vec![diagnostic])?;
        let module = parse_all(tokens)?;
        TypeChecker::new().check_module_all(&module)
    }

    pub fn verify_source(source: &str) -> Result<(), Vec<Diagnostic>> {
        let tokens = lex(source).map_err(|diagnostic| vec![diagnostic])?;
        let module = parse_all(tokens)?;
        TypeChecker::new().check_module_all(&module)?;
        let bytecode = compile(&module);
        verify(&bytecode).map_err(|diagnostic| vec![diagnostic])
    }
}

#[cfg(test)]
mod api_tests;
#[cfg(test)]
mod compiler_tests;
#[cfg(test)]
mod language_tests;

use bytecode::{verify, BytecodeModule, Compiler};
pub use ffi::{
    nox_core_array_free, nox_core_array_get, nox_core_array_len, nox_core_engine_check,
    nox_core_engine_clear_error, nox_core_engine_eval, nox_core_engine_free,
    nox_core_engine_last_error, nox_core_engine_new, nox_core_engine_register_host_function,
    nox_core_engine_register_host_function_ex, nox_core_engine_set_userdata,
    nox_core_engine_userdata, nox_core_map_free, nox_core_map_get, nox_core_map_keys,
    nox_core_map_len, nox_core_option_free, nox_core_option_is_some, nox_core_option_payload,
    nox_core_record_field, nox_core_record_free, nox_core_result_free, nox_core_result_is_ok,
    nox_core_result_payload, nox_core_string_free, nox_core_version, NoxCoreArrayHandle,
    NoxCoreEngine, NoxCoreHostCallback, NoxCoreMapHandle, NoxCoreOptionHandle, NoxCoreRecordHandle,
    NoxCoreResultHandle, NoxCoreStatus, NoxCoreValue, NoxCoreValueKind,
};
use heap::GcHeap;
use lexer::lex;
use parser::{parse, parse_all};
use typecheck::TypeChecker;
use vm::{value_type, Control, Env, EnvData, Vm};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    fn join(self, other: Self) -> Self {
        Self {
            start: self.start,
            end: other.end,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub source: Option<SourceLocation>,
    pub stack_frames: Vec<StackFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub name: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackFrameKind {
    Script,
    Host,
}

impl StackFrameKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StackFrameKind::Script => "script",
            StackFrameKind::Host => "host",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackFrame {
    pub name: String,
    pub span: Span,
    pub source: Option<SourceLocation>,
    pub kind: StackFrameKind,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            code: "error",
            message: message.into(),
            span,
            source: None,
            stack_frames: Vec::new(),
        }
    }

    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = code;
        self
    }

    pub fn with_source(mut self, name: impl Into<String>, source: &str) -> Self {
        let name = name.into();
        self.source = Some(SourceLocation {
            name: name.clone(),
            line: line_column(source, self.span.start).0,
            column: line_column(source, self.span.start).1,
        });
        for frame in &mut self.stack_frames {
            if frame.source.is_none() {
                frame.source = Some(SourceLocation {
                    name: name.clone(),
                    line: line_column(source, frame.span.start).0,
                    column: line_column(source, frame.span.start).1,
                });
            }
        }
        self
    }

    pub fn with_stack_frame(self, name: impl Into<String>, span: Span) -> Self {
        self.with_stack_frame_kind(name, span, StackFrameKind::Script)
    }

    pub fn with_host_stack_frame(self, name: impl Into<String>, span: Span) -> Self {
        self.with_stack_frame_kind(name, span, StackFrameKind::Host)
    }

    pub fn with_stack_frame_kind(
        mut self,
        name: impl Into<String>,
        span: Span,
        kind: StackFrameKind,
    ) -> Self {
        const MAX_STACK_FRAMES: usize = 32;
        if self.stack_frames.len() < MAX_STACK_FRAMES {
            self.stack_frames.push(StackFrame {
                name: name.into(),
                span,
                source: None,
                kind,
            });
        }
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for Diagnostic {}

fn line_column(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for (index, byte) in source.bytes().enumerate() {
        if index >= byte_offset {
            break;
        }
        if byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TokenKind {
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Comma,
    Colon,
    Semicolon,
    Dot,
    DotDot,
    Ellipsis,
    Ampersand,
    AndAnd,
    Pipe,
    OrOr,
    Caret,
    Tilde,
    Plus,
    Minus,
    Arrow,
    FatArrow,
    Star,
    Slash,
    Question,
    Bang,
    BangEqual,
    Equal,
    EqualEqual,
    Greater,
    GreaterEqual,
    LeftShift,
    RightShift,
    Less,
    LessEqual,
    Identifier(String),
    Int(i64),
    Float(f64),
    String(String),
    InterpolatedString(Vec<TokenStringInterpolationPart>),
    Let,
    Const,
    Type,
    Enum,
    Fn,
    Return,
    If,
    Else,
    Match,
    While,
    For,
    In,
    Import,
    As,
    Export,
    Record,
    True,
    False,
    Null,
    Break,
    Continue,
    Reserved(String),
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Module {
    statements: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TokenStringInterpolationPart {
    pub(crate) text: String,
    pub(crate) expression: Option<String>,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ArrayElement {
    Expr(Expr),
    Spread(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MapEntry {
    Entry { key: Expr, value: Expr },
    Spread(Expr),
}

struct ModuleUnit {
    specifier: String,
    module: Module,
}

struct NamespaceImport {
    specifier: String,
    members: HashMap<String, String>,
}

struct ResolvedModule {
    imports: Vec<ModuleUnit>,
    entry: Module,
}

impl ResolvedModule {
    fn into_flat_module(self) -> Module {
        let mut statements = Vec::new();
        for unit in self.imports {
            let _specifier = unit.specifier;
            statements.extend(unit.module.statements);
        }
        statements.extend(self.entry.statements);
        Module { statements }
    }
}

fn imported_module_surface(specifier: &str, module: Module) -> Result<Module, Diagnostic> {
    validate_module_declaration_names(&module)?;
    if !module.statements.iter().any(statement_is_exported) {
        return Ok(module);
    }

    let renames = module
        .statements
        .iter()
        .filter_map(|statement| {
            let name = top_level_declaration_name(statement)?;
            if statement_is_exported(statement) {
                None
            } else {
                Some((name.to_string(), internal_import_name(specifier, name)))
            }
        })
        .collect::<HashMap<_, _>>();
    let mut rewriter = NameRewriter::new(renames);
    Ok(Module {
        statements: module
            .statements
            .into_iter()
            .map(|statement| rewriter.rewrite_statement(statement, true))
            .collect(),
    })
}

fn module_export_members(module: &Module) -> HashSet<String> {
    let has_exports = module.statements.iter().any(statement_is_exported);
    module
        .statements
        .iter()
        .filter_map(|statement| {
            let name = top_level_declaration_name(statement)?;
            if !has_exports || statement_is_exported(statement) {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn namespace_module_surface(
    specifier: &str,
    module: Module,
) -> Result<(Module, HashSet<String>), Diagnostic> {
    validate_module_declaration_names(&module)?;
    let members = module_export_members(&module);
    let renames = module
        .statements
        .iter()
        .filter_map(|statement| {
            let name = top_level_declaration_name(statement)?;
            Some((name.to_string(), internal_import_name(specifier, name)))
        })
        .collect::<HashMap<_, _>>();
    let mut rewriter = NameRewriter::new(renames);
    Ok((
        Module {
            statements: module
                .statements
                .into_iter()
                .map(|statement| rewriter.rewrite_statement(statement, true))
                .collect(),
        },
        members,
    ))
}

fn validate_module_declaration_names(module: &Module) -> Result<(), Diagnostic> {
    let mut names = HashMap::new();
    for statement in &module.statements {
        let Some(name) = top_level_declaration_name(statement) else {
            continue;
        };
        if names
            .insert(name.to_string(), top_level_declaration_span(statement))
            .is_some()
        {
            return Err(Diagnostic::new(
                format!("name '{name}' redeclared"),
                top_level_declaration_span(statement),
            )
            .with_code("module.name-conflict"));
        }
    }
    Ok(())
}

fn statement_is_exported(statement: &Stmt) -> bool {
    match statement {
        Stmt::Let { exported, .. }
        | Stmt::TypeAlias { exported, .. }
        | Stmt::Enum { exported, .. }
        | Stmt::Function { exported, .. }
        | Stmt::Record { exported, .. } => *exported,
        _ => false,
    }
}

fn top_level_declaration_name(statement: &Stmt) -> Option<&str> {
    match statement {
        Stmt::Let { target, .. } => target.single_name(),
        Stmt::TypeAlias { name, .. } | Stmt::Enum { name, .. } => Some(name),
        Stmt::Function { name, .. } | Stmt::Record { name, .. } => Some(name),
        _ => None,
    }
}

fn top_level_declaration_span(statement: &Stmt) -> Span {
    match statement {
        Stmt::Let { span, .. }
        | Stmt::TypeAlias { span, .. }
        | Stmt::Enum { span, .. }
        | Stmt::Function { span, .. }
        | Stmt::Record { span, .. } => *span,
        _ => Span { start: 0, end: 0 },
    }
}

fn internal_import_name(specifier: &str, name: &str) -> String {
    let escaped = specifier
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '$'
            }
        })
        .collect::<String>();
    format!("$import${escaped}${name}")
}

struct NameRewriter {
    renames: HashMap<String, String>,
    scopes: Vec<HashSet<String>>,
}

impl NameRewriter {
    fn new(renames: HashMap<String, String>) -> Self {
        Self {
            renames,
            scopes: Vec::new(),
        }
    }

    fn rewrite_statement(&mut self, statement: Stmt, top_level: bool) -> Stmt {
        match statement {
            Stmt::Import { .. } => statement,
            Stmt::Let {
                target,
                ty,
                initializer,
                exported,
                is_const,
                span,
            } => {
                let target = self.rewrite_binding_target(target, top_level);
                let ty = ty.map(|ty| self.rewrite_type(ty));
                let initializer = self.rewrite_expr(initializer);
                Stmt::Let {
                    target,
                    ty,
                    initializer,
                    exported,
                    is_const,
                    span,
                }
            }
            Stmt::TypeAlias {
                name,
                ty,
                exported,
                span,
            } => {
                let name = if top_level {
                    self.rename_declaration(name)
                } else {
                    name
                };
                Stmt::TypeAlias {
                    name,
                    ty: self.rewrite_type(ty),
                    exported,
                    span,
                }
            }
            Stmt::Enum {
                name,
                variants,
                exported,
                span,
            } => {
                let name = if top_level {
                    self.rename_declaration(name)
                } else {
                    name
                };
                let variants = variants
                    .into_iter()
                    .map(|variant| EnumVariant {
                        name: variant.name,
                        payload: variant.payload.map(|ty| self.rewrite_type(ty)),
                        span: variant.span,
                    })
                    .collect();
                Stmt::Enum {
                    name,
                    variants,
                    exported,
                    span,
                }
            }
            Stmt::Function {
                name,
                type_params,
                type_param_constraints,
                params,
                return_type,
                body,
                exported,
                span,
            } => {
                let name = if top_level {
                    self.rename_declaration(name)
                } else {
                    self.define_local(name.clone());
                    name
                };
                let params = params
                    .into_iter()
                    .map(|param| Param {
                        name: param.name,
                        ty: self.rewrite_type(param.ty),
                    })
                    .collect::<Vec<_>>();
                let return_type = self.rewrite_type(return_type);
                self.push_scope();
                for param in &params {
                    self.define_local(param.name.clone());
                }
                let body = body
                    .into_iter()
                    .map(|statement| self.rewrite_statement(statement, false))
                    .collect();
                self.pop_scope();
                Stmt::Function {
                    name,
                    type_params,
                    type_param_constraints,
                    params,
                    return_type,
                    body,
                    exported,
                    span,
                }
            }
            Stmt::Record {
                name,
                fields,
                exported,
                span,
            } => {
                let name = if top_level {
                    self.rename_declaration(name)
                } else {
                    name
                };
                let fields = fields
                    .into_iter()
                    .map(|field| RecordField {
                        name: field.name,
                        ty: self.rewrite_type(field.ty),
                        span: field.span,
                    })
                    .collect();
                Stmt::Record {
                    name,
                    fields,
                    exported,
                    span,
                }
            }
            Stmt::Return { value, span } => Stmt::Return {
                value: self.rewrite_expr(value),
                span,
            },
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => Stmt::If {
                condition: self.rewrite_expr(condition),
                then_branch: self.rewrite_block(then_branch),
                else_branch: self.rewrite_block(else_branch),
                span,
            },
            Stmt::IfLet {
                pattern,
                value,
                then_branch,
                else_branch,
                span,
            } => {
                let value = self.rewrite_expr(value);
                self.push_scope();
                for name in match_pattern_binding_names(&pattern) {
                    self.define_local(name);
                }
                let then_branch = then_branch
                    .into_iter()
                    .map(|statement| self.rewrite_statement(statement, false))
                    .collect();
                self.pop_scope();
                Stmt::IfLet {
                    pattern,
                    value,
                    then_branch,
                    else_branch: self.rewrite_block(else_branch),
                    span,
                }
            }
            Stmt::Match {
                value,
                cases,
                default,
                span,
            } => Stmt::Match {
                value: self.rewrite_expr(value),
                cases: cases
                    .into_iter()
                    .map(|case| MatchCase {
                        pattern: case.pattern,
                        body: self.rewrite_block(case.body),
                        span: case.span,
                    })
                    .collect(),
                default: default.map(|default| self.rewrite_block(default)),
                span,
            },
            Stmt::LetElse {
                pattern,
                value,
                else_branch,
                span,
            } => {
                let value = self.rewrite_expr(value);
                let else_branch = self.rewrite_block(else_branch);
                if !top_level {
                    for name in match_pattern_binding_names(&pattern) {
                        self.define_local(name);
                    }
                }
                Stmt::LetElse {
                    pattern,
                    value,
                    else_branch,
                    span,
                }
            }
            Stmt::While {
                condition,
                body,
                span,
            } => Stmt::While {
                condition: self.rewrite_expr(condition),
                body: self.rewrite_block(body),
                span,
            },
            Stmt::WhileLet {
                pattern,
                value,
                body,
                span,
            } => {
                let value = self.rewrite_expr(value);
                self.push_scope();
                for name in match_pattern_binding_names(&pattern) {
                    self.define_local(name);
                }
                let body = body
                    .into_iter()
                    .map(|statement| self.rewrite_statement(statement, false))
                    .collect();
                self.pop_scope();
                Stmt::WhileLet {
                    pattern,
                    value,
                    body,
                    span,
                }
            }
            Stmt::For {
                name,
                start,
                end,
                body,
                span,
            } => {
                let start = self.rewrite_expr(start);
                let end = self.rewrite_expr(end);
                self.push_scope();
                self.define_local(name.clone());
                let body = body
                    .into_iter()
                    .map(|statement| self.rewrite_statement(statement, false))
                    .collect();
                self.pop_scope();
                Stmt::For {
                    name,
                    start,
                    end,
                    body,
                    span,
                }
            }
            Stmt::Block { statements, span } => Stmt::Block {
                statements: self.rewrite_block(statements),
                span,
            },
            Stmt::Break { span } => Stmt::Break { span },
            Stmt::Continue { span } => Stmt::Continue { span },
            Stmt::Expression { expression, span } => Stmt::Expression {
                expression: self.rewrite_expr(expression),
                span,
            },
        }
    }

    fn rewrite_block(&mut self, statements: Vec<Stmt>) -> Vec<Stmt> {
        self.push_scope();
        let statements = statements
            .into_iter()
            .map(|statement| self.rewrite_statement(statement, false))
            .collect();
        self.pop_scope();
        statements
    }

    fn rewrite_expr(&mut self, expr: Expr) -> Expr {
        let kind = match expr.kind {
            ExprKind::Literal(value) => ExprKind::Literal(value),
            ExprKind::StringInterpolation(parts) => ExprKind::StringInterpolation(
                parts
                    .into_iter()
                    .map(|part| StringInterpolationPart {
                        text: part.text,
                        expression: part.expression.map(|expr| self.rewrite_expr(expr)),
                        span: part.span,
                    })
                    .collect(),
            ),
            ExprKind::Question { value } => ExprKind::Question {
                value: Box::new(self.rewrite_expr(*value)),
            },
            ExprKind::Variable(name) => ExprKind::Variable(self.rename_reference(name)),
            ExprKind::Assign { name, value } => ExprKind::Assign {
                name: self.rename_reference(name),
                value: Box::new(self.rewrite_expr(*value)),
            },
            ExprKind::Unary { op, right } => ExprKind::Unary {
                op,
                right: Box::new(self.rewrite_expr(*right)),
            },
            ExprKind::Binary { left, op, right } => ExprKind::Binary {
                left: Box::new(self.rewrite_expr(*left)),
                op,
                right: Box::new(self.rewrite_expr(*right)),
            },
            ExprKind::Call {
                callee,
                args,
                paren_span,
            } => ExprKind::Call {
                callee: Box::new(self.rewrite_expr(*callee)),
                args: args.into_iter().map(|arg| self.rewrite_expr(arg)).collect(),
                paren_span,
            },
            ExprKind::ArrayLiteral { elements } => ExprKind::ArrayLiteral {
                elements: elements
                    .into_iter()
                    .map(|element| match element {
                        ArrayElement::Expr(value) => ArrayElement::Expr(self.rewrite_expr(value)),
                        ArrayElement::Spread(value) => {
                            ArrayElement::Spread(self.rewrite_expr(value))
                        }
                    })
                    .collect(),
            },
            ExprKind::TupleLiteral { elements } => ExprKind::TupleLiteral {
                elements: elements
                    .into_iter()
                    .map(|element| self.rewrite_expr(element))
                    .collect(),
            },
            ExprKind::MapLiteral { entries } => ExprKind::MapLiteral {
                entries: entries
                    .into_iter()
                    .map(|entry| match entry {
                        MapEntry::Entry { key, value } => MapEntry::Entry {
                            key: self.rewrite_expr(key),
                            value: self.rewrite_expr(value),
                        },
                        MapEntry::Spread(value) => MapEntry::Spread(self.rewrite_expr(value)),
                    })
                    .collect(),
            },
            ExprKind::RecordLiteral { name, fields } => ExprKind::RecordLiteral {
                name: self.rename_type_name(name),
                fields: fields
                    .into_iter()
                    .map(|(name, value, span)| (name, self.rewrite_expr(value), span))
                    .collect(),
            },
            ExprKind::Index { array, index } => ExprKind::Index {
                array: Box::new(self.rewrite_expr(*array)),
                index: Box::new(self.rewrite_expr(*index)),
            },
            ExprKind::IndexAssign {
                container,
                index,
                value,
            } => ExprKind::IndexAssign {
                container: Box::new(self.rewrite_expr(*container)),
                index: Box::new(self.rewrite_expr(*index)),
                value: Box::new(self.rewrite_expr(*value)),
            },
            ExprKind::FunctionLiteral {
                params,
                return_type,
                body,
            } => {
                self.push_scope();
                for param in &params {
                    self.define_local(param.name.clone());
                }
                let body = self.rewrite_block(body);
                self.pop_scope();
                ExprKind::FunctionLiteral {
                    params,
                    return_type,
                    body,
                }
            }
            ExprKind::Field {
                receiver,
                name,
                span,
            } => ExprKind::Field {
                receiver: Box::new(self.rewrite_expr(*receiver)),
                name,
                span,
            },
        };
        Expr {
            kind,
            span: expr.span,
        }
    }

    fn rewrite_type(&self, ty: Type) -> Type {
        match ty {
            Type::Array(element) => Type::Array(Box::new(self.rewrite_type(*element))),
            Type::Tuple(elements) => Type::Tuple(
                elements
                    .into_iter()
                    .map(|element| self.rewrite_type(element))
                    .collect(),
            ),
            Type::Map(value) => Type::Map(Box::new(self.rewrite_type(*value))),
            Type::Option(value) => Type::Option(Box::new(self.rewrite_type(*value))),
            Type::Result { ok, err } => Type::Result {
                ok: Box::new(self.rewrite_type(*ok)),
                err: Box::new(self.rewrite_type(*err)),
            },
            Type::Record(name) => Type::Record(self.rename_type_name(name)),
            Type::Enum(name) => Type::Enum(self.rename_type_name(name)),
            Type::Function {
                type_params,
                params,
                return_type,
            } => Type::Function {
                type_params,
                params: params
                    .into_iter()
                    .map(|param| self.rewrite_type(param))
                    .collect(),
                return_type: Box::new(self.rewrite_type(*return_type)),
            },
            other => other,
        }
    }

    fn rewrite_binding_target(&mut self, target: BindingTarget, top_level: bool) -> BindingTarget {
        match target {
            BindingTarget::Name { name, span } => {
                let name = if top_level {
                    self.rename_declaration(name)
                } else {
                    self.define_local(name.clone());
                    name
                };
                BindingTarget::Name { name, span }
            }
            BindingTarget::Tuple { names, span } => {
                if !top_level {
                    for name in &names {
                        self.define_local(name.clone());
                    }
                }
                BindingTarget::Tuple { names, span }
            }
            BindingTarget::Record { names, span } => {
                if !top_level {
                    for name in &names {
                        self.define_local(name.clone());
                    }
                }
                BindingTarget::Record { names, span }
            }
        }
    }

    fn rename_declaration(&self, name: String) -> String {
        self.renames.get(&name).cloned().unwrap_or(name)
    }

    fn rename_reference(&self, name: String) -> String {
        if self.is_local(&name) {
            return name;
        }
        self.renames.get(&name).cloned().unwrap_or(name)
    }

    fn rename_type_name(&self, name: String) -> String {
        self.renames.get(&name).cloned().unwrap_or(name)
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define_local(&mut self, name: String) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name);
        }
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }
}

struct NamespaceRewriter {
    namespaces: HashMap<String, NamespaceImport>,
    scopes: Vec<HashSet<String>>,
}

impl NamespaceRewriter {
    fn new(namespaces: HashMap<String, NamespaceImport>) -> Self {
        Self {
            namespaces,
            scopes: Vec::new(),
        }
    }

    fn rewrite_module(&mut self, module: Module) -> Result<Module, Diagnostic> {
        let statements = module
            .statements
            .into_iter()
            .map(|statement| self.rewrite_statement(statement, true))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Module { statements })
    }

    fn rewrite_statement(&mut self, statement: Stmt, top_level: bool) -> Result<Stmt, Diagnostic> {
        Ok(match statement {
            Stmt::Import { .. } => statement,
            Stmt::Let {
                target,
                ty,
                initializer,
                exported,
                is_const,
                span,
            } => {
                let initializer = self.rewrite_expr(initializer)?;
                if !top_level {
                    for name in target.names() {
                        self.define_local(name.to_string());
                    }
                }
                Stmt::Let {
                    target,
                    ty,
                    initializer,
                    exported,
                    is_const,
                    span,
                }
            }
            Stmt::TypeAlias { .. } | Stmt::Enum { .. } => statement,
            Stmt::Function {
                name,
                type_params,
                type_param_constraints,
                params,
                return_type,
                body,
                exported,
                span,
            } => {
                if !top_level {
                    self.define_local(name.clone());
                }
                self.push_scope();
                for param in &params {
                    self.define_local(param.name.clone());
                }
                let body = body
                    .into_iter()
                    .map(|statement| self.rewrite_statement(statement, false))
                    .collect::<Result<Vec<_>, _>>()?;
                self.pop_scope();
                Stmt::Function {
                    name,
                    type_params,
                    type_param_constraints,
                    params,
                    return_type,
                    body,
                    exported,
                    span,
                }
            }
            Stmt::Record { .. } => statement,
            Stmt::Return { value, span } => Stmt::Return {
                value: self.rewrite_expr(value)?,
                span,
            },
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => Stmt::If {
                condition: self.rewrite_expr(condition)?,
                then_branch: self.rewrite_block(then_branch)?,
                else_branch: self.rewrite_block(else_branch)?,
                span,
            },
            Stmt::IfLet {
                pattern,
                value,
                then_branch,
                else_branch,
                span,
            } => {
                let value = self.rewrite_expr(value)?;
                self.push_scope();
                for name in match_pattern_binding_names(&pattern) {
                    self.define_local(name);
                }
                let then_branch = then_branch
                    .into_iter()
                    .map(|statement| self.rewrite_statement(statement, false))
                    .collect::<Result<Vec<_>, _>>()?;
                self.pop_scope();
                Stmt::IfLet {
                    pattern,
                    value,
                    then_branch,
                    else_branch: self.rewrite_block(else_branch)?,
                    span,
                }
            }
            Stmt::Match {
                value,
                cases,
                default,
                span,
            } => Stmt::Match {
                value: self.rewrite_expr(value)?,
                cases: cases
                    .into_iter()
                    .map(|case| {
                        Ok(MatchCase {
                            pattern: case.pattern,
                            body: self.rewrite_block(case.body)?,
                            span: case.span,
                        })
                    })
                    .collect::<Result<Vec<_>, Diagnostic>>()?,
                default: default
                    .map(|default| self.rewrite_block(default))
                    .transpose()?,
                span,
            },
            Stmt::LetElse {
                pattern,
                value,
                else_branch,
                span,
            } => {
                let value = self.rewrite_expr(value)?;
                let else_branch = self.rewrite_block(else_branch)?;
                if !top_level {
                    for name in match_pattern_binding_names(&pattern) {
                        self.define_local(name);
                    }
                }
                Stmt::LetElse {
                    pattern,
                    value,
                    else_branch,
                    span,
                }
            }
            Stmt::While {
                condition,
                body,
                span,
            } => Stmt::While {
                condition: self.rewrite_expr(condition)?,
                body: self.rewrite_block(body)?,
                span,
            },
            Stmt::WhileLet {
                pattern,
                value,
                body,
                span,
            } => {
                let value = self.rewrite_expr(value)?;
                self.push_scope();
                for name in match_pattern_binding_names(&pattern) {
                    self.define_local(name);
                }
                let body = body
                    .into_iter()
                    .map(|statement| self.rewrite_statement(statement, false))
                    .collect::<Result<Vec<_>, _>>()?;
                self.pop_scope();
                Stmt::WhileLet {
                    pattern,
                    value,
                    body,
                    span,
                }
            }
            Stmt::For {
                name,
                start,
                end,
                body,
                span,
            } => {
                let start = self.rewrite_expr(start)?;
                let end = self.rewrite_expr(end)?;
                self.push_scope();
                self.define_local(name.clone());
                let body = body
                    .into_iter()
                    .map(|statement| self.rewrite_statement(statement, false))
                    .collect::<Result<Vec<_>, _>>()?;
                self.pop_scope();
                Stmt::For {
                    name,
                    start,
                    end,
                    body,
                    span,
                }
            }
            Stmt::Block { statements, span } => Stmt::Block {
                statements: self.rewrite_block(statements)?,
                span,
            },
            Stmt::Break { span } => Stmt::Break { span },
            Stmt::Continue { span } => Stmt::Continue { span },
            Stmt::Expression { expression, span } => Stmt::Expression {
                expression: self.rewrite_expr(expression)?,
                span,
            },
        })
    }

    fn rewrite_block(&mut self, statements: Vec<Stmt>) -> Result<Vec<Stmt>, Diagnostic> {
        self.push_scope();
        let statements = statements
            .into_iter()
            .map(|statement| self.rewrite_statement(statement, false))
            .collect::<Result<Vec<_>, _>>();
        self.pop_scope();
        statements
    }

    fn rewrite_expr(&mut self, expr: Expr) -> Result<Expr, Diagnostic> {
        let kind = match expr.kind {
            ExprKind::Literal(value) => ExprKind::Literal(value),
            ExprKind::StringInterpolation(parts) => ExprKind::StringInterpolation(
                parts
                    .into_iter()
                    .map(|part| {
                        Ok(StringInterpolationPart {
                            text: part.text,
                            expression: part
                                .expression
                                .map(|expr| self.rewrite_expr(expr))
                                .transpose()?,
                            span: part.span,
                        })
                    })
                    .collect::<Result<Vec<_>, Diagnostic>>()?,
            ),
            ExprKind::Question { value } => ExprKind::Question {
                value: Box::new(self.rewrite_expr(*value)?),
            },
            ExprKind::Variable(name) => ExprKind::Variable(name),
            ExprKind::Assign { name, value } => ExprKind::Assign {
                name,
                value: Box::new(self.rewrite_expr(*value)?),
            },
            ExprKind::Unary { op, right } => ExprKind::Unary {
                op,
                right: Box::new(self.rewrite_expr(*right)?),
            },
            ExprKind::Binary { left, op, right } => ExprKind::Binary {
                left: Box::new(self.rewrite_expr(*left)?),
                op,
                right: Box::new(self.rewrite_expr(*right)?),
            },
            ExprKind::Call {
                callee,
                args,
                paren_span,
            } => ExprKind::Call {
                callee: Box::new(self.rewrite_expr(*callee)?),
                args: args
                    .into_iter()
                    .map(|arg| self.rewrite_expr(arg))
                    .collect::<Result<Vec<_>, _>>()?,
                paren_span,
            },
            ExprKind::ArrayLiteral { elements } => ExprKind::ArrayLiteral {
                elements: elements
                    .into_iter()
                    .map(|element| match element {
                        ArrayElement::Expr(value) => {
                            Ok(ArrayElement::Expr(self.rewrite_expr(value)?))
                        }
                        ArrayElement::Spread(value) => {
                            Ok(ArrayElement::Spread(self.rewrite_expr(value)?))
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            },
            ExprKind::TupleLiteral { elements } => ExprKind::TupleLiteral {
                elements: elements
                    .into_iter()
                    .map(|element| self.rewrite_expr(element))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            ExprKind::MapLiteral { entries } => ExprKind::MapLiteral {
                entries: entries
                    .into_iter()
                    .map(|entry| match entry {
                        MapEntry::Entry { key, value } => Ok(MapEntry::Entry {
                            key: self.rewrite_expr(key)?,
                            value: self.rewrite_expr(value)?,
                        }),
                        MapEntry::Spread(value) => Ok(MapEntry::Spread(self.rewrite_expr(value)?)),
                    })
                    .collect::<Result<Vec<_>, Diagnostic>>()?,
            },
            ExprKind::RecordLiteral { name, fields } => ExprKind::RecordLiteral {
                name,
                fields: fields
                    .into_iter()
                    .map(|(name, value, span)| Ok((name, self.rewrite_expr(value)?, span)))
                    .collect::<Result<Vec<_>, Diagnostic>>()?,
            },
            ExprKind::Index { array, index } => ExprKind::Index {
                array: Box::new(self.rewrite_expr(*array)?),
                index: Box::new(self.rewrite_expr(*index)?),
            },
            ExprKind::IndexAssign {
                container,
                index,
                value,
            } => ExprKind::IndexAssign {
                container: Box::new(self.rewrite_expr(*container)?),
                index: Box::new(self.rewrite_expr(*index)?),
                value: Box::new(self.rewrite_expr(*value)?),
            },
            ExprKind::FunctionLiteral {
                params,
                return_type,
                body,
            } => {
                self.push_scope();
                for param in &params {
                    self.define_local(param.name.clone());
                }
                let body = self.rewrite_block(body)?;
                self.pop_scope();
                ExprKind::FunctionLiteral {
                    params,
                    return_type,
                    body,
                }
            }
            ExprKind::Field {
                receiver,
                name,
                span,
            } => {
                if let ExprKind::Variable(alias) = &receiver.kind {
                    if !self.is_local(alias) {
                        if let Some(namespace) = self.namespaces.get(alias) {
                            if !namespace.members.contains_key(&name) {
                                return Err(Diagnostic::new(
                                    format!("module namespace '{alias}' has no member '{name}'"),
                                    span,
                                )
                                .with_code("module.member-not-found"));
                            }
                            ExprKind::Variable(
                                namespace.members.get(&name).cloned().unwrap_or_else(|| {
                                    internal_import_name(&namespace.specifier, &name)
                                }),
                            )
                        } else {
                            ExprKind::Field {
                                receiver: Box::new(self.rewrite_expr(*receiver)?),
                                name,
                                span,
                            }
                        }
                    } else {
                        ExprKind::Field {
                            receiver: Box::new(self.rewrite_expr(*receiver)?),
                            name,
                            span,
                        }
                    }
                } else {
                    ExprKind::Field {
                        receiver: Box::new(self.rewrite_expr(*receiver)?),
                        name,
                        span,
                    }
                }
            }
        };
        Ok(Expr {
            kind,
            span: expr.span,
        })
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define_local(&mut self, name: String) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name);
        }
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Stmt {
    Import {
        path: String,
        alias: Option<String>,
        span: Span,
    },
    Let {
        target: BindingTarget,
        ty: Option<Type>,
        initializer: Expr,
        exported: bool,
        is_const: bool,
        span: Span,
    },
    TypeAlias {
        name: String,
        ty: Type,
        exported: bool,
        span: Span,
    },
    Enum {
        name: String,
        variants: Vec<EnumVariant>,
        exported: bool,
        span: Span,
    },
    Function {
        name: String,
        type_params: Vec<String>,
        type_param_constraints: Vec<Vec<ConstraintMarker>>,
        params: Vec<Param>,
        return_type: Type,
        body: Vec<Stmt>,
        exported: bool,
        span: Span,
    },
    Record {
        name: String,
        fields: Vec<RecordField>,
        exported: bool,
        span: Span,
    },
    Return {
        value: Expr,
        span: Span,
    },
    If {
        condition: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Vec<Stmt>,
        span: Span,
    },
    IfLet {
        pattern: MatchCaseValue,
        value: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Vec<Stmt>,
        span: Span,
    },
    Match {
        value: Expr,
        cases: Vec<MatchCase>,
        default: Option<Vec<Stmt>>,
        span: Span,
    },
    LetElse {
        pattern: MatchCaseValue,
        value: Expr,
        else_branch: Vec<Stmt>,
        span: Span,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    WhileLet {
        pattern: MatchCaseValue,
        value: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    For {
        name: String,
        start: Expr,
        end: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    Block {
        statements: Vec<Stmt>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Expression {
        expression: Expr,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BindingTarget {
    Name { name: String, span: Span },
    Tuple { names: Vec<String>, span: Span },
    Record { names: Vec<String>, span: Span },
}

impl BindingTarget {
    fn single_name(&self) -> Option<&str> {
        match self {
            Self::Name { name, .. } => Some(name),
            Self::Tuple { .. } | Self::Record { .. } => None,
        }
    }

    fn names(&self) -> Vec<&str> {
        match self {
            Self::Name { name, .. } => vec![name.as_str()],
            Self::Tuple { names, .. } | Self::Record { names, .. } => {
                names.iter().map(String::as_str).collect()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct MatchCase {
    pattern: MatchCaseValue,
    body: Vec<Stmt>,
    span: Span,
}

#[derive(Debug, Clone, PartialEq)]
struct StringInterpolationPart {
    text: String,
    expression: Option<Expr>,
    span: Span,
}

#[derive(Debug, Clone, PartialEq)]
enum MatchCaseValue {
    Int(i64),
    Float(f64),
    Str(String),
    IntRange {
        start: i64,
        end: i64,
    },
    Bind(String),
    Some(Box<MatchCaseValue>),
    None,
    Ok(Box<MatchCaseValue>),
    Err(Box<MatchCaseValue>),
    EnumVariant {
        name: String,
        payload: Option<Box<MatchCaseValue>>,
    },
}

fn match_pattern_binding_names(pattern: &MatchCaseValue) -> Vec<String> {
    let mut names = Vec::new();
    collect_match_pattern_binding_names(pattern, &mut names);
    names
}

fn collect_match_pattern_binding_names(pattern: &MatchCaseValue, names: &mut Vec<String>) {
    match pattern {
        MatchCaseValue::Bind(name) => names.push(name.clone()),
        MatchCaseValue::Some(inner) | MatchCaseValue::Ok(inner) | MatchCaseValue::Err(inner) => {
            collect_match_pattern_binding_names(inner, names);
        }
        MatchCaseValue::EnumVariant {
            payload: Some(inner),
            ..
        } => collect_match_pattern_binding_names(inner, names),
        MatchCaseValue::Int(_)
        | MatchCaseValue::Float(_)
        | MatchCaseValue::Str(_)
        | MatchCaseValue::IntRange { .. }
        | MatchCaseValue::None
        | MatchCaseValue::EnumVariant { payload: None, .. } => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Param {
    name: String,
    ty: Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConstraintMarker {
    Equatable,
    Comparable,
    Stringify,
    Hashable,
}

impl ConstraintMarker {
    pub fn as_str(self) -> &'static str {
        match self {
            ConstraintMarker::Equatable => "Equatable",
            ConstraintMarker::Comparable => "Comparable",
            ConstraintMarker::Stringify => "Stringify",
            ConstraintMarker::Hashable => "Hashable",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "Equatable" => Some(ConstraintMarker::Equatable),
            "Comparable" => Some(ConstraintMarker::Comparable),
            "Stringify" => Some(ConstraintMarker::Stringify),
            "Hashable" => Some(ConstraintMarker::Hashable),
            _ => None,
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            ConstraintMarker::Equatable,
            ConstraintMarker::Comparable,
            ConstraintMarker::Stringify,
            ConstraintMarker::Hashable,
        ]
    }
}

fn type_implements_marker(ty: &Type, marker: ConstraintMarker) -> bool {
    match ty {
        Type::Bool | Type::Int | Type::Float | Type::Str => true,
        Type::Null => matches!(
            marker,
            ConstraintMarker::Stringify | ConstraintMarker::Equatable
        ),
        Type::Array(element) | Type::Option(element) => type_implements_marker(element, marker),
        Type::Map(value) => type_implements_marker(value, marker),
        Type::Tuple(elements) => elements
            .iter()
            .all(|element| type_implements_marker(element, marker)),
        Type::Result { ok, err } => {
            type_implements_marker(ok, marker) && type_implements_marker(err, marker)
        }
        Type::Function { .. } => matches!(marker, ConstraintMarker::Stringify),
        Type::Json => matches!(
            marker,
            ConstraintMarker::Stringify | ConstraintMarker::Equatable
        ),
        Type::Record(_) | Type::Enum(_) => {
            // Records and enums are treated as opaque for now; only Stringify always holds.
            matches!(marker, ConstraintMarker::Stringify)
        }
        Type::Generic(_) => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordField {
    name: String,
    ty: Type,
    span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumVariant {
    name: String,
    payload: Option<Type>,
    span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Null,
    Bool,
    Int,
    Float,
    Str,
    Json,
    Array(Box<Type>),
    Tuple(Vec<Type>),
    Map(Box<Type>),
    Option(Box<Type>),
    Result {
        ok: Box<Type>,
        err: Box<Type>,
    },
    Record(String),
    Enum(String),
    Function {
        type_params: Vec<String>,
        params: Vec<Type>,
        return_type: Box<Type>,
    },
    Generic(String),
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Bool => write!(f, "bool"),
            Self::Int => write!(f, "int"),
            Self::Float => write!(f, "float"),
            Self::Str => write!(f, "str"),
            Self::Json => write!(f, "json"),
            Self::Array(element) => write!(f, "[{element}]"),
            Self::Tuple(elements) => {
                write!(f, "(")?;
                for (index, element) in elements.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{element}")?;
                }
                write!(f, ")")
            }
            Self::Map(value) => write!(f, "map[str, {value}]"),
            Self::Option(value) => write!(f, "option[{value}]"),
            Self::Result { ok, err } => write!(f, "result[{ok}, {err}]"),
            Self::Record(name) => write!(f, "{name}"),
            Self::Enum(name) => write!(f, "{name}"),
            Self::Generic(name) => write!(f, "{name}"),
            Self::Function {
                type_params,
                params,
                return_type,
            } => {
                write!(f, "fn")?;
                if !type_params.is_empty() {
                    write!(f, "<")?;
                    for (index, type_param) in type_params.iter().enumerate() {
                        if index > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{type_param}")?;
                    }
                    write!(f, ">")?;
                }
                write!(f, "(")?;
                for (index, param) in params.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{param}")?;
                }
                write!(f, ") -> {return_type}")
            }
        }
    }
}

impl Type {
    fn is_numeric(&self) -> bool {
        matches!(self, Self::Int | Self::Float)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Expr {
    kind: ExprKind,
    span: Span,
}

#[derive(Debug, Clone, PartialEq)]
enum ExprKind {
    Literal(Value),
    StringInterpolation(Vec<StringInterpolationPart>),
    Question {
        value: Box<Expr>,
    },
    Variable(String),
    Assign {
        name: String,
        value: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        right: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        paren_span: Span,
    },
    ArrayLiteral {
        elements: Vec<ArrayElement>,
    },
    TupleLiteral {
        elements: Vec<Expr>,
    },
    MapLiteral {
        entries: Vec<MapEntry>,
    },
    RecordLiteral {
        name: String,
        fields: Vec<(String, Expr, Span)>,
    },
    Index {
        array: Box<Expr>,
        index: Box<Expr>,
    },
    IndexAssign {
        container: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
    },
    FunctionLiteral {
        params: Vec<Param>,
        return_type: Type,
        body: Vec<Stmt>,
    },
    Field {
        receiver: Box<Expr>,
        name: String,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnaryOp {
    Negate,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    RangeLessThan,
    And,
    Or,
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
}

#[derive(Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Rc<str>),
    Json(Rc<JsonValue>),
    Array(Rc<Array>),
    Tuple(Rc<Tuple>),
    Map(Rc<Map>),
    Option(Rc<OptionValue>),
    Result(Rc<ResultValue>),
    Enum(Rc<EnumValue>),
    Record(Rc<Record>),
    Function(Rc<Function>),
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "Null"),
            Self::Bool(value) => f.debug_tuple("Bool").field(value).finish(),
            Self::Int(value) => f.debug_tuple("Int").field(value).finish(),
            Self::Float(value) => f.debug_tuple("Float").field(value).finish(),
            Self::String(value) => f.debug_tuple("String").field(value).finish(),
            Self::Json(value) => f.debug_tuple("Json").field(value).finish(),
            Self::Array(array) => f
                .debug_struct("Array")
                .field("element_type", &array.element_type)
                .field("elements", &array.elements)
                .finish(),
            Self::Tuple(tuple) => f
                .debug_struct("Tuple")
                .field("element_types", &tuple.element_types)
                .field("elements", &tuple.elements)
                .finish(),
            Self::Map(map) => f
                .debug_struct("Map")
                .field("value_type", &map.value_type)
                .field("entries", &map.entries)
                .finish(),
            Self::Option(option) => f
                .debug_struct("Option")
                .field("payload_type", &option.payload_type)
                .field("payload", &option.payload)
                .finish(),
            Self::Result(result) => f
                .debug_struct("Result")
                .field("ok_type", &result.ok_type)
                .field("err_type", &result.err_type)
                .field("variant", &result.variant)
                .finish(),
            Self::Enum(value) => f
                .debug_struct("Enum")
                .field("name", &value.name)
                .field("variant", &value.variant)
                .field("payload", &value.payload)
                .finish(),
            Self::Record(record) => f
                .debug_struct("Record")
                .field("name", &record.name)
                .field("fields", &record.fields)
                .finish(),
            Self::Function(function) => f
                .debug_tuple("Function")
                .field(&function.signature_name())
                .finish(),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::Int(left), Self::Int(right)) => left == right,
            (Self::Float(left), Self::Float(right)) => left == right,
            (Self::String(left), Self::String(right)) => left == right,
            (Self::Json(left), Self::Json(right)) => left == right,
            (Self::Array(left), Self::Array(right)) => Rc::ptr_eq(left, right),
            (Self::Tuple(left), Self::Tuple(right)) => Rc::ptr_eq(left, right),
            (Self::Map(left), Self::Map(right)) => Rc::ptr_eq(left, right),
            (Self::Option(left), Self::Option(right)) => Rc::ptr_eq(left, right),
            (Self::Result(left), Self::Result(right)) => Rc::ptr_eq(left, right),
            (Self::Enum(left), Self::Enum(right)) => Rc::ptr_eq(left, right),
            (Self::Record(left), Self::Record(right)) => Rc::ptr_eq(left, right),
            (Self::Function(left), Self::Function(right)) => Rc::ptr_eq(left, right),
            _ => false,
        }
    }
}

impl Value {
    pub fn string(value: impl Into<Rc<str>>) -> Self {
        Self::String(value.into())
    }

    pub fn json(value: JsonValue) -> Self {
        Self::Json(Rc::new(value))
    }

    pub fn array(element_type: Type, elements: Vec<Value>) -> Self {
        Self::Array(Rc::new(Array::new(element_type, elements)))
    }

    pub fn tuple(element_types: Vec<Type>, elements: Vec<Value>) -> Self {
        Self::Tuple(Rc::new(Tuple::new(element_types, elements)))
    }

    pub fn map(value_type: Type, entries: BTreeMap<String, Value>) -> Self {
        Self::Map(Rc::new(Map::new(value_type, entries)))
    }

    pub fn some(payload_type: Type, payload: Value) -> Self {
        Self::Option(Rc::new(OptionValue::some(payload_type, payload)))
    }

    pub fn none(payload_type: Type) -> Self {
        Self::Option(Rc::new(OptionValue::none(payload_type)))
    }

    pub fn ok(ok_type: Type, err_type: Type, payload: Value) -> Self {
        Self::Result(Rc::new(ResultValue::ok(ok_type, err_type, payload)))
    }

    pub fn err(ok_type: Type, err_type: Type, payload: Value) -> Self {
        Self::Result(Rc::new(ResultValue::err(ok_type, err_type, payload)))
    }

    pub fn enum_variant(
        name: impl Into<String>,
        variant: impl Into<String>,
        payload: Option<Value>,
    ) -> Self {
        Self::Enum(Rc::new(EnumValue::new(name, variant, payload)))
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::String(_) => "string",
            Self::Json(_) => "json",
            Self::Array(_) => "array",
            Self::Tuple(_) => "tuple",
            Self::Map(_) => "map",
            Self::Option(_) => "option",
            Self::Result(_) => "result",
            Self::Enum(_) => "enum",
            Self::Record(_) => "record",
            Self::Function(_) => "function",
        }
    }

    fn is_truthy(&self) -> bool {
        !matches!(self, Self::Null | Self::Bool(false))
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value}"),
            Self::String(value) => write!(f, "{value}"),
            Self::Json(value) => write!(f, "{value}"),
            Self::Array(array) => {
                write!(f, "[")?;
                for (index, value) in array.snapshot().iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{value}")?;
                }
                write!(f, "]")
            }
            Self::Tuple(tuple) => {
                write!(f, "(")?;
                for (index, value) in tuple.elements.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{value}")?;
                }
                write!(f, ")")
            }
            Self::Map(map) => {
                write!(f, "{{")?;
                let snapshot = map.entries();
                for (index, (key, value)) in snapshot.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{key}\": {value}")?;
                }
                write!(f, "}}")
            }
            Self::Option(option) => match &option.payload {
                Some(payload) => write!(f, "some({payload})"),
                None => write!(f, "none"),
            },
            Self::Result(result) => match &result.variant {
                ResultVariant::Ok(payload) => write!(f, "ok({payload})"),
                ResultVariant::Err(payload) => write!(f, "err({payload})"),
            },
            Self::Enum(value) => {
                write!(f, "{}.{}", value.name, value.variant)?;
                if let Some(payload) = &value.payload {
                    write!(f, "({payload})")?;
                }
                Ok(())
            }
            Self::Record(record) => {
                write!(f, "{} {{", record.name)?;
                for (index, (key, value)) in record.fields.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{key}: {value}")?;
                }
                write!(f, "}}")
            }
            Self::Function(function) => write!(f, "<fn {}>", function.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl fmt::Display for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Number(value) => write!(f, "{value}"),
            Self::String(value) => write!(f, "\"{}\"", escape_string(value)),
            Self::Array(values) => {
                write!(f, "[")?;
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{value}")?;
                }
                write!(f, "]")
            }
            Self::Object(entries) => {
                write!(f, "{{")?;
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "\"{}\":{value}", escape_string(key))?;
                }
                write!(f, "}}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptionValue {
    pub(crate) payload_type: Type,
    pub(crate) payload: Option<Value>,
}

impl OptionValue {
    pub(crate) fn some(payload_type: Type, payload: Value) -> Self {
        Self {
            payload_type,
            payload: Some(payload),
        }
    }

    pub(crate) fn none(payload_type: Type) -> Self {
        Self {
            payload_type,
            payload: None,
        }
    }

    pub fn payload_type(&self) -> &Type {
        &self.payload_type
    }

    pub fn payload(&self) -> Option<&Value> {
        self.payload.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResultValue {
    pub(crate) ok_type: Type,
    pub(crate) err_type: Type,
    pub(crate) variant: ResultVariant,
}

impl ResultValue {
    pub(crate) fn ok(ok_type: Type, err_type: Type, payload: Value) -> Self {
        Self {
            ok_type,
            err_type,
            variant: ResultVariant::Ok(payload),
        }
    }

    pub(crate) fn err(ok_type: Type, err_type: Type, payload: Value) -> Self {
        Self {
            ok_type,
            err_type,
            variant: ResultVariant::Err(payload),
        }
    }

    pub fn ok_type(&self) -> &Type {
        &self.ok_type
    }

    pub fn err_type(&self) -> &Type {
        &self.err_type
    }

    pub fn is_ok(&self) -> bool {
        matches!(self.variant, ResultVariant::Ok(_))
    }

    pub fn payload(&self) -> &Value {
        match &self.variant {
            ResultVariant::Ok(payload) | ResultVariant::Err(payload) => payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ResultVariant {
    Ok(Value),
    Err(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumValue {
    name: String,
    variant: String,
    payload: Option<Value>,
}

impl EnumValue {
    fn new(name: impl Into<String>, variant: impl Into<String>, payload: Option<Value>) -> Self {
        Self {
            name: name.into(),
            variant: variant.into(),
            payload,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn variant(&self) -> &str {
        &self.variant
    }

    pub fn payload(&self) -> Option<&Value> {
        self.payload.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    element_type: Type,
    elements: RefCell<Vec<Value>>,
    max_length: Option<usize>,
}

impl Array {
    fn new(element_type: Type, elements: Vec<Value>) -> Self {
        Self::new_with_cap(element_type, elements, None)
    }

    pub(crate) fn new_with_cap(
        element_type: Type,
        elements: Vec<Value>,
        max_length: Option<usize>,
    ) -> Self {
        Self {
            element_type,
            elements: RefCell::new(elements),
            max_length,
        }
    }

    pub fn element_type(&self) -> &Type {
        &self.element_type
    }

    pub fn len(&self) -> usize {
        self.elements.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.borrow().is_empty()
    }

    pub fn get(&self, index: usize) -> Option<Value> {
        self.elements.borrow().get(index).cloned()
    }

    pub fn snapshot(&self) -> Vec<Value> {
        self.elements.borrow().clone()
    }

    pub fn elements(&self) -> Vec<Value> {
        self.snapshot()
    }

    pub fn push(&self, value: Value) {
        self.elements.borrow_mut().push(value);
    }

    pub fn try_push(&self, value: Value) -> Result<(), usize> {
        let mut elements = self.elements.borrow_mut();
        if let Some(max) = self.max_length {
            if elements.len() >= max {
                return Err(max);
            }
        }
        elements.push(value);
        Ok(())
    }

    pub fn pop(&self) -> Option<Value> {
        self.elements.borrow_mut().pop()
    }

    pub fn set(&self, index: usize, value: Value) -> Result<(), usize> {
        let mut elements = self.elements.borrow_mut();
        if index >= elements.len() {
            return Err(elements.len());
        }
        elements[index] = value;
        Ok(())
    }

    pub(crate) fn with_elements<R>(&self, f: impl FnOnce(&[Value]) -> R) -> R {
        f(&self.elements.borrow())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tuple {
    element_types: Vec<Type>,
    elements: Vec<Value>,
}

impl Tuple {
    fn new(element_types: Vec<Type>, elements: Vec<Value>) -> Self {
        Self {
            element_types,
            elements,
        }
    }

    pub fn elements(&self) -> &[Value] {
        &self.elements
    }

    pub fn element_types(&self) -> &[Type] {
        &self.element_types
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Map {
    value_type: Type,
    entries: RefCell<BTreeMap<String, Value>>,
    max_entries: Option<usize>,
}

impl Map {
    fn new(value_type: Type, entries: BTreeMap<String, Value>) -> Self {
        Self::new_with_cap(value_type, entries, None)
    }

    pub(crate) fn new_with_cap(
        value_type: Type,
        entries: BTreeMap<String, Value>,
        max_entries: Option<usize>,
    ) -> Self {
        Self {
            value_type,
            entries: RefCell::new(entries),
            max_entries,
        }
    }

    pub fn value_type(&self) -> &Type {
        &self.value_type
    }

    pub fn entries(&self) -> BTreeMap<String, Value> {
        self.entries.borrow().clone()
    }

    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.borrow().is_empty()
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        self.entries.borrow().get(key).cloned()
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.borrow().contains_key(key)
    }

    pub fn keys(&self) -> Vec<String> {
        self.entries.borrow().keys().cloned().collect()
    }

    pub fn values(&self) -> Vec<Value> {
        self.entries.borrow().values().cloned().collect()
    }

    pub fn set(&self, key: String, value: Value) {
        self.entries.borrow_mut().insert(key, value);
    }

    pub fn try_set(&self, key: String, value: Value) -> Result<(), usize> {
        let mut entries = self.entries.borrow_mut();
        if !entries.contains_key(&key) {
            if let Some(max) = self.max_entries {
                if entries.len() >= max {
                    return Err(max);
                }
            }
        }
        entries.insert(key, value);
        Ok(())
    }

    pub fn delete(&self, key: &str) -> bool {
        self.entries.borrow_mut().remove(key).is_some()
    }

    pub(crate) fn with_entries<R>(&self, f: impl FnOnce(&BTreeMap<String, Value>) -> R) -> R {
        f(&self.entries.borrow())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    name: String,
    fields: BTreeMap<String, Value>,
}

impl Record {
    fn new(name: String, fields: BTreeMap<String, Value>) -> Self {
        Self { name, fields }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn fields(&self) -> &BTreeMap<String, Value> {
        &self.fields
    }
}

pub struct Function {
    name: String,
    type_params: Vec<String>,
    params: Vec<Param>,
    return_type: Type,
    kind: FunctionKind,
}

enum FunctionKind {
    Script {
        body: BytecodeModule,
        env: Weak<RefCell<EnvData>>,
    },
    Host {
        callback: Rc<HostCallback>,
    },
}

type HostCallback = dyn Fn(&[Value]) -> Result<Value, Diagnostic>;
type ModuleLoader = dyn Fn(&str) -> Result<String, Diagnostic>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileReport {
    pub functions: BTreeMap<String, FunctionProfile>,
    pub operations: BTreeMap<String, OperationProfile>,
    pub host_callbacks: Vec<HostCallbackTraceEvent>,
    pub statements: BTreeMap<Span, StatementCoverage>,
    pub branches: BTreeMap<Span, BranchCoverage>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionProfile {
    pub call_count: u64,
    pub total_time: Duration,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperationProfile {
    pub count: u64,
    pub total_time: Duration,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatementCoverage {
    pub execution_count: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchCoverage {
    pub true_count: u64,
    pub false_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCallbackTraceEvent {
    pub name: String,
    pub phase: HostCallbackTracePhase,
    pub span: Span,
    pub elapsed: Duration,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostCallbackTracePhase {
    Enter,
    Exit,
}

impl HostCallbackTracePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enter => "enter",
            Self::Exit => "exit",
        }
    }
}

impl ProfileReport {
    pub(crate) fn record_call(&mut self, name: &str, elapsed: Duration) {
        let entry = self.functions.entry(name.to_string()).or_default();
        entry.call_count += 1;
        entry.total_time += elapsed;
    }

    pub(crate) fn record_operation(&mut self, name: &str, elapsed: Duration) {
        let entry = self.operations.entry(name.to_string()).or_default();
        entry.count += 1;
        entry.total_time += elapsed;
    }

    pub(crate) fn record_statement(&mut self, span: Span) {
        if span.start == span.end {
            return;
        }
        let entry = self.statements.entry(span).or_default();
        entry.execution_count += 1;
    }

    pub(crate) fn record_branch(&mut self, span: Span, condition_value: bool) {
        if span.start == span.end {
            return;
        }
        let entry = self.branches.entry(span).or_default();
        if condition_value {
            entry.true_count += 1;
        } else {
            entry.false_count += 1;
        }
    }

    pub(crate) fn record_host_callback(
        &mut self,
        name: &str,
        phase: HostCallbackTracePhase,
        span: Span,
        elapsed: Duration,
        status: Option<&str>,
    ) {
        self.host_callbacks.push(HostCallbackTraceEvent {
            name: name.to_string(),
            phase,
            span,
            elapsed,
            status: status.map(str::to_string),
        });
    }
}

impl Clone for FunctionKind {
    fn clone(&self) -> Self {
        match self {
            Self::Script { body, env } => Self::Script {
                body: body.clone(),
                env: env.clone(),
            },
            Self::Host { callback } => Self::Host {
                callback: callback.clone(),
            },
        }
    }
}

impl fmt::Debug for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Function")
            .field("name", &self.name)
            .field("type_params", &self.type_params)
            .field("params", &self.params)
            .field("return_type", &self.return_type)
            .finish_non_exhaustive()
    }
}

impl PartialEq for Function {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.type_params == other.type_params
            && self.params == other.params
            && self.return_type == other.return_type
    }
}

impl Function {
    fn signature_type(&self) -> Type {
        Type::Function {
            type_params: self.type_params.clone(),
            params: self.params.iter().map(|param| param.ty.clone()).collect(),
            return_type: Box::new(self.return_type.clone()),
        }
    }

    fn signature_name(&self) -> String {
        format!("{}: {}", self.name, self.signature_type())
    }
}

#[derive(Clone)]
struct HostFunction {
    function: Rc<Function>,
    docstring: Option<String>,
    capabilities: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScalarType {
    Null,
    Bool,
    Int,
    Float,
}

impl From<ScalarType> for Type {
    fn from(value: ScalarType) -> Self {
        match value {
            ScalarType::Null => Self::Null,
            ScalarType::Bool => Self::Bool,
            ScalarType::Int => Self::Int,
            ScalarType::Float => Self::Float,
        }
    }
}

pub struct HostFunctionBuilder {
    name: String,
    type_params: Vec<String>,
    params: Vec<Param>,
    return_type: Type,
    docstring: Option<String>,
    capabilities: Vec<String>,
}

impl HostFunctionBuilder {
    pub fn new(name: impl Into<String>, return_type: Type) -> Self {
        Self {
            name: name.into(),
            type_params: Vec::new(),
            params: Vec::new(),
            return_type,
            docstring: None,
            capabilities: Vec::new(),
        }
    }

    pub fn type_param(mut self, name: impl Into<String>) -> Self {
        self.type_params.push(name.into());
        self
    }

    pub fn param(mut self, name: impl Into<String>, ty: Type) -> Self {
        self.params.push(Param {
            name: name.into(),
            ty,
        });
        self
    }

    pub fn docstring(mut self, doc: impl Into<String>) -> Self {
        self.docstring = Some(doc.into());
        self
    }

    pub fn capability(mut self, capability: impl Into<String>) -> Self {
        self.capabilities.push(capability.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HostFunctionSignature {
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub docstring: Option<String>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestModuleResult {
    pub tests: Vec<TestCaseResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestCaseResult {
    pub name: String,
    pub passed: bool,
    pub diagnostic: Option<Diagnostic>,
    pub duration_us: u128,
    pub stdout: String,
    pub stderr: String,
    pub mock_events: Vec<String>,
}

#[derive(Default)]
pub struct ModuleGraph {
    cache: HashMap<String, String>,
    overlay: HashMap<String, String>,
}

impl ModuleGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    pub fn set_overlay(&mut self, specifier: impl Into<String>, source: impl Into<String>) {
        let specifier = specifier.into();
        self.cache.remove(&specifier);
        self.overlay.insert(specifier, source.into());
    }

    pub fn remove_overlay(&mut self, specifier: &str) {
        self.overlay.remove(specifier);
        self.cache.remove(specifier);
    }

    pub fn cached_source(&self, specifier: &str) -> Option<&str> {
        self.cache.get(specifier).map(String::as_str)
    }

    fn load(
        &mut self,
        specifier: &str,
        loader: Option<&Rc<ModuleLoader>>,
    ) -> Result<String, Diagnostic> {
        if let Some(source) = self.overlay.get(specifier) {
            return Ok(source.clone());
        }
        if let Some(source) = self.cache.get(specifier) {
            return Ok(source.clone());
        }
        let Some(loader) = loader else {
            return Err(Diagnostic::new(
                "module import requires a session module loader or overlay",
                Span { start: 0, end: 0 },
            ));
        };
        let source = loader(specifier)?;
        self.cache.insert(specifier.to_string(), source.clone());
        Ok(source)
    }
}

pub struct Session {
    engine: Engine,
    graph: Rc<RefCell<ModuleGraph>>,
    module_loader: Rc<RefCell<Option<Rc<ModuleLoader>>>>,
}

impl Session {
    pub fn new() -> Self {
        let graph = Rc::new(RefCell::new(ModuleGraph::new()));
        let module_loader: Rc<RefCell<Option<Rc<ModuleLoader>>>> = Rc::new(RefCell::new(None));
        let mut engine = Engine::new();
        let graph_for_loader = graph.clone();
        let loader_for_loader = module_loader.clone();
        engine.set_module_loader(move |specifier| {
            let loader = loader_for_loader.borrow();
            graph_for_loader
                .borrow_mut()
                .load(specifier, loader.as_ref())
        });
        Self {
            engine,
            graph,
            module_loader,
        }
    }

    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    pub fn module_graph(&self) -> std::cell::Ref<'_, ModuleGraph> {
        self.graph.borrow()
    }

    pub fn set_module_loader<F>(&mut self, loader: F)
    where
        F: Fn(&str) -> Result<String, Diagnostic> + 'static,
    {
        *self.module_loader.borrow_mut() = Some(Rc::new(loader));
    }

    pub fn clear_module_cache(&mut self) {
        self.graph.borrow_mut().clear_cache();
    }

    pub fn set_module_overlay(&mut self, specifier: impl Into<String>, source: impl Into<String>) {
        self.graph.borrow_mut().set_overlay(specifier, source);
    }

    pub fn remove_module_overlay(&mut self, specifier: &str) {
        self.graph.borrow_mut().remove_overlay(specifier);
    }

    pub fn eval(&mut self, source: &str) -> Result<Value, Diagnostic> {
        self.engine.eval(source)
    }

    pub fn check(&mut self, source: &str) -> Result<(), Diagnostic> {
        self.engine.check(source)
    }

    pub fn check_diagnostics(&mut self, source: &str) -> Result<(), Vec<Diagnostic>> {
        self.engine.check_diagnostics(source)
    }

    pub fn lint(&mut self, source: &str) -> Result<Vec<LintWarning>, Diagnostic> {
        self.engine.lint(source)
    }

    pub fn hover_type(
        &mut self,
        source: &str,
        byte_offset: usize,
    ) -> Result<Option<Type>, Diagnostic> {
        self.engine.hover_type(source, byte_offset)
    }

    pub fn host_function_names(&self) -> Vec<String> {
        self.engine.host_function_names()
    }

    pub fn host_function_signature(&self, name: &str) -> Option<HostFunctionSignature> {
        self.engine.host_function_signature(name)
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
pub struct Engine {
    host_functions: HashMap<String, HostFunction>,
    module_loader: Option<Rc<ModuleLoader>>,
    test_output_snapshot: Option<Rc<dyn Fn() -> (String, String)>>,
    instruction_budget: Option<usize>,
    max_call_stack_depth: Option<usize>,
    max_string_length: Option<usize>,
    max_array_length: Option<usize>,
    max_map_entries: Option<usize>,
    max_heap_objects: Option<usize>,
    heap: Rc<RefCell<GcHeap>>,
}

impl Engine {
    pub fn new() -> Self {
        let mut engine = Self::default();
        engine.install_core_intrinsics();
        engine
    }

    fn install_core_intrinsics(&mut self) {
        self.register_host_function(
            HostFunctionBuilder::new("to_float", Type::Float).param("value", Type::Int),
            |args| match args {
                [Value::Int(value)] => Ok(Value::Float(*value as f64)),
                _ => unreachable!("static checker guarantees to_float argument type"),
            },
        )
        .expect("core intrinsic registration is static");

        self.register_host_function(
            HostFunctionBuilder::new("to_int", Type::Int).param("value", Type::Float),
            |args| match args {
                [Value::Float(value)]
                    if value.is_finite()
                        && *value >= i64::MIN as f64
                        && *value <= i64::MAX as f64 =>
                {
                    Ok(Value::Int(value.trunc() as i64))
                }
                [Value::Float(_)] => Err(Diagnostic::new(
                    "to_int expects a finite float in int range",
                    Span { start: 0, end: 0 },
                )),
                _ => unreachable!("static checker guarantees to_int argument type"),
            },
        )
        .expect("core intrinsic registration is static");
    }

    pub fn register_host_function<F>(
        &mut self,
        builder: HostFunctionBuilder,
        callback: F,
    ) -> Result<(), Diagnostic>
    where
        F: Fn(&[Value]) -> Result<Value, Diagnostic> + 'static,
    {
        if builder.name.is_empty() {
            return Err(Diagnostic::new(
                "host function name cannot be empty",
                Span { start: 0, end: 0 },
            ));
        }

        let function = Rc::new(Function {
            name: builder.name.clone(),
            type_params: builder.type_params,
            params: builder.params,
            return_type: builder.return_type,
            kind: FunctionKind::Host {
                callback: Rc::new(callback),
            },
        });
        self.host_functions.insert(
            builder.name,
            HostFunction {
                function,
                docstring: builder.docstring,
                capabilities: builder.capabilities,
            },
        );
        Ok(())
    }

    pub fn host_function_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.host_functions.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn host_function_signature(&self, name: &str) -> Option<HostFunctionSignature> {
        self.host_functions.get(name).map(|entry| {
            let function = &entry.function;
            HostFunctionSignature {
                name: function.name.clone(),
                type_params: function.type_params.clone(),
                params: function
                    .params
                    .iter()
                    .map(|p| (p.name.clone(), p.ty.clone()))
                    .collect(),
                return_type: function.return_type.clone(),
                docstring: entry.docstring.clone(),
                capabilities: entry.capabilities.clone(),
            }
        })
    }

    pub fn set_module_loader<F>(&mut self, loader: F)
    where
        F: Fn(&str) -> Result<String, Diagnostic> + 'static,
    {
        self.module_loader = Some(Rc::new(loader));
    }

    pub fn set_test_output_snapshot<F>(&mut self, snapshot: F)
    where
        F: Fn() -> (String, String) + 'static,
    {
        self.test_output_snapshot = Some(Rc::new(snapshot));
    }

    pub fn set_instruction_budget(&mut self, budget: Option<usize>) {
        self.instruction_budget = budget;
    }

    pub fn set_max_call_stack_depth(&mut self, depth: Option<usize>) {
        self.max_call_stack_depth = depth;
    }

    pub fn set_max_string_length(&mut self, max: Option<usize>) {
        self.max_string_length = max;
    }

    pub fn set_max_array_length(&mut self, max: Option<usize>) {
        self.max_array_length = max;
    }

    pub fn set_max_map_entries(&mut self, max: Option<usize>) {
        self.max_map_entries = max;
    }

    pub fn set_max_heap_objects(&mut self, max: Option<usize>) {
        self.max_heap_objects = max;
    }

    pub fn collect_garbage(&mut self) {
        self.heap.borrow_mut().collect();
    }

    pub fn heap_object_count(&self) -> usize {
        self.heap.borrow().object_count()
    }

    pub fn eval(&mut self, source: &str) -> Result<Value, Diagnostic> {
        let bytecode = self.compile_source(source)?;
        self.execute(&bytecode)
    }

    pub fn profile(&mut self, source: &str) -> Result<(Value, ProfileReport), Diagnostic> {
        let bytecode = self.compile_source(source)?;
        self.execute_profiled(&bytecode)
    }

    pub fn run_tests(&mut self, source: &str) -> Result<TestModuleResult, Diagnostic> {
        let tokens = lex(source)?;
        let module = parse(tokens)?;
        let test_functions = collect_test_functions(&module)?;
        let lifecycle = collect_lifecycle_functions(&module)?;
        let module = self.resolve_imports(module)?.into_flat_module();
        TypeChecker::new_with_hosts(&self.host_functions).check_module(&module)?;
        let bytecode = compile(&module);
        verify(&bytecode)?;

        let env = self.root_env();
        let mut vm = Vm::new(env.clone(), self.instruction_budget, self.heap.clone());
        vm.set_max_call_depth(self.max_call_stack_depth);
        vm.set_max_string_length(self.max_string_length);
        vm.set_max_array_length(self.max_array_length);
        vm.set_max_map_entries(self.max_map_entries);
        vm.set_max_heap_objects(self.max_heap_objects);
        match vm.execute(&bytecode)? {
            Control::Value(_) | Control::Return(_) => {}
        }

        let mut tests = Vec::with_capacity(test_functions.len());
        for test in test_functions {
            let test_start = std::time::Instant::now();
            let output_before = self.snapshot_test_output();
            if let Some(before_each_name) = &lifecycle.before_each {
                if let Some(before_each) = env.get(before_each_name) {
                    if let Err(diagnostic) = vm.call_value(test.span, before_each, Vec::new()) {
                        let (stdout, stderr) = self.test_output_delta(output_before);
                        tests.push(TestCaseResult {
                            name: test.name.clone(),
                            passed: false,
                            diagnostic: Some(diagnostic),
                            duration_us: test_start.elapsed().as_micros(),
                            stdout,
                            stderr,
                            mock_events: Vec::new(),
                        });
                        continue;
                    }
                }
            }
            let Some(callee) = env.get(&test.name) else {
                return Err(Diagnostic::new(
                    format!("test function '{}' is not available", test.name),
                    test.span,
                ));
            };
            let mut result = match vm.call_value(test.span, callee, Vec::new()) {
                Ok(Value::Bool(passed)) => TestCaseResult {
                    name: test.name.clone(),
                    passed,
                    diagnostic: None,
                    duration_us: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    mock_events: Vec::new(),
                },
                Ok(Value::Null) => TestCaseResult {
                    name: test.name.clone(),
                    passed: true,
                    diagnostic: None,
                    duration_us: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    mock_events: Vec::new(),
                },
                Ok(value) => TestCaseResult {
                    name: test.name.clone(),
                    passed: false,
                    diagnostic: Some(Diagnostic::new(
                        format!(
                            "test returned {}, expected bool or null",
                            value_type(&value)
                        ),
                        test.span,
                    )),
                    duration_us: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    mock_events: Vec::new(),
                },
                Err(diagnostic) => TestCaseResult {
                    name: test.name.clone(),
                    passed: false,
                    diagnostic: Some(diagnostic),
                    duration_us: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    mock_events: Vec::new(),
                },
            };
            if let Some(after_each_name) = &lifecycle.after_each {
                if let Some(after_each) = env.get(after_each_name) {
                    if let Err(diagnostic) = vm.call_value(test.span, after_each, Vec::new()) {
                        if result.passed {
                            result.passed = false;
                            result.diagnostic = Some(diagnostic);
                        }
                    }
                }
            }
            result.duration_us = test_start.elapsed().as_micros();
            let (stdout, stderr) = self.test_output_delta(output_before);
            result.stdout = stdout;
            result.stderr = stderr;
            tests.push(result);
        }

        Ok(TestModuleResult { tests })
    }

    fn snapshot_test_output(&self) -> (String, String) {
        self.test_output_snapshot
            .as_ref()
            .map(|snapshot| snapshot())
            .unwrap_or_else(|| (String::new(), String::new()))
    }

    fn test_output_delta(&self, before: (String, String)) -> (String, String) {
        let after = self.snapshot_test_output();
        (
            after
                .0
                .strip_prefix(&before.0)
                .unwrap_or(after.0.as_str())
                .to_string(),
            after
                .1
                .strip_prefix(&before.1)
                .unwrap_or(after.1.as_str())
                .to_string(),
        )
    }

    pub fn check(&mut self, source: &str) -> Result<(), Diagnostic> {
        self.compile_source(source).map(|_| ())
    }

    pub fn check_diagnostics(&mut self, source: &str) -> Result<(), Vec<Diagnostic>> {
        let tokens = lex(source).map_err(|err| vec![err])?;
        let module = parse_all(tokens)?;
        let module = self
            .resolve_imports(module)
            .map(|resolved| resolved.into_flat_module())
            .map_err(|err| vec![err])?;
        TypeChecker::new_with_hosts(&self.host_functions).check_module_all(&module)?;
        let bytecode = compile(&module);
        verify(&bytecode).map_err(|err| vec![err])
    }

    pub fn lint(&mut self, source: &str) -> Result<Vec<LintWarning>, Diagnostic> {
        let tokens = lex(source)?;
        let module = parse(tokens)?;
        Ok(collect_lint_warnings(&module))
    }

    pub fn inspect_bytecode(&mut self, source: &str) -> Result<String, Diagnostic> {
        let bytecode = self.compile_source(source)?;
        Ok(format!("{bytecode:#?}"))
    }

    pub fn inspect_bytecode_compact(&mut self, source: &str) -> Result<String, Diagnostic> {
        let bytecode = self.compile_source(source)?;
        Ok(bytecode.format_compact())
    }

    pub fn format_source(&self, source: &str) -> Result<String, Diagnostic> {
        let tokens = lex(source)?;
        let module = parse(tokens)?;
        Ok(format_module(&module))
    }

    pub fn hover_type(
        &mut self,
        source: &str,
        byte_offset: usize,
    ) -> Result<Option<Type>, Diagnostic> {
        let tokens = lex(source)?;
        let module = parse(tokens)?;
        let module = self.resolve_imports(module)?.into_flat_module();
        TypeChecker::new_hover(&self.host_functions, byte_offset).hover_type(&module)
    }

    fn compile_source(&mut self, source: &str) -> Result<BytecodeModule, Diagnostic> {
        let tokens = lex(source)?;
        let module = parse(tokens)?;
        let module = self.resolve_imports(module)?.into_flat_module();
        TypeChecker::new_with_hosts(&self.host_functions).check_module(&module)?;
        let bytecode = compile(&module);
        verify(&bytecode)?;
        Ok(bytecode)
    }

    fn resolve_imports(&self, module: Module) -> Result<ResolvedModule, Diagnostic> {
        let mut loaded = Vec::new();
        let mut loading = Vec::new();
        let mut imports = Vec::new();
        let mut namespace_surfaces = HashMap::new();
        let entry = self.resolve_imports_inner(
            module,
            &mut loaded,
            &mut loading,
            &mut imports,
            &mut namespace_surfaces,
        )?;
        Ok(ResolvedModule { imports, entry })
    }

    fn resolve_imports_inner(
        &self,
        module: Module,
        loaded: &mut Vec<String>,
        loading: &mut Vec<String>,
        imports: &mut Vec<ModuleUnit>,
        namespace_surfaces: &mut HashMap<String, HashMap<String, String>>,
    ) -> Result<Module, Diagnostic> {
        let top_level_names = module
            .statements
            .iter()
            .filter_map(top_level_declaration_name)
            .map(str::to_string)
            .collect::<HashSet<_>>();
        let mut namespaces = HashMap::new();
        let mut statements = Vec::new();
        for statement in module.statements {
            let Stmt::Import { path, alias, span } = statement else {
                statements.push(statement);
                continue;
            };

            if let Some(alias) = &alias {
                if top_level_names.contains(alias) || namespaces.contains_key(alias) {
                    return Err(Diagnostic::new(
                        format!(
                            "module namespace '{alias}' conflicts with an existing declaration"
                        ),
                        span,
                    )
                    .with_code("module.name-conflict"));
                }
            }

            if loaded.iter().any(|loaded_path| loaded_path == &path) {
                if let Some(alias) = alias {
                    if let Some(members) = namespace_surfaces.get(&path) {
                        namespaces.insert(
                            alias,
                            NamespaceImport {
                                specifier: path,
                                members: members.clone(),
                            },
                        );
                    }
                }
                continue;
            }
            if loading.iter().any(|loading_path| loading_path == &path) {
                return Err(Diagnostic::new(
                    format!("cyclic import detected for '{path}'"),
                    span,
                ));
            }
            let Some(loader) = &self.module_loader else {
                return Err(Diagnostic::new("module import requires a loader", span));
            };
            loading.push(path.clone());
            let source = loader(&path)?;
            let imported = parse(lex(&source)?)?;
            let imported =
                self.resolve_imports_inner(imported, loaded, loading, imports, namespace_surfaces)?;
            let members = module_export_members(&imported);
            let imported = if alias.is_some() {
                namespace_module_surface(&path, imported)?.0
            } else {
                imported_module_surface(&path, imported)?
            };
            loading.pop();
            loaded.push(path.clone());
            let member_targets = members
                .into_iter()
                .map(|member| {
                    let target = if alias.is_some() {
                        internal_import_name(&path, &member)
                    } else {
                        member.clone()
                    };
                    (member, target)
                })
                .collect::<HashMap<_, _>>();
            namespace_surfaces.insert(path.clone(), member_targets.clone());
            imports.push(ModuleUnit {
                specifier: path.clone(),
                module: imported,
            });
            if let Some(alias) = alias {
                namespaces.insert(
                    alias,
                    NamespaceImport {
                        specifier: path,
                        members: member_targets,
                    },
                );
            }
        }
        NamespaceRewriter::new(namespaces).rewrite_module(Module { statements })
    }

    fn execute(&mut self, module: &BytecodeModule) -> Result<Value, Diagnostic> {
        let env = self.root_env();
        let mut vm = Vm::new(env, self.instruction_budget, self.heap.clone());
        vm.set_max_call_depth(self.max_call_stack_depth);
        vm.set_max_string_length(self.max_string_length);
        vm.set_max_array_length(self.max_array_length);
        vm.set_max_map_entries(self.max_map_entries);
        vm.set_max_heap_objects(self.max_heap_objects);
        match vm.execute(module)? {
            Control::Value(value) => Ok(value),
            Control::Return(value) => Ok(value),
        }
    }

    fn execute_profiled(
        &mut self,
        module: &BytecodeModule,
    ) -> Result<(Value, ProfileReport), Diagnostic> {
        let env = self.root_env();
        let profile = Rc::new(RefCell::new(ProfileReport::default()));
        let mut vm = Vm::new_profiled(
            env,
            self.instruction_budget,
            self.heap.clone(),
            profile.clone(),
        );
        vm.set_max_call_depth(self.max_call_stack_depth);
        vm.set_max_string_length(self.max_string_length);
        vm.set_max_array_length(self.max_array_length);
        vm.set_max_map_entries(self.max_map_entries);
        vm.set_max_heap_objects(self.max_heap_objects);
        let start = std::time::Instant::now();
        let value = match vm.execute(module)? {
            Control::Value(value) | Control::Return(value) => value,
        };
        profile
            .borrow_mut()
            .record_call("<module>", start.elapsed());
        let report = profile.borrow().clone();
        Ok((value, report))
    }

    fn root_env(&self) -> Env {
        let env = Env::new();
        for (name, host) in &self.host_functions {
            env.define(name.clone(), Value::Function(host.function.clone()));
        }
        env
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintWarning {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

pub(crate) fn collect_lint_warnings(module: &Module) -> Vec<LintWarning> {
    let mut warnings = Vec::new();
    let mut declared: Vec<(String, Span, &'static str)> = Vec::new();
    for statement in &module.statements {
        match statement {
            Stmt::Function {
                name,
                exported,
                span,
                ..
            } if !exported
                && name != "main"
                && !name.starts_with("test_")
                && name != "before_each"
                && name != "after_each" =>
            {
                declared.push((name.clone(), *span, "lint.unused-function"));
            }
            Stmt::Function { .. } => {}
            Stmt::Let {
                target,
                exported,
                span,
                ..
            } => {
                if *exported {
                    continue;
                }
                if let Some(name) = target.single_name() {
                    if !name.starts_with('_') {
                        declared.push((name.to_string(), *span, "lint.unused-variable"));
                    }
                }
            }
            Stmt::Import {
                alias: Some(alias),
                span,
                ..
            } => {
                declared.push((alias.clone(), *span, "lint.unused-import"));
            }
            _ => {}
        }
    }

    let mut references: HashSet<String> = HashSet::new();
    for statement in &module.statements {
        collect_references_in_stmt(statement, &mut references);
    }

    for (name, span, code) in declared {
        if references.contains(&name) {
            continue;
        }
        let label = match code {
            "lint.unused-function" => format!("function '{name}' is declared but never used"),
            "lint.unused-import" => format!("import alias '{name}' is declared but never used"),
            _ => format!("variable '{name}' is declared but never used"),
        };
        warnings.push(LintWarning {
            code,
            message: label,
            span,
        });
    }

    for statement in &module.statements {
        collect_unreachable_in_stmt(statement, &mut warnings);
    }

    let mut shadow_scopes: Vec<HashSet<String>> = vec![HashSet::new()];
    for statement in &module.statements {
        collect_shadowing_in_stmt(statement, &mut shadow_scopes, &mut warnings);
    }

    for statement in &module.statements {
        collect_constant_condition_in_stmt(statement, &mut warnings);
    }

    for statement in &module.statements {
        collect_duplicate_match_arms_in_stmt(statement, &mut warnings);
    }

    warnings.sort_by_key(|w| (w.span.start, w.code));
    warnings
}

fn collect_duplicate_match_arms_in_stmt(statement: &Stmt, warnings: &mut Vec<LintWarning>) {
    match statement {
        Stmt::Function { body, .. } => {
            for stmt in body {
                collect_duplicate_match_arms_in_stmt(stmt, warnings);
            }
        }
        Stmt::If {
            then_branch,
            else_branch,
            ..
        }
        | Stmt::IfLet {
            then_branch,
            else_branch,
            ..
        } => {
            for stmt in then_branch {
                collect_duplicate_match_arms_in_stmt(stmt, warnings);
            }
            for stmt in else_branch {
                collect_duplicate_match_arms_in_stmt(stmt, warnings);
            }
        }
        Stmt::While { body, .. } | Stmt::WhileLet { body, .. } | Stmt::For { body, .. } => {
            for stmt in body {
                collect_duplicate_match_arms_in_stmt(stmt, warnings);
            }
        }
        Stmt::LetElse { else_branch, .. } => {
            for stmt in else_branch {
                collect_duplicate_match_arms_in_stmt(stmt, warnings);
            }
        }
        Stmt::Block { statements, .. } => {
            for stmt in statements {
                collect_duplicate_match_arms_in_stmt(stmt, warnings);
            }
        }
        Stmt::Match { cases, default, .. } => {
            let mut seen_bind_or_wildcard = false;
            let mut seen_patterns: Vec<&MatchCaseValue> = Vec::new();
            for case in cases {
                if seen_bind_or_wildcard {
                    warnings.push(LintWarning {
                        code: "lint.duplicate-match-arm",
                        message:
                            "match arm is unreachable because an earlier arm binds every value"
                                .to_string(),
                        span: case.span,
                    });
                } else if seen_patterns
                    .iter()
                    .any(|p| pattern_equal(p, &case.pattern))
                {
                    warnings.push(LintWarning {
                        code: "lint.duplicate-match-arm",
                        message: "match arm pattern duplicates an earlier arm".to_string(),
                        span: case.span,
                    });
                }
                if matches!(case.pattern, MatchCaseValue::Bind(_)) {
                    seen_bind_or_wildcard = true;
                } else {
                    seen_patterns.push(&case.pattern);
                }
                for stmt in &case.body {
                    collect_duplicate_match_arms_in_stmt(stmt, warnings);
                }
            }
            if let Some(default) = default {
                for stmt in default {
                    collect_duplicate_match_arms_in_stmt(stmt, warnings);
                }
            }
        }
        Stmt::Let { initializer, .. } => {
            collect_duplicate_match_arms_in_expr(initializer, warnings);
        }
        Stmt::Return { value, .. } => {
            collect_duplicate_match_arms_in_expr(value, warnings);
        }
        Stmt::Expression { expression, .. } => {
            collect_duplicate_match_arms_in_expr(expression, warnings);
        }
        _ => {}
    }
}

fn collect_duplicate_match_arms_in_expr(expr: &Expr, warnings: &mut Vec<LintWarning>) {
    if let ExprKind::FunctionLiteral { body, .. } = &expr.kind {
        for stmt in body {
            collect_duplicate_match_arms_in_stmt(stmt, warnings);
        }
    }
}

fn pattern_equal(a: &MatchCaseValue, b: &MatchCaseValue) -> bool {
    match (a, b) {
        (MatchCaseValue::Int(l), MatchCaseValue::Int(r)) => l == r,
        (MatchCaseValue::Float(l), MatchCaseValue::Float(r)) => l == r,
        (MatchCaseValue::Str(l), MatchCaseValue::Str(r)) => l == r,
        (
            MatchCaseValue::IntRange { start: ls, end: le },
            MatchCaseValue::IntRange { start: rs, end: re },
        ) => ls == rs && le == re,
        (MatchCaseValue::None, MatchCaseValue::None) => true,
        (MatchCaseValue::Some(l), MatchCaseValue::Some(r)) => pattern_equal(l, r),
        (MatchCaseValue::Ok(l), MatchCaseValue::Ok(r)) => pattern_equal(l, r),
        (MatchCaseValue::Err(l), MatchCaseValue::Err(r)) => pattern_equal(l, r),
        (
            MatchCaseValue::EnumVariant {
                name: ln,
                payload: lp,
            },
            MatchCaseValue::EnumVariant {
                name: rn,
                payload: rp,
            },
        ) => {
            ln == rn
                && match (lp, rp) {
                    (None, None) => true,
                    (Some(l), Some(r)) => pattern_equal(l, r),
                    _ => false,
                }
        }
        _ => false,
    }
}

fn constant_bool_value(expr: &Expr) -> Option<bool> {
    match &expr.kind {
        ExprKind::Literal(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn collect_constant_condition_in_stmt(statement: &Stmt, warnings: &mut Vec<LintWarning>) {
    match statement {
        Stmt::Function { body, .. } => {
            for stmt in body {
                collect_constant_condition_in_stmt(stmt, warnings);
            }
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            span,
        } => {
            if let Some(value) = constant_bool_value(condition) {
                warnings.push(LintWarning {
                    code: "lint.constant-condition",
                    message: format!(
                        "condition is always {value}; the branch never changes outcome"
                    ),
                    span: *span,
                });
            }
            for stmt in then_branch {
                collect_constant_condition_in_stmt(stmt, warnings);
            }
            for stmt in else_branch {
                collect_constant_condition_in_stmt(stmt, warnings);
            }
        }
        Stmt::IfLet {
            then_branch,
            else_branch,
            ..
        } => {
            for stmt in then_branch {
                collect_constant_condition_in_stmt(stmt, warnings);
            }
            for stmt in else_branch {
                collect_constant_condition_in_stmt(stmt, warnings);
            }
        }
        Stmt::While {
            condition,
            body,
            span,
        } => {
            if let Some(false) = constant_bool_value(condition) {
                warnings.push(LintWarning {
                    code: "lint.constant-condition",
                    message: "while condition is always false; body never executes".to_string(),
                    span: *span,
                });
            }
            for stmt in body {
                collect_constant_condition_in_stmt(stmt, warnings);
            }
        }
        Stmt::WhileLet { body, .. } | Stmt::For { body, .. } => {
            for stmt in body {
                collect_constant_condition_in_stmt(stmt, warnings);
            }
        }
        Stmt::Match { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_constant_condition_in_stmt(stmt, warnings);
                }
            }
            if let Some(default) = default {
                for stmt in default {
                    collect_constant_condition_in_stmt(stmt, warnings);
                }
            }
        }
        Stmt::Block { statements, .. } => {
            for stmt in statements {
                collect_constant_condition_in_stmt(stmt, warnings);
            }
        }
        Stmt::LetElse { else_branch, .. } => {
            for stmt in else_branch {
                collect_constant_condition_in_stmt(stmt, warnings);
            }
        }
        Stmt::Let { initializer, .. } => {
            collect_constant_condition_in_expr(initializer, warnings);
        }
        Stmt::Return { value, .. } => {
            collect_constant_condition_in_expr(value, warnings);
        }
        Stmt::Expression { expression, .. } => {
            collect_constant_condition_in_expr(expression, warnings);
        }
        _ => {}
    }
}

fn collect_constant_condition_in_expr(expr: &Expr, warnings: &mut Vec<LintWarning>) {
    if let ExprKind::FunctionLiteral { body, .. } = &expr.kind {
        for stmt in body {
            collect_constant_condition_in_stmt(stmt, warnings);
        }
    }
}

fn collect_shadowing_in_block(
    statements: &[Stmt],
    scopes: &mut Vec<HashSet<String>>,
    warnings: &mut Vec<LintWarning>,
) {
    scopes.push(HashSet::new());
    for stmt in statements {
        collect_shadowing_in_stmt(stmt, scopes, warnings);
    }
    scopes.pop();
}

fn collect_shadowing_in_stmt(
    statement: &Stmt,
    scopes: &mut Vec<HashSet<String>>,
    warnings: &mut Vec<LintWarning>,
) {
    match statement {
        Stmt::Let {
            target,
            initializer,
            span,
            ..
        } => {
            collect_shadowing_in_expr(initializer, scopes, warnings);
            if let Some(name) = target.single_name() {
                if !name.starts_with('_') {
                    let outer_match = scopes.iter().rev().skip(1).any(|s| s.contains(name));
                    if outer_match {
                        warnings.push(LintWarning {
                            code: "lint.shadowed-variable",
                            message: format!(
                                "variable '{name}' shadows an outer binding with the same name"
                            ),
                            span: *span,
                        });
                    }
                }
                if let Some(current) = scopes.last_mut() {
                    current.insert(name.to_string());
                }
            }
        }
        Stmt::Function { params, body, .. } => {
            scopes.push(HashSet::new());
            if let Some(current) = scopes.last_mut() {
                for param in params {
                    current.insert(param.name.clone());
                }
            }
            for stmt in body {
                collect_shadowing_in_stmt(stmt, scopes, warnings);
            }
            scopes.pop();
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_shadowing_in_expr(condition, scopes, warnings);
            collect_shadowing_in_block(then_branch, scopes, warnings);
            collect_shadowing_in_block(else_branch, scopes, warnings);
        }
        Stmt::IfLet {
            value,
            then_branch,
            else_branch,
            ..
        } => {
            collect_shadowing_in_expr(value, scopes, warnings);
            collect_shadowing_in_block(then_branch, scopes, warnings);
            collect_shadowing_in_block(else_branch, scopes, warnings);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_shadowing_in_expr(condition, scopes, warnings);
            collect_shadowing_in_block(body, scopes, warnings);
        }
        Stmt::WhileLet { value, body, .. } => {
            collect_shadowing_in_expr(value, scopes, warnings);
            collect_shadowing_in_block(body, scopes, warnings);
        }
        Stmt::For {
            name,
            start,
            end,
            body,
            ..
        } => {
            collect_shadowing_in_expr(start, scopes, warnings);
            collect_shadowing_in_expr(end, scopes, warnings);
            scopes.push(HashSet::new());
            if let Some(current) = scopes.last_mut() {
                current.insert(name.clone());
            }
            for stmt in body {
                collect_shadowing_in_stmt(stmt, scopes, warnings);
            }
            scopes.pop();
        }
        Stmt::Block { statements, .. } => collect_shadowing_in_block(statements, scopes, warnings),
        Stmt::Match {
            value,
            cases,
            default,
            ..
        } => {
            collect_shadowing_in_expr(value, scopes, warnings);
            for case in cases {
                collect_shadowing_in_block(&case.body, scopes, warnings);
            }
            if let Some(default) = default {
                collect_shadowing_in_block(default, scopes, warnings);
            }
        }
        Stmt::LetElse {
            value, else_branch, ..
        } => {
            collect_shadowing_in_expr(value, scopes, warnings);
            collect_shadowing_in_block(else_branch, scopes, warnings);
        }
        Stmt::Return { value, .. } => collect_shadowing_in_expr(value, scopes, warnings),
        Stmt::Expression { expression, .. } => {
            collect_shadowing_in_expr(expression, scopes, warnings);
        }
        _ => {}
    }
}

fn collect_shadowing_in_expr(
    expr: &Expr,
    scopes: &mut Vec<HashSet<String>>,
    warnings: &mut Vec<LintWarning>,
) {
    if let ExprKind::FunctionLiteral { params, body, .. } = &expr.kind {
        scopes.push(HashSet::new());
        if let Some(current) = scopes.last_mut() {
            for param in params {
                current.insert(param.name.clone());
            }
        }
        for stmt in body {
            collect_shadowing_in_stmt(stmt, scopes, warnings);
        }
        scopes.pop();
    }
}

fn collect_unreachable_in_block(statements: &[Stmt], warnings: &mut Vec<LintWarning>) {
    let mut terminated_at: Option<usize> = None;
    for (idx, stmt) in statements.iter().enumerate() {
        if terminated_at.is_none() && statement_is_terminator(stmt) {
            terminated_at = Some(idx);
        }
        collect_unreachable_in_stmt(stmt, warnings);
    }
    if let Some(idx) = terminated_at {
        if let Some(next) = statements.get(idx + 1) {
            let span = stmt_span(next);
            warnings.push(LintWarning {
                code: "lint.unreachable-code",
                message: "statement is unreachable after preceding return/break/continue"
                    .to_string(),
                span,
            });
        }
    }
}

fn collect_unreachable_in_stmt(statement: &Stmt, warnings: &mut Vec<LintWarning>) {
    match statement {
        Stmt::Function { body, .. } => collect_unreachable_in_block(body, warnings),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        }
        | Stmt::IfLet {
            then_branch,
            else_branch,
            ..
        } => {
            collect_unreachable_in_block(then_branch, warnings);
            collect_unreachable_in_block(else_branch, warnings);
        }
        Stmt::While { body, .. } | Stmt::WhileLet { body, .. } | Stmt::For { body, .. } => {
            collect_unreachable_in_block(body, warnings);
        }
        Stmt::Block { statements, .. } => collect_unreachable_in_block(statements, warnings),
        Stmt::LetElse { else_branch, .. } => {
            collect_unreachable_in_block(else_branch, warnings);
        }
        Stmt::Match { cases, default, .. } => {
            for case in cases {
                collect_unreachable_in_block(&case.body, warnings);
            }
            if let Some(default) = default {
                collect_unreachable_in_block(default, warnings);
            }
        }
        Stmt::Let { initializer, .. } => collect_unreachable_in_expr(initializer, warnings),
        Stmt::Return { value, .. } => collect_unreachable_in_expr(value, warnings),
        Stmt::Expression { expression, .. } => collect_unreachable_in_expr(expression, warnings),
        _ => {}
    }
}

fn collect_unreachable_in_expr(expr: &Expr, warnings: &mut Vec<LintWarning>) {
    if let ExprKind::FunctionLiteral { body, .. } = &expr.kind {
        collect_unreachable_in_block(body, warnings);
    }
}

fn statement_is_terminator(statement: &Stmt) -> bool {
    matches!(
        statement,
        Stmt::Return { .. } | Stmt::Break { .. } | Stmt::Continue { .. }
    )
}

fn stmt_span(statement: &Stmt) -> Span {
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
        | Stmt::Break { span }
        | Stmt::Continue { span }
        | Stmt::Expression { span, .. } => *span,
    }
}

fn collect_references_in_stmt(statement: &Stmt, references: &mut HashSet<String>) {
    match statement {
        Stmt::Let { initializer, .. } => collect_references_in_expr(initializer, references),
        Stmt::Function { body, .. } => {
            for stmt in body {
                collect_references_in_stmt(stmt, references);
            }
        }
        Stmt::Return { value, .. } => collect_references_in_expr(value, references),
        Stmt::Expression { expression, .. } => collect_references_in_expr(expression, references),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_references_in_expr(condition, references);
            for stmt in then_branch {
                collect_references_in_stmt(stmt, references);
            }
            for stmt in else_branch {
                collect_references_in_stmt(stmt, references);
            }
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_references_in_expr(condition, references);
            for stmt in body {
                collect_references_in_stmt(stmt, references);
            }
        }
        _ => {}
    }
}

fn collect_references_in_expr(expr: &Expr, references: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Variable(name) => {
            references.insert(name.clone());
        }
        ExprKind::Assign { name, value } => {
            references.insert(name.clone());
            collect_references_in_expr(value, references);
        }
        ExprKind::Unary { right, .. } => collect_references_in_expr(right, references),
        ExprKind::Binary { left, right, .. } => {
            collect_references_in_expr(left, references);
            collect_references_in_expr(right, references);
        }
        ExprKind::Call { callee, args, .. } => {
            collect_references_in_expr(callee, references);
            for arg in args {
                collect_references_in_expr(arg, references);
            }
        }
        ExprKind::Field { receiver, .. } => collect_references_in_expr(receiver, references),
        ExprKind::Index { array, index } => {
            collect_references_in_expr(array, references);
            collect_references_in_expr(index, references);
        }
        ExprKind::IndexAssign {
            container,
            index,
            value,
        } => {
            collect_references_in_expr(container, references);
            collect_references_in_expr(index, references);
            collect_references_in_expr(value, references);
        }
        ExprKind::ArrayLiteral { elements } => {
            for element in elements {
                match element {
                    ArrayElement::Expr(e) | ArrayElement::Spread(e) => {
                        collect_references_in_expr(e, references);
                    }
                }
            }
        }
        ExprKind::TupleLiteral { elements } => {
            for e in elements {
                collect_references_in_expr(e, references);
            }
        }
        ExprKind::MapLiteral { entries } => {
            for entry in entries {
                match entry {
                    MapEntry::Entry { key, value } => {
                        collect_references_in_expr(key, references);
                        collect_references_in_expr(value, references);
                    }
                    MapEntry::Spread(e) => collect_references_in_expr(e, references),
                }
            }
        }
        ExprKind::RecordLiteral { fields, .. } => {
            for (_, value, _) in fields {
                collect_references_in_expr(value, references);
            }
        }
        ExprKind::Question { value } => collect_references_in_expr(value, references),
        ExprKind::StringInterpolation(parts) => {
            for part in parts {
                if let Some(expr) = &part.expression {
                    collect_references_in_expr(expr, references);
                }
            }
        }
        ExprKind::FunctionLiteral { body, .. } => {
            for stmt in body {
                collect_references_in_stmt(stmt, references);
            }
        }
        ExprKind::Literal(_) => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestFunction {
    name: String,
    span: Span,
}

#[derive(Debug, Default)]
struct LifecycleHooks {
    before_each: Option<String>,
    after_each: Option<String>,
}

fn collect_lifecycle_functions(module: &Module) -> Result<LifecycleHooks, Diagnostic> {
    let mut hooks = LifecycleHooks::default();
    for statement in &module.statements {
        let Stmt::Function {
            name,
            params,
            return_type,
            span,
            ..
        } = statement
        else {
            continue;
        };
        let role = match name.as_str() {
            "before_each" => &mut hooks.before_each,
            "after_each" => &mut hooks.after_each,
            _ => continue,
        };
        if !params.is_empty() {
            return Err(Diagnostic::new(
                format!("'{name}' lifecycle hook must not take parameters"),
                *span,
            )
            .with_code("test.signature"));
        }
        if !matches!(return_type, Type::Null | Type::Bool) {
            return Err(Diagnostic::new(
                format!("'{name}' lifecycle hook must return null or bool"),
                *span,
            )
            .with_code("test.signature"));
        }
        if role.is_some() {
            return Err(Diagnostic::new(
                format!("'{name}' lifecycle hook is declared more than once"),
                *span,
            )
            .with_code("test.signature"));
        }
        *role = Some(name.clone());
    }
    Ok(hooks)
}

fn collect_test_functions(module: &Module) -> Result<Vec<TestFunction>, Diagnostic> {
    let mut tests = Vec::new();
    for statement in &module.statements {
        let Stmt::Function {
            name,
            params,
            return_type,
            span,
            ..
        } = statement
        else {
            continue;
        };
        if !name.starts_with("test_") {
            continue;
        }
        if !params.is_empty() {
            return Err(Diagnostic::new(
                format!("test function '{name}' must not take parameters"),
                *span,
            )
            .with_code("test.signature"));
        }
        if !matches!(return_type, Type::Bool | Type::Null) {
            return Err(Diagnostic::new(
                format!("test function '{name}' must return bool or null"),
                *span,
            )
            .with_code("test.signature"));
        }
        tests.push(TestFunction {
            name: name.clone(),
            span: *span,
        });
    }
    Ok(tests)
}

pub(crate) fn compile(module: &Module) -> BytecodeModule {
    Compiler::compile_module(module)
}

fn format_module(module: &Module) -> String {
    let mut formatter = Formatter::default();
    for (index, statement) in module.statements.iter().enumerate() {
        if index > 0 {
            formatter.output.push('\n');
        }
        formatter.format_statement(statement);
    }
    formatter.output
}

#[derive(Default)]
struct Formatter {
    output: String,
    indent: usize,
}

impl Formatter {
    fn format_statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Import { path, alias, .. } => {
                if let Some(alias) = alias {
                    self.line(&format!("import \"{}\" as {alias};", escape_string(path)));
                } else {
                    self.line(&format!("import \"{}\";", escape_string(path)));
                }
            }
            Stmt::Let {
                target,
                ty,
                initializer,
                exported,
                is_const,
                ..
            } => {
                let keyword = if *is_const { "const" } else { "let" };
                let target = format_binding_target(target, ty.as_ref());
                self.line(&format!(
                    "{}{keyword} {target} = {};",
                    export_prefix(*exported),
                    format_expr(initializer)
                ));
            }
            Stmt::TypeAlias {
                name, ty, exported, ..
            } => {
                self.line(&format!("{}type {name} = {ty};", export_prefix(*exported)));
            }
            Stmt::Enum {
                name,
                variants,
                exported,
                ..
            } => {
                self.line(&format!("{}enum {name} {{", export_prefix(*exported)));
                self.indented(|formatter| {
                    for variant in variants {
                        if let Some(payload) = &variant.payload {
                            formatter.line(&format!("{}({payload}),", variant.name));
                        } else {
                            formatter.line(&format!("{},", variant.name));
                        }
                    }
                });
                self.line("}");
            }
            Stmt::Function {
                name,
                params,
                return_type,
                body,
                exported,
                ..
            } => {
                let params = params
                    .iter()
                    .map(|param| format!("{}: {}", param.name, param.ty))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.line(&format!(
                    "{}fn {name}({params}) -> {return_type} {{",
                    export_prefix(*exported)
                ));
                self.indented(|formatter| formatter.format_block_body(body));
                self.line("}");
            }
            Stmt::Record {
                name,
                fields,
                exported,
                ..
            } => {
                self.line(&format!("{}record {name} {{", export_prefix(*exported)));
                self.indented(|formatter| {
                    for field in fields {
                        formatter.line(&format!("{}: {},", field.name, field.ty));
                    }
                });
                self.line("}");
            }
            Stmt::Return { value, .. } => {
                self.line(&format!("return {};", format_expr(value)));
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.format_if_statement(condition, then_branch, else_branch),
            Stmt::IfLet {
                pattern,
                value,
                then_branch,
                else_branch,
                ..
            } => self.format_if_let_statement(pattern, value, then_branch, else_branch),
            Stmt::Match {
                value,
                cases,
                default,
                ..
            } => {
                self.line(&format!("match ({}) {{", format_expr(value)));
                self.indented(|formatter| {
                    for case in cases {
                        formatter.line(&format!("{} => {{", format_match_case(&case.pattern)));
                        formatter.indented(|formatter| formatter.format_block_body(&case.body));
                        formatter.line("}");
                    }
                    if let Some(default) = default {
                        formatter.line("_ => {");
                        formatter.indented(|formatter| formatter.format_block_body(default));
                        formatter.line("}");
                    }
                });
                self.line("}");
            }
            Stmt::LetElse {
                pattern,
                value,
                else_branch,
                ..
            } => {
                self.line(&format!(
                    "let {} = {} else {{",
                    format_match_case(pattern),
                    format_expr(value)
                ));
                self.indented(|formatter| formatter.format_block_body(else_branch));
                self.line("};");
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.line(&format!("while ({}) {{", format_expr(condition)));
                self.indented(|formatter| formatter.format_block_body(body));
                self.line("}");
            }
            Stmt::WhileLet {
                pattern,
                value,
                body,
                ..
            } => {
                self.line(&format!(
                    "while let {} = {} {{",
                    format_match_case(pattern),
                    format_expr(value)
                ));
                self.indented(|formatter| formatter.format_block_body(body));
                self.line("}");
            }
            Stmt::For {
                name,
                start,
                end,
                body,
                ..
            } => {
                self.line(&format!(
                    "for {name} in {}..{} {{",
                    format_expr(start),
                    format_expr(end)
                ));
                self.indented(|formatter| formatter.format_block_body(body));
                self.line("}");
            }
            Stmt::Block { statements, .. } => {
                self.line("{");
                self.indented(|formatter| formatter.format_block_body(statements));
                self.line("}");
            }
            Stmt::Break { .. } => self.line("break;"),
            Stmt::Continue { .. } => self.line("continue;"),
            Stmt::Expression { expression, .. } => {
                self.line(&format!("{};", format_expr(expression)));
            }
        }
    }

    fn format_block_body(&mut self, statements: &[Stmt]) {
        for statement in statements {
            self.format_statement(statement);
        }
    }

    fn format_if_statement(
        &mut self,
        condition: &Expr,
        then_branch: &[Stmt],
        else_branch: &[Stmt],
    ) {
        self.line(&format!("if ({}) {{", format_expr(condition)));
        self.indented(|formatter| formatter.format_block_body(then_branch));
        self.format_else_branch(else_branch);
    }

    fn format_if_let_statement(
        &mut self,
        pattern: &MatchCaseValue,
        value: &Expr,
        then_branch: &[Stmt],
        else_branch: &[Stmt],
    ) {
        self.line(&format!(
            "if let {} = {} {{",
            format_match_case(pattern),
            format_expr(value)
        ));
        self.indented(|formatter| formatter.format_block_body(then_branch));
        self.format_else_branch(else_branch);
    }

    fn format_else_branch(&mut self, else_branch: &[Stmt]) {
        match else_branch {
            [] => self.line("}"),
            [Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            }] => {
                self.line(&format!("}} else if ({}) {{", format_expr(condition)));
                self.indented(|formatter| formatter.format_block_body(then_branch));
                self.format_else_branch(else_branch);
            }
            [Stmt::IfLet {
                pattern,
                value,
                then_branch,
                else_branch,
                ..
            }] => {
                self.line(&format!(
                    "}} else if let {} = {} {{",
                    format_match_case(pattern),
                    format_expr(value)
                ));
                self.indented(|formatter| formatter.format_block_body(then_branch));
                self.format_else_branch(else_branch);
            }
            _ => {
                self.line("} else {");
                self.indented(|formatter| formatter.format_block_body(else_branch));
                self.line("}");
            }
        }
    }

    fn indented(&mut self, format: impl FnOnce(&mut Self)) {
        self.indent += 1;
        format(self);
        self.indent -= 1;
    }

    fn line(&mut self, line: &str) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
        self.output.push_str(line);
        self.output.push('\n');
    }
}

fn export_prefix(exported: bool) -> &'static str {
    if exported {
        "export "
    } else {
        ""
    }
}

fn format_binding_target(target: &BindingTarget, ty: Option<&Type>) -> String {
    match target {
        BindingTarget::Name { name, .. } => {
            if let Some(ty) = ty {
                format!("{name}: {ty}")
            } else {
                name.clone()
            }
        }
        BindingTarget::Tuple { names, .. } => format!("({})", names.join(", ")),
        BindingTarget::Record { names, .. } => format!("{{ {} }}", names.join(", ")),
    }
}

fn format_match_case(value: &MatchCaseValue) -> String {
    match value {
        MatchCaseValue::Int(value) => value.to_string(),
        MatchCaseValue::Float(value) => format_float_literal(*value),
        MatchCaseValue::Str(value) => format!("\"{}\"", escape_string(value)),
        MatchCaseValue::IntRange { start, end } => format!("{start}..{end}"),
        MatchCaseValue::Bind(name) => name.clone(),
        MatchCaseValue::Some(pattern) => format!("some({})", format_match_case(pattern)),
        MatchCaseValue::None => "none".to_string(),
        MatchCaseValue::Ok(pattern) => format!("ok({})", format_match_case(pattern)),
        MatchCaseValue::Err(pattern) => format!("err({})", format_match_case(pattern)),
        MatchCaseValue::EnumVariant { name, payload } => match payload {
            Some(payload) => format!("{name}({})", format_match_case(payload)),
            None => name.clone(),
        },
    }
}

fn format_expr(expr: &Expr) -> String {
    format_expr_prec(expr, 0)
}

fn format_expr_prec(expr: &Expr, parent_precedence: u8) -> String {
    let precedence = expr_precedence(expr);
    let mut text = match &expr.kind {
        ExprKind::Literal(value) => format_literal(value),
        ExprKind::StringInterpolation(parts) => format_interpolated_string(parts),
        ExprKind::Question { value } => format!("{}?", format_expr_prec(value, precedence)),
        ExprKind::Variable(name) => name.clone(),
        ExprKind::Assign { name, value } => {
            format!("{name} = {}", format_expr_prec(value, precedence))
        }
        ExprKind::Unary { op, right } => {
            format!(
                "{}{}",
                unary_symbol(*op),
                format_expr_prec(right, precedence)
            )
        }
        ExprKind::Binary { left, op, right } => format!(
            "{} {} {}",
            format_expr_prec(left, precedence),
            binary_symbol(*op),
            format_expr_prec(right, precedence + 1)
        ),
        ExprKind::Call { callee, args, .. } => {
            let args = args.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("{}({args})", format_expr_prec(callee, precedence))
        }
        ExprKind::ArrayLiteral { elements } => {
            let elements = elements
                .iter()
                .map(|element| match element {
                    ArrayElement::Expr(value) => format_expr(value),
                    ArrayElement::Spread(value) => format!("...{}", format_expr(value)),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{elements}]")
        }
        ExprKind::TupleLiteral { elements } => {
            let elements = elements
                .iter()
                .map(format_expr)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({elements})")
        }
        ExprKind::MapLiteral { entries } => {
            let entries = entries
                .iter()
                .map(|entry| match entry {
                    MapEntry::Entry { key, value } => {
                        format!("{}: {}", format_expr(key), format_expr(value))
                    }
                    MapEntry::Spread(value) => format!("...{}", format_expr(value)),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{entries}}}")
        }
        ExprKind::RecordLiteral { name, fields } => {
            let fields = fields
                .iter()
                .map(|(name, value, _)| format!("{name}: {}", format_expr(value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name} {{{fields}}}")
        }
        ExprKind::Index { array, index } => {
            format!(
                "{}[{}]",
                format_expr_prec(array, precedence),
                format_expr(index)
            )
        }
        ExprKind::IndexAssign {
            container,
            index,
            value,
        } => {
            format!(
                "{}[{}] = {}",
                format_expr_prec(container, precedence),
                format_expr(index),
                format_expr(value)
            )
        }
        ExprKind::FunctionLiteral {
            params,
            return_type,
            body,
        } => {
            let params_text = params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.ty))
                .collect::<Vec<_>>()
                .join(", ");
            let body_text = format_lambda_body(body);
            format!("fn({params_text}) -> {return_type} {{{body_text}}}")
        }
        ExprKind::Field { receiver, name, .. } => {
            format!("{}.{}", format_expr_prec(receiver, precedence), name)
        }
    };
    if precedence < parent_precedence {
        text = format!("({text})");
    }
    text
}

fn format_lambda_body(body: &[Stmt]) -> String {
    let mut formatter = Formatter {
        output: String::new(),
        indent: 1,
    };
    formatter.format_block_body(body);
    let text = formatter.output.trim_end().to_string();
    if text.is_empty() {
        String::new()
    } else {
        format!("\n{}\n", text)
    }
}

fn expr_precedence(expr: &Expr) -> u8 {
    match &expr.kind {
        ExprKind::Assign { .. }
        | ExprKind::IndexAssign { .. }
        | ExprKind::FunctionLiteral { .. } => 1,
        ExprKind::Binary { op, .. } => match op {
            BinaryOp::Or => 2,
            BinaryOp::And => 3,
            BinaryOp::BitOr => 4,
            BinaryOp::BitXor => 5,
            BinaryOp::BitAnd => 6,
            BinaryOp::Equal | BinaryOp::NotEqual => 7,
            BinaryOp::Greater
            | BinaryOp::GreaterEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::RangeLessThan => 8,
            BinaryOp::ShiftLeft | BinaryOp::ShiftRight => 9,
            BinaryOp::Add | BinaryOp::Subtract => 10,
            BinaryOp::Multiply | BinaryOp::Divide => 11,
        },
        ExprKind::Unary { .. } => 12,
        ExprKind::Call { .. }
        | ExprKind::Index { .. }
        | ExprKind::Field { .. }
        | ExprKind::Question { .. } => 13,
        ExprKind::Literal(_)
        | ExprKind::StringInterpolation(_)
        | ExprKind::Variable(_)
        | ExprKind::ArrayLiteral { .. }
        | ExprKind::TupleLiteral { .. }
        | ExprKind::MapLiteral { .. }
        | ExprKind::RecordLiteral { .. } => 14,
    }
}

fn format_interpolated_string(parts: &[StringInterpolationPart]) -> String {
    let mut output = String::from("\"");
    for part in parts {
        if let Some(expression) = &part.expression {
            output.push_str("${");
            output.push_str(&format_expr(expression));
            output.push('}');
        } else {
            output.push_str(&escape_interpolated_text(&part.text));
        }
    }
    output.push('"');
    output
}

fn format_literal(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) => format_float_literal(*value),
        Value::String(value) => format!("\"{}\"", escape_string(value)),
        Value::Json(value) => value.to_string(),
        Value::Array(_)
        | Value::Tuple(_)
        | Value::Map(_)
        | Value::Option(_)
        | Value::Result(_)
        | Value::Enum(_)
        | Value::Record(_)
        | Value::Function(_) => value.to_string(),
    }
}

fn escape_interpolated_text(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '\n' => escaped.push_str("\\n"),
            '\t' => escaped.push_str("\\t"),
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '$' => escaped.push_str("\\$"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn format_float_literal(value: f64) -> String {
    let text = value.to_string();
    if text.contains('.') || text.contains('e') || text.contains('E') {
        text
    } else {
        format!("{text}.0")
    }
}

fn escape_string(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '\n' => escaped.push_str("\\n"),
            '\t' => escaped.push_str("\\t"),
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            character => escaped.push(character),
        }
    }
    escaped
}

fn unary_symbol(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Negate => "-",
        UnaryOp::Not => "!",
        UnaryOp::BitNot => "~",
    }
}

fn binary_symbol(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Subtract => "-",
        BinaryOp::Multiply => "*",
        BinaryOp::Divide => "/",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::ShiftLeft => "<<",
        BinaryOp::ShiftRight => ">>",
        BinaryOp::RangeLessThan | BinaryOp::Less => "<",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::Equal => "==",
        BinaryOp::NotEqual => "!=",
        BinaryOp::Greater => ">",
        BinaryOp::GreaterEqual => ">=",
        BinaryOp::LessEqual => "<=",
    }
}
