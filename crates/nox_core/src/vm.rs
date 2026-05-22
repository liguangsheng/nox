use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    rc::Rc,
};

use crate::{
    bytecode::Instruction, Array, BinaryOp, BytecodeModule, Diagnostic, Function, FunctionKind,
    GcHeap, Map, OptionValue, Record, ResultValue, ResultVariant, Span, Type, UnaryOp, Value,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Env(Rc<RefCell<EnvData>>);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EnvData {
    values: HashMap<String, Value>,
    parent: Option<Env>,
}

impl Env {
    pub(crate) fn new() -> Self {
        Self(Rc::new(RefCell::new(EnvData {
            values: HashMap::new(),
            parent: None,
        })))
    }

    fn child(parent: Self) -> Self {
        Self(Rc::new(RefCell::new(EnvData {
            values: HashMap::new(),
            parent: Some(parent),
        })))
    }

    fn downgrade(&self) -> std::rc::Weak<RefCell<EnvData>> {
        Rc::downgrade(&self.0)
    }

    fn parent(&self) -> Option<Self> {
        self.0.borrow().parent.clone()
    }

    pub(crate) fn define(&self, name: String, value: Value) {
        self.0.borrow_mut().values.insert(name, value);
    }

    pub(crate) fn get(&self, name: &str) -> Option<Value> {
        let data = self.0.borrow();
        data.values
            .get(name)
            .cloned()
            .or_else(|| data.parent.as_ref().and_then(|parent| parent.get(name)))
    }

    fn assign(&self, name: &str, value: Value) -> bool {
        {
            let mut data = self.0.borrow_mut();
            if data.values.contains_key(name) {
                data.values.insert(name.to_string(), value);
                return true;
            }
        }

        let parent = self.0.borrow().parent.clone();
        parent
            .as_ref()
            .is_some_and(|parent| parent.assign(name, value))
    }
}

pub(crate) enum Control {
    Value(Value),
    Return(Value),
}

pub(crate) struct Vm {
    env: Env,
    instruction_budget: Rc<RefCell<Option<usize>>>,
    heap: Rc<RefCell<GcHeap>>,
}

impl Vm {
    pub(crate) fn new(
        env: Env,
        instruction_budget: Option<usize>,
        heap: Rc<RefCell<GcHeap>>,
    ) -> Self {
        Self {
            env,
            instruction_budget: Rc::new(RefCell::new(instruction_budget)),
            heap,
        }
    }

    fn with_shared_state(
        env: Env,
        instruction_budget: Rc<RefCell<Option<usize>>>,
        heap: Rc<RefCell<GcHeap>>,
    ) -> Self {
        Self {
            env,
            instruction_budget,
            heap,
        }
    }

    pub(crate) fn execute(&mut self, module: &BytecodeModule) -> Result<Control, Diagnostic> {
        self.execute_instructions(module)
    }

    fn execute_child(&self, module: &BytecodeModule, env: Env) -> Result<Control, Diagnostic> {
        Vm::with_shared_state(env, self.instruction_budget.clone(), self.heap.clone())
            .execute(module)
    }

    fn execute_instructions(&mut self, module: &BytecodeModule) -> Result<Control, Diagnostic> {
        let mut pc = 0;
        let mut stack = Vec::new();
        let mut last = Value::Null;
        let mut env = self.env.clone();

        while pc < module.instructions.len() {
            let instruction = &module.instructions[pc];
            self.consume_instruction(flat_instruction_span(instruction))?;
            pc += 1;

            match instruction {
                Instruction::Constant { value, .. } => stack.push(match value {
                    Value::String(value) => {
                        Value::String(self.heap.borrow_mut().alloc_string(value.as_ref()))
                    }
                    _ => value.clone(),
                }),
                Instruction::Load { name, span } => {
                    let value = env.get(name).ok_or_else(|| {
                        Diagnostic::new(format!("undefined variable '{name}'"), *span)
                    })?;
                    stack.push(value);
                }
                Instruction::Store { name, span } => {
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    if env.assign(name, value.clone()) {
                        stack.push(value);
                    } else {
                        return Err(Diagnostic::new(
                            format!("undefined variable '{name}'"),
                            *span,
                        ));
                    }
                }
                Instruction::Define { name, span } => {
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    env.define(name.clone(), value);
                    last = Value::Null;
                }
                Instruction::Function {
                    name,
                    params,
                    return_type,
                    body,
                    ..
                } => {
                    let function = Function {
                        name: name.clone(),
                        params: params.clone(),
                        return_type: return_type.clone(),
                        kind: FunctionKind::Script {
                            body: body.clone(),
                            env: env.downgrade(),
                        },
                    };
                    let function = self.heap.borrow_mut().alloc_function(function);
                    stack.push(Value::Function(function));
                }
                Instruction::Unary { op, span } => {
                    let right = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    stack.push(eval_unary(*span, *op, right)?);
                }
                Instruction::Binary { op, span } => {
                    let right = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let left = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let value = eval_binary(*span, left, *op, right)?;
                    stack.push(match value {
                        Value::String(value) => {
                            Value::String(self.heap.borrow_mut().alloc_string(value.as_ref()))
                        }
                        _ => value,
                    });
                }
                Instruction::Call { arg_count, span } => {
                    if stack.len() < arg_count + 1 {
                        return Err(Diagnostic::new("internal bytecode stack underflow", *span));
                    }
                    let args_start = stack.len() - arg_count;
                    let args = stack.split_off(args_start);
                    let callee = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    stack.push(self.call_value(*span, callee, args)?);
                }
                Instruction::Array {
                    element_type,
                    element_count,
                    span,
                } => {
                    if stack.len() < *element_count {
                        return Err(Diagnostic::new("internal bytecode stack underflow", *span));
                    }
                    let elements_start = stack.len() - element_count;
                    let elements = stack.split_off(elements_start);
                    let array = Array::new(element_type.clone(), elements);
                    let array = self.heap.borrow_mut().alloc_array(array);
                    stack.push(Value::Array(array));
                }
                Instruction::Map {
                    value_type,
                    entry_count,
                    span,
                } => {
                    if stack.len() < entry_count * 2 {
                        return Err(Diagnostic::new("internal bytecode stack underflow", *span));
                    }
                    let entries_start = stack.len() - entry_count * 2;
                    let raw_entries = stack.split_off(entries_start);
                    let mut entries = BTreeMap::new();
                    for pair in raw_entries.chunks_exact(2) {
                        let Value::String(key) = &pair[0] else {
                            return Err(Diagnostic::new("map key must be str", *span));
                        };
                        entries.insert(key.to_string(), pair[1].clone());
                    }
                    let map = Map::new(value_type.clone(), entries);
                    let map = self.heap.borrow_mut().alloc_map(map);
                    stack.push(Value::Map(map));
                }
                Instruction::Option {
                    payload_type,
                    has_payload,
                    span,
                } => {
                    let option = if *has_payload {
                        let payload = stack.pop().ok_or_else(|| {
                            Diagnostic::new("internal bytecode stack underflow", *span)
                        })?;
                        OptionValue::some(payload_type.clone(), payload)
                    } else {
                        OptionValue::none(payload_type.clone())
                    };
                    let option = self.heap.borrow_mut().alloc_option(option);
                    stack.push(Value::Option(option));
                }
                Instruction::Result {
                    ok_type,
                    err_type,
                    is_ok,
                    span,
                } => {
                    let payload = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let result = if *is_ok {
                        ResultValue::ok(ok_type.clone(), err_type.clone(), payload)
                    } else {
                        ResultValue::err(ok_type.clone(), err_type.clone(), payload)
                    };
                    let result = self.heap.borrow_mut().alloc_result(result);
                    stack.push(Value::Result(result));
                }
                Instruction::Record { name, fields, span } => {
                    if stack.len() < fields.len() {
                        return Err(Diagnostic::new("internal bytecode stack underflow", *span));
                    }
                    let values_start = stack.len() - fields.len();
                    let values = stack.split_off(values_start);
                    let entries = fields.iter().cloned().zip(values).collect();
                    let record = Record::new(name.clone(), entries);
                    let record = self.heap.borrow_mut().alloc_record(record);
                    stack.push(Value::Record(record));
                }
                Instruction::Index { span } => {
                    let index = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let indexed = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    match indexed {
                        Value::Array(array) => {
                            let Value::Int(index) = index else {
                                return Err(Diagnostic::new("array index must be int", *span));
                            };
                            let index = usize::try_from(index)
                                .map_err(|_| Diagnostic::new("array index out of bounds", *span))?;
                            let value = array.elements.get(index).cloned().ok_or_else(|| {
                                Diagnostic::new("array index out of bounds", *span)
                            })?;
                            stack.push(value);
                        }
                        Value::Map(map) => {
                            let Value::String(key) = index else {
                                return Err(Diagnostic::new("map key must be str", *span));
                            };
                            let value = map
                                .entries
                                .get(key.as_ref())
                                .cloned()
                                .ok_or_else(|| Diagnostic::new("map key not found", *span))?;
                            stack.push(value);
                        }
                        _ => {
                            return Err(Diagnostic::new(
                                "indexed value is not an array or map",
                                *span,
                            ));
                        }
                    }
                }
                Instruction::Field { name, span } => {
                    let record = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Record(record) = record else {
                        return Err(Diagnostic::new(
                            "field access requires a record value",
                            *span,
                        ));
                    };
                    let value = record.fields.get(name).cloned().ok_or_else(|| {
                        Diagnostic::new(format!("record field '{name}' is missing"), *span)
                    })?;
                    stack.push(value);
                }
                Instruction::ArrayLen { span } => {
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let len = match value {
                        Value::Array(array) => {
                            i64::try_from(array.elements.len()).map_err(|_| {
                                Diagnostic::new("array length does not fit in int", *span)
                            })?
                        }
                        Value::String(string) => {
                            i64::try_from(string.chars().count()).map_err(|_| {
                                Diagnostic::new("string length does not fit in int", *span)
                            })?
                        }
                        _ => {
                            return Err(Diagnostic::new("expected array or str", *span));
                        }
                    };
                    stack.push(Value::Int(len));
                }
                Instruction::MapContains { span } => {
                    let key = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let map = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Map(map) = map else {
                        return Err(Diagnostic::new("expected map", *span));
                    };
                    let Value::String(key) = key else {
                        return Err(Diagnostic::new("expected str key", *span));
                    };
                    stack.push(Value::Bool(map.entries.contains_key(key.as_ref())));
                }
                Instruction::MapGet { span } => {
                    let key = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let map = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Map(map) = map else {
                        return Err(Diagnostic::new("expected map", *span));
                    };
                    let Value::String(key) = key else {
                        return Err(Diagnostic::new("expected str key", *span));
                    };
                    let payload_type = map.value_type.clone();
                    let option = match map.entries.get(key.as_ref()).cloned() {
                        Some(value) => OptionValue::some(payload_type, value),
                        None => OptionValue::none(payload_type),
                    };
                    let option = self.heap.borrow_mut().alloc_option(option);
                    stack.push(Value::Option(option));
                }
                Instruction::OptionIsSome { span } => {
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Option(option) = value else {
                        return Err(Diagnostic::new("expected option", *span));
                    };
                    stack.push(Value::Bool(option.payload.is_some()));
                }
                Instruction::OptionPayload { span } => {
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Option(option) = value else {
                        return Err(Diagnostic::new("expected option", *span));
                    };
                    let payload = option
                        .payload
                        .clone()
                        .ok_or_else(|| Diagnostic::new("option has no payload", *span))?;
                    stack.push(payload);
                }
                Instruction::ResultIsOk { span } => {
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Result(result) = value else {
                        return Err(Diagnostic::new("expected result", *span));
                    };
                    stack.push(Value::Bool(matches!(result.variant, ResultVariant::Ok(_))));
                }
                Instruction::ResultPayload { span } => {
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Result(result) = value else {
                        return Err(Diagnostic::new("expected result", *span));
                    };
                    let payload = match &result.variant {
                        ResultVariant::Ok(payload) | ResultVariant::Err(payload) => payload.clone(),
                    };
                    stack.push(payload);
                }
                Instruction::Pop { span } => {
                    last = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                }
                Instruction::Drop { span } => {
                    stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                }
                Instruction::Return { span } => {
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    self.env = env;
                    return Ok(Control::Return(value));
                }
                Instruction::JumpIfFalse { target, span } => {
                    let condition = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    if !condition.is_truthy() {
                        pc = *target;
                    }
                }
                Instruction::Jump { target, .. } | Instruction::Loop { target, .. } => {
                    pc = *target;
                }
                Instruction::BranchExit {
                    exits,
                    target,
                    span,
                } => {
                    for _ in 0..*exits {
                        env = env.parent().ok_or_else(|| {
                            Diagnostic::new("internal bytecode scope underflow", *span)
                        })?;
                    }
                    pc = *target;
                }
                Instruction::BreakPlaceholder { span }
                | Instruction::ContinuePlaceholder { span } => {
                    return Err(Diagnostic::new(
                        "internal bytecode: unpatched break/continue placeholder",
                        *span,
                    ));
                }
                Instruction::BeginScope { .. } => {
                    env = Env::child(env);
                }
                Instruction::EndScope { span } => {
                    env = env.parent().ok_or_else(|| {
                        Diagnostic::new("internal bytecode scope underflow", *span)
                    })?;
                }
            }
        }

        self.env = env;
        Ok(Control::Value(last))
    }

    fn consume_instruction(&mut self, span: Span) -> Result<(), Diagnostic> {
        let mut budget = self.instruction_budget.borrow_mut();
        let Some(remaining) = &mut *budget else {
            return Ok(());
        };
        if *remaining == 0 {
            return Err(Diagnostic::new(
                "execution cancelled: instruction budget exhausted",
                span,
            ));
        }
        *remaining -= 1;
        Ok(())
    }

    pub(crate) fn call_value(
        &self,
        span: Span,
        callee: Value,
        args: Vec<Value>,
    ) -> Result<Value, Diagnostic> {
        let Value::Function(function) = callee else {
            return Err(Diagnostic::new("called value is not a function", span));
        };
        if args.len() != function.params.len() {
            return Err(Diagnostic::new(
                format!(
                    "expected {} arguments but got {}",
                    function.params.len(),
                    args.len()
                ),
                span,
            ));
        }

        match &function.kind {
            FunctionKind::Script { body, env } => {
                let env = env.upgrade().map(Env).ok_or_else(|| {
                    Diagnostic::new("function environment is no longer available", span)
                })?;
                let call_env = Env::child(env.clone());
                for (param, value) in function.params.iter().zip(args) {
                    call_env.define(param.name.clone(), value);
                }

                match self.execute_child(body, call_env)? {
                    Control::Value(value) | Control::Return(value) => Ok(value),
                }
            }
            FunctionKind::Host { callback } => {
                let value = callback(&args).map_err(|err| {
                    if err.message.contains("host function")
                        || err.message.contains("host callback")
                    {
                        err
                    } else {
                        Diagnostic::new(
                            format!("host function '{}': {}", function.name, err.message),
                            span,
                        )
                    }
                })?;
                let actual = value_type(&value);
                if actual != function.return_type {
                    Err(Diagnostic::new(
                        format!(
                            "host function '{}' returned {}, expected {}",
                            function.name, actual, function.return_type
                        ),
                        span,
                    ))
                } else {
                    Ok(value)
                }
            }
        }
    }
}

pub(crate) fn flat_instruction_span(instruction: &Instruction) -> Span {
    match instruction {
        Instruction::Constant { span, .. }
        | Instruction::Load { span, .. }
        | Instruction::Store { span, .. }
        | Instruction::Define { span, .. }
        | Instruction::Function { span, .. }
        | Instruction::Unary { span, .. }
        | Instruction::Binary { span, .. }
        | Instruction::Call { span, .. }
        | Instruction::Array { span, .. }
        | Instruction::Map { span, .. }
        | Instruction::Option { span, .. }
        | Instruction::Result { span, .. }
        | Instruction::Record { span, .. }
        | Instruction::Index { span }
        | Instruction::Field { span, .. }
        | Instruction::ArrayLen { span }
        | Instruction::MapContains { span }
        | Instruction::MapGet { span }
        | Instruction::OptionIsSome { span }
        | Instruction::OptionPayload { span }
        | Instruction::ResultIsOk { span }
        | Instruction::ResultPayload { span }
        | Instruction::Pop { span }
        | Instruction::Drop { span }
        | Instruction::Return { span }
        | Instruction::JumpIfFalse { span, .. }
        | Instruction::Jump { span, .. }
        | Instruction::Loop { span, .. }
        | Instruction::BranchExit { span, .. }
        | Instruction::BreakPlaceholder { span }
        | Instruction::ContinuePlaceholder { span }
        | Instruction::BeginScope { span }
        | Instruction::EndScope { span } => *span,
    }
}

pub(crate) fn value_type(value: &Value) -> Type {
    match value {
        Value::Null => Type::Null,
        Value::Bool(_) => Type::Bool,
        Value::Int(_) => Type::Int,
        Value::Float(_) => Type::Float,
        Value::String(_) => Type::Str,
        Value::Array(array) => Type::Array(Box::new(array.element_type.clone())),
        Value::Map(map) => Type::Map(Box::new(map.value_type.clone())),
        Value::Option(option) => Type::Option(Box::new(option.payload_type.clone())),
        Value::Result(result) => Type::Result {
            ok: Box::new(result.ok_type.clone()),
            err: Box::new(result.err_type.clone()),
        },
        Value::Record(record) => Type::Record(record.name.clone()),
        Value::Function(function) => function.signature_type(),
    }
}

fn eval_unary(span: Span, op: UnaryOp, right: Value) -> Result<Value, Diagnostic> {
    match op {
        UnaryOp::Not => Ok(Value::Bool(!right.is_truthy())),
        UnaryOp::Negate => match right {
            Value::Int(value) => value
                .checked_neg()
                .map(Value::Int)
                .ok_or_else(|| Diagnostic::new("integer overflow", span)),
            Value::Float(value) => Ok(Value::Float(-value)),
            _ => Err(Diagnostic::new("unary '-' expects int or float", span)),
        },
    }
}

fn eval_binary(span: Span, left: Value, op: BinaryOp, right: Value) -> Result<Value, Diagnostic> {
    match op {
        BinaryOp::And | BinaryOp::Or => Err(Diagnostic::new(
            "logical operator was not lowered to short-circuit bytecode",
            span,
        )),
        BinaryOp::RangeLessThan => match (left, right) {
            (Value::Int(left), Value::Int(right)) => Ok(Value::Bool(left < right)),
            _ => Err(Diagnostic::new(
                "range comparison expects int operands",
                span,
            )),
        },
        BinaryOp::Add => match (left, right) {
            (Value::Int(left), Value::Int(right)) => left
                .checked_add(right)
                .map(Value::Int)
                .ok_or_else(|| Diagnostic::new("integer overflow", span)),
            (Value::Float(left), Value::Float(right)) => finite_float(span, left + right),
            (Value::String(left), Value::String(right)) => {
                Ok(Value::string(format!("{left}{right}")))
            }
            _ => Err(Diagnostic::new(
                "'+' expects matching ints, matching floats, or strings",
                span,
            )),
        },
        BinaryOp::Subtract => {
            numeric_binary_checked(span, left, right, i64::checked_sub, |left, right| {
                left - right
            })
        }
        BinaryOp::Multiply => {
            numeric_binary_checked(span, left, right, i64::checked_mul, |left, right| {
                left * right
            })
        }
        BinaryOp::Divide => numeric_binary_checked_divide(span, left, right),
        BinaryOp::Equal => Ok(Value::Bool(left == right)),
        BinaryOp::NotEqual => Ok(Value::Bool(left != right)),
        BinaryOp::Greater => numeric_compare(
            span,
            left,
            right,
            |left, right| left > right,
            |left, right| left > right,
        ),
        BinaryOp::GreaterEqual => numeric_compare(
            span,
            left,
            right,
            |left, right| left >= right,
            |left, right| left >= right,
        ),
        BinaryOp::Less => numeric_compare(
            span,
            left,
            right,
            |left, right| left < right,
            |left, right| left < right,
        ),
        BinaryOp::LessEqual => numeric_compare(
            span,
            left,
            right,
            |left, right| left <= right,
            |left, right| left <= right,
        ),
    }
}

fn numeric_binary_checked(
    span: Span,
    left: Value,
    right: Value,
    int_op: impl FnOnce(i64, i64) -> Option<i64>,
    float_op: impl FnOnce(f64, f64) -> f64,
) -> Result<Value, Diagnostic> {
    match (left, right) {
        (Value::Int(left), Value::Int(right)) => int_op(left, right)
            .map(Value::Int)
            .ok_or_else(|| Diagnostic::new("integer overflow", span)),
        (Value::Float(left), Value::Float(right)) => finite_float(span, float_op(left, right)),
        _ => Err(Diagnostic::new(
            "operator expects matching int or float operands",
            span,
        )),
    }
}

fn numeric_binary_checked_divide(
    span: Span,
    left: Value,
    right: Value,
) -> Result<Value, Diagnostic> {
    match (left, right) {
        (Value::Int(_), Value::Int(0)) => {
            Err(Diagnostic::new("division by zero", span).with_code("runtime.division-by-zero"))
        }
        (Value::Int(left), Value::Int(right)) => left
            .checked_div(right)
            .map(Value::Int)
            .ok_or_else(|| Diagnostic::new("integer overflow", span)),
        (Value::Float(_), Value::Float(0.0)) => {
            Err(Diagnostic::new("division by zero", span).with_code("runtime.division-by-zero"))
        }
        (Value::Float(left), Value::Float(right)) => finite_float(span, left / right),
        _ => Err(Diagnostic::new(
            "operator expects matching int or float operands",
            span,
        )),
    }
}

fn finite_float(span: Span, value: f64) -> Result<Value, Diagnostic> {
    if value.is_finite() {
        Ok(Value::Float(value))
    } else {
        Err(Diagnostic::new("float result is not finite", span))
    }
}

fn numeric_compare(
    span: Span,
    left: Value,
    right: Value,
    int_op: impl FnOnce(i64, i64) -> bool,
    float_op: impl FnOnce(f64, f64) -> bool,
) -> Result<Value, Diagnostic> {
    match (left, right) {
        (Value::Int(left), Value::Int(right)) => Ok(Value::Bool(int_op(left, right))),
        (Value::Float(left), Value::Float(right)) => Ok(Value::Bool(float_op(left, right))),
        _ => Err(Diagnostic::new(
            "comparison expects matching int or float operands",
            span,
        )),
    }
}
