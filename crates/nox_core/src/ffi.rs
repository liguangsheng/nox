use std::{
    cell::Cell,
    ffi::{c_void, CStr, CString},
    os::raw::c_char,
    ptr,
    rc::Rc,
};

use crate::{
    Array, Diagnostic, Engine, HostFunctionBuilder, Map, OptionValue, Record, ResultValue,
    ResultVariant, ScalarType, Span, Type, Value,
};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoxCoreStatus {
    Ok = 0,
    NullPointer = 1,
    InvalidUtf8 = 2,
    Error = 3,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoxCoreValueKind {
    Null = 0,
    Bool = 1,
    Int = 2,
    Float = 3,
    String = 4,
    Function = 5,
    Array = 6,
    Map = 7,
    Record = 8,
    Option = 9,
    Result = 10,
    Json = 11,
    Tuple = 12,
    Enum = 13,
    Task = 14,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoxCoreValue {
    pub kind: NoxCoreValueKind,
    pub bool_value: bool,
    pub int_value: i64,
    pub float_value: f64,
    pub string_value: *mut c_char,
    pub array_handle: *mut NoxCoreArrayHandle,
    pub map_handle: *mut NoxCoreMapHandle,
    pub record_handle: *mut NoxCoreRecordHandle,
    pub option_handle: *mut NoxCoreOptionHandle,
    pub result_handle: *mut NoxCoreResultHandle,
}

impl NoxCoreValue {
    fn null() -> Self {
        Self {
            kind: NoxCoreValueKind::Null,
            bool_value: false,
            int_value: 0,
            float_value: 0.0,
            string_value: ptr::null_mut(),
            array_handle: ptr::null_mut(),
            map_handle: ptr::null_mut(),
            record_handle: ptr::null_mut(),
            option_handle: ptr::null_mut(),
            result_handle: ptr::null_mut(),
        }
    }
}

impl Default for NoxCoreValue {
    fn default() -> Self {
        Self::null()
    }
}

#[repr(C)]
pub struct NoxCoreArrayHandle {
    array: Rc<Array>,
}

#[repr(C)]
pub struct NoxCoreMapHandle {
    map: Rc<Map>,
}

#[repr(C)]
pub struct NoxCoreRecordHandle {
    record: Rc<Record>,
}

#[repr(C)]
pub struct NoxCoreOptionHandle {
    option: Rc<OptionValue>,
}

#[repr(C)]
pub struct NoxCoreResultHandle {
    result: Rc<ResultValue>,
}

#[repr(C)]
pub struct NoxCoreEngine {
    pub(crate) engine: Engine,
    pub(crate) last_error: Option<CString>,
    pub(crate) userdata: Rc<Cell<*mut c_void>>,
}

pub type NoxCoreHostCallback = unsafe extern "C" fn(
    ctx: *mut c_void,
    args: *const NoxCoreValue,
    arg_count: usize,
    out_value: *mut NoxCoreValue,
) -> NoxCoreStatus;

#[no_mangle]
pub extern "C" fn nox_core_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

/// # Safety
///
/// `value` must be either null or a pointer returned in `NoxCoreValue` by
/// `nox_core_engine_eval` for a string result. It must be freed at most once.
#[no_mangle]
pub unsafe extern "C" fn nox_core_string_free(value: *mut c_char) {
    if !value.is_null() {
        drop(CString::from_raw(value));
    }
}

/// # Safety
///
/// `handle` must be either null or a pointer returned in `NoxCoreValue` for an
/// array result. It must be freed at most once.
#[no_mangle]
pub unsafe extern "C" fn nox_core_array_free(handle: *mut NoxCoreArrayHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// # Safety
///
/// `handle` must be either null or a pointer returned in `NoxCoreValue` for a
/// map result. It must be freed at most once.
#[no_mangle]
pub unsafe extern "C" fn nox_core_map_free(handle: *mut NoxCoreMapHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// # Safety
///
/// `handle` must be either null or a pointer returned in `NoxCoreValue` for a
/// record result. It must be freed at most once.
#[no_mangle]
pub unsafe extern "C" fn nox_core_record_free(handle: *mut NoxCoreRecordHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// # Safety
///
/// `handle` must be either null or a pointer returned in `NoxCoreValue` for an
/// option result. It must be freed at most once.
#[no_mangle]
pub unsafe extern "C" fn nox_core_option_free(handle: *mut NoxCoreOptionHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// # Safety
///
/// `handle` must be either null or a pointer returned in `NoxCoreValue` for a
/// result result. It must be freed at most once.
#[no_mangle]
pub unsafe extern "C" fn nox_core_result_free(handle: *mut NoxCoreResultHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// # Safety
///
/// `handle` must point to a live array handle.
#[no_mangle]
pub unsafe extern "C" fn nox_core_array_len(handle: *const NoxCoreArrayHandle) -> usize {
    if handle.is_null() {
        return 0;
    }
    let handle = &*handle;
    handle.array.len()
}

/// # Safety
///
/// `handle` must point to a live array handle. `out_value` must point to
/// writable storage for one `NoxCoreValue`.
#[no_mangle]
pub unsafe extern "C" fn nox_core_array_get(
    handle: *const NoxCoreArrayHandle,
    index: usize,
    out_value: *mut NoxCoreValue,
) -> NoxCoreStatus {
    if handle.is_null() || out_value.is_null() {
        return NoxCoreStatus::NullPointer;
    }
    let handle = &*handle;
    let Some(value) = handle.array.get(index) else {
        return NoxCoreStatus::Error;
    };
    ptr::write(out_value, value.into());
    NoxCoreStatus::Ok
}

/// # Safety
///
/// `handle` must point to a live map handle.
#[no_mangle]
pub unsafe extern "C" fn nox_core_map_len(handle: *const NoxCoreMapHandle) -> usize {
    if handle.is_null() {
        return 0;
    }
    let handle = &*handle;
    handle.map.len()
}

/// # Safety
///
/// `handle` must point to a live map handle. If `capacity` is non-zero,
/// `out_values` must point to `capacity` writable `NoxCoreValue` slots.
/// `written` must point to writable storage for one `usize`.
#[no_mangle]
pub unsafe extern "C" fn nox_core_map_keys(
    handle: *const NoxCoreMapHandle,
    out_values: *mut NoxCoreValue,
    capacity: usize,
    written: *mut usize,
) -> NoxCoreStatus {
    if handle.is_null() || written.is_null() || (capacity > 0 && out_values.is_null()) {
        return NoxCoreStatus::NullPointer;
    }
    let handle = &*handle;
    let mut count = 0;
    for key in handle.map.keys().into_iter().take(capacity) {
        ptr::write(out_values.add(count), Value::string(key).into());
        count += 1;
    }
    ptr::write(written, count);
    NoxCoreStatus::Ok
}

/// # Safety
///
/// `handle` must point to a live map handle. `key` must point to a valid
/// NUL-terminated UTF-8 string. `out_value` must point to writable storage for
/// one `NoxCoreValue`.
#[no_mangle]
pub unsafe extern "C" fn nox_core_map_get(
    handle: *const NoxCoreMapHandle,
    key: *const c_char,
    out_value: *mut NoxCoreValue,
) -> NoxCoreStatus {
    if handle.is_null() || key.is_null() || out_value.is_null() {
        return NoxCoreStatus::NullPointer;
    }
    let Ok(key) = CStr::from_ptr(key).to_str() else {
        return NoxCoreStatus::InvalidUtf8;
    };
    let handle = &*handle;
    let Some(value) = handle.map.get(key) else {
        return NoxCoreStatus::Error;
    };
    ptr::write(out_value, value.into());
    NoxCoreStatus::Ok
}

/// # Safety
///
/// `handle` must point to a live record handle. `name` must point to a valid
/// NUL-terminated UTF-8 string. `out_value` must point to writable storage for
/// one `NoxCoreValue`.
#[no_mangle]
pub unsafe extern "C" fn nox_core_record_field(
    handle: *const NoxCoreRecordHandle,
    name: *const c_char,
    out_value: *mut NoxCoreValue,
) -> NoxCoreStatus {
    if handle.is_null() || name.is_null() || out_value.is_null() {
        return NoxCoreStatus::NullPointer;
    }
    let Ok(name) = CStr::from_ptr(name).to_str() else {
        return NoxCoreStatus::InvalidUtf8;
    };
    let handle = &*handle;
    let Some(value) = handle.record.fields.get(name).cloned() else {
        return NoxCoreStatus::Error;
    };
    ptr::write(out_value, value.into());
    NoxCoreStatus::Ok
}

/// # Safety
///
/// `handle` must point to a live option handle.
#[no_mangle]
pub unsafe extern "C" fn nox_core_option_is_some(handle: *const NoxCoreOptionHandle) -> bool {
    if handle.is_null() {
        return false;
    }
    let handle = &*handle;
    handle.option.payload.is_some()
}

/// # Safety
///
/// `handle` must point to a live option handle. `out_value` must point to
/// writable storage for one `NoxCoreValue`.
#[no_mangle]
pub unsafe extern "C" fn nox_core_option_payload(
    handle: *const NoxCoreOptionHandle,
    out_value: *mut NoxCoreValue,
) -> NoxCoreStatus {
    if handle.is_null() || out_value.is_null() {
        return NoxCoreStatus::NullPointer;
    }
    let handle = &*handle;
    let Some(payload) = handle.option.payload.clone() else {
        return NoxCoreStatus::Error;
    };
    ptr::write(out_value, payload.into());
    NoxCoreStatus::Ok
}

/// # Safety
///
/// `handle` must point to a live result handle.
#[no_mangle]
pub unsafe extern "C" fn nox_core_result_is_ok(handle: *const NoxCoreResultHandle) -> bool {
    if handle.is_null() {
        return false;
    }
    let handle = &*handle;
    matches!(handle.result.variant, ResultVariant::Ok(_))
}

/// # Safety
///
/// `handle` must point to a live result handle. `out_value` must point to
/// writable storage for one `NoxCoreValue`.
#[no_mangle]
pub unsafe extern "C" fn nox_core_result_payload(
    handle: *const NoxCoreResultHandle,
    out_value: *mut NoxCoreValue,
) -> NoxCoreStatus {
    if handle.is_null() || out_value.is_null() {
        return NoxCoreStatus::NullPointer;
    }
    let handle = &*handle;
    let payload = match &handle.result.variant {
        ResultVariant::Ok(payload) | ResultVariant::Err(payload) => payload.clone(),
    };
    ptr::write(out_value, payload.into());
    NoxCoreStatus::Ok
}

impl TryFrom<NoxCoreValueKind> for ScalarType {
    type Error = ();

    fn try_from(value: NoxCoreValueKind) -> Result<Self, Self::Error> {
        match value {
            NoxCoreValueKind::Null => Ok(Self::Null),
            NoxCoreValueKind::Bool => Ok(Self::Bool),
            NoxCoreValueKind::Int => Ok(Self::Int),
            NoxCoreValueKind::Float => Ok(Self::Float),
            NoxCoreValueKind::String
            | NoxCoreValueKind::Json
            | NoxCoreValueKind::Tuple
            | NoxCoreValueKind::Enum
            | NoxCoreValueKind::Function
            | NoxCoreValueKind::Task
            | NoxCoreValueKind::Array
            | NoxCoreValueKind::Map
            | NoxCoreValueKind::Record
            | NoxCoreValueKind::Option
            | NoxCoreValueKind::Result => Err(()),
        }
    }
}

impl From<Value> for NoxCoreValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Self { ..Self::null() },
            Value::Bool(value) => Self {
                kind: NoxCoreValueKind::Bool,
                bool_value: value,
                ..Self::null()
            },
            Value::Int(value) => Self {
                kind: NoxCoreValueKind::Int,
                int_value: value,
                ..Self::null()
            },
            Value::Float(value) => Self {
                kind: NoxCoreValueKind::Float,
                float_value: value,
                ..Self::null()
            },
            Value::String(value) => Self {
                kind: NoxCoreValueKind::String,
                string_value: CString::new(value.as_ref().replace('\0', "\\0"))
                    .expect("sanitized string has no interior NUL")
                    .into_raw(),
                ..Self::null()
            },
            Value::Json(value) => Self {
                kind: NoxCoreValueKind::Json,
                string_value: CString::new(value.to_string())
                    .expect("serialized JSON has no interior NUL")
                    .into_raw(),
                ..Self::null()
            },
            Value::Array(array) => Self {
                kind: NoxCoreValueKind::Array,
                array_handle: Box::into_raw(Box::new(NoxCoreArrayHandle { array })),
                ..Self::null()
            },
            Value::Tuple(_) => Self {
                kind: NoxCoreValueKind::Tuple,
                ..Self::null()
            },
            Value::Map(map) => Self {
                kind: NoxCoreValueKind::Map,
                map_handle: Box::into_raw(Box::new(NoxCoreMapHandle { map })),
                ..Self::null()
            },
            Value::Record(record) => Self {
                kind: NoxCoreValueKind::Record,
                record_handle: Box::into_raw(Box::new(NoxCoreRecordHandle { record })),
                ..Self::null()
            },
            Value::Option(option) => Self {
                kind: NoxCoreValueKind::Option,
                option_handle: Box::into_raw(Box::new(NoxCoreOptionHandle { option })),
                ..Self::null()
            },
            Value::Result(result) => Self {
                kind: NoxCoreValueKind::Result,
                result_handle: Box::into_raw(Box::new(NoxCoreResultHandle { result })),
                ..Self::null()
            },
            Value::Task(_) => Self {
                kind: NoxCoreValueKind::Task,
                ..Self::null()
            },
            Value::Enum(_) => Self {
                kind: NoxCoreValueKind::Enum,
                ..Self::null()
            },
            Value::Function(_) => Self {
                kind: NoxCoreValueKind::Function,
                ..Self::null()
            },
        }
    }
}

impl TryFrom<NoxCoreValue> for Value {
    type Error = ();

    fn try_from(value: NoxCoreValue) -> Result<Self, Self::Error> {
        match value.kind {
            NoxCoreValueKind::Null => Ok(Self::Null),
            NoxCoreValueKind::Bool => Ok(Self::Bool(value.bool_value)),
            NoxCoreValueKind::Int => Ok(Self::Int(value.int_value)),
            NoxCoreValueKind::Float => Ok(Self::Float(value.float_value)),
            NoxCoreValueKind::String
            | NoxCoreValueKind::Json
            | NoxCoreValueKind::Tuple
            | NoxCoreValueKind::Enum
            | NoxCoreValueKind::Function
            | NoxCoreValueKind::Task
            | NoxCoreValueKind::Array
            | NoxCoreValueKind::Map
            | NoxCoreValueKind::Record
            | NoxCoreValueKind::Option
            | NoxCoreValueKind::Result => Err(()),
        }
    }
}

#[no_mangle]
pub extern "C" fn nox_core_engine_new() -> *mut NoxCoreEngine {
    Box::into_raw(Box::new(NoxCoreEngine {
        engine: Engine::new(),
        last_error: None,
        userdata: Rc::new(Cell::new(ptr::null_mut())),
    }))
}

impl NoxCoreEngine {
    fn clear_error(&mut self) {
        self.last_error = None;
    }

    fn set_error(&mut self, message: impl AsRef<str>) {
        let sanitized = message.as_ref().replace('\0', "\\0");
        self.last_error = CString::new(sanitized).ok();
    }
}

/// # Safety
///
/// `engine` must be either null or a pointer returned by `nox_core_engine_new`
/// that has not already been freed. After this call, the pointer must not be
/// used again.
#[no_mangle]
pub unsafe extern "C" fn nox_core_engine_free(engine: *mut NoxCoreEngine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine));
    }
}

/// # Safety
///
/// `engine` must point to a live engine returned by `nox_core_engine_new`.
/// `userdata` is stored verbatim and never dereferenced or freed by Nox.
#[no_mangle]
pub unsafe extern "C" fn nox_core_engine_set_userdata(
    engine: *mut NoxCoreEngine,
    userdata: *mut c_void,
) -> NoxCoreStatus {
    if engine.is_null() {
        return NoxCoreStatus::NullPointer;
    }
    (*engine).userdata.set(userdata);
    NoxCoreStatus::Ok
}

/// # Safety
///
/// `engine` must point to a live engine returned by `nox_core_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn nox_core_engine_userdata(engine: *const NoxCoreEngine) -> *mut c_void {
    if engine.is_null() {
        return ptr::null_mut();
    }
    (*engine).userdata.get()
}

/// # Safety
///
/// `engine` must point to a live engine returned by `nox_core_engine_new`.
/// `name` must point to a valid NUL-terminated UTF-8 string. If `param_count`
/// is non-zero, `param_types` must point to `param_count` readable value-kind
/// entries. `callback` must remain callable for as long as the registered
/// function can be invoked by the engine.
#[no_mangle]
pub unsafe extern "C" fn nox_core_engine_register_host_function(
    engine: *mut NoxCoreEngine,
    name: *const c_char,
    param_types: *const NoxCoreValueKind,
    param_count: usize,
    return_type: NoxCoreValueKind,
    callback: Option<NoxCoreHostCallback>,
    ctx: *mut c_void,
) -> NoxCoreStatus {
    register_c_host_function(CHostRegistration {
        engine,
        name,
        param_types,
        param_count,
        return_type,
        callback,
        ctx,
        docstring: ptr::null(),
        capabilities: ptr::null(),
        capability_count: 0,
    })
}

/// # Safety
///
/// `engine` must point to a live engine returned by `nox_core_engine_new`.
/// `name` must point to a valid NUL-terminated UTF-8 string. If `param_count`
/// is non-zero, `param_types` must point to `param_count` readable value-kind
/// entries. `callback` must remain callable for as long as the registered
/// function can be invoked by the engine. `docstring` may be null. If
/// `capability_count` is non-zero, `capabilities` must point to
/// `capability_count` readable NUL-terminated UTF-8 string pointers.
#[no_mangle]
pub unsafe extern "C" fn nox_core_engine_register_host_function_ex(
    engine: *mut NoxCoreEngine,
    name: *const c_char,
    param_types: *const NoxCoreValueKind,
    param_count: usize,
    return_type: NoxCoreValueKind,
    callback: Option<NoxCoreHostCallback>,
    ctx: *mut c_void,
    docstring: *const c_char,
    capabilities: *const *const c_char,
    capability_count: usize,
) -> NoxCoreStatus {
    register_c_host_function(CHostRegistration {
        engine,
        name,
        param_types,
        param_count,
        return_type,
        callback,
        ctx,
        docstring,
        capabilities,
        capability_count,
    })
}

struct CHostRegistration {
    engine: *mut NoxCoreEngine,
    name: *const c_char,
    param_types: *const NoxCoreValueKind,
    param_count: usize,
    return_type: NoxCoreValueKind,
    callback: Option<NoxCoreHostCallback>,
    ctx: *mut c_void,
    docstring: *const c_char,
    capabilities: *const *const c_char,
    capability_count: usize,
}

unsafe fn register_c_host_function(registration: CHostRegistration) -> NoxCoreStatus {
    let CHostRegistration {
        engine,
        name,
        param_types,
        param_count,
        return_type,
        callback,
        ctx,
        docstring,
        capabilities,
        capability_count,
    } = registration;

    if engine.is_null() || name.is_null() || callback.is_none() {
        return NoxCoreStatus::NullPointer;
    }
    (*engine).clear_error();
    if param_count > 0 && param_types.is_null() {
        (*engine).set_error("param_types cannot be null when param_count is non-zero");
        return NoxCoreStatus::NullPointer;
    }
    if capability_count > 0 && capabilities.is_null() {
        (*engine).set_error("capabilities cannot be null when capability_count is non-zero");
        return NoxCoreStatus::NullPointer;
    }

    let Ok(name) = CStr::from_ptr(name).to_str() else {
        (*engine).set_error("host function name is not valid UTF-8");
        return NoxCoreStatus::InvalidUtf8;
    };
    let docstring = if docstring.is_null() {
        None
    } else {
        let Ok(value) = CStr::from_ptr(docstring).to_str() else {
            (*engine).set_error("host function docstring is not valid UTF-8");
            return NoxCoreStatus::InvalidUtf8;
        };
        Some(value.to_string())
    };
    let raw_capabilities = if capability_count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(capabilities, capability_count)
    };
    let mut capability_names = Vec::with_capacity(raw_capabilities.len());
    for (index, capability) in raw_capabilities.iter().copied().enumerate() {
        if capability.is_null() {
            (*engine).set_error(format!("host capability at index {index} is null"));
            return NoxCoreStatus::NullPointer;
        }
        let Ok(value) = CStr::from_ptr(capability).to_str() else {
            (*engine).set_error(format!(
                "host capability at index {index} is not valid UTF-8"
            ));
            return NoxCoreStatus::InvalidUtf8;
        };
        capability_names.push(value.to_string());
    }

    let Ok(return_type) = ScalarType::try_from(return_type) else {
        (*engine).set_error("unsupported C ABI host function return type");
        return NoxCoreStatus::Error;
    };

    let raw_params = if param_count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(param_types, param_count)
    };
    let mut builder = HostFunctionBuilder::new(name, Type::from(return_type));
    for (index, kind) in raw_params.iter().copied().enumerate() {
        let Ok(ty) = ScalarType::try_from(kind) else {
            (*engine).set_error(format!(
                "unsupported C ABI host parameter type at index {index}"
            ));
            return NoxCoreStatus::Error;
        };
        builder = builder.param(format!("arg{index}"), Type::from(ty));
    }
    if let Some(docstring) = docstring {
        builder = builder.docstring(docstring);
    }
    for capability in capability_names {
        builder = builder.capability(capability);
    }

    let callback = callback.expect("checked above");
    let host_name = name.to_string();
    let engine_userdata = (*engine).userdata.clone();
    let fixed_ctx = ctx;
    let result = (*engine)
        .engine
        .register_host_function(builder, move |args| {
            let c_args = args
                .iter()
                .cloned()
                .map(NoxCoreValue::from)
                .collect::<Vec<_>>();
            let mut out = NoxCoreValue::null();
            let callback_ctx = if fixed_ctx.is_null() {
                engine_userdata.get()
            } else {
                fixed_ctx
            };
            let status = unsafe { callback(callback_ctx, c_args.as_ptr(), c_args.len(), &mut out) };
            if status != NoxCoreStatus::Ok {
                return Err(Diagnostic::new(
                    format!("host callback '{host_name}' returned status {status:?}"),
                    Span { start: 0, end: 0 },
                ));
            }
            Value::try_from(out).map_err(|_| {
                Diagnostic::new(
                    format!("host callback '{host_name}' returned unsupported value kind"),
                    Span { start: 0, end: 0 },
                )
            })
        });

    match result {
        Ok(()) => NoxCoreStatus::Ok,
        Err(err) => {
            (*engine).set_error(err.to_string());
            NoxCoreStatus::Error
        }
    }
}

/// # Safety
///
/// `engine` must point to a live engine returned by `nox_core_engine_new`.
/// `source` must point to a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn nox_core_engine_check(
    engine: *mut NoxCoreEngine,
    source: *const c_char,
) -> NoxCoreStatus {
    if engine.is_null() || source.is_null() {
        return NoxCoreStatus::NullPointer;
    }
    (*engine).clear_error();

    let Ok(source) = CStr::from_ptr(source).to_str() else {
        (*engine).set_error("source is not valid UTF-8");
        return NoxCoreStatus::InvalidUtf8;
    };

    match (*engine).engine.check(source) {
        Ok(()) => NoxCoreStatus::Ok,
        Err(err) => {
            (*engine).set_error(err.to_string());
            NoxCoreStatus::Error
        }
    }
}

/// # Safety
///
/// `engine` must point to a live engine returned by `nox_core_engine_new`.
/// `source` must point to a valid NUL-terminated UTF-8 string. `out_value` must
/// point to writable storage for one `NoxCoreValue`.
#[no_mangle]
pub unsafe extern "C" fn nox_core_engine_eval(
    engine: *mut NoxCoreEngine,
    source: *const c_char,
    out_value: *mut NoxCoreValue,
) -> NoxCoreStatus {
    if engine.is_null() || source.is_null() || out_value.is_null() {
        return NoxCoreStatus::NullPointer;
    }
    (*engine).clear_error();

    let Ok(source) = CStr::from_ptr(source).to_str() else {
        (*engine).set_error("source is not valid UTF-8");
        return NoxCoreStatus::InvalidUtf8;
    };

    match (*engine).engine.eval(source) {
        Ok(value) => {
            ptr::write(out_value, value.into());
            NoxCoreStatus::Ok
        }
        Err(err) => {
            (*engine).set_error(err.to_string());
            NoxCoreStatus::Error
        }
    }
}

/// # Safety
///
/// `engine` must point to a live engine returned by `nox_core_engine_new`.
/// The returned pointer is owned by the engine and remains valid until the next
/// operation that mutates the engine's last-error slot or until engine free.
#[no_mangle]
pub unsafe extern "C" fn nox_core_engine_last_error(engine: *const NoxCoreEngine) -> *const c_char {
    if engine.is_null() {
        return ptr::null();
    }
    (*engine)
        .last_error
        .as_ref()
        .map_or(ptr::null(), |error| error.as_ptr())
}

/// # Safety
///
/// `engine` must be either null or point to a live engine returned by
/// `nox_core_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn nox_core_engine_clear_error(engine: *mut NoxCoreEngine) {
    if !engine.is_null() {
        (*engine).clear_error();
    }
}
