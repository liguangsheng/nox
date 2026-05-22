use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    rc::{Rc, Weak},
};

mod bytecode;
mod ffi;
mod heap;
mod lexer;
mod parser;
mod typecheck;
mod vm;

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
    nox_core_engine_set_userdata, nox_core_engine_userdata, nox_core_map_free, nox_core_map_get,
    nox_core_map_keys, nox_core_map_len, nox_core_option_free, nox_core_option_is_some,
    nox_core_option_payload, nox_core_record_field, nox_core_record_free, nox_core_result_free,
    nox_core_result_is_ok, nox_core_result_payload, nox_core_string_free, nox_core_version,
    NoxCoreArrayHandle, NoxCoreEngine, NoxCoreHostCallback, NoxCoreMapHandle, NoxCoreOptionHandle,
    NoxCoreRecordHandle, NoxCoreResultHandle, NoxCoreStatus, NoxCoreValue, NoxCoreValueKind,
};
use heap::GcHeap;
use lexer::lex;
use parser::{parse, parse_all};
use typecheck::TypeChecker;
use vm::{value_type, Control, Env, EnvData, Vm};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub name: String,
    pub line: usize,
    pub column: usize,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            code: "error",
            message: message.into(),
            span,
            source: None,
        }
    }

    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = code;
        self
    }

    pub fn with_source(mut self, name: impl Into<String>, source: &str) -> Self {
        self.source = Some(SourceLocation {
            name: name.into(),
            line: line_column(source, self.span.start).0,
            column: line_column(source, self.span.start).1,
        });
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
    AndAnd,
    OrOr,
    Plus,
    Minus,
    Arrow,
    FatArrow,
    Star,
    Slash,
    Bang,
    BangEqual,
    Equal,
    EqualEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Identifier(String),
    Int(i64),
    Float(f64),
    String(String),
    Let,
    Const,
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
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Module {
    statements: Vec<Stmt>,
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
        | Stmt::Function { exported, .. }
        | Stmt::Record { exported, .. } => *exported,
        _ => false,
    }
}

fn top_level_declaration_name(statement: &Stmt) -> Option<&str> {
    match statement {
        Stmt::Let { name, .. } | Stmt::Function { name, .. } | Stmt::Record { name, .. } => {
            Some(name)
        }
        _ => None,
    }
}

fn top_level_declaration_span(statement: &Stmt) -> Span {
    match statement {
        Stmt::Let { span, .. } | Stmt::Function { span, .. } | Stmt::Record { span, .. } => *span,
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
                name,
                ty,
                initializer,
                exported,
                is_const,
                span,
            } => {
                let name = if top_level {
                    self.rename_declaration(name)
                } else {
                    name
                };
                let ty = self.rewrite_type(ty);
                let initializer = self.rewrite_expr(initializer);
                if !top_level {
                    self.define_local(name.clone());
                }
                Stmt::Let {
                    name,
                    ty,
                    initializer,
                    exported,
                    is_const,
                    span,
                }
            }
            Stmt::Function {
                name,
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
                        value: case.value,
                        body: self.rewrite_block(case.body),
                        span: case.span,
                    })
                    .collect(),
                default: default.map(|default| self.rewrite_block(default)),
                span,
            },
            Stmt::While {
                condition,
                body,
                span,
            } => Stmt::While {
                condition: self.rewrite_expr(condition),
                body: self.rewrite_block(body),
                span,
            },
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
                    .map(|element| self.rewrite_expr(element))
                    .collect(),
            },
            ExprKind::MapLiteral { entries } => ExprKind::MapLiteral {
                entries: entries
                    .into_iter()
                    .map(|(key, value)| (self.rewrite_expr(key), self.rewrite_expr(value)))
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
            Type::Map(value) => Type::Map(Box::new(self.rewrite_type(*value))),
            Type::Option(value) => Type::Option(Box::new(self.rewrite_type(*value))),
            Type::Result { ok, err } => Type::Result {
                ok: Box::new(self.rewrite_type(*ok)),
                err: Box::new(self.rewrite_type(*err)),
            },
            Type::Record(name) => Type::Record(self.rename_type_name(name)),
            Type::Function {
                params,
                return_type,
            } => Type::Function {
                params: params
                    .into_iter()
                    .map(|param| self.rewrite_type(param))
                    .collect(),
                return_type: Box::new(self.rewrite_type(*return_type)),
            },
            other => other,
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
                name,
                ty,
                initializer,
                exported,
                is_const,
                span,
            } => {
                let initializer = self.rewrite_expr(initializer)?;
                if !top_level {
                    self.define_local(name.clone());
                }
                Stmt::Let {
                    name,
                    ty,
                    initializer,
                    exported,
                    is_const,
                    span,
                }
            }
            Stmt::Function {
                name,
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
                            value: case.value,
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
            Stmt::While {
                condition,
                body,
                span,
            } => Stmt::While {
                condition: self.rewrite_expr(condition)?,
                body: self.rewrite_block(body)?,
                span,
            },
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
                    .map(|element| self.rewrite_expr(element))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            ExprKind::MapLiteral { entries } => ExprKind::MapLiteral {
                entries: entries
                    .into_iter()
                    .map(|(key, value)| Ok((self.rewrite_expr(key)?, self.rewrite_expr(value)?)))
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
        name: String,
        ty: Type,
        initializer: Expr,
        exported: bool,
        is_const: bool,
        span: Span,
    },
    Function {
        name: String,
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
    Match {
        value: Expr,
        cases: Vec<MatchCase>,
        default: Option<Vec<Stmt>>,
        span: Span,
    },
    While {
        condition: Expr,
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

#[derive(Debug, Clone, PartialEq)]
struct MatchCase {
    value: MatchCaseValue,
    body: Vec<Stmt>,
    span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum MatchCaseValue {
    Int(i64),
    Str(String),
    Some(String),
    None,
    Ok(String),
    Err(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Param {
    name: String,
    ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordField {
    name: String,
    ty: Type,
    span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Null,
    Bool,
    Int,
    Float,
    Str,
    Array(Box<Type>),
    Map(Box<Type>),
    Option(Box<Type>),
    Result {
        ok: Box<Type>,
        err: Box<Type>,
    },
    Record(String),
    Function {
        params: Vec<Type>,
        return_type: Box<Type>,
    },
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Bool => write!(f, "bool"),
            Self::Int => write!(f, "int"),
            Self::Float => write!(f, "float"),
            Self::Str => write!(f, "str"),
            Self::Array(element) => write!(f, "[{element}]"),
            Self::Map(value) => write!(f, "map[str, {value}]"),
            Self::Option(value) => write!(f, "option[{value}]"),
            Self::Result { ok, err } => write!(f, "result[{ok}, {err}]"),
            Self::Record(name) => write!(f, "{name}"),
            Self::Function {
                params,
                return_type,
            } => {
                write!(f, "fn(")?;
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
        elements: Vec<Expr>,
    },
    MapLiteral {
        entries: Vec<(Expr, Expr)>,
    },
    RecordLiteral {
        name: String,
        fields: Vec<(String, Expr, Span)>,
    },
    Index {
        array: Box<Expr>,
        index: Box<Expr>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
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
    Array(Rc<Array>),
    Map(Rc<Map>),
    Option(Rc<OptionValue>),
    Result(Rc<ResultValue>),
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
            Self::Array(array) => f
                .debug_struct("Array")
                .field("element_type", &array.element_type)
                .field("elements", &array.elements)
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
            (Self::Array(left), Self::Array(right)) => Rc::ptr_eq(left, right),
            (Self::Map(left), Self::Map(right)) => Rc::ptr_eq(left, right),
            (Self::Option(left), Self::Option(right)) => Rc::ptr_eq(left, right),
            (Self::Result(left), Self::Result(right)) => Rc::ptr_eq(left, right),
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

    pub fn array(element_type: Type, elements: Vec<Value>) -> Self {
        Self::Array(Rc::new(Array::new(element_type, elements)))
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

    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::String(_) => "string",
            Self::Array(_) => "array",
            Self::Map(_) => "map",
            Self::Option(_) => "option",
            Self::Result(_) => "result",
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
            Self::Array(array) => {
                write!(f, "[")?;
                for (index, value) in array.elements.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{value}")?;
                }
                write!(f, "]")
            }
            Self::Map(map) => {
                write!(f, "{{")?;
                for (index, (key, value)) in map.entries.iter().enumerate() {
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
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ResultVariant {
    Ok(Value),
    Err(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    element_type: Type,
    elements: Vec<Value>,
}

impl Array {
    fn new(element_type: Type, elements: Vec<Value>) -> Self {
        Self {
            element_type,
            elements,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Map {
    value_type: Type,
    entries: BTreeMap<String, Value>,
}

impl Map {
    fn new(value_type: Type, entries: BTreeMap<String, Value>) -> Self {
        Self {
            value_type,
            entries,
        }
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
}

pub struct Function {
    name: String,
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
            .field("params", &self.params)
            .field("return_type", &self.return_type)
            .finish_non_exhaustive()
    }
}

impl PartialEq for Function {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.params == other.params
            && self.return_type == other.return_type
    }
}

impl Function {
    fn signature_type(&self) -> Type {
        Type::Function {
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
    params: Vec<Param>,
    return_type: Type,
}

impl HostFunctionBuilder {
    pub fn new(name: impl Into<String>, return_type: Type) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            return_type,
        }
    }

    pub fn param(mut self, name: impl Into<String>, ty: Type) -> Self {
        self.params.push(Param {
            name: name.into(),
            ty,
        });
        self
    }
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

    pub fn hover_type(
        &mut self,
        source: &str,
        byte_offset: usize,
    ) -> Result<Option<Type>, Diagnostic> {
        self.engine.hover_type(source, byte_offset)
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
    instruction_budget: Option<usize>,
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
            params: builder.params,
            return_type: builder.return_type,
            kind: FunctionKind::Host {
                callback: Rc::new(callback),
            },
        });
        self.host_functions
            .insert(builder.name, HostFunction { function });
        Ok(())
    }

    pub fn set_module_loader<F>(&mut self, loader: F)
    where
        F: Fn(&str) -> Result<String, Diagnostic> + 'static,
    {
        self.module_loader = Some(Rc::new(loader));
    }

    pub fn set_instruction_budget(&mut self, budget: Option<usize>) {
        self.instruction_budget = budget;
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

    pub fn run_tests(&mut self, source: &str) -> Result<TestModuleResult, Diagnostic> {
        let tokens = lex(source)?;
        let module = parse(tokens)?;
        let test_functions = collect_test_functions(&module)?;
        let module = self.resolve_imports(module)?.into_flat_module();
        TypeChecker::new_with_hosts(&self.host_functions).check_module(&module)?;
        let bytecode = compile(&module);
        verify(&bytecode)?;

        let env = self.root_env();
        let mut vm = Vm::new(env.clone(), self.instruction_budget, self.heap.clone());
        match vm.execute(&bytecode)? {
            Control::Value(_) | Control::Return(_) => {}
        }

        let mut tests = Vec::with_capacity(test_functions.len());
        for test in test_functions {
            let Some(callee) = env.get(&test.name) else {
                return Err(Diagnostic::new(
                    format!("test function '{}' is not available", test.name),
                    test.span,
                ));
            };
            match vm.call_value(test.span, callee, Vec::new()) {
                Ok(Value::Bool(passed)) => tests.push(TestCaseResult {
                    name: test.name,
                    passed,
                    diagnostic: None,
                }),
                Ok(value) => tests.push(TestCaseResult {
                    name: test.name,
                    passed: false,
                    diagnostic: Some(Diagnostic::new(
                        format!("test returned {}, expected bool", value_type(&value)),
                        test.span,
                    )),
                }),
                Err(diagnostic) => tests.push(TestCaseResult {
                    name: test.name,
                    passed: false,
                    diagnostic: Some(diagnostic),
                }),
            }
        }

        Ok(TestModuleResult { tests })
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
        match Vm::new(env, self.instruction_budget, self.heap.clone()).execute(module)? {
            Control::Value(value) => Ok(value),
            Control::Return(value) => Ok(value),
        }
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
struct TestFunction {
    name: String,
    span: Span,
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
        if return_type != &Type::Bool {
            return Err(
                Diagnostic::new(format!("test function '{name}' must return bool"), *span)
                    .with_code("test.signature"),
            );
        }
        tests.push(TestFunction {
            name: name.clone(),
            span: *span,
        });
    }
    Ok(tests)
}

pub(crate) fn compile(module: &Module) -> BytecodeModule {
    Compiler.compile_module(module)
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
                name,
                ty,
                initializer,
                exported,
                is_const,
                ..
            } => {
                let keyword = if *is_const { "const" } else { "let" };
                self.line(&format!(
                    "{}{keyword} {name}: {ty} = {};",
                    export_prefix(*exported),
                    format_expr(initializer)
                ));
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
            Stmt::Match {
                value,
                cases,
                default,
                ..
            } => {
                self.line(&format!("match ({}) {{", format_expr(value)));
                self.indented(|formatter| {
                    for case in cases {
                        formatter.line(&format!("{} => {{", format_match_case(&case.value)));
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
            Stmt::While {
                condition, body, ..
            } => {
                self.line(&format!("while ({}) {{", format_expr(condition)));
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

fn format_match_case(value: &MatchCaseValue) -> String {
    match value {
        MatchCaseValue::Int(value) => value.to_string(),
        MatchCaseValue::Str(value) => format!("\"{}\"", escape_string(value)),
        MatchCaseValue::Some(name) => format!("some({name})"),
        MatchCaseValue::None => "none".to_string(),
        MatchCaseValue::Ok(name) => format!("ok({name})"),
        MatchCaseValue::Err(name) => format!("err({name})"),
    }
}

fn format_expr(expr: &Expr) -> String {
    format_expr_prec(expr, 0)
}

fn format_expr_prec(expr: &Expr, parent_precedence: u8) -> String {
    let precedence = expr_precedence(expr);
    let mut text = match &expr.kind {
        ExprKind::Literal(value) => format_literal(value),
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
                .map(format_expr)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{elements}]")
        }
        ExprKind::MapLiteral { entries } => {
            let entries = entries
                .iter()
                .map(|(key, value)| format!("{}: {}", format_expr(key), format_expr(value)))
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
        ExprKind::Field { receiver, name, .. } => {
            format!("{}.{}", format_expr_prec(receiver, precedence), name)
        }
    };
    if precedence < parent_precedence {
        text = format!("({text})");
    }
    text
}

fn expr_precedence(expr: &Expr) -> u8 {
    match &expr.kind {
        ExprKind::Assign { .. } => 1,
        ExprKind::Binary { op, .. } => match op {
            BinaryOp::Or => 2,
            BinaryOp::And => 3,
            BinaryOp::Equal | BinaryOp::NotEqual => 4,
            BinaryOp::Greater
            | BinaryOp::GreaterEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::RangeLessThan => 5,
            BinaryOp::Add | BinaryOp::Subtract => 6,
            BinaryOp::Multiply | BinaryOp::Divide => 7,
        },
        ExprKind::Unary { .. } => 8,
        ExprKind::Call { .. } | ExprKind::Index { .. } | ExprKind::Field { .. } => 9,
        ExprKind::Literal(_)
        | ExprKind::Variable(_)
        | ExprKind::ArrayLiteral { .. }
        | ExprKind::MapLiteral { .. }
        | ExprKind::RecordLiteral { .. } => 10,
    }
}

fn format_literal(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) => format_float_literal(*value),
        Value::String(value) => format!("\"{}\"", escape_string(value)),
        Value::Array(_)
        | Value::Map(_)
        | Value::Option(_)
        | Value::Result(_)
        | Value::Record(_)
        | Value::Function(_) => value.to_string(),
    }
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
    }
}

fn binary_symbol(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Subtract => "-",
        BinaryOp::Multiply => "*",
        BinaryOp::Divide => "/",
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
