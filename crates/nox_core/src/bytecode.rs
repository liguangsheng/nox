use std::{
    collections::{BTreeMap, HashSet},
    fmt::Write,
};

use crate::{
    vm::flat_instruction_span, ArrayElement, BinaryOp, Diagnostic, Expr, ExprKind, MapEntry,
    MatchCaseValue, Module, Param, Span, Stmt, StringInterpolationPart, Type, UnaryOp, Value,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BytecodeModule {
    pub(crate) instructions: Vec<Instruction>,
}

impl BytecodeModule {
    pub(crate) fn format_compact(&self) -> String {
        let mut output = String::new();
        for (index, instruction) in self.instructions.iter().enumerate() {
            writeln!(&mut output, "{index:04} {}", instruction.format_compact())
                .expect("writing to String cannot fail");
        }
        output
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Instruction {
    Constant {
        value: Value,
        span: Span,
    },
    Load {
        name: String,
        span: Span,
    },
    Store {
        name: String,
        span: Span,
    },
    Define {
        name: String,
        span: Span,
    },
    Function {
        name: String,
        type_params: Vec<String>,
        params: Vec<Param>,
        return_type: Type,
        body: BytecodeModule,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        span: Span,
    },
    Binary {
        op: BinaryOp,
        span: Span,
    },
    StringInterpolate {
        parts: Vec<StringInterpolationInstructionPart>,
        span: Span,
    },
    Question {
        return_type: Type,
        span: Span,
    },
    MatchPattern {
        pattern: MatchCaseValue,
        span: Span,
    },
    Call {
        arg_count: usize,
        span: Span,
    },
    JsonDecode {
        target_type: Type,
        schema: JsonDecodeSchema,
        span: Span,
    },
    Array {
        element_type: Type,
        elements: Vec<ArrayInstructionElement>,
        span: Span,
    },
    Tuple {
        element_types: Vec<Type>,
        element_count: usize,
        span: Span,
    },
    Map {
        value_type: Type,
        entries: Vec<MapInstructionEntry>,
        span: Span,
    },
    Option {
        payload_type: Type,
        has_payload: bool,
        span: Span,
    },
    Result {
        ok_type: Type,
        err_type: Type,
        is_ok: bool,
        span: Span,
    },
    EnumVariant {
        enum_name: String,
        variant_name: String,
        has_payload: bool,
        span: Span,
    },
    Record {
        name: String,
        fields: Vec<String>,
        span: Span,
    },
    Index {
        span: Span,
    },
    IndexAssign {
        span: Span,
    },
    Field {
        name: String,
        span: Span,
    },
    RecordElement {
        name: String,
        span: Span,
    },
    TupleElement {
        index: usize,
        span: Span,
    },
    ArrayLen {
        span: Span,
    },
    MapContains {
        span: Span,
    },
    MapKeys {
        span: Span,
    },
    MapValues {
        span: Span,
    },
    MapSize {
        span: Span,
    },
    MapGet {
        span: Span,
    },
    OptionPayload {
        span: Span,
    },
    ResultPayload {
        span: Span,
    },
    EnumPayload {
        span: Span,
    },
    Pop {
        span: Span,
    },
    Drop {
        span: Span,
    },
    Return {
        span: Span,
    },
    JumpIfFalse {
        target: usize,
        span: Span,
    },
    Jump {
        target: usize,
        span: Span,
    },
    Loop {
        target: usize,
        span: Span,
    },
    BranchExit {
        exits: usize,
        target: usize,
        span: Span,
    },
    BreakPlaceholder {
        span: Span,
    },
    ContinuePlaceholder {
        span: Span,
    },
    BeginScope {
        span: Span,
    },
    EndScope {
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArrayInstructionElement {
    Expr,
    Spread,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MapInstructionEntry {
    Entry,
    Spread,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JsonDecodeSchema {
    pub(crate) records: BTreeMap<String, Vec<(String, Type)>>,
    pub(crate) enums: BTreeMap<String, Vec<(String, Option<Type>)>>,
}

impl Instruction {
    fn format_compact(&self) -> String {
        match self {
            Self::Constant { value, .. } => format!("Constant {value:?}"),
            Self::Load { name, .. } => format!("Load {name}"),
            Self::Store { name, .. } => format!("Store {name}"),
            Self::Define { name, .. } => format!("Define {name}"),
            Self::Function {
                name,
                params,
                return_type,
                body,
                ..
            } => {
                let params = params
                    .iter()
                    .map(|param| format!("{}: {}", param.name, param.ty))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "Function {name}({params}) -> {return_type} [{} instructions]",
                    body.instructions.len()
                )
            }
            Self::Unary { op, .. } => format!("Unary {op:?}"),
            Self::Binary { op, .. } => format!("Binary {op:?}"),
            Self::StringInterpolate { parts, .. } => {
                let expression_count = parts
                    .iter()
                    .filter(|part| matches!(part, StringInterpolationInstructionPart::Expression))
                    .count();
                format!(
                    "StringInterpolate parts={} exprs={expression_count}",
                    parts.len()
                )
            }
            Self::Question { return_type, .. } => format!("Question return={return_type}"),
            Self::MatchPattern { pattern, .. } => format!("MatchPattern {pattern:?}"),
            Self::Call { arg_count, .. } => format!("Call argc={arg_count}"),
            Self::JsonDecode { target_type, .. } => format!("JsonDecode {target_type}"),
            Self::Array {
                element_type,
                elements,
                ..
            } => {
                let element_count = elements
                    .iter()
                    .filter(|element| matches!(element, ArrayInstructionElement::Expr))
                    .count();
                let spread_count = elements.len() - element_count;
                format!("Array [{element_type}; {element_count} values, {spread_count} spreads]")
            }
            Self::Tuple {
                element_types,
                element_count,
                ..
            } => {
                let types = element_types
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("Tuple ({types}; {element_count})")
            }
            Self::Map {
                value_type,
                entries,
                ..
            } => {
                let entry_count = entries
                    .iter()
                    .filter(|entry| matches!(entry, MapInstructionEntry::Entry))
                    .count();
                let spread_count = entries.len() - entry_count;
                format!("Map map[str, {value_type}; {entry_count} entries, {spread_count} spreads]")
            }
            Self::Option {
                payload_type,
                has_payload,
                ..
            } => {
                if *has_payload {
                    format!("Option some[{payload_type}]")
                } else {
                    format!("Option none[{payload_type}]")
                }
            }
            Self::Result {
                ok_type,
                err_type,
                is_ok,
                ..
            } => {
                if *is_ok {
                    format!("Result ok[{ok_type}, {err_type}]")
                } else {
                    format!("Result err[{ok_type}, {err_type}]")
                }
            }
            Self::EnumVariant {
                enum_name,
                variant_name,
                has_payload,
                ..
            } => {
                if *has_payload {
                    format!("Enum {enum_name}.{variant_name}(payload)")
                } else {
                    format!("Enum {enum_name}.{variant_name}")
                }
            }
            Self::Record { name, fields, .. } => {
                format!("Record {name} {{{}}}", fields.join(", "))
            }
            Self::Index { .. } => "Index".to_string(),
            Self::IndexAssign { .. } => "IndexAssign".to_string(),
            Self::Field { name, .. } => format!("Field {name}"),
            Self::RecordElement { name, .. } => format!("RecordElement {name}"),
            Self::TupleElement { index, .. } => format!("TupleElement {index}"),
            Self::ArrayLen { .. } => "ArrayLen".to_string(),
            Self::MapContains { .. } => "MapContains".to_string(),
            Self::MapKeys { .. } => "MapKeys".to_string(),
            Self::MapValues { .. } => "MapValues".to_string(),
            Self::MapSize { .. } => "MapSize".to_string(),
            Self::MapGet { .. } => "MapGet".to_string(),
            Self::OptionPayload { .. } => "OptionPayload".to_string(),
            Self::ResultPayload { .. } => "ResultPayload".to_string(),
            Self::EnumPayload { .. } => "EnumPayload".to_string(),
            Self::Pop { .. } => "Pop".to_string(),
            Self::Drop { .. } => "Drop".to_string(),
            Self::Return { .. } => "Return".to_string(),
            Self::JumpIfFalse { target, .. } => format!("JumpIfFalse {target}"),
            Self::Jump { target, .. } => format!("Jump {target}"),
            Self::Loop { target, .. } => format!("Loop {target}"),
            Self::BranchExit { exits, target, .. } => {
                format!("BranchExit exits={exits} target={target}")
            }
            Self::BreakPlaceholder { .. } => "Break (unbound)".to_string(),
            Self::ContinuePlaceholder { .. } => "Continue (unbound)".to_string(),
            Self::BeginScope { .. } => "BeginScope".to_string(),
            Self::EndScope { .. } => "EndScope".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StatementInstruction {
    Let {
        target: crate::BindingTarget,
        initializer: ByteExpr,
        span: Span,
    },
    Function {
        name: String,
        type_params: Vec<String>,
        params: Vec<Param>,
        return_type: Type,
        body: BytecodeModule,
        span: Span,
    },
    Record {
        span: Span,
    },
    TypeAlias {
        span: Span,
    },
    Enum {
        span: Span,
    },
    Return {
        value: ByteExpr,
        span: Span,
    },
    If {
        condition: ByteExpr,
        then_body: BytecodeModule,
        else_body: BytecodeModule,
        span: Span,
    },
    IfLet {
        pattern: MatchCaseValue,
        value: ByteExpr,
        then_body: BytecodeModule,
        else_body: BytecodeModule,
        span: Span,
    },
    Match {
        value: ByteExpr,
        cases: Vec<ByteMatchCase>,
        default: BytecodeModule,
        span: Span,
    },
    LetElse {
        pattern: MatchCaseValue,
        value: ByteExpr,
        else_body: BytecodeModule,
        span: Span,
    },
    While {
        condition: ByteExpr,
        body: BytecodeModule,
        span: Span,
    },
    WhileLet {
        pattern: MatchCaseValue,
        value: ByteExpr,
        body: BytecodeModule,
        span: Span,
    },
    For {
        name: String,
        start: ByteExpr,
        end: ByteExpr,
        body: BytecodeModule,
        span: Span,
    },
    Block {
        body: BytecodeModule,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Expression {
        expression: ByteExpr,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ByteMatchCase {
    pattern: MatchCaseValue,
    body: BytecodeModule,
    span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ByteExpr {
    pub(crate) kind: ByteExprKind,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ByteArrayElement {
    Expr(ByteExpr),
    Spread(ByteExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ByteMapEntry {
    Entry { key: ByteExpr, value: ByteExpr },
    Spread(ByteExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ByteExprKind {
    Constant(Value),
    Get(String),
    Assign {
        name: String,
        value: Box<ByteExpr>,
    },
    Unary {
        op: UnaryOp,
        right: Box<ByteExpr>,
    },
    Binary {
        left: Box<ByteExpr>,
        op: BinaryOp,
        right: Box<ByteExpr>,
    },
    StringInterpolation {
        parts: Vec<ByteStringInterpolationPart>,
    },
    Question {
        value: Box<ByteExpr>,
        return_type: Type,
    },
    Call {
        callee: Box<ByteExpr>,
        args: Vec<ByteExpr>,
    },
    JsonDecode {
        value: Box<ByteExpr>,
        target_type: Type,
        schema: JsonDecodeSchema,
    },
    ArrayLiteral {
        element_type: Type,
        elements: Vec<ByteArrayElement>,
    },
    TupleLiteral {
        element_types: Vec<Type>,
        elements: Vec<ByteExpr>,
    },
    MapLiteral {
        value_type: Type,
        entries: Vec<ByteMapEntry>,
    },
    Some {
        payload_type: Type,
        payload: Box<ByteExpr>,
    },
    None {
        payload_type: Type,
    },
    Ok {
        ok_type: Type,
        err_type: Type,
        payload: Box<ByteExpr>,
    },
    Err {
        ok_type: Type,
        err_type: Type,
        payload: Box<ByteExpr>,
    },
    EnumVariant {
        enum_name: String,
        variant_name: String,
        payload: Option<Box<ByteExpr>>,
    },
    RecordLiteral {
        name: String,
        fields: Vec<(String, ByteExpr)>,
    },
    Index {
        array: Box<ByteExpr>,
        index: Box<ByteExpr>,
    },
    IndexAssign {
        container: Box<ByteExpr>,
        index: Box<ByteExpr>,
        value: Box<ByteExpr>,
    },
    FunctionLiteral {
        params: Vec<Param>,
        return_type: Type,
        body: BytecodeModule,
    },
    Field {
        receiver: Box<ByteExpr>,
        name: String,
    },
    ArrayLen {
        value: Box<ByteExpr>,
    },
    MapContains {
        map: Box<ByteExpr>,
        key: Box<ByteExpr>,
    },
    MapKeys {
        map: Box<ByteExpr>,
    },
    MapValues {
        map: Box<ByteExpr>,
    },
    MapSize {
        map: Box<ByteExpr>,
    },
    MapGet {
        map: Box<ByteExpr>,
        key: Box<ByteExpr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StringInterpolationInstructionPart {
    Text(String),
    Expression,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ByteStringInterpolationPart {
    text: String,
    expression: Option<ByteExpr>,
}

pub(crate) struct Compiler {
    enum_names: HashSet<String>,
    json_decode_schema: JsonDecodeSchema,
}

pub(crate) fn verify(module: &BytecodeModule) -> Result<(), Diagnostic> {
    Verifier::new(module).verify()
}

fn verifier_error(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::new(message, span).with_code("bytecode.verifier")
}

struct Verifier<'a> {
    module: &'a BytecodeModule,
    stack_depth: isize,
    scope_depth: isize,
    reachable: bool,
    join_states: std::collections::HashMap<usize, (isize, isize)>,
}

impl<'a> Verifier<'a> {
    fn new(module: &'a BytecodeModule) -> Self {
        Self {
            module,
            stack_depth: 0,
            scope_depth: 0,
            reachable: true,
            join_states: std::collections::HashMap::new(),
        }
    }

    fn verify(mut self) -> Result<(), Diagnostic> {
        for (index, instruction) in self.module.instructions.iter().enumerate() {
            self.enter_instruction(index, instruction)?;
            if !self.reachable {
                self.advance_scope_only(instruction);
                continue;
            }
            self.verify_instruction(index, instruction)?;
            self.update_reachability(instruction);
        }
        if self.scope_depth != 0 {
            return Err(verifier_error(
                "bytecode verifier: unbalanced scope stack",
                Span { start: 0, end: 0 },
            ));
        }
        Ok(())
    }

    fn advance_scope_only(&mut self, instruction: &Instruction) {
        match instruction {
            Instruction::BeginScope { .. } => self.scope_depth += 1,
            Instruction::EndScope { .. } => self.scope_depth -= 1,
            Instruction::Function { body, .. } => {
                let _ = verify(body);
            }
            _ => {}
        }
    }

    fn enter_instruction(
        &mut self,
        index: usize,
        instruction: &Instruction,
    ) -> Result<(), Diagnostic> {
        match self.join_states.get(&index).copied() {
            Some((expected_stack, expected_scope)) => {
                if self.reachable {
                    if expected_stack != self.stack_depth {
                        return Err(verifier_error(
                            format!(
                                "bytecode verifier: stack height mismatch at join target {index}: branch enters with {expected_stack}, fallthrough has {}",
                                self.stack_depth
                            ),
                            flat_instruction_span(instruction),
                        ));
                    }
                    if expected_scope != self.scope_depth {
                        return Err(verifier_error(
                            format!(
                                "bytecode verifier: scope depth mismatch at join target {index}: branch enters with {expected_scope}, fallthrough has {}",
                                self.scope_depth
                            ),
                            flat_instruction_span(instruction),
                        ));
                    }
                } else {
                    self.stack_depth = expected_stack;
                    self.scope_depth = expected_scope;
                    self.reachable = true;
                }
            }
            None => {
                // No incoming branch: continue with fallthrough state, including the
                // "unreachable" mode after an unconditional jump.
            }
        }
        Ok(())
    }

    fn update_reachability(&mut self, instruction: &Instruction) {
        match instruction {
            Instruction::Jump { .. }
            | Instruction::Loop { .. }
            | Instruction::Return { .. }
            | Instruction::BranchExit { .. } => {
                self.reachable = false;
            }
            _ => {}
        }
    }

    fn record_join_state(
        &mut self,
        target: usize,
        stack_depth: isize,
        scope_depth: isize,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if let Some(existing) = self.join_states.get(&target).copied() {
            if existing != (stack_depth, scope_depth) {
                return Err(verifier_error(
                    format!(
                        "bytecode verifier: incompatible branches to target {target}: previously stack={}, scope={}, now stack={stack_depth}, scope={scope_depth}",
                        existing.0, existing.1
                    ),
                    span,
                ));
            }
        } else {
            self.join_states.insert(target, (stack_depth, scope_depth));
        }
        Ok(())
    }

    fn verify_instruction(
        &mut self,
        index: usize,
        instruction: &Instruction,
    ) -> Result<(), Diagnostic> {
        match instruction {
            Instruction::Constant { .. }
            | Instruction::Load { .. }
            | Instruction::Function { .. } => self.push(1),
            Instruction::Store { span, .. } => self.require(1, *span)?,
            Instruction::Define { span, .. } => self.pop(1, *span)?,
            Instruction::Unary { span, .. } => self.require(1, *span)?,
            Instruction::Binary { span, .. } => {
                self.pop(2, *span)?;
                self.push(1);
            }
            Instruction::StringInterpolate { parts, span } => {
                let expression_count = parts
                    .iter()
                    .filter(|part| matches!(part, StringInterpolationInstructionPart::Expression))
                    .count();
                self.pop(expression_count, *span)?;
                self.push(1);
            }
            Instruction::Question { span, .. } => {
                self.pop(1, *span)?;
                self.push(1);
            }
            Instruction::MatchPattern { span, .. } => self.require(1, *span)?,
            Instruction::Call { arg_count, span } => {
                self.pop(arg_count + 1, *span)?;
                self.push(1);
            }
            Instruction::JsonDecode { span, .. } => self.require(1, *span)?,
            Instruction::Array { elements, span, .. } => {
                self.pop(elements.len(), *span)?;
                self.push(1);
            }
            Instruction::Tuple {
                element_count,
                span,
                ..
            } => {
                self.pop(*element_count, *span)?;
                self.push(1);
            }
            Instruction::Map { entries, span, .. } => {
                let value_count = entries
                    .iter()
                    .map(|entry| match entry {
                        MapInstructionEntry::Entry => 2,
                        MapInstructionEntry::Spread => 1,
                    })
                    .sum::<usize>();
                self.pop(value_count, *span)?;
                self.push(1);
            }
            Instruction::Option {
                has_payload, span, ..
            } => {
                if *has_payload {
                    self.pop(1, *span)?;
                }
                self.push(1);
            }
            Instruction::Result { span, .. } => {
                self.pop(1, *span)?;
                self.push(1);
            }
            Instruction::EnumVariant {
                has_payload, span, ..
            } => {
                if *has_payload {
                    self.pop(1, *span)?;
                }
                self.push(1);
            }
            Instruction::Record { fields, span, .. } => {
                self.pop(fields.len(), *span)?;
                self.push(1);
            }
            Instruction::Index { span } => {
                self.pop(2, *span)?;
                self.push(1);
            }
            Instruction::IndexAssign { span } => {
                self.pop(3, *span)?;
                self.push(1);
            }
            Instruction::Field { span, .. } => self.require(1, *span)?,
            Instruction::RecordElement { span, .. } => {
                self.require(1, *span)?;
                self.push(1);
            }
            Instruction::TupleElement { span, .. } => {
                self.require(1, *span)?;
                self.push(1);
            }
            Instruction::ArrayLen { span } => self.require(1, *span)?,
            Instruction::MapContains { span } => {
                self.pop(2, *span)?;
                self.push(1);
            }
            Instruction::MapKeys { span }
            | Instruction::MapValues { span }
            | Instruction::MapSize { span } => self.require(1, *span)?,
            Instruction::MapGet { span } => {
                self.pop(2, *span)?;
                self.push(1);
            }
            Instruction::OptionPayload { span }
            | Instruction::ResultPayload { span }
            | Instruction::EnumPayload { span } => self.require(1, *span)?,
            Instruction::Pop { span }
            | Instruction::Drop { span }
            | Instruction::Return { span } => {
                self.pop(1, *span)?;
            }
            Instruction::JumpIfFalse { target, span } => {
                self.pop(1, *span)?;
                self.verify_target(index, *target, *span)?;
                self.record_join_state(*target, self.stack_depth, self.scope_depth, *span)?;
            }
            Instruction::Jump { target, span } | Instruction::Loop { target, span } => {
                self.verify_target(index, *target, *span)?;
                self.record_join_state(*target, self.stack_depth, self.scope_depth, *span)?;
            }
            Instruction::BranchExit {
                exits,
                target,
                span,
            } => {
                if (*exits as isize) > self.scope_depth {
                    return Err(verifier_error(
                        "bytecode verifier: branch exit pops more scopes than are open",
                        *span,
                    ));
                }
                self.verify_target(index, *target, *span)?;
                let target_scope = self.scope_depth - (*exits as isize);
                self.record_join_state(*target, self.stack_depth, target_scope, *span)?;
            }
            Instruction::BreakPlaceholder { span } | Instruction::ContinuePlaceholder { span } => {
                return Err(verifier_error(
                    "bytecode verifier: 'break' or 'continue' outside of a loop",
                    *span,
                ));
            }
            Instruction::BeginScope { .. } => self.scope_depth += 1,
            Instruction::EndScope { span } => {
                self.scope_depth -= 1;
                if self.scope_depth < 0 {
                    return Err(verifier_error(
                        "bytecode verifier: scope stack underflow",
                        *span,
                    ));
                }
            }
        }

        if let Instruction::Function { body, .. } = instruction {
            verify(body)?;
        }

        Ok(())
    }

    fn verify_target(&self, index: usize, target: usize, span: Span) -> Result<(), Diagnostic> {
        if target > self.module.instructions.len() {
            return Err(verifier_error(
                format!("bytecode verifier: invalid jump target {target} from {index}"),
                span,
            ));
        }
        Ok(())
    }

    fn require(&self, count: usize, span: Span) -> Result<(), Diagnostic> {
        if self.stack_depth < count as isize {
            return Err(verifier_error("bytecode verifier: stack underflow", span));
        }
        Ok(())
    }

    fn pop(&mut self, count: usize, span: Span) -> Result<(), Diagnostic> {
        self.require(count, span)?;
        self.stack_depth -= count as isize;
        Ok(())
    }

    fn push(&mut self, count: usize) {
        self.stack_depth += count as isize;
    }
}

impl Compiler {
    pub(crate) fn compile_module(module: &Module) -> BytecodeModule {
        let enum_names = module
            .statements
            .iter()
            .filter_map(|statement| match statement {
                Stmt::Enum { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        let json_decode_schema = JsonDecodeSchema {
            records: module
                .statements
                .iter()
                .filter_map(|statement| match statement {
                    Stmt::Record { name, fields, .. } => Some((
                        name.clone(),
                        fields
                            .iter()
                            .map(|field| (field.name.clone(), field.ty.clone()))
                            .collect(),
                    )),
                    _ => None,
                })
                .collect(),
            enums: module
                .statements
                .iter()
                .filter_map(|statement| match statement {
                    Stmt::Enum { name, variants, .. } => Some((
                        name.clone(),
                        variants
                            .iter()
                            .map(|variant| (variant.name.clone(), variant.payload.clone()))
                            .collect(),
                    )),
                    _ => None,
                })
                .collect(),
        };
        Self {
            enum_names,
            json_decode_schema,
        }
        .compile_statements(&module.statements)
    }

    fn compile_statements(&self, statements: &[Stmt]) -> BytecodeModule {
        self.compile_statements_with_return(statements, None)
    }

    fn compile_statements_with_return(
        &self,
        statements: &[Stmt],
        current_return: Option<&Type>,
    ) -> BytecodeModule {
        let statements = statements
            .iter()
            .map(|statement| self.compile_statement(statement, current_return))
            .collect::<Vec<_>>();
        let mut instructions = Vec::new();
        for statement in &statements {
            Self::emit_statement(statement, &mut instructions);
        }
        BytecodeModule { instructions }
    }

    fn compile_statement(
        &self,
        statement: &Stmt,
        current_return: Option<&Type>,
    ) -> StatementInstruction {
        match statement {
            Stmt::Import { .. } => unreachable!("imports are resolved before compilation"),
            Stmt::Let {
                target,
                ty,
                initializer,
                exported: _,
                is_const: _,
                span,
            } => StatementInstruction::Let {
                target: target.clone(),
                initializer: self.compile_expr_with_context(
                    initializer,
                    ty.as_ref(),
                    current_return,
                ),
                span: *span,
            },
            Stmt::Function {
                name,
                type_params,
                type_param_constraints: _,
                params,
                return_type,
                body,
                exported: _,
                span,
            } => StatementInstruction::Function {
                name: name.clone(),
                type_params: type_params.clone(),
                params: params.clone(),
                return_type: return_type.clone(),
                body: self.compile_statements_with_return(body, Some(return_type)),
                span: *span,
            },
            Stmt::Record { span, .. } => StatementInstruction::Record { span: *span },
            Stmt::TypeAlias { span, .. } => StatementInstruction::TypeAlias { span: *span },
            Stmt::Enum { span, .. } => StatementInstruction::Enum { span: *span },
            Stmt::Return { value, span } => StatementInstruction::Return {
                value: self.compile_expr_with_context(value, current_return, current_return),
                span: *span,
            },
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => StatementInstruction::If {
                condition: self.compile_expr_with_context(condition, None, current_return),
                then_body: self.compile_statements_with_return(then_branch, current_return),
                else_body: self.compile_statements_with_return(else_branch, current_return),
                span: *span,
            },
            Stmt::IfLet {
                pattern,
                value,
                then_branch,
                else_branch,
                span,
            } => StatementInstruction::IfLet {
                pattern: pattern.clone(),
                value: self.compile_expr_with_context(value, None, current_return),
                then_body: self.compile_statements_with_return(then_branch, current_return),
                else_body: self.compile_statements_with_return(else_branch, current_return),
                span: *span,
            },
            Stmt::Match {
                value,
                cases,
                default,
                span,
            } => StatementInstruction::Match {
                value: self.compile_expr_with_context(value, None, current_return),
                cases: cases
                    .iter()
                    .map(|case| ByteMatchCase {
                        pattern: case.pattern.clone(),
                        body: self.compile_statements_with_return(&case.body, current_return),
                        span: case.span,
                    })
                    .collect(),
                default: self.compile_statements_with_return(
                    default.as_deref().unwrap_or_default(),
                    current_return,
                ),
                span: *span,
            },
            Stmt::LetElse {
                pattern,
                value,
                else_branch,
                span,
            } => StatementInstruction::LetElse {
                pattern: pattern.clone(),
                value: self.compile_expr_with_context(value, None, current_return),
                else_body: self.compile_statements_with_return(else_branch, current_return),
                span: *span,
            },
            Stmt::While {
                condition,
                body,
                span,
            } => StatementInstruction::While {
                condition: self.compile_expr_with_context(condition, None, current_return),
                body: self.compile_statements_with_return(body, current_return),
                span: *span,
            },
            Stmt::WhileLet {
                pattern,
                value,
                body,
                span,
            } => StatementInstruction::WhileLet {
                pattern: pattern.clone(),
                value: self.compile_expr_with_context(value, None, current_return),
                body: self.compile_statements_with_return(body, current_return),
                span: *span,
            },
            Stmt::For {
                name,
                start,
                end,
                body,
                span,
            } => StatementInstruction::For {
                name: name.clone(),
                start: self.compile_expr_with_context(start, None, current_return),
                end: self.compile_expr_with_context(end, None, current_return),
                body: self.compile_statements_with_return(body, current_return),
                span: *span,
            },
            Stmt::Block { statements, span } => StatementInstruction::Block {
                body: self.compile_statements_with_return(statements, current_return),
                span: *span,
            },
            Stmt::Break { span } => StatementInstruction::Break { span: *span },
            Stmt::Continue { span } => StatementInstruction::Continue { span: *span },
            Stmt::Expression { expression, span } => StatementInstruction::Expression {
                expression: self.compile_expr_with_context(expression, None, current_return),
                span: *span,
            },
        }
    }

    fn compile_expr_with_context(
        &self,
        expr: &Expr,
        expected: Option<&Type>,
        current_return: Option<&Type>,
    ) -> ByteExpr {
        let kind = match &expr.kind {
            ExprKind::Literal(value) => ByteExprKind::Constant(value.clone()),
            ExprKind::Variable(name) if name == "none" => {
                let payload_type = match expected {
                    Some(Type::Option(payload)) => payload.as_ref().clone(),
                    _ => Type::Null,
                };
                ByteExprKind::None { payload_type }
            }
            ExprKind::Variable(name) => ByteExprKind::Get(name.clone()),
            ExprKind::Assign { name, value } => ByteExprKind::Assign {
                name: name.clone(),
                value: Box::new(self.compile_expr_with_context(value, None, current_return)),
            },
            ExprKind::Unary { op, right } => ByteExprKind::Unary {
                op: *op,
                right: Box::new(self.compile_expr_with_context(right, None, current_return)),
            },
            ExprKind::Binary { left, op, right } => ByteExprKind::Binary {
                left: Box::new(self.compile_expr_with_context(left, None, current_return)),
                op: *op,
                right: Box::new(self.compile_expr_with_context(right, None, current_return)),
            },
            ExprKind::StringInterpolation(parts) => ByteExprKind::StringInterpolation {
                parts: parts
                    .iter()
                    .map(|part| self.compile_string_interpolation_part(part, current_return))
                    .collect(),
            },
            ExprKind::Question { value } => ByteExprKind::Question {
                value: Box::new(self.compile_expr_with_context(value, None, current_return)),
                return_type: current_return.cloned().unwrap_or(Type::Null),
            },
            ExprKind::Call { callee, args, .. } => {
                if let ExprKind::Field { receiver, name, .. } = &callee.kind {
                    if let (ExprKind::Variable(enum_name), [payload]) =
                        (&receiver.kind, args.as_slice())
                    {
                        if self.enum_names.contains(enum_name) {
                            ByteExprKind::EnumVariant {
                                enum_name: enum_name.clone(),
                                variant_name: name.clone(),
                                payload: Some(Box::new(self.compile_expr_with_context(
                                    payload,
                                    None,
                                    current_return,
                                ))),
                            }
                        } else {
                            self.compile_record_method_call(
                                callee,
                                receiver,
                                name,
                                args,
                                current_return,
                            )
                        }
                    } else {
                        self.compile_record_method_call(
                            callee,
                            receiver,
                            name,
                            args,
                            current_return,
                        )
                    }
                } else if let (ExprKind::Variable(name), [value]) = (&callee.kind, args.as_slice())
                {
                    if name == "len" {
                        ByteExprKind::ArrayLen {
                            value: Box::new(self.compile_expr_with_context(
                                value,
                                None,
                                current_return,
                            )),
                        }
                    } else if is_std_json_from_json_name(name) {
                        let target_type = match expected {
                            Some(Type::Result { ok, .. }) => ok.as_ref().clone(),
                            Some(other) => other.clone(),
                            None => Type::Json,
                        };
                        ByteExprKind::JsonDecode {
                            value: Box::new(self.compile_expr_with_context(
                                value,
                                None,
                                current_return,
                            )),
                            target_type,
                            schema: self.json_decode_schema.clone(),
                        }
                    } else if name == "map_keys" {
                        ByteExprKind::MapKeys {
                            map: Box::new(self.compile_expr_with_context(
                                value,
                                None,
                                current_return,
                            )),
                        }
                    } else if name == "map_values" {
                        ByteExprKind::MapValues {
                            map: Box::new(self.compile_expr_with_context(
                                value,
                                None,
                                current_return,
                            )),
                        }
                    } else if name == "map_size" {
                        ByteExprKind::MapSize {
                            map: Box::new(self.compile_expr_with_context(
                                value,
                                None,
                                current_return,
                            )),
                        }
                    } else if name == "some" {
                        let payload_type = match expected {
                            Some(Type::Option(payload)) => payload.as_ref().clone(),
                            _ => Type::Null,
                        };
                        ByteExprKind::Some {
                            payload_type,
                            payload: Box::new(self.compile_expr_with_context(
                                value,
                                None,
                                current_return,
                            )),
                        }
                    } else if name == "ok" {
                        let (ok_type, err_type) = match expected {
                            Some(Type::Result { ok, err }) => {
                                (ok.as_ref().clone(), err.as_ref().clone())
                            }
                            _ => (Type::Null, Type::Null),
                        };
                        ByteExprKind::Ok {
                            ok_type,
                            err_type,
                            payload: Box::new(self.compile_expr_with_context(
                                value,
                                None,
                                current_return,
                            )),
                        }
                    } else if name == "err" {
                        let (ok_type, err_type) = match expected {
                            Some(Type::Result { ok, err }) => {
                                (ok.as_ref().clone(), err.as_ref().clone())
                            }
                            _ => (Type::Null, Type::Null),
                        };
                        ByteExprKind::Err {
                            ok_type,
                            err_type,
                            payload: Box::new(self.compile_expr_with_context(
                                value,
                                None,
                                current_return,
                            )),
                        }
                    } else {
                        ByteExprKind::Call {
                            callee: Box::new(self.compile_expr_with_context(
                                callee,
                                None,
                                current_return,
                            )),
                            args: args
                                .iter()
                                .map(|arg| {
                                    self.compile_expr_with_context(arg, None, current_return)
                                })
                                .collect(),
                        }
                    }
                } else if let (ExprKind::Variable(name), [map, key]) =
                    (&callee.kind, args.as_slice())
                {
                    if name == "contains" || name == "map_has" {
                        ByteExprKind::MapContains {
                            map: Box::new(self.compile_expr_with_context(
                                map,
                                None,
                                current_return,
                            )),
                            key: Box::new(self.compile_expr_with_context(
                                key,
                                None,
                                current_return,
                            )),
                        }
                    } else if name == "map_get" {
                        ByteExprKind::MapGet {
                            map: Box::new(self.compile_expr_with_context(
                                map,
                                None,
                                current_return,
                            )),
                            key: Box::new(self.compile_expr_with_context(
                                key,
                                None,
                                current_return,
                            )),
                        }
                    } else {
                        ByteExprKind::Call {
                            callee: Box::new(self.compile_expr_with_context(
                                callee,
                                None,
                                current_return,
                            )),
                            args: args
                                .iter()
                                .map(|arg| {
                                    self.compile_expr_with_context(arg, None, current_return)
                                })
                                .collect(),
                        }
                    }
                } else {
                    ByteExprKind::Call {
                        callee: Box::new(self.compile_expr_with_context(
                            callee,
                            None,
                            current_return,
                        )),
                        args: args
                            .iter()
                            .map(|arg| self.compile_expr_with_context(arg, None, current_return))
                            .collect(),
                    }
                }
            }
            ExprKind::ArrayLiteral { elements } => {
                let element_type = match expected {
                    Some(Type::Array(element)) => element.as_ref().clone(),
                    _ => Type::Null,
                };
                ByteExprKind::ArrayLiteral {
                    element_type,
                    elements: elements
                        .iter()
                        .map(|element| match element {
                            ArrayElement::Expr(value) => ByteArrayElement::Expr(
                                self.compile_expr_with_context(value, None, current_return),
                            ),
                            ArrayElement::Spread(value) => ByteArrayElement::Spread(
                                self.compile_expr_with_context(value, None, current_return),
                            ),
                        })
                        .collect(),
                }
            }
            ExprKind::TupleLiteral { elements } => {
                let element_types = match expected {
                    Some(Type::Tuple(elements)) => elements.clone(),
                    _ => vec![Type::Null; elements.len()],
                };
                ByteExprKind::TupleLiteral {
                    element_types,
                    elements: elements
                        .iter()
                        .map(|element| {
                            self.compile_expr_with_context(element, None, current_return)
                        })
                        .collect(),
                }
            }
            ExprKind::MapLiteral { entries } => {
                let value_type = match expected {
                    Some(Type::Map(value)) => value.as_ref().clone(),
                    _ => Type::Null,
                };
                ByteExprKind::MapLiteral {
                    value_type,
                    entries: entries
                        .iter()
                        .map(|entry| match entry {
                            MapEntry::Entry { key, value } => ByteMapEntry::Entry {
                                key: self.compile_expr_with_context(key, None, current_return),
                                value: self.compile_expr_with_context(value, None, current_return),
                            },
                            MapEntry::Spread(value) => ByteMapEntry::Spread(
                                self.compile_expr_with_context(value, None, current_return),
                            ),
                        })
                        .collect(),
                }
            }
            ExprKind::RecordLiteral { name, fields } => ByteExprKind::RecordLiteral {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|(field, value, _)| {
                        (
                            field.clone(),
                            self.compile_expr_with_context(value, None, current_return),
                        )
                    })
                    .collect(),
            },
            ExprKind::Index { array, index } => ByteExprKind::Index {
                array: Box::new(self.compile_expr_with_context(array, None, current_return)),
                index: Box::new(self.compile_expr_with_context(index, None, current_return)),
            },
            ExprKind::IndexAssign {
                container,
                index,
                value,
            } => ByteExprKind::IndexAssign {
                container: Box::new(self.compile_expr_with_context(
                    container,
                    None,
                    current_return,
                )),
                index: Box::new(self.compile_expr_with_context(index, None, current_return)),
                value: Box::new(self.compile_expr_with_context(value, None, current_return)),
            },
            ExprKind::FunctionLiteral {
                params,
                return_type,
                body,
            } => {
                let body_module = self.compile_statements_with_return(body, Some(return_type));
                ByteExprKind::FunctionLiteral {
                    params: params.clone(),
                    return_type: return_type.clone(),
                    body: body_module,
                }
            }
            ExprKind::Field { receiver, name, .. } => {
                if let ExprKind::Variable(enum_name) = &receiver.kind {
                    if self.enum_names.contains(enum_name) {
                        ByteExprKind::EnumVariant {
                            enum_name: enum_name.clone(),
                            variant_name: name.clone(),
                            payload: None,
                        }
                    } else {
                        ByteExprKind::Field {
                            receiver: Box::new(self.compile_expr_with_context(
                                receiver,
                                None,
                                current_return,
                            )),
                            name: name.clone(),
                        }
                    }
                } else {
                    ByteExprKind::Field {
                        receiver: Box::new(self.compile_expr_with_context(
                            receiver,
                            None,
                            current_return,
                        )),
                        name: name.clone(),
                    }
                }
            }
        };
        ByteExpr {
            kind,
            span: expr.span,
        }
    }

    fn compile_string_interpolation_part(
        &self,
        part: &StringInterpolationPart,
        current_return: Option<&Type>,
    ) -> ByteStringInterpolationPart {
        ByteStringInterpolationPart {
            text: part.text.clone(),
            expression: part
                .expression
                .as_ref()
                .map(|expression| self.compile_expr_with_context(expression, None, current_return)),
        }
    }

    fn compile_record_method_call(
        &self,
        callee: &Expr,
        receiver: &Expr,
        name: &str,
        args: &[Expr],
        current_return: Option<&Type>,
    ) -> ByteExprKind {
        let mut method_args = Vec::with_capacity(args.len() + 1);
        method_args.push(self.compile_expr_with_context(receiver, None, current_return));
        method_args.extend(
            args.iter()
                .map(|arg| self.compile_expr_with_context(arg, None, current_return)),
        );
        ByteExprKind::Call {
            callee: Box::new(ByteExpr {
                kind: ByteExprKind::Get(name.to_string()),
                span: callee.span,
            }),
            args: method_args,
        }
    }

    fn emit_binding_target(
        target: &crate::BindingTarget,
        span: Span,
        instructions: &mut Vec<Instruction>,
    ) {
        match target {
            crate::BindingTarget::Name { name, .. } => {
                instructions.push(Instruction::Define {
                    name: name.clone(),
                    span,
                });
            }
            crate::BindingTarget::Tuple { names, .. } => {
                for (index, name) in names.iter().enumerate() {
                    instructions.push(Instruction::TupleElement { index, span });
                    instructions.push(Instruction::Define {
                        name: name.clone(),
                        span,
                    });
                }
                instructions.push(Instruction::Pop { span });
            }
            crate::BindingTarget::Record { names, .. } => {
                for name in names {
                    instructions.push(Instruction::RecordElement {
                        name: name.clone(),
                        span,
                    });
                    instructions.push(Instruction::Define {
                        name: name.clone(),
                        span,
                    });
                }
                instructions.push(Instruction::Pop { span });
            }
        }
    }

    fn emit_statement(statement: &StatementInstruction, instructions: &mut Vec<Instruction>) {
        match statement {
            StatementInstruction::Let {
                target,
                initializer,
                span,
            } => {
                Self::emit_expr(initializer, instructions);
                Self::emit_binding_target(target, *span, instructions);
            }
            StatementInstruction::Function {
                name,
                type_params,
                params,
                return_type,
                body,
                span,
            } => {
                instructions.push(Instruction::Function {
                    name: name.clone(),
                    type_params: type_params.clone(),
                    params: params.clone(),
                    return_type: return_type.clone(),
                    body: body.clone(),
                    span: *span,
                });
                instructions.push(Instruction::Define {
                    name: name.clone(),
                    span: *span,
                });
            }
            StatementInstruction::Record { .. }
            | StatementInstruction::TypeAlias { .. }
            | StatementInstruction::Enum { .. } => {}
            StatementInstruction::Return { value, span } => {
                Self::emit_expr(value, instructions);
                instructions.push(Instruction::Return { span: *span });
            }
            StatementInstruction::If {
                condition,
                then_body,
                else_body,
                span,
            } => {
                Self::emit_expr(condition, instructions);
                let jump_if_false_index = instructions.len();
                instructions.push(Instruction::JumpIfFalse {
                    target: usize::MAX,
                    span: *span,
                });
                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_child_instructions(then_body, instructions);
                instructions.push(Instruction::EndScope { span: *span });
                let jump_index = instructions.len();
                instructions.push(Instruction::Jump {
                    target: usize::MAX,
                    span: *span,
                });
                let else_start = instructions.len();
                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_child_instructions(else_body, instructions);
                instructions.push(Instruction::EndScope { span: *span });
                let end = instructions.len();
                if let Instruction::JumpIfFalse { target, .. } =
                    &mut instructions[jump_if_false_index]
                {
                    *target = else_start;
                }
                if let Instruction::Jump { target, .. } = &mut instructions[jump_index] {
                    *target = end;
                }
            }
            StatementInstruction::IfLet {
                pattern,
                value,
                then_body,
                else_body,
                span,
            } => {
                let temp_name = format!("$if_let${}", span.start);
                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_expr(value, instructions);
                instructions.push(Instruction::Define {
                    name: temp_name.clone(),
                    span: *span,
                });
                Self::emit_match_case_condition(&temp_name, pattern, *span, instructions);
                let jump_if_false_index = instructions.len();
                instructions.push(Instruction::JumpIfFalse {
                    target: usize::MAX,
                    span: *span,
                });
                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_match_case_payload_binding(&temp_name, pattern, *span, instructions);
                Self::emit_child_instructions(then_body, instructions);
                instructions.push(Instruction::EndScope { span: *span });
                let jump_index = instructions.len();
                instructions.push(Instruction::Jump {
                    target: usize::MAX,
                    span: *span,
                });
                let else_start = instructions.len();
                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_child_instructions(else_body, instructions);
                instructions.push(Instruction::EndScope { span: *span });
                let end = instructions.len();
                if let Instruction::JumpIfFalse { target, .. } =
                    &mut instructions[jump_if_false_index]
                {
                    *target = else_start;
                }
                if let Instruction::Jump { target, .. } = &mut instructions[jump_index] {
                    *target = end;
                }
                instructions.push(Instruction::EndScope { span: *span });
            }
            StatementInstruction::Match {
                value,
                cases,
                default,
                span,
            } => {
                let temp_name = format!("$match${}", span.start);
                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_expr(value, instructions);
                instructions.push(Instruction::Define {
                    name: temp_name.clone(),
                    span: *span,
                });

                let mut end_jumps = Vec::new();
                for case in cases {
                    Self::emit_match_case_condition(
                        &temp_name,
                        &case.pattern,
                        case.span,
                        instructions,
                    );
                    let next_case_jump = instructions.len();
                    instructions.push(Instruction::JumpIfFalse {
                        target: usize::MAX,
                        span: case.span,
                    });
                    instructions.push(Instruction::BeginScope { span: case.span });
                    Self::emit_match_case_payload_binding(
                        &temp_name,
                        &case.pattern,
                        case.span,
                        instructions,
                    );
                    Self::emit_child_instructions(&case.body, instructions);
                    instructions.push(Instruction::EndScope { span: case.span });
                    let end_jump = instructions.len();
                    instructions.push(Instruction::Jump {
                        target: usize::MAX,
                        span: case.span,
                    });
                    let next_case = instructions.len();
                    if let Instruction::JumpIfFalse { target, .. } =
                        &mut instructions[next_case_jump]
                    {
                        *target = next_case;
                    }
                    end_jumps.push(end_jump);
                }

                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_child_instructions(default, instructions);
                instructions.push(Instruction::EndScope { span: *span });
                let end = instructions.len();
                for index in end_jumps {
                    if let Instruction::Jump { target, .. } = &mut instructions[index] {
                        *target = end;
                    }
                }
                instructions.push(Instruction::EndScope { span: *span });
            }
            StatementInstruction::LetElse {
                pattern,
                value,
                else_body,
                span,
            } => {
                let temp_name = format!("$let_else${}", span.start);
                Self::emit_expr(value, instructions);
                instructions.push(Instruction::Define {
                    name: temp_name.clone(),
                    span: *span,
                });
                Self::emit_match_case_condition(&temp_name, pattern, *span, instructions);
                let else_jump_index = instructions.len();
                instructions.push(Instruction::JumpIfFalse {
                    target: usize::MAX,
                    span: *span,
                });
                Self::emit_match_case_payload_binding(&temp_name, pattern, *span, instructions);
                let end_jump_index = instructions.len();
                instructions.push(Instruction::Jump {
                    target: usize::MAX,
                    span: *span,
                });
                let else_start = instructions.len();
                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_child_instructions(else_body, instructions);
                instructions.push(Instruction::EndScope { span: *span });
                let end = instructions.len();
                if let Instruction::JumpIfFalse { target, .. } = &mut instructions[else_jump_index]
                {
                    *target = else_start;
                }
                if let Instruction::Jump { target, .. } = &mut instructions[end_jump_index] {
                    *target = end;
                }
            }
            StatementInstruction::While {
                condition,
                body,
                span,
            } => {
                let loop_start = instructions.len();
                Self::emit_expr(condition, instructions);
                let exit_jump_index = instructions.len();
                instructions.push(Instruction::JumpIfFalse {
                    target: usize::MAX,
                    span: *span,
                });
                let body_start = instructions.len();
                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_child_instructions(body, instructions);
                instructions.push(Instruction::EndScope { span: *span });
                instructions.push(Instruction::Loop {
                    target: loop_start,
                    span: *span,
                });
                let end = instructions.len();
                if let Instruction::JumpIfFalse { target, .. } = &mut instructions[exit_jump_index]
                {
                    *target = end;
                }
                Self::patch_loop_placeholders(instructions, body_start, end, loop_start, end);
            }
            StatementInstruction::WhileLet {
                pattern,
                value,
                body,
                span,
            } => {
                let temp_name = format!("$while_let${}", span.start);
                let loop_start = instructions.len();
                let condition_scope_start = instructions.len();
                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_expr(value, instructions);
                instructions.push(Instruction::Define {
                    name: temp_name.clone(),
                    span: *span,
                });
                Self::emit_match_case_condition(&temp_name, pattern, *span, instructions);
                let exit_jump_index = instructions.len();
                instructions.push(Instruction::JumpIfFalse {
                    target: usize::MAX,
                    span: *span,
                });
                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_match_case_payload_binding(&temp_name, pattern, *span, instructions);
                Self::emit_child_instructions(body, instructions);
                instructions.push(Instruction::EndScope { span: *span });
                let condition_scope_end = instructions.len();
                instructions.push(Instruction::EndScope { span: *span });
                instructions.push(Instruction::Loop {
                    target: loop_start,
                    span: *span,
                });
                let false_exit = instructions.len();
                instructions.push(Instruction::EndScope { span: *span });
                let end = instructions.len();
                if let Instruction::JumpIfFalse { target, .. } = &mut instructions[exit_jump_index]
                {
                    *target = false_exit;
                }
                Self::patch_loop_placeholders(
                    instructions,
                    condition_scope_start,
                    condition_scope_end,
                    loop_start,
                    end,
                );
            }
            StatementInstruction::For {
                name,
                start,
                end,
                body,
                span,
            } => {
                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_expr(start, instructions);
                instructions.push(Instruction::Define {
                    name: name.clone(),
                    span: *span,
                });
                let loop_start = instructions.len();
                instructions.push(Instruction::Load {
                    name: name.clone(),
                    span: *span,
                });
                Self::emit_expr(end, instructions);
                instructions.push(Instruction::Binary {
                    op: BinaryOp::RangeLessThan,
                    span: *span,
                });
                let exit_jump_index = instructions.len();
                instructions.push(Instruction::JumpIfFalse {
                    target: usize::MAX,
                    span: *span,
                });
                let body_start = instructions.len();
                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_child_instructions(body, instructions);
                instructions.push(Instruction::EndScope { span: *span });
                let step_start = instructions.len();
                instructions.push(Instruction::Load {
                    name: name.clone(),
                    span: *span,
                });
                instructions.push(Instruction::Constant {
                    value: Value::Int(1),
                    span: *span,
                });
                instructions.push(Instruction::Binary {
                    op: BinaryOp::Add,
                    span: *span,
                });
                instructions.push(Instruction::Store {
                    name: name.clone(),
                    span: *span,
                });
                instructions.push(Instruction::Drop { span: *span });
                instructions.push(Instruction::Loop {
                    target: loop_start,
                    span: *span,
                });
                let outer_end = instructions.len();
                if let Instruction::JumpIfFalse { target, .. } = &mut instructions[exit_jump_index]
                {
                    *target = outer_end;
                }
                instructions.push(Instruction::EndScope { span: *span });
                Self::patch_loop_placeholders(
                    instructions,
                    body_start,
                    step_start,
                    step_start,
                    outer_end,
                );
            }
            StatementInstruction::Block { body, span } => {
                instructions.push(Instruction::BeginScope { span: *span });
                Self::emit_child_instructions(body, instructions);
                instructions.push(Instruction::EndScope { span: *span });
            }
            StatementInstruction::Break { span } => {
                instructions.push(Instruction::BreakPlaceholder { span: *span });
            }
            StatementInstruction::Continue { span } => {
                instructions.push(Instruction::ContinuePlaceholder { span: *span });
            }
            StatementInstruction::Expression { expression, span } => {
                Self::emit_expr(expression, instructions);
                instructions.push(Instruction::Pop { span: *span });
            }
        }
    }

    fn emit_match_case_condition(
        temp_name: &str,
        pattern: &MatchCaseValue,
        span: Span,
        instructions: &mut Vec<Instruction>,
    ) {
        instructions.push(Instruction::Load {
            name: temp_name.to_string(),
            span,
        });
        instructions.push(Instruction::MatchPattern {
            pattern: pattern.clone(),
            span,
        });
    }

    fn emit_match_case_payload_binding(
        temp_name: &str,
        pattern: &MatchCaseValue,
        span: Span,
        instructions: &mut Vec<Instruction>,
    ) {
        Self::emit_match_pattern_binding(temp_name, pattern, span, instructions);
    }

    fn emit_match_pattern_binding(
        source_name: &str,
        pattern: &MatchCaseValue,
        span: Span,
        instructions: &mut Vec<Instruction>,
    ) {
        match pattern {
            MatchCaseValue::Bind(name) => {
                instructions.push(Instruction::Load {
                    name: source_name.to_string(),
                    span,
                });
                instructions.push(Instruction::Define {
                    name: name.clone(),
                    span,
                });
            }
            MatchCaseValue::Some(inner) => {
                let payload_name = format!("$match_payload${}${}", span.start, instructions.len());
                instructions.push(Instruction::Load {
                    name: source_name.to_string(),
                    span,
                });
                instructions.push(Instruction::OptionPayload { span });
                instructions.push(Instruction::Define {
                    name: payload_name.clone(),
                    span,
                });
                Self::emit_match_pattern_binding(&payload_name, inner, span, instructions);
            }
            MatchCaseValue::Ok(inner) | MatchCaseValue::Err(inner) => {
                let payload_name = format!("$match_payload${}${}", span.start, instructions.len());
                instructions.push(Instruction::Load {
                    name: source_name.to_string(),
                    span,
                });
                instructions.push(Instruction::ResultPayload { span });
                instructions.push(Instruction::Define {
                    name: payload_name.clone(),
                    span,
                });
                Self::emit_match_pattern_binding(&payload_name, inner, span, instructions);
            }
            MatchCaseValue::EnumVariant {
                payload: Some(inner),
                ..
            } => {
                let payload_name = format!("$match_payload${}${}", span.start, instructions.len());
                instructions.push(Instruction::Load {
                    name: source_name.to_string(),
                    span,
                });
                instructions.push(Instruction::EnumPayload { span });
                instructions.push(Instruction::Define {
                    name: payload_name.clone(),
                    span,
                });
                Self::emit_match_pattern_binding(&payload_name, inner, span, instructions);
            }
            MatchCaseValue::Int(_)
            | MatchCaseValue::Float(_)
            | MatchCaseValue::Str(_)
            | MatchCaseValue::IntRange { .. }
            | MatchCaseValue::None
            | MatchCaseValue::EnumVariant { payload: None, .. } => {}
        }
    }

    fn emit_expr(expr: &ByteExpr, instructions: &mut Vec<Instruction>) {
        match &expr.kind {
            ByteExprKind::Constant(value) => instructions.push(Instruction::Constant {
                value: value.clone(),
                span: expr.span,
            }),
            ByteExprKind::Get(name) => instructions.push(Instruction::Load {
                name: name.clone(),
                span: expr.span,
            }),
            ByteExprKind::Assign { name, value } => {
                Self::emit_expr(value, instructions);
                instructions.push(Instruction::Store {
                    name: name.clone(),
                    span: expr.span,
                });
            }
            ByteExprKind::Unary { op, right } => {
                Self::emit_expr(right, instructions);
                instructions.push(Instruction::Unary {
                    op: *op,
                    span: expr.span,
                });
            }
            ByteExprKind::Binary { left, op, right } => {
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    Self::emit_logical_expr(left, *op, right, expr.span, instructions);
                } else {
                    Self::emit_expr(left, instructions);
                    Self::emit_expr(right, instructions);
                    instructions.push(Instruction::Binary {
                        op: *op,
                        span: expr.span,
                    });
                }
            }
            ByteExprKind::StringInterpolation { parts } => {
                for part in parts {
                    if let Some(expression) = &part.expression {
                        Self::emit_expr(expression, instructions);
                    }
                }
                instructions.push(Instruction::StringInterpolate {
                    parts: parts
                        .iter()
                        .map(|part| {
                            if part.expression.is_some() {
                                StringInterpolationInstructionPart::Expression
                            } else {
                                StringInterpolationInstructionPart::Text(part.text.clone())
                            }
                        })
                        .collect(),
                    span: expr.span,
                });
            }
            ByteExprKind::Question { value, return_type } => {
                Self::emit_expr(value, instructions);
                instructions.push(Instruction::Question {
                    return_type: return_type.clone(),
                    span: expr.span,
                });
            }
            ByteExprKind::Call { callee, args } => {
                Self::emit_expr(callee, instructions);
                for arg in args {
                    Self::emit_expr(arg, instructions);
                }
                instructions.push(Instruction::Call {
                    arg_count: args.len(),
                    span: expr.span,
                });
            }
            ByteExprKind::JsonDecode {
                value,
                target_type,
                schema,
            } => {
                Self::emit_expr(value, instructions);
                instructions.push(Instruction::JsonDecode {
                    target_type: target_type.clone(),
                    schema: schema.clone(),
                    span: expr.span,
                });
            }
            ByteExprKind::ArrayLiteral {
                element_type,
                elements,
            } => {
                for element in elements {
                    match element {
                        ByteArrayElement::Expr(value) | ByteArrayElement::Spread(value) => {
                            Self::emit_expr(value, instructions);
                        }
                    }
                }
                instructions.push(Instruction::Array {
                    element_type: element_type.clone(),
                    elements: elements
                        .iter()
                        .map(|element| match element {
                            ByteArrayElement::Expr(_) => ArrayInstructionElement::Expr,
                            ByteArrayElement::Spread(_) => ArrayInstructionElement::Spread,
                        })
                        .collect(),
                    span: expr.span,
                });
            }
            ByteExprKind::TupleLiteral {
                element_types,
                elements,
            } => {
                for element in elements {
                    Self::emit_expr(element, instructions);
                }
                instructions.push(Instruction::Tuple {
                    element_types: element_types.clone(),
                    element_count: elements.len(),
                    span: expr.span,
                });
            }
            ByteExprKind::MapLiteral {
                value_type,
                entries,
            } => {
                for entry in entries {
                    match entry {
                        ByteMapEntry::Entry { key, value } => {
                            Self::emit_expr(key, instructions);
                            Self::emit_expr(value, instructions);
                        }
                        ByteMapEntry::Spread(value) => Self::emit_expr(value, instructions),
                    }
                }
                instructions.push(Instruction::Map {
                    value_type: value_type.clone(),
                    entries: entries
                        .iter()
                        .map(|entry| match entry {
                            ByteMapEntry::Entry { .. } => MapInstructionEntry::Entry,
                            ByteMapEntry::Spread(_) => MapInstructionEntry::Spread,
                        })
                        .collect(),
                    span: expr.span,
                });
            }
            ByteExprKind::Some {
                payload_type,
                payload,
            } => {
                Self::emit_expr(payload, instructions);
                instructions.push(Instruction::Option {
                    payload_type: payload_type.clone(),
                    has_payload: true,
                    span: expr.span,
                });
            }
            ByteExprKind::None { payload_type } => {
                instructions.push(Instruction::Option {
                    payload_type: payload_type.clone(),
                    has_payload: false,
                    span: expr.span,
                });
            }
            ByteExprKind::Ok {
                ok_type,
                err_type,
                payload,
            } => {
                Self::emit_expr(payload, instructions);
                instructions.push(Instruction::Result {
                    ok_type: ok_type.clone(),
                    err_type: err_type.clone(),
                    is_ok: true,
                    span: expr.span,
                });
            }
            ByteExprKind::Err {
                ok_type,
                err_type,
                payload,
            } => {
                Self::emit_expr(payload, instructions);
                instructions.push(Instruction::Result {
                    ok_type: ok_type.clone(),
                    err_type: err_type.clone(),
                    is_ok: false,
                    span: expr.span,
                });
            }
            ByteExprKind::EnumVariant {
                enum_name,
                variant_name,
                payload,
            } => {
                if let Some(payload) = payload {
                    Self::emit_expr(payload, instructions);
                }
                instructions.push(Instruction::EnumVariant {
                    enum_name: enum_name.clone(),
                    variant_name: variant_name.clone(),
                    has_payload: payload.is_some(),
                    span: expr.span,
                });
            }
            ByteExprKind::RecordLiteral { name, fields } => {
                for (_, value) in fields {
                    Self::emit_expr(value, instructions);
                }
                instructions.push(Instruction::Record {
                    name: name.clone(),
                    fields: fields.iter().map(|(field, _)| field.clone()).collect(),
                    span: expr.span,
                });
            }
            ByteExprKind::Index { array, index } => {
                Self::emit_expr(array, instructions);
                Self::emit_expr(index, instructions);
                instructions.push(Instruction::Index { span: expr.span });
            }
            ByteExprKind::IndexAssign {
                container,
                index,
                value,
            } => {
                Self::emit_expr(container, instructions);
                Self::emit_expr(index, instructions);
                Self::emit_expr(value, instructions);
                instructions.push(Instruction::IndexAssign { span: expr.span });
            }
            ByteExprKind::FunctionLiteral {
                params,
                return_type,
                body,
            } => {
                instructions.push(Instruction::Function {
                    name: String::new(),
                    type_params: Vec::new(),
                    params: params.clone(),
                    return_type: return_type.clone(),
                    body: body.clone(),
                    span: expr.span,
                });
            }
            ByteExprKind::Field { receiver, name } => {
                Self::emit_expr(receiver, instructions);
                instructions.push(Instruction::Field {
                    name: name.clone(),
                    span: expr.span,
                });
            }
            ByteExprKind::ArrayLen { value } => {
                Self::emit_expr(value, instructions);
                instructions.push(Instruction::ArrayLen { span: expr.span });
            }
            ByteExprKind::MapContains { map, key } => {
                Self::emit_expr(map, instructions);
                Self::emit_expr(key, instructions);
                instructions.push(Instruction::MapContains { span: expr.span });
            }
            ByteExprKind::MapKeys { map } => {
                Self::emit_expr(map, instructions);
                instructions.push(Instruction::MapKeys { span: expr.span });
            }
            ByteExprKind::MapValues { map } => {
                Self::emit_expr(map, instructions);
                instructions.push(Instruction::MapValues { span: expr.span });
            }
            ByteExprKind::MapSize { map } => {
                Self::emit_expr(map, instructions);
                instructions.push(Instruction::MapSize { span: expr.span });
            }
            ByteExprKind::MapGet { map, key } => {
                Self::emit_expr(map, instructions);
                Self::emit_expr(key, instructions);
                instructions.push(Instruction::MapGet { span: expr.span });
            }
        }
    }

    fn emit_logical_expr(
        left: &ByteExpr,
        op: BinaryOp,
        right: &ByteExpr,
        span: Span,
        instructions: &mut Vec<Instruction>,
    ) {
        Self::emit_expr(left, instructions);
        match op {
            BinaryOp::And => {
                let false_jump = instructions.len();
                instructions.push(Instruction::JumpIfFalse {
                    target: usize::MAX,
                    span,
                });
                Self::emit_expr(right, instructions);
                let end_jump = instructions.len();
                instructions.push(Instruction::Jump {
                    target: usize::MAX,
                    span,
                });
                let false_start = instructions.len();
                instructions.push(Instruction::Constant {
                    value: Value::Bool(false),
                    span,
                });
                let end = instructions.len();
                if let Instruction::JumpIfFalse { target, .. } = &mut instructions[false_jump] {
                    *target = false_start;
                }
                if let Instruction::Jump { target, .. } = &mut instructions[end_jump] {
                    *target = end;
                }
            }
            BinaryOp::Or => {
                let right_jump = instructions.len();
                instructions.push(Instruction::JumpIfFalse {
                    target: usize::MAX,
                    span,
                });
                instructions.push(Instruction::Constant {
                    value: Value::Bool(true),
                    span,
                });
                let end_jump = instructions.len();
                instructions.push(Instruction::Jump {
                    target: usize::MAX,
                    span,
                });
                let right_start = instructions.len();
                Self::emit_expr(right, instructions);
                let end = instructions.len();
                if let Instruction::JumpIfFalse { target, .. } = &mut instructions[right_jump] {
                    *target = right_start;
                }
                if let Instruction::Jump { target, .. } = &mut instructions[end_jump] {
                    *target = end;
                }
            }
            _ => unreachable!("only logical operators use short-circuit emission"),
        }
    }

    fn patch_loop_placeholders(
        instructions: &mut [Instruction],
        scan_start: usize,
        scan_end: usize,
        continue_target: usize,
        break_target: usize,
    ) {
        let mut depth: isize = 0;
        for instruction in instructions[scan_start..scan_end].iter_mut() {
            match instruction {
                Instruction::BeginScope { .. } => depth += 1,
                Instruction::EndScope { .. } => depth -= 1,
                Instruction::BreakPlaceholder { span } => {
                    let span = *span;
                    let exits = if depth > 0 { depth as usize } else { 0 };
                    *instruction = Instruction::BranchExit {
                        exits,
                        target: break_target,
                        span,
                    };
                }
                Instruction::ContinuePlaceholder { span } => {
                    let span = *span;
                    let exits = if depth > 0 { depth as usize } else { 0 };
                    *instruction = Instruction::BranchExit {
                        exits,
                        target: continue_target,
                        span,
                    };
                }
                _ => {}
            }
        }
    }

    fn emit_child_instructions(body: &BytecodeModule, instructions: &mut Vec<Instruction>) {
        let offset = instructions.len();
        instructions.extend(
            body.instructions
                .iter()
                .cloned()
                .map(|instruction| Self::offset_jump_targets(instruction, offset)),
        );
    }

    fn offset_jump_targets(instruction: Instruction, offset: usize) -> Instruction {
        match instruction {
            Instruction::JumpIfFalse { target, span } => Instruction::JumpIfFalse {
                target: target + offset,
                span,
            },
            Instruction::Jump { target, span } => Instruction::Jump {
                target: target + offset,
                span,
            },
            Instruction::Loop { target, span } => Instruction::Loop {
                target: target + offset,
                span,
            },
            Instruction::BranchExit {
                exits,
                target,
                span,
            } => Instruction::BranchExit {
                exits,
                target: target + offset,
                span,
            },
            instruction => instruction,
        }
    }
}

fn is_std_json_from_json_name(name: &str) -> bool {
    name == "$import$std$json$nox$from_json" || name == "__nox_std_json_from_json"
}
