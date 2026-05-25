use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    panic::{catch_unwind, AssertUnwindSafe},
    rc::Rc,
};

use crate::{
    bytecode::{
        ArrayInstructionElement, Instruction, JsonDecodeSchema, MapInstructionEntry,
        StringInterpolationInstructionPart, TraitMethodDispatch,
    },
    Array, BinaryOp, BytecodeModule, Diagnostic, EnumValue, Function, FunctionKind, GcHeap,
    HostCallbackTracePhase, JsonValue, Map, MatchCaseValue, OptionValue, ProfileReport, Record,
    ResultValue, ResultVariant, Span, Tuple, Type, UnaryOp, Value,
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

struct CallGuard {
    depth: Rc<RefCell<usize>>,
}

impl Drop for CallGuard {
    fn drop(&mut self) {
        let mut depth = self.depth.borrow_mut();
        if *depth > 0 {
            *depth -= 1;
        }
    }
}

pub(crate) struct Vm {
    env: Env,
    instruction_budget: Rc<RefCell<Option<usize>>>,
    heap: Rc<RefCell<GcHeap>>,
    profile: Option<Rc<RefCell<ProfileReport>>>,
    call_depth: Rc<RefCell<usize>>,
    max_call_depth: Option<usize>,
    max_string_length: Option<usize>,
    max_array_length: Option<usize>,
    max_map_entries: Option<usize>,
    max_heap_objects: Option<usize>,
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
            profile: None,
            call_depth: Rc::new(RefCell::new(0)),
            max_call_depth: None,
            max_string_length: None,
            max_array_length: None,
            max_map_entries: None,
            max_heap_objects: None,
        }
    }

    pub(crate) fn new_profiled(
        env: Env,
        instruction_budget: Option<usize>,
        heap: Rc<RefCell<GcHeap>>,
        profile: Rc<RefCell<ProfileReport>>,
    ) -> Self {
        Self {
            env,
            instruction_budget: Rc::new(RefCell::new(instruction_budget)),
            heap,
            profile: Some(profile),
            call_depth: Rc::new(RefCell::new(0)),
            max_call_depth: None,
            max_string_length: None,
            max_array_length: None,
            max_map_entries: None,
            max_heap_objects: None,
        }
    }

    pub(crate) fn set_max_call_depth(&mut self, max: Option<usize>) {
        self.max_call_depth = max;
    }

    pub(crate) fn set_max_string_length(&mut self, max: Option<usize>) {
        self.max_string_length = max;
    }

    pub(crate) fn set_max_array_length(&mut self, max: Option<usize>) {
        self.max_array_length = max;
    }

    pub(crate) fn set_max_map_entries(&mut self, max: Option<usize>) {
        self.max_map_entries = max;
    }

    pub(crate) fn set_max_heap_objects(&mut self, max: Option<usize>) {
        self.max_heap_objects = max;
    }

    #[allow(clippy::too_many_arguments)]
    fn with_shared_state(
        env: Env,
        instruction_budget: Rc<RefCell<Option<usize>>>,
        heap: Rc<RefCell<GcHeap>>,
        profile: Option<Rc<RefCell<ProfileReport>>>,
        call_depth: Rc<RefCell<usize>>,
        max_call_depth: Option<usize>,
        max_string_length: Option<usize>,
        max_array_length: Option<usize>,
        max_map_entries: Option<usize>,
        max_heap_objects: Option<usize>,
    ) -> Self {
        Self {
            env,
            instruction_budget,
            heap,
            profile,
            call_depth,
            max_call_depth,
            max_string_length,
            max_array_length,
            max_map_entries,
            max_heap_objects,
        }
    }

    pub(crate) fn execute(&mut self, module: &BytecodeModule) -> Result<Control, Diagnostic> {
        self.execute_instructions(module)
    }

    fn execute_child(&self, module: &BytecodeModule, env: Env) -> Result<Control, Diagnostic> {
        Vm::with_shared_state(
            env,
            self.instruction_budget.clone(),
            self.heap.clone(),
            self.profile.clone(),
            self.call_depth.clone(),
            self.max_call_depth,
            self.max_string_length,
            self.max_array_length,
            self.max_map_entries,
            self.max_heap_objects,
        )
        .execute(module)
    }

    fn enter_call(&self, span: Span) -> Result<CallGuard, Diagnostic> {
        let mut depth = self.call_depth.borrow_mut();
        *depth += 1;
        if let Some(max) = self.max_call_depth {
            if *depth > max {
                *depth -= 1;
                return Err(Diagnostic::new(
                    format!("call stack depth exceeded configured limit of {max}"),
                    span,
                )
                .with_code("runtime.call-stack-overflow"));
            }
        }
        Ok(CallGuard {
            depth: self.call_depth.clone(),
        })
    }

    fn track_heap_value(&self, value: &Value, span: Span) -> Result<(), Diagnostic> {
        let mut heap = self.heap.borrow_mut();
        heap.track_value(value);
        if let Some(max) = self.max_heap_objects {
            let count = heap.object_count();
            if count > max {
                return Err(Diagnostic::new(
                    format!("heap object count {count} exceeds configured cap of {max}"),
                    span,
                )
                .with_code("runtime.heap-object-cap"));
            }
        }
        Ok(())
    }

    fn decode_json_value(
        &self,
        value: &JsonValue,
        target_type: &Type,
        schema: &JsonDecodeSchema,
        path: &str,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let err = |message: String| Diagnostic::new(message, span).with_code("json.decode");
        match target_type {
            Type::Null => match value {
                JsonValue::Null => Ok(Value::Null),
                other => Err(err(format!(
                    "{}expected null, got {}",
                    decode_path_prefix(path),
                    json_kind_name(other)
                ))),
            },
            Type::Bool => match value {
                JsonValue::Bool(value) => Ok(Value::Bool(*value)),
                other => Err(err(format!(
                    "{}expected bool, got {}",
                    decode_path_prefix(path),
                    json_kind_name(other)
                ))),
            },
            Type::Int => match value {
                JsonValue::Number(value) if value.is_finite() && value.fract() == 0.0 => {
                    Ok(Value::Int(*value as i64))
                }
                JsonValue::Number(value) => Err(err(format!(
                    "{}expected integer JSON number, got {value}",
                    decode_path_prefix(path)
                ))),
                other => Err(err(format!(
                    "{}expected number, got {}",
                    decode_path_prefix(path),
                    json_kind_name(other)
                ))),
            },
            Type::Float => match value {
                JsonValue::Number(value) => Ok(Value::Float(*value)),
                other => Err(err(format!(
                    "{}expected number, got {}",
                    decode_path_prefix(path),
                    json_kind_name(other)
                ))),
            },
            Type::Str => match value {
                JsonValue::String(value) => Ok(Value::string(value.clone())),
                other => Err(err(format!(
                    "{}expected string, got {}",
                    decode_path_prefix(path),
                    json_kind_name(other)
                ))),
            },
            Type::Json => Ok(Value::json(value.clone())),
            Type::Array(element_type) => {
                let JsonValue::Array(items) = value else {
                    return Err(err(format!(
                        "{}expected array, got {}",
                        decode_path_prefix(path),
                        json_kind_name(value)
                    )));
                };
                let mut elements = Vec::with_capacity(items.len());
                for (index, item) in items.iter().enumerate() {
                    elements.push(self.decode_json_value(
                        item,
                        element_type,
                        schema,
                        &decode_index_path(path, index),
                        span,
                    )?);
                }
                let array = Array::new_with_cap(
                    element_type.as_ref().clone(),
                    elements,
                    self.max_array_length,
                );
                let array = self.heap.borrow_mut().alloc_array(array);
                Ok(Value::Array(array))
            }
            Type::Map(value_type) => {
                let JsonValue::Object(entries) = value else {
                    return Err(err(format!(
                        "{}expected object, got {}",
                        decode_path_prefix(path),
                        json_kind_name(value)
                    )));
                };
                let mut out = BTreeMap::new();
                for (key, item) in entries {
                    out.insert(
                        key.clone(),
                        self.decode_json_value(
                            item,
                            value_type,
                            schema,
                            &decode_field_path(path, key),
                            span,
                        )?,
                    );
                }
                let map = Map::new_with_cap(value_type.as_ref().clone(), out, self.max_map_entries);
                let map = self.heap.borrow_mut().alloc_map(map);
                Ok(Value::Map(map))
            }
            Type::Option(payload_type) => {
                if matches!(value, JsonValue::Null) {
                    let option = self
                        .heap
                        .borrow_mut()
                        .alloc_option(OptionValue::none(payload_type.as_ref().clone()));
                    Ok(Value::Option(option))
                } else {
                    let payload =
                        self.decode_json_value(value, payload_type, schema, path, span)?;
                    let option = self
                        .heap
                        .borrow_mut()
                        .alloc_option(OptionValue::some(payload_type.as_ref().clone(), payload));
                    Ok(Value::Option(option))
                }
            }
            Type::Result { ok, err: err_type } => {
                let (variant, payload) = decode_adjacent_payload(value, path, span)?;
                let (is_ok, payload_type) = match variant.as_str() {
                    "ok" => (true, ok.as_ref()),
                    "err" => (false, err_type.as_ref()),
                    other => {
                        return Err(err(format!(
                            "{}unknown result variant {other}",
                            decode_path_prefix(path)
                        )));
                    }
                };
                let payload = self.decode_json_value(payload, payload_type, schema, path, span)?;
                let result = if is_ok {
                    ResultValue::ok(ok.as_ref().clone(), err_type.as_ref().clone(), payload)
                } else {
                    ResultValue::err(ok.as_ref().clone(), err_type.as_ref().clone(), payload)
                };
                let result = self.heap.borrow_mut().alloc_result(result);
                Ok(Value::Result(result))
            }
            Type::Record(name) => {
                if !schema.records.contains_key(name) && schema.enums.contains_key(name) {
                    let (variant, payload) = decode_enum_value(value, path, span)?;
                    let variants = schema.enums.get(name).expect("checked above");
                    let Some((_, payload_type)) =
                        variants.iter().find(|(candidate, _)| candidate == &variant)
                    else {
                        return Err(err(format!(
                            "{}unknown variant {variant}",
                            decode_path_prefix(path)
                        )));
                    };
                    let payload = match (payload_type, payload) {
                        (Some(payload_type), Some(payload)) => Some(self.decode_json_value(
                            payload,
                            payload_type,
                            schema,
                            &decode_field_path(path, "payload"),
                            span,
                        )?),
                        (Some(_), None) => {
                            return Err(err(format!(
                                "{}missing payload for variant {variant}",
                                decode_path_prefix(path)
                            )));
                        }
                        (None, Some(_)) => {
                            return Err(err(format!(
                                "{}unexpected payload for variant {variant}",
                                decode_path_prefix(path)
                            )));
                        }
                        (None, None) => None,
                    };
                    let value = EnumValue::new(name.clone(), variant, payload);
                    let value = self.heap.borrow_mut().alloc_enum(value);
                    return Ok(Value::Enum(value));
                }
                let JsonValue::Object(entries) = value else {
                    return Err(err(format!(
                        "{}expected object for record {name}, got {}",
                        decode_path_prefix(path),
                        json_kind_name(value)
                    )));
                };
                let fields = schema.records.get(name).ok_or_else(|| {
                    err(format!(
                        "{}unknown record type {name}",
                        decode_path_prefix(path)
                    ))
                })?;
                for key in entries.keys() {
                    if !fields.iter().any(|(field, _)| field == key) {
                        return Err(err(format!(
                            "{}unknown field {key}",
                            decode_path_prefix(&decode_field_path(path, key))
                        )));
                    }
                }
                let mut out = BTreeMap::new();
                for (field, field_type) in fields {
                    let field_path = decode_field_path(path, field);
                    let Some(field_value) = entries.get(field) else {
                        return Err(err(format!(
                            "{}missing required field",
                            decode_path_prefix(&field_path)
                        )));
                    };
                    out.insert(
                        field.clone(),
                        self.decode_json_value(field_value, field_type, schema, &field_path, span)?,
                    );
                }
                let record = self
                    .heap
                    .borrow_mut()
                    .alloc_record(Record::new(name.clone(), out));
                Ok(Value::Record(record))
            }
            Type::Enum(name) => {
                let (variant, payload) = decode_enum_value(value, path, span)?;
                let variants = schema.enums.get(name).ok_or_else(|| {
                    err(format!(
                        "{}unknown enum type {name}",
                        decode_path_prefix(path)
                    ))
                })?;
                let Some((_, payload_type)) =
                    variants.iter().find(|(candidate, _)| candidate == &variant)
                else {
                    return Err(err(format!(
                        "{}unknown variant {variant}",
                        decode_path_prefix(path)
                    )));
                };
                let payload = match (payload_type, payload) {
                    (Some(payload_type), Some(payload)) => Some(self.decode_json_value(
                        payload,
                        payload_type,
                        schema,
                        &decode_field_path(path, "payload"),
                        span,
                    )?),
                    (Some(_), None) => {
                        return Err(err(format!(
                            "{}missing payload for variant {variant}",
                            decode_path_prefix(path)
                        )));
                    }
                    (None, Some(_)) => {
                        return Err(err(format!(
                            "{}unexpected payload for variant {variant}",
                            decode_path_prefix(path)
                        )));
                    }
                    (None, None) => None,
                };
                let value = EnumValue::new(name.clone(), variant, payload);
                let value = self.heap.borrow_mut().alloc_enum(value);
                Ok(Value::Enum(value))
            }
            Type::Tuple(_) | Type::Function { .. } | Type::Task(_) | Type::Generic(_) => {
                Err(err(format!(
                    "{}json.from_json does not support target type {target_type}",
                    decode_path_prefix(path)
                )))
            }
        }
    }

    fn operation_start(&self) -> Option<std::time::Instant> {
        self.profile.as_ref().map(|_| std::time::Instant::now())
    }

    fn record_operation(&self, name: &str, start: Option<std::time::Instant>) {
        if let (Some(profile), Some(start)) = (&self.profile, start) {
            profile.borrow_mut().record_operation(name, start.elapsed());
        }
    }

    fn record_statement_coverage(&self, span: Span) {
        if let Some(profile) = &self.profile {
            profile.borrow_mut().record_statement(span);
        }
    }

    fn record_branch_coverage(&self, span: Span, condition_value: bool) {
        if let Some(profile) = &self.profile {
            profile.borrow_mut().record_branch(span, condition_value);
        }
    }

    fn record_host_callback(
        &self,
        name: &str,
        phase: HostCallbackTracePhase,
        span: Span,
        elapsed: std::time::Duration,
        status: Option<&str>,
    ) {
        if let Some(profile) = &self.profile {
            profile
                .borrow_mut()
                .record_host_callback(name, phase, span, elapsed, status);
        }
    }

    fn execute_instructions(&mut self, module: &BytecodeModule) -> Result<Control, Diagnostic> {
        let mut pc = 0;
        let mut stack = Vec::new();
        let mut last = Value::Null;
        let mut env = self.env.clone();

        while pc < module.instructions.len() {
            let instruction = &module.instructions[pc];
            let span = flat_instruction_span(instruction);
            self.consume_instruction(span)?;
            self.record_statement_coverage(span);
            pc += 1;

            match instruction {
                Instruction::Constant { value, span } => {
                    let value = match value {
                        Value::String(value) => {
                            Value::String(self.heap.borrow_mut().alloc_string(value.as_ref()))
                        }
                        _ => value.clone(),
                    };
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
                }
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
                    is_async,
                    type_params,
                    params,
                    return_type,
                    body,
                    span,
                    ..
                } => {
                    let function = Function {
                        name: name.clone(),
                        is_async: *is_async,
                        type_params: type_params.clone(),
                        params: params.clone(),
                        return_type: return_type.clone(),
                        kind: FunctionKind::Script {
                            body: body.clone(),
                            env: env.downgrade(),
                        },
                    };
                    let function = self.heap.borrow_mut().alloc_function(function);
                    let value = Value::Function(function);
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
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
                    if let Value::String(s) = &value {
                        if let Some(cap) = self.max_string_length {
                            if s.as_ref().len() > cap {
                                return Err(Diagnostic::new(
                                    format!(
                                        "string length {} exceeds configured cap of {} bytes",
                                        s.as_ref().len(),
                                        cap
                                    ),
                                    *span,
                                )
                                .with_code("runtime.string-length-cap"));
                            }
                        }
                    }
                    let value = match value {
                        Value::String(value) => {
                            Value::String(self.heap.borrow_mut().alloc_string(value.as_ref()))
                        }
                        _ => value,
                    };
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
                }
                Instruction::StringInterpolate { parts, span } => {
                    let expression_count = parts
                        .iter()
                        .filter(|part| {
                            matches!(part, StringInterpolationInstructionPart::Expression)
                        })
                        .count();
                    if stack.len() < expression_count {
                        return Err(Diagnostic::new("internal bytecode stack underflow", *span));
                    }
                    let values = stack.split_off(stack.len() - expression_count);
                    let mut values = values.into_iter();
                    let mut output = String::new();
                    for part in parts {
                        match part {
                            StringInterpolationInstructionPart::Text(text) => {
                                output.push_str(text);
                            }
                            StringInterpolationInstructionPart::Expression => {
                                let value = values.next().ok_or_else(|| {
                                    Diagnostic::new("internal bytecode stack underflow", *span)
                                })?;
                                output.push_str(&value.to_string());
                            }
                        }
                    }
                    let value = Value::String(self.heap.borrow_mut().alloc_string(&output));
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
                }
                Instruction::Question { return_type, span } => {
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    match value {
                        Value::Option(option) => match &option.payload {
                            Some(payload) => stack.push(payload.clone()),
                            None => {
                                let Type::Option(payload_type) = return_type else {
                                    return Err(Diagnostic::new(
                                        "internal '?' return type mismatch",
                                        *span,
                                    ));
                                };
                                self.env = env;
                                let value = Value::none(payload_type.as_ref().clone());
                                self.track_heap_value(&value, *span)?;
                                return Ok(Control::Return(value));
                            }
                        },
                        Value::Result(result) => match &result.variant {
                            ResultVariant::Ok(payload) => stack.push(payload.clone()),
                            ResultVariant::Err(payload) => {
                                let Type::Result { ok, err } = return_type else {
                                    return Err(Diagnostic::new(
                                        "internal '?' return type mismatch",
                                        *span,
                                    ));
                                };
                                self.env = env;
                                let value = Value::err(
                                    ok.as_ref().clone(),
                                    err.as_ref().clone(),
                                    payload.clone(),
                                );
                                self.track_heap_value(&value, *span)?;
                                return Ok(Control::Return(value));
                            }
                        },
                        _ => {
                            return Err(Diagnostic::new(
                                "'?' expects option or result value",
                                *span,
                            ));
                        }
                    }
                }
                Instruction::Await { span } => {
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Task(task) = value else {
                        return Err(Diagnostic::new("'await' expects task value", *span)
                            .with_code("async.await-non-task"));
                    };
                    let payload = task.await_value(*span)?;
                    self.track_heap_value(&payload, *span)?;
                    stack.push(payload);
                }
                Instruction::MatchPattern { pattern, span } => {
                    let profile_start = self.operation_start();
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let matched = match_pattern(&value, pattern);
                    stack.push(Value::Bool(matched));
                    self.record_operation("match_pattern", profile_start);
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
                Instruction::TraitMethodCall {
                    method_name,
                    dispatch,
                    fallback_function,
                    arg_count,
                    span,
                } => {
                    if stack.len() < arg_count + 1 {
                        return Err(Diagnostic::new("internal bytecode stack underflow", *span));
                    }
                    let args_start = stack.len() - arg_count;
                    let args = stack.split_off(args_start);
                    let receiver = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let mut call_args = Vec::with_capacity(args.len() + 1);
                    call_args.push(receiver.clone());
                    call_args.extend(args);
                    if let Some(function_name) = fallback_function {
                        if let Some(callee) = env.get(function_name) {
                            if record_method_fallback_matches(&callee, &receiver) {
                                stack.push(self.call_value(*span, callee, call_args)?);
                                continue;
                            }
                        }
                    }
                    let function_name =
                        resolve_trait_method_dispatch(method_name, dispatch, &receiver, *span)?;
                    let callee = env.get(function_name).ok_or_else(|| {
                        Diagnostic::new(
                            format!("trait method '{method_name}' implementation is missing"),
                            *span,
                        )
                        .with_code("trait.method-not-found")
                    })?;
                    stack.push(self.call_value(*span, callee, call_args)?);
                }
                Instruction::JsonDecode {
                    target_type,
                    schema,
                    span,
                } => {
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Json(json) = value else {
                        return Err(Diagnostic::new("json.from_json expects json value", *span));
                    };
                    let result =
                        match self.decode_json_value(json.as_ref(), target_type, schema, "", *span)
                        {
                            Ok(decoded) => Value::ok(target_type.clone(), Type::Str, decoded),
                            Err(error) => Value::err(
                                target_type.clone(),
                                Type::Str,
                                Value::string(error.message),
                            ),
                        };
                    self.track_heap_value(&result, *span)?;
                    stack.push(result);
                }
                Instruction::Array {
                    element_type,
                    elements: layout,
                    span,
                } => {
                    let profile_start = self.operation_start();
                    if stack.len() < layout.len() {
                        return Err(Diagnostic::new("internal bytecode stack underflow", *span));
                    }
                    let elements_start = stack.len() - layout.len();
                    let raw_elements = stack.split_off(elements_start);
                    let mut elements = Vec::new();
                    for (kind, value) in layout.iter().zip(raw_elements) {
                        match (kind, value) {
                            (ArrayInstructionElement::Spread, Value::Array(array)) => {
                                array.with_elements(|values| {
                                    elements.extend(values.iter().cloned());
                                });
                            }
                            (ArrayInstructionElement::Spread, _) => {
                                return Err(Diagnostic::new("array spread expects array", *span));
                            }
                            (ArrayInstructionElement::Expr, value) => elements.push(value),
                        }
                    }
                    if let Some(cap) = self.max_array_length {
                        if elements.len() > cap {
                            return Err(Diagnostic::new(
                                format!(
                                    "array length {} exceeds configured cap of {} elements",
                                    elements.len(),
                                    cap
                                ),
                                *span,
                            )
                            .with_code("runtime.array-length-cap"));
                        }
                    }
                    let array =
                        Array::new_with_cap(element_type.clone(), elements, self.max_array_length);
                    let array = self.heap.borrow_mut().alloc_array(array);
                    let value = Value::Array(array);
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
                    self.record_operation("array_literal", profile_start);
                }
                Instruction::Tuple {
                    element_types,
                    element_count,
                    span,
                } => {
                    let profile_start = self.operation_start();
                    if stack.len() < *element_count {
                        return Err(Diagnostic::new("internal bytecode stack underflow", *span));
                    }
                    let elements_start = stack.len() - element_count;
                    let elements = stack.split_off(elements_start);
                    let tuple = Tuple::new(element_types.clone(), elements);
                    let tuple = self.heap.borrow_mut().alloc_tuple(tuple);
                    let value = Value::Tuple(tuple);
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
                    self.record_operation("tuple_literal", profile_start);
                }
                Instruction::Map {
                    value_type,
                    entries: layout,
                    span,
                } => {
                    let profile_start = self.operation_start();
                    let value_count = layout
                        .iter()
                        .map(|entry| match entry {
                            MapInstructionEntry::Entry => 2,
                            MapInstructionEntry::Spread => 1,
                        })
                        .sum::<usize>();
                    if stack.len() < value_count {
                        return Err(Diagnostic::new("internal bytecode stack underflow", *span));
                    }
                    let entries_start = stack.len() - value_count;
                    let raw_entries = stack.split_off(entries_start);
                    let mut entries = BTreeMap::new();
                    let mut index = 0;
                    for kind in layout {
                        match kind {
                            MapInstructionEntry::Spread => {
                                let Value::Map(map) = &raw_entries[index] else {
                                    return Err(Diagnostic::new("map spread expects map", *span));
                                };
                                map.with_entries(|map_entries| {
                                    for (key, value) in map_entries {
                                        entries.insert(key.clone(), value.clone());
                                    }
                                });
                                index += 1;
                            }
                            MapInstructionEntry::Entry => {
                                let Some(value) = raw_entries.get(index + 1) else {
                                    return Err(Diagnostic::new(
                                        "internal bytecode stack underflow",
                                        *span,
                                    ));
                                };
                                let Value::String(key) = &raw_entries[index] else {
                                    return Err(Diagnostic::new("map key must be str", *span));
                                };
                                entries.insert(key.to_string(), value.clone());
                                index += 2;
                            }
                        }
                    }
                    if let Some(cap) = self.max_map_entries {
                        if entries.len() > cap {
                            return Err(Diagnostic::new(
                                format!(
                                    "map size {} exceeds configured cap of {} entries",
                                    entries.len(),
                                    cap
                                ),
                                *span,
                            )
                            .with_code("runtime.map-size-cap"));
                        }
                    }
                    let map = Map::new_with_cap(value_type.clone(), entries, self.max_map_entries);
                    let map = self.heap.borrow_mut().alloc_map(map);
                    let value = Value::Map(map);
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
                    self.record_operation("map_literal", profile_start);
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
                    let value = Value::Option(option);
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
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
                    let value = Value::Result(result);
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
                }
                Instruction::EnumVariant {
                    enum_name,
                    variant_name,
                    has_payload,
                    span,
                } => {
                    let payload = if *has_payload {
                        Some(stack.pop().ok_or_else(|| {
                            Diagnostic::new("internal bytecode stack underflow", *span)
                        })?)
                    } else {
                        None
                    };
                    let value = EnumValue::new(enum_name.clone(), variant_name.clone(), payload);
                    let value = self.heap.borrow_mut().alloc_enum(value);
                    let value = Value::Enum(value);
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
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
                    let value = Value::Record(record);
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
                }
                Instruction::Index { span } => {
                    let profile_start = self.operation_start();
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
                            let value = array.get(index).ok_or_else(|| {
                                Diagnostic::new("array index out of bounds", *span)
                            })?;
                            stack.push(value);
                        }
                        Value::Map(map) => {
                            let Value::String(key) = index else {
                                return Err(Diagnostic::new("map key must be str", *span));
                            };
                            let value = map
                                .get(key.as_ref())
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
                    self.record_operation("index", profile_start);
                }
                Instruction::IndexAssign { span } => {
                    let profile_start = self.operation_start();
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let index = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let container = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    match container {
                        Value::Array(array) => {
                            let Value::Int(index) = index else {
                                return Err(Diagnostic::new("array index must be int", *span));
                            };
                            let len = array.len();
                            let idx = usize::try_from(index).map_err(|_| {
                                Diagnostic::new(
                                    format!(
                                        "index {index} out of bounds for array of length {len}"
                                    ),
                                    *span,
                                )
                                .with_code("runtime.index-out-of-range")
                            })?;
                            array.set(idx, value).map_err(|len| {
                                Diagnostic::new(
                                    format!("index {idx} out of bounds for array of length {len}"),
                                    *span,
                                )
                                .with_code("runtime.index-out-of-range")
                            })?;
                        }
                        Value::Map(map) => {
                            let Value::String(key) = index else {
                                return Err(Diagnostic::new("map key must be str", *span));
                            };
                            let key = key.as_ref().to_string();
                            map.try_set(key, value).map_err(|max| {
                                Diagnostic::new(
                                    format!(
                                        "map size would exceed configured cap of {max} entries"
                                    ),
                                    *span,
                                )
                                .with_code("runtime.map-size-cap")
                            })?;
                        }
                        _ => {
                            return Err(Diagnostic::new(
                                "indexed assignment target is not an array or map",
                                *span,
                            ));
                        }
                    }
                    stack.push(Value::Null);
                    self.record_operation("index_assign", profile_start);
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
                Instruction::RecordElement { name, span } => {
                    let record = stack.last().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Record(record) = record else {
                        return Err(Diagnostic::new(
                            "record destructuring requires a record value",
                            *span,
                        ));
                    };
                    let value = record.fields.get(name).cloned().ok_or_else(|| {
                        Diagnostic::new(format!("record field '{name}' is missing"), *span)
                    })?;
                    stack.push(value);
                }
                Instruction::TupleElement { index, span } => {
                    let tuple = stack.last().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Tuple(tuple) = tuple else {
                        return Err(Diagnostic::new(
                            "tuple element access requires tuple",
                            *span,
                        ));
                    };
                    let value = tuple.elements.get(*index).cloned().ok_or_else(|| {
                        Diagnostic::new(format!("tuple element {index} is missing"), *span)
                    })?;
                    stack.push(value);
                }
                Instruction::ArrayLen { span } => {
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let len = match value {
                        Value::Array(array) => i64::try_from(array.len()).map_err(|_| {
                            Diagnostic::new("array length does not fit in int", *span)
                        })?,
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
                    stack.push(Value::Bool(map.contains_key(key.as_ref())));
                }
                Instruction::MapKeys { span } => {
                    let profile_start = self.operation_start();
                    let map = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Map(map) = map else {
                        return Err(Diagnostic::new("expected map", *span));
                    };
                    let mut heap = self.heap.borrow_mut();
                    let keys: Vec<Value> = map
                        .keys()
                        .into_iter()
                        .map(|key| Value::String(heap.alloc_string(&key)))
                        .collect();
                    let array = heap.alloc_array(Array::new(Type::Str, keys));
                    drop(heap);
                    let value = Value::Array(array);
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
                    self.record_operation("map_keys", profile_start);
                }
                Instruction::MapValues { span } => {
                    let profile_start = self.operation_start();
                    let map = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Map(map) = map else {
                        return Err(Diagnostic::new("expected map", *span));
                    };
                    let array = Array::new(map.value_type().clone(), map.values());
                    let array = self.heap.borrow_mut().alloc_array(array);
                    let value = Value::Array(array);
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
                    self.record_operation("map_values", profile_start);
                }
                Instruction::MapSize { span } => {
                    let map = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Map(map) = map else {
                        return Err(Diagnostic::new("expected map", *span));
                    };
                    let len = i64::try_from(map.len())
                        .map_err(|_| Diagnostic::new("map length does not fit in int", *span))?;
                    stack.push(Value::Int(len));
                }
                Instruction::MapGet { span } => {
                    let profile_start = self.operation_start();
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
                    let payload_type = map.value_type().clone();
                    let option = match map.get(key.as_ref()) {
                        Some(value) => OptionValue::some(payload_type, value),
                        None => OptionValue::none(payload_type),
                    };
                    let option = self.heap.borrow_mut().alloc_option(option);
                    let value = Value::Option(option);
                    self.track_heap_value(&value, *span)?;
                    stack.push(value);
                    self.record_operation("map_get", profile_start);
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
                Instruction::EnumPayload { span } => {
                    let value = stack.pop().ok_or_else(|| {
                        Diagnostic::new("internal bytecode stack underflow", *span)
                    })?;
                    let Value::Enum(value) = value else {
                        return Err(Diagnostic::new("expected enum", *span));
                    };
                    let payload = value
                        .payload
                        .clone()
                        .ok_or_else(|| Diagnostic::new("enum variant has no payload", *span))?;
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
                    let condition_value = condition.is_truthy();
                    self.record_branch_coverage(*span, condition_value);
                    if !condition_value {
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
        let return_type = instantiated_return_type(&function, &args);

        match &function.kind {
            FunctionKind::Script { body, env } => {
                let env = env.upgrade().map(Env).ok_or_else(|| {
                    Diagnostic::new("function environment is no longer available", span)
                })?;
                let call_env = Env::child(env.clone());
                for (param, value) in function.params.iter().zip(args) {
                    call_env.define(param.name.clone(), value);
                }

                let _guard = self.enter_call(span)?;
                let start = std::time::Instant::now();
                let control = self
                    .execute_child(body, call_env)
                    .map_err(|err| err.with_stack_frame(function.name.clone(), span))?;
                if let Some(profile) = &self.profile {
                    profile
                        .borrow_mut()
                        .record_call(&function.name, start.elapsed());
                }
                let value = match control {
                    Control::Value(value) | Control::Return(value) => value,
                };
                if function.is_async {
                    Ok(Value::ready_task(return_type, value))
                } else {
                    Ok(value)
                }
            }
            FunctionKind::Host { callback } => {
                let callback_start = std::time::Instant::now();
                self.record_host_callback(
                    &function.name,
                    HostCallbackTracePhase::Enter,
                    span,
                    std::time::Duration::ZERO,
                    None,
                );
                let result =
                    catch_unwind(AssertUnwindSafe(|| callback(&args))).map_err(|panic| {
                        self.record_host_callback(
                            &function.name,
                            HostCallbackTracePhase::Exit,
                            span,
                            callback_start.elapsed(),
                            Some("panic"),
                        );
                        let message = if let Some(message) = panic.downcast_ref::<&str>() {
                            *message
                        } else if let Some(message) = panic.downcast_ref::<String>() {
                            message.as_str()
                        } else {
                            "unknown panic payload"
                        };
                        Diagnostic::new(
                            format!(
                                "host function '{}': host callback panicked: {message}",
                                function.name
                            ),
                            span,
                        )
                        .with_code("host.callback")
                        .with_host_stack_frame(function.name.clone(), span)
                    })?;
                let value = result.map_err(|mut err| {
                    self.record_host_callback(
                        &function.name,
                        HostCallbackTracePhase::Exit,
                        span,
                        callback_start.elapsed(),
                        Some("error"),
                    );
                    if err.code == "error" {
                        err.code = "host.callback";
                    }
                    let err = if err.message.contains("host function")
                        || err.message.contains("host callback")
                    {
                        err
                    } else {
                        err.message = format!("host function '{}': {}", function.name, err.message);
                        err.span = span;
                        err
                    };
                    err.with_host_stack_frame(function.name.clone(), span)
                })?;
                let actual = value_type(&value);
                let expected = instantiated_return_type(function.as_ref(), &args);
                if actual != expected {
                    self.record_host_callback(
                        &function.name,
                        HostCallbackTracePhase::Exit,
                        span,
                        callback_start.elapsed(),
                        Some("type_error"),
                    );
                    Err(Diagnostic::new(
                        format!(
                            "host function '{}' returned {}, expected {}",
                            function.name, actual, expected
                        ),
                        span,
                    )
                    .with_host_stack_frame(function.name.clone(), span))
                } else {
                    self.track_heap_value(&value, span)?;
                    self.record_operation("host_callback", Some(callback_start));
                    self.record_host_callback(
                        &function.name,
                        HostCallbackTracePhase::Exit,
                        span,
                        callback_start.elapsed(),
                        Some("ok"),
                    );
                    Ok(value)
                }
            }
        }
    }
}

fn resolve_trait_method_dispatch<'a>(
    method_name: &str,
    dispatch: &'a [TraitMethodDispatch],
    receiver: &Value,
    span: Span,
) -> Result<&'a str, Diagnostic> {
    let receiver_type = value_type(receiver).to_string();
    let mut matches = dispatch
        .iter()
        .filter(|entry| entry.receiver_type == receiver_type)
        .map(|entry| entry.function_name.as_str());
    let Some(function_name) = matches.next() else {
        return Err(Diagnostic::new(
            format!(
                "type '{receiver_type}' has no implementation for trait method '{method_name}'"
            ),
            span,
        )
        .with_code("trait.method-not-found"));
    };
    if matches.next().is_some() {
        return Err(Diagnostic::new(
            format!("trait method '{method_name}' is ambiguous for type '{receiver_type}'"),
            span,
        )
        .with_code("trait.method-ambiguous"));
    }
    Ok(function_name)
}

fn record_method_fallback_matches(callee: &Value, receiver: &Value) -> bool {
    let Value::Function(function) = callee else {
        return false;
    };
    function
        .params
        .first()
        .is_some_and(|param| param.ty == value_type(receiver))
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
        | Instruction::StringInterpolate { span, .. }
        | Instruction::Question { span, .. }
        | Instruction::Await { span }
        | Instruction::MatchPattern { span, .. }
        | Instruction::Call { span, .. }
        | Instruction::TraitMethodCall { span, .. }
        | Instruction::JsonDecode { span, .. }
        | Instruction::Array { span, .. }
        | Instruction::Tuple { span, .. }
        | Instruction::Map { span, .. }
        | Instruction::Option { span, .. }
        | Instruction::Result { span, .. }
        | Instruction::EnumVariant { span, .. }
        | Instruction::Record { span, .. }
        | Instruction::Index { span }
        | Instruction::IndexAssign { span }
        | Instruction::Field { span, .. }
        | Instruction::RecordElement { span, .. }
        | Instruction::TupleElement { span, .. }
        | Instruction::ArrayLen { span }
        | Instruction::MapContains { span }
        | Instruction::MapKeys { span }
        | Instruction::MapValues { span }
        | Instruction::MapSize { span }
        | Instruction::MapGet { span }
        | Instruction::OptionPayload { span }
        | Instruction::ResultPayload { span }
        | Instruction::EnumPayload { span }
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
        Value::Json(_) => Type::Json,
        Value::Array(array) => Type::Array(Box::new(array.element_type.clone())),
        Value::Tuple(tuple) => Type::Tuple(tuple.element_types.clone()),
        Value::Map(map) => Type::Map(Box::new(map.value_type.clone())),
        Value::Option(option) => Type::Option(Box::new(option.payload_type.clone())),
        Value::Result(result) => Type::Result {
            ok: Box::new(result.ok_type.clone()),
            err: Box::new(result.err_type.clone()),
        },
        Value::Task(task) => Type::Task(Box::new(task.payload_type.clone())),
        Value::Enum(value) => Type::Enum(value.name.clone()),
        Value::Record(record) => Type::Record(record.name.clone()),
        Value::Function(function) => function.signature_type(),
    }
}

fn instantiated_return_type(function: &Function, args: &[Value]) -> Type {
    if function.type_params.is_empty() {
        return function.return_type.clone();
    }
    let mut bindings = HashMap::new();
    for (param, arg) in function.params.iter().zip(args) {
        bind_generic_types(
            &param.ty,
            &value_type(arg),
            &function.type_params,
            &mut bindings,
        );
    }
    substitute_generic_type(&function.return_type, &bindings)
}

fn bind_generic_types(
    expected: &Type,
    actual: &Type,
    type_params: &[String],
    bindings: &mut HashMap<String, Type>,
) {
    match (expected, actual) {
        (Type::Generic(name), actual) if type_params.iter().any(|param| param == name) => {
            bindings
                .entry(name.clone())
                .or_insert_with(|| actual.clone());
        }
        (Type::Array(expected), Type::Array(actual)) | (Type::Map(expected), Type::Map(actual)) => {
            bind_generic_types(expected, actual, type_params, bindings);
        }
        (Type::Tuple(expected), Type::Tuple(actual)) if expected.len() == actual.len() => {
            for (expected, actual) in expected.iter().zip(actual) {
                bind_generic_types(expected, actual, type_params, bindings);
            }
        }
        (Type::Option(expected), Type::Option(actual)) => {
            bind_generic_types(expected, actual, type_params, bindings);
        }
        (Type::Task(expected), Type::Task(actual)) => {
            bind_generic_types(expected, actual, type_params, bindings);
        }
        (
            Type::Result {
                ok: expected_ok,
                err: expected_err,
            },
            Type::Result {
                ok: actual_ok,
                err: actual_err,
            },
        ) => {
            bind_generic_types(expected_ok, actual_ok, type_params, bindings);
            bind_generic_types(expected_err, actual_err, type_params, bindings);
        }
        _ => {}
    }
}

fn substitute_generic_type(ty: &Type, bindings: &HashMap<String, Type>) -> Type {
    match ty {
        Type::Generic(name) => bindings.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::Array(element) => Type::Array(Box::new(substitute_generic_type(element, bindings))),
        Type::Tuple(elements) => Type::Tuple(
            elements
                .iter()
                .map(|element| substitute_generic_type(element, bindings))
                .collect(),
        ),
        Type::Map(value) => Type::Map(Box::new(substitute_generic_type(value, bindings))),
        Type::Option(value) => Type::Option(Box::new(substitute_generic_type(value, bindings))),
        Type::Task(value) => Type::Task(Box::new(substitute_generic_type(value, bindings))),
        Type::Result { ok, err } => Type::Result {
            ok: Box::new(substitute_generic_type(ok, bindings)),
            err: Box::new(substitute_generic_type(err, bindings)),
        },
        Type::Function {
            type_params,
            params,
            return_type,
        } => Type::Function {
            type_params: type_params.clone(),
            params: params
                .iter()
                .map(|param| substitute_generic_type(param, bindings))
                .collect(),
            return_type: Box::new(substitute_generic_type(return_type, bindings)),
        },
        other => other.clone(),
    }
}

fn match_pattern(value: &Value, pattern: &MatchCaseValue) -> bool {
    match pattern {
        MatchCaseValue::Int(expected) => matches!(value, Value::Int(actual) if actual == expected),
        MatchCaseValue::Float(expected) => {
            matches!(value, Value::Float(actual) if actual == expected)
        }
        MatchCaseValue::Str(expected) => {
            matches!(value, Value::String(actual) if actual.as_ref() == expected)
        }
        MatchCaseValue::IntRange { start, end } => {
            matches!(value, Value::Int(actual) if actual >= start && actual < end)
        }
        MatchCaseValue::Bind(_) => true,
        MatchCaseValue::Some(inner) => match value {
            Value::Option(option) => option
                .payload
                .as_ref()
                .is_some_and(|payload| match_pattern(payload, inner)),
            _ => false,
        },
        MatchCaseValue::None => matches!(value, Value::Option(option) if option.payload.is_none()),
        MatchCaseValue::Ok(inner) => match value {
            Value::Result(result) => match &result.variant {
                ResultVariant::Ok(payload) => match_pattern(payload, inner),
                ResultVariant::Err(_) => false,
            },
            _ => false,
        },
        MatchCaseValue::Err(inner) => match value {
            Value::Result(result) => match &result.variant {
                ResultVariant::Ok(_) => false,
                ResultVariant::Err(payload) => match_pattern(payload, inner),
            },
            _ => false,
        },
        MatchCaseValue::EnumVariant { name, payload } => match value {
            Value::Enum(value) if &value.variant == name => match (&value.payload, payload) {
                (Some(value), Some(pattern)) => match_pattern(value, pattern),
                (None, None) => true,
                _ => false,
            },
            _ => false,
        },
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
        UnaryOp::BitNot => match right {
            Value::Int(value) => Ok(Value::Int(!value)),
            _ => Err(Diagnostic::new("bitwise operator expects int", span)
                .with_code("type.bitwise-non-int")),
        },
    }
}

fn decode_adjacent_payload<'a>(
    value: &'a JsonValue,
    path: &str,
    span: Span,
) -> Result<(String, &'a JsonValue), Diagnostic> {
    let JsonValue::Object(entries) = value else {
        return Err(Diagnostic::new(
            format!(
                "{}expected adjacent object, got {}",
                decode_path_prefix(path),
                json_kind_name(value)
            ),
            span,
        )
        .with_code("json.decode"));
    };
    let Some(JsonValue::String(variant)) = entries.get("_variant") else {
        return Err(Diagnostic::new(
            format!("{}missing string field _variant", decode_path_prefix(path)),
            span,
        )
        .with_code("json.decode"));
    };
    let Some(payload) = entries.get("payload") else {
        return Err(
            Diagnostic::new(format!("{}missing payload", decode_path_prefix(path)), span)
                .with_code("json.decode"),
        );
    };
    Ok((variant.clone(), payload))
}

fn decode_enum_value<'a>(
    value: &'a JsonValue,
    path: &str,
    span: Span,
) -> Result<(String, Option<&'a JsonValue>), Diagnostic> {
    match value {
        JsonValue::String(variant) => Ok((variant.clone(), None)),
        JsonValue::Object(entries) => {
            let Some(JsonValue::String(variant)) = entries.get("_variant") else {
                return Err(Diagnostic::new(
                    format!("{}missing string field _variant", decode_path_prefix(path)),
                    span,
                )
                .with_code("json.decode"));
            };
            Ok((variant.clone(), entries.get("payload")))
        }
        other => Err(Diagnostic::new(
            format!(
                "{}expected enum string or adjacent object, got {}",
                decode_path_prefix(path),
                json_kind_name(other)
            ),
            span,
        )
        .with_code("json.decode")),
    }
}

fn decode_path_prefix(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format!("{path}: ")
    }
}

fn decode_field_path(base: &str, field: &str) -> String {
    if base.is_empty() {
        field.to_string()
    } else {
        format!("{base}.{field}")
    }
}

fn decode_index_path(base: &str, index: usize) -> String {
    if base.is_empty() {
        format!("[{index}]")
    } else {
        format!("{base}[{index}]")
    }
}

fn json_kind_name(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
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
        BinaryOp::BitAnd => bitwise_binary(span, left, right, |left, right| left & right),
        BinaryOp::BitOr => bitwise_binary(span, left, right, |left, right| left | right),
        BinaryOp::BitXor => bitwise_binary(span, left, right, |left, right| left ^ right),
        BinaryOp::ShiftLeft => bitwise_shift(span, left, right, i64::wrapping_shl),
        BinaryOp::ShiftRight => bitwise_shift(span, left, right, i64::wrapping_shr),
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

fn bitwise_binary(
    span: Span,
    left: Value,
    right: Value,
    op: impl FnOnce(i64, i64) -> i64,
) -> Result<Value, Diagnostic> {
    match (left, right) {
        (Value::Int(left), Value::Int(right)) => Ok(Value::Int(op(left, right))),
        _ => Err(
            Diagnostic::new("bitwise operator expects int operands", span)
                .with_code("type.bitwise-non-int"),
        ),
    }
}

fn bitwise_shift(
    span: Span,
    left: Value,
    right: Value,
    op: impl FnOnce(i64, u32) -> i64,
) -> Result<Value, Diagnostic> {
    match (left, right) {
        (Value::Int(left), Value::Int(right)) if (0..64).contains(&right) => {
            Ok(Value::Int(op(left, right as u32)))
        }
        (Value::Int(_), Value::Int(_)) => Err(Diagnostic::new("shift count out of range", span)),
        _ => Err(
            Diagnostic::new("bitwise operator expects int operands", span)
                .with_code("type.bitwise-non-int"),
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
