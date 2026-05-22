use super::*;
use std::{
    cell::Cell,
    ffi::{c_void, CStr},
    ptr,
    rc::Rc,
};

fn c_engine() -> NoxCoreEngine {
    NoxCoreEngine {
        engine: Engine::new(),
        last_error: None,
        userdata: Rc::new(Cell::new(ptr::null_mut())),
    }
}

#[test]
fn c_abi_exposes_version_string() {
    let version = unsafe { CStr::from_ptr(nox_core_version()) }
        .to_str()
        .unwrap();
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn c_abi_enum_values_are_stable() {
    assert_eq!(NoxCoreStatus::Ok as i32, 0);
    assert_eq!(NoxCoreStatus::NullPointer as i32, 1);
    assert_eq!(NoxCoreStatus::InvalidUtf8 as i32, 2);
    assert_eq!(NoxCoreStatus::Error as i32, 3);

    assert_eq!(NoxCoreValueKind::Null as i32, 0);
    assert_eq!(NoxCoreValueKind::Bool as i32, 1);
    assert_eq!(NoxCoreValueKind::Int as i32, 2);
    assert_eq!(NoxCoreValueKind::Float as i32, 3);
    assert_eq!(NoxCoreValueKind::String as i32, 4);
    assert_eq!(NoxCoreValueKind::Function as i32, 5);
    assert_eq!(NoxCoreValueKind::Array as i32, 6);
    assert_eq!(NoxCoreValueKind::Map as i32, 7);
    assert_eq!(NoxCoreValueKind::Record as i32, 8);
    assert_eq!(NoxCoreValueKind::Option as i32, 9);
    assert_eq!(NoxCoreValueKind::Result as i32, 10);
}

#[test]
fn runs_registered_rust_host_function() {
    let mut engine = Engine::new();
    engine
        .register_host_function(
            HostFunctionBuilder::new("host_add", Type::Int)
                .param("left", Type::Int)
                .param("right", Type::Int),
            |args| match args {
                [Value::Int(left), Value::Int(right)] => Ok(Value::Int(left + right)),
                _ => unreachable!("static checker guarantees host_add argument types"),
            },
        )
        .unwrap();

    let value = engine.eval("host_add(20, 22);").unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn checks_registered_host_function_return_type() {
    let mut engine = Engine::new();
    engine
        .register_host_function(HostFunctionBuilder::new("bad", Type::Int), |_| {
            Ok(Value::Bool(true))
        })
        .unwrap();

    let err = engine.eval("bad();").unwrap_err();
    assert!(err.message.contains("returned bool, expected int"));
}

#[test]
fn run_tests_calls_top_level_test_functions() {
    let mut engine = Engine::new();
    let result = engine
        .run_tests(
            "fn helper() -> int { return 21; }\n\
             fn test_pass() -> bool { return helper() * 2 == 42; }\n\
             fn test_fail() -> bool { return false; }\n",
        )
        .unwrap();

    assert_eq!(result.tests.len(), 2);
    assert_eq!(result.tests[0].name, "test_pass");
    assert!(result.tests[0].passed);
    assert!(result.tests[0].diagnostic.is_none());
    assert_eq!(result.tests[1].name, "test_fail");
    assert!(!result.tests[1].passed);
    assert!(result.tests[1].diagnostic.is_none());
}

#[test]
fn run_tests_rejects_invalid_test_signature() {
    let mut engine = Engine::new();
    let err = engine
        .run_tests("fn test_bad(value: int) -> bool { return value == 1; }\n")
        .unwrap_err();

    assert_eq!(err.code, "test.signature");
    assert!(err.message.contains("must not take parameters"));
}

#[test]
fn host_callback_error_includes_function_name() {
    let mut engine = Engine::new();
    engine
        .register_host_function(HostFunctionBuilder::new("explode", Type::Int), |_| {
            Err(crate::Diagnostic::new(
                "boom",
                crate::Span { start: 0, end: 0 },
            ))
        })
        .unwrap();

    let err = engine.eval("explode();").unwrap_err();
    assert_eq!(err.code, "host.callback");
    assert!(
        err.message.contains("host function 'explode'"),
        "expected message to name the host function, got: {}",
        err.message
    );
    assert!(err.message.contains("boom"));
}

#[test]
fn host_callback_error_preserves_diagnostic_code() {
    let mut engine = Engine::new();
    engine
        .register_host_function(HostFunctionBuilder::new("deny", Type::Int), |_| {
            Err(
                crate::Diagnostic::new("permission denied", crate::Span { start: 0, end: 0 })
                    .with_code("permission.denied"),
            )
        })
        .unwrap();

    let err = engine.eval("deny();").unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("host function 'deny'"));
    assert!(err.message.contains("permission denied"));
}

#[test]
fn host_callback_error_preserves_explicit_source_label() {
    let mut engine = Engine::new();
    engine
        .register_host_function(HostFunctionBuilder::new("custom", Type::Int), |_| {
            Err(crate::Diagnostic::new(
                "host function 'custom': already labeled",
                crate::Span { start: 0, end: 0 },
            ))
        })
        .unwrap();

    let err = engine.eval("custom();").unwrap_err();
    assert_eq!(err.code, "host.callback");
    // 不再包装第二层 "host function 'custom':" 前缀。
    assert_eq!(
        err.message.matches("host function 'custom'").count(),
        1,
        "expected single source label, got: {}",
        err.message
    );
}

#[test]
fn host_callback_panic_becomes_diagnostic_and_engine_remains_reusable() {
    let mut engine = Engine::new();
    engine
        .register_host_function(HostFunctionBuilder::new("panic_host", Type::Int), |_| {
            panic!("host exploded")
        })
        .unwrap();

    let err = engine.eval("panic_host();").unwrap_err();
    assert_eq!(err.code, "host.callback");
    assert!(err.message.contains("host function 'panic_host'"));
    assert!(err.message.contains("host callback panicked"));
    assert!(err.message.contains("host exploded"));

    let value = engine.eval("1 + 1;").unwrap();
    assert_eq!(value, Value::Int(2));
}

unsafe extern "C" fn c_host_double(
    ctx: *mut c_void,
    args: *const NoxCoreValue,
    arg_count: usize,
    out_value: *mut NoxCoreValue,
) -> NoxCoreStatus {
    assert!(ctx.is_null());
    assert_eq!(arg_count, 1);
    let arg = *args;
    assert_eq!(arg.kind, NoxCoreValueKind::Int);
    ptr::write(
        out_value,
        NoxCoreValue {
            kind: NoxCoreValueKind::Int,
            int_value: arg.int_value * 2,
            ..NoxCoreValue::default()
        },
    );
    NoxCoreStatus::Ok
}

#[test]
fn runs_registered_c_host_function() {
    let mut engine = c_engine();
    let name = std::ffi::CString::new("double").unwrap();
    let params = [NoxCoreValueKind::Int];

    let status = unsafe {
        nox_core_engine_register_host_function(
            &mut engine,
            name.as_ptr(),
            params.as_ptr(),
            params.len(),
            NoxCoreValueKind::Int,
            Some(c_host_double),
            ptr::null_mut(),
        )
    };
    assert_eq!(status, NoxCoreStatus::Ok);

    let source = std::ffi::CString::new("double(21);").unwrap();
    let mut out = NoxCoreValue::default();
    let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut out as *mut _) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(out.kind, NoxCoreValueKind::Int);
    assert_eq!(out.int_value, 42);
}

unsafe extern "C" fn c_host_answer(
    ctx: *mut c_void,
    args: *const NoxCoreValue,
    arg_count: usize,
    out_value: *mut NoxCoreValue,
) -> NoxCoreStatus {
    assert!(ctx.is_null());
    assert!(args.is_null() || arg_count == 0);
    assert_eq!(arg_count, 0);
    ptr::write(
        out_value,
        NoxCoreValue {
            kind: NoxCoreValueKind::Int,
            int_value: 42,
            ..NoxCoreValue::default()
        },
    );
    NoxCoreStatus::Ok
}

unsafe extern "C" fn c_host_add_userdata(
    ctx: *mut c_void,
    args: *const NoxCoreValue,
    arg_count: usize,
    out_value: *mut NoxCoreValue,
) -> NoxCoreStatus {
    assert!(!ctx.is_null());
    assert_eq!(arg_count, 1);
    let offset = *(ctx.cast::<i64>());
    let arg = *args;
    assert_eq!(arg.kind, NoxCoreValueKind::Int);
    ptr::write(
        out_value,
        NoxCoreValue {
            kind: NoxCoreValueKind::Int,
            int_value: arg.int_value + offset,
            ..NoxCoreValue::default()
        },
    );
    NoxCoreStatus::Ok
}

unsafe extern "C" fn c_host_fail(
    _ctx: *mut c_void,
    _args: *const NoxCoreValue,
    _arg_count: usize,
    _out_value: *mut NoxCoreValue,
) -> NoxCoreStatus {
    NoxCoreStatus::Error
}

#[test]
fn registers_zero_arg_c_host_function_with_null_param_types() {
    let mut engine = c_engine();
    let name = std::ffi::CString::new("answer").unwrap();

    let status = unsafe {
        nox_core_engine_register_host_function(
            &mut engine,
            name.as_ptr(),
            ptr::null(),
            0,
            NoxCoreValueKind::Int,
            Some(c_host_answer),
            ptr::null_mut(),
        )
    };
    assert_eq!(status, NoxCoreStatus::Ok);

    let source = std::ffi::CString::new("answer();").unwrap();
    let mut out = NoxCoreValue::default();
    let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut out as *mut _) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(out.kind, NoxCoreValueKind::Int);
    assert_eq!(out.int_value, 42);
}

#[test]
fn c_abi_engine_userdata_is_used_when_callback_ctx_is_null() {
    let mut engine = c_engine();
    let mut first_offset = 20_i64;
    let mut second_offset = 21_i64;
    let name = std::ffi::CString::new("add_offset").unwrap();
    let params = [NoxCoreValueKind::Int];

    let status = unsafe {
        nox_core_engine_set_userdata(
            &mut engine,
            (&mut first_offset as *mut i64).cast::<c_void>(),
        )
    };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(
        unsafe { nox_core_engine_userdata(&engine) },
        (&mut first_offset as *mut i64).cast::<c_void>()
    );

    let status = unsafe {
        nox_core_engine_register_host_function(
            &mut engine,
            name.as_ptr(),
            params.as_ptr(),
            params.len(),
            NoxCoreValueKind::Int,
            Some(c_host_add_userdata),
            ptr::null_mut(),
        )
    };
    assert_eq!(status, NoxCoreStatus::Ok);

    let status = unsafe {
        nox_core_engine_set_userdata(
            &mut engine,
            (&mut second_offset as *mut i64).cast::<c_void>(),
        )
    };
    assert_eq!(status, NoxCoreStatus::Ok);

    let source = std::ffi::CString::new("add_offset(21);").unwrap();
    let mut out = NoxCoreValue::default();
    let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut out as *mut _) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(out.kind, NoxCoreValueKind::Int);
    assert_eq!(out.int_value, 42);
}

#[test]
fn c_abi_callback_error_sets_last_error_with_host_name() {
    let mut engine = c_engine();
    let name = std::ffi::CString::new("fail").unwrap();

    let status = unsafe {
        nox_core_engine_register_host_function(
            &mut engine,
            name.as_ptr(),
            ptr::null(),
            0,
            NoxCoreValueKind::Int,
            Some(c_host_fail),
            ptr::null_mut(),
        )
    };
    assert_eq!(status, NoxCoreStatus::Ok);

    let source = std::ffi::CString::new("fail();").unwrap();
    let mut out = NoxCoreValue::default();
    let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut out as *mut _) };
    assert_eq!(status, NoxCoreStatus::Error);
    let error = unsafe { CStr::from_ptr(nox_core_engine_last_error(&engine)) }
        .to_str()
        .unwrap();
    assert!(
        error.contains("host callback 'fail' returned status Error"),
        "{error}"
    );
}

#[test]
fn c_abi_eval_returns_owned_string_value() {
    let mut engine = c_engine();
    let source = std::ffi::CString::new(r#""hello" + " nox";"#).unwrap();
    let mut out = NoxCoreValue::default();

    let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut out as *mut _) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(out.kind, NoxCoreValueKind::String);
    assert!(!out.string_value.is_null());
    let value = unsafe { CStr::from_ptr(out.string_value) }
        .to_str()
        .unwrap();
    assert_eq!(value, "hello nox");
    unsafe { nox_core_string_free(out.string_value) };
}

#[test]
fn c_abi_reads_compound_values() {
    let mut engine = c_engine();

    let source = std::ffi::CString::new("[10, 20];").unwrap();
    let mut array = NoxCoreValue::default();
    let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut array) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(array.kind, NoxCoreValueKind::Array);
    assert!(!array.array_handle.is_null());
    assert_eq!(unsafe { nox_core_array_len(array.array_handle) }, 2);
    let mut element = NoxCoreValue::default();
    let status = unsafe { nox_core_array_get(array.array_handle, 1, &mut element) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(element.kind, NoxCoreValueKind::Int);
    assert_eq!(element.int_value, 20);
    unsafe { nox_core_array_free(array.array_handle) };

    let source =
        std::ffi::CString::new(r#"let scores: map[str, int] = {"core": 20}; scores;"#).unwrap();
    let mut map = NoxCoreValue::default();
    let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut map) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(map.kind, NoxCoreValueKind::Map);
    assert!(!map.map_handle.is_null());
    assert_eq!(unsafe { nox_core_map_len(map.map_handle) }, 1);
    let key = std::ffi::CString::new("core").unwrap();
    let mut value = NoxCoreValue::default();
    let status = unsafe { nox_core_map_get(map.map_handle, key.as_ptr(), &mut value) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(value.kind, NoxCoreValueKind::Int);
    assert_eq!(value.int_value, 20);
    let mut keys = [NoxCoreValue::default()];
    let mut written = 0;
    let status = unsafe { nox_core_map_keys(map.map_handle, keys.as_mut_ptr(), 1, &mut written) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(written, 1);
    assert_eq!(keys[0].kind, NoxCoreValueKind::String);
    let key = unsafe { CStr::from_ptr(keys[0].string_value) }
        .to_str()
        .unwrap();
    assert_eq!(key, "core");
    unsafe { nox_core_string_free(keys[0].string_value) };
    unsafe { nox_core_map_free(map.map_handle) };

    let source = std::ffi::CString::new(
        r#"
        record User {
            name: str,
            score: int,
        }
        let user: User = User { name: "nox", score: 42 };
        user;
        "#,
    )
    .unwrap();
    let mut record = NoxCoreValue::default();
    let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut record) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(record.kind, NoxCoreValueKind::Record);
    assert!(!record.record_handle.is_null());
    let field = std::ffi::CString::new("score").unwrap();
    let mut value = NoxCoreValue::default();
    let status = unsafe { nox_core_record_field(record.record_handle, field.as_ptr(), &mut value) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(value.kind, NoxCoreValueKind::Int);
    assert_eq!(value.int_value, 42);
    unsafe { nox_core_record_free(record.record_handle) };
}

#[test]
fn c_abi_reads_option_and_result_values() {
    let mut engine = c_engine();

    let source = std::ffi::CString::new("let value: option[int] = some(42); value;").unwrap();
    let mut option = NoxCoreValue::default();
    let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut option) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(option.kind, NoxCoreValueKind::Option);
    assert!(!option.option_handle.is_null());
    assert!(unsafe { nox_core_option_is_some(option.option_handle) });
    let mut payload = NoxCoreValue::default();
    let status = unsafe { nox_core_option_payload(option.option_handle, &mut payload) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(payload.kind, NoxCoreValueKind::Int);
    assert_eq!(payload.int_value, 42);
    unsafe { nox_core_option_free(option.option_handle) };

    let source = std::ffi::CString::new("let value: option[int] = none; value;").unwrap();
    let mut option = NoxCoreValue::default();
    let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut option) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(option.kind, NoxCoreValueKind::Option);
    assert!(!unsafe { nox_core_option_is_some(option.option_handle) });
    let mut payload = NoxCoreValue::default();
    let status = unsafe { nox_core_option_payload(option.option_handle, &mut payload) };
    assert_eq!(status, NoxCoreStatus::Error);
    unsafe { nox_core_option_free(option.option_handle) };

    let source =
        std::ffi::CString::new(r#"let value: result[int, str] = err("missing"); value;"#).unwrap();
    let mut result = NoxCoreValue::default();
    let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut result) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(result.kind, NoxCoreValueKind::Result);
    assert!(!result.result_handle.is_null());
    assert!(!unsafe { nox_core_result_is_ok(result.result_handle) });
    let mut payload = NoxCoreValue::default();
    let status = unsafe { nox_core_result_payload(result.result_handle, &mut payload) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(payload.kind, NoxCoreValueKind::String);
    let text = unsafe { CStr::from_ptr(payload.string_value) }
        .to_str()
        .unwrap();
    assert_eq!(text, "missing");
    unsafe { nox_core_string_free(payload.string_value) };
    unsafe { nox_core_result_free(result.result_handle) };
}

#[test]
fn c_abi_check_reports_last_error() {
    let mut engine = c_engine();
    let source = std::ffi::CString::new(r#"let value: int = "bad";"#).unwrap();

    let status = unsafe { nox_core_engine_check(&mut engine, source.as_ptr()) };
    assert_eq!(status, NoxCoreStatus::Error);

    let error = unsafe { nox_core_engine_last_error(&engine) };
    assert!(!error.is_null());
    let error = unsafe { CStr::from_ptr(error) }.to_str().unwrap();
    assert!(error.contains("expected int, got str"));

    unsafe { nox_core_engine_clear_error(&mut engine) };
    assert!(unsafe { nox_core_engine_last_error(&engine) }.is_null());
}

#[test]
fn resolves_imports_through_host_loader() {
    let mut engine = Engine::new();
    engine.set_module_loader(|specifier| {
        assert_eq!(specifier, "math.nox");
        Ok(r#"
                fn double(value: int) -> int {
                    return value * 2;
                }
                "#
        .to_string())
    });

    let value = engine
        .eval(
            r#"
                import "math.nox";
                double(21);
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn loads_repeated_import_once() {
    let mut engine = Engine::new();
    let load_count = std::rc::Rc::new(std::cell::RefCell::new(0));
    let observed = load_count.clone();
    engine.set_module_loader(move |specifier| {
        assert_eq!(specifier, "math.nox");
        *load_count.borrow_mut() += 1;
        Ok(r#"
                fn double(value: int) -> int {
                    return value * 2;
                }
                "#
        .to_string())
    });

    let value = engine
        .eval(
            r#"
                import "math.nox";
                import "math.nox";
                double(21);
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
    assert_eq!(*observed.borrow(), 1);
}

#[test]
fn session_reuses_module_source_across_calls() {
    let mut session = Session::new();
    let load_count = std::rc::Rc::new(std::cell::RefCell::new(0));
    let observed = load_count.clone();
    session.set_module_loader(move |specifier| {
        assert_eq!(specifier, "math.nox");
        *load_count.borrow_mut() += 1;
        Ok(r#"
                fn double(value: int) -> int {
                    return value * 2;
                }
                "#
        .to_string())
    });

    let source = r#"
        import "math.nox";
        double(21);
        "#;
    assert_eq!(session.eval(source).unwrap(), Value::Int(42));
    assert_eq!(session.eval(source).unwrap(), Value::Int(42));

    assert_eq!(*observed.borrow(), 1);
    assert!(session.module_graph().cached_source("math.nox").is_some());
}

#[test]
fn session_overlay_overrides_cached_module_source() {
    let mut session = Session::new();
    session.set_module_loader(|specifier| {
        assert_eq!(specifier, "math.nox");
        Ok(r#"
                fn value() -> int {
                    return 1;
                }
                "#
        .to_string())
    });

    let source = r#"
        import "math.nox";
        value();
        "#;
    assert_eq!(session.eval(source).unwrap(), Value::Int(1));

    session.set_module_overlay(
        "math.nox",
        r#"
        fn value() -> int {
            return 2;
        }
        "#,
    );
    assert_eq!(session.eval(source).unwrap(), Value::Int(2));

    session.remove_module_overlay("math.nox");
    assert_eq!(session.eval(source).unwrap(), Value::Int(1));
}

#[test]
fn session_engine_mut_keeps_simple_api_compatible() {
    let mut session = Session::new();
    session
        .engine_mut()
        .register_host_function(HostFunctionBuilder::new("host", Type::Int), |_| {
            Ok(Value::Int(42))
        })
        .unwrap();

    assert_eq!(session.eval("host();").unwrap(), Value::Int(42));
}

#[test]
fn diamond_imports_load_shared_module_once() {
    use std::cell::RefCell;
    use std::collections::HashMap;

    let mut engine = Engine::new();
    let load_counts: std::rc::Rc<RefCell<HashMap<String, usize>>> = Default::default();
    let observed = load_counts.clone();
    engine.set_module_loader(move |specifier| {
        *load_counts
            .borrow_mut()
            .entry(specifier.to_string())
            .or_insert(0) += 1;
        let source = match specifier {
            "shared.nox" => "fn answer() -> int { return 42; }",
            "left.nox" => "import \"shared.nox\";\nfn left() -> int { return answer(); }",
            "right.nox" => "import \"shared.nox\";\nfn right() -> int { return answer(); }",
            other => panic!("unexpected import '{other}'"),
        };
        Ok(source.to_string())
    });

    let value = engine
        .eval(
            r#"
                import "left.nox";
                import "right.nox";
                left() + right();
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(84));
    let observed = observed.borrow();
    assert_eq!(observed.get("shared.nox").copied(), Some(1));
    assert_eq!(observed.get("left.nox").copied(), Some(1));
    assert_eq!(observed.get("right.nox").copied(), Some(1));
}

#[test]
fn rejects_cyclic_imports() {
    let mut engine = Engine::new();
    engine.set_module_loader(|specifier| match specifier {
        "a.nox" => Ok(r#"import "b.nox";"#.to_string()),
        "b.nox" => Ok(r#"import "a.nox";"#.to_string()),
        _ => unreachable!("unexpected import {specifier}"),
    });

    let err = engine.eval(r#"import "a.nox";"#).unwrap_err();
    assert!(err.message.contains("cyclic import detected for 'a.nox'"));
}

#[test]
fn imports_only_exported_declarations_when_module_uses_export() {
    let mut engine = Engine::new();
    engine.set_module_loader(|specifier| {
        assert_eq!(specifier, "math.nox");
        Ok(r#"
                export fn double(value: int) -> int {
                    return helper(value);
                }

                fn helper(value: int) -> int {
                    return value * 2;
                }
                "#
        .to_string())
    });

    let value = engine
        .eval(
            r#"
                import "math.nox";
                double(21);
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));

    let err = engine
        .eval(
            r#"
                import "math.nox";
                helper(21);
                "#,
        )
        .unwrap_err();
    assert!(err.message.contains("undefined variable 'helper'"));
}

#[test]
fn exported_const_is_visible_to_importers_and_remains_constant() {
    let mut engine = Engine::new();
    engine.set_module_loader(|specifier| {
        assert_eq!(specifier, "config.nox");
        Ok(r#"
                export const limit: int = 7;
                const internal: int = 1;
                "#
        .to_string())
    });

    let value = engine
        .eval(
            r#"
                import "config.nox";
                limit + 1;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(8));

    let err = engine
        .eval(
            r#"
                import "config.nox";
                limit = 0;
                "#,
        )
        .unwrap_err();
    assert!(err.message.contains("constant"));

    let mut engine = Engine::new();
    engine.set_module_loader(|specifier| {
        assert_eq!(specifier, "config.nox");
        Ok(r#"
                export const limit: int = 7;
                const internal: int = 1;
                "#
        .to_string())
    });
    let err = engine
        .eval(
            r#"
                import "config.nox";
                internal;
                "#,
        )
        .unwrap_err();
    assert!(err.message.contains("undefined variable 'internal'"));
}

#[test]
fn rejects_top_level_redeclarations() {
    let mut engine = Engine::new();
    let err = engine
        .eval("fn answer() -> int { return 1; }\nlet answer: int = 2;\nanswer;")
        .unwrap_err();

    assert_eq!(err.code, "module.name-conflict");
    assert!(err.message.contains("name 'answer' redeclared"));
}

#[test]
fn rejects_import_conflict_with_entry_declaration() {
    let mut engine = Engine::new();
    engine.set_module_loader(|specifier| {
        assert_eq!(specifier, "math.nox");
        Ok("export fn answer() -> int { return 42; }".to_string())
    });

    let err = engine
        .eval(
            r#"
                import "math.nox";
                let answer: int = 1;
                answer;
                "#,
        )
        .unwrap_err();

    assert_eq!(err.code, "module.name-conflict");
    assert!(err.message.contains("name 'answer' redeclared"));
}

#[test]
fn rejects_conflicting_import_surfaces() {
    let mut engine = Engine::new();
    engine.set_module_loader(|specifier| {
        let source = match specifier {
            "left.nox" => "export fn answer() -> int { return 1; }",
            "right.nox" => "export fn answer() -> int { return 2; }",
            other => panic!("unexpected import '{other}'"),
        };
        Ok(source.to_string())
    });

    let err = engine
        .eval(
            r#"
                import "left.nox";
                import "right.nox";
                answer();
                "#,
        )
        .unwrap_err();

    assert_eq!(err.code, "module.name-conflict");
    assert!(err.message.contains("name 'answer' redeclared"));
}

#[test]
fn namespace_import_accesses_exported_members_without_flattening() {
    let mut engine = Engine::new();
    engine.set_module_loader(|specifier| match specifier {
        "math.nox" => Ok(r#"
            export fn double(value: int) -> int {
                return helper(value);
            }

            fn helper(value: int) -> int {
                return value * 2;
            }
        "#
        .to_string()),
        other => panic!("unexpected import '{other}'"),
    });

    let value = engine
        .eval(r#"import "math.nox" as math; math.double(21);"#)
        .unwrap();
    assert_eq!(value, Value::Int(42));

    let err = engine
        .eval(r#"import "math.nox" as math; double(21);"#)
        .unwrap_err();
    assert!(err.message.contains("undefined variable 'double'"));
}

#[test]
fn namespace_import_rejects_missing_members_and_alias_conflicts() {
    let mut engine = Engine::new();
    engine.set_module_loader(|specifier| match specifier {
        "math.nox" => {
            Ok(r#"export fn double(value: int) -> int { return value * 2; }"#.to_string())
        }
        other => panic!("unexpected import '{other}'"),
    });

    let err = engine
        .eval(r#"import "math.nox" as math; math.missing(21);"#)
        .unwrap_err();
    assert_eq!(err.code, "module.member-not-found");
    assert!(err
        .message
        .contains("module namespace 'math' has no member 'missing'"));

    let err = engine
        .eval(r#"import "math.nox" as math; let math: int = 1; math;"#)
        .unwrap_err();
    assert_eq!(err.code, "module.name-conflict");
    assert!(err.message.contains("module namespace 'math' conflicts"));
}

#[test]
fn namespace_import_reuses_loaded_module_surface() {
    let mut engine = Engine::new();
    engine.set_module_loader(|specifier| match specifier {
        "math.nox" => {
            Ok(r#"export fn double(value: int) -> int { return value * 2; }"#.to_string())
        }
        other => panic!("unexpected import '{other}'"),
    });

    let value = engine
        .eval(r#"import "math.nox"; import "math.nox" as math; double(10) + math.double(11);"#)
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn rejects_redeclaration_inside_exporting_import() {
    let mut engine = Engine::new();
    engine.set_module_loader(|specifier| {
        assert_eq!(specifier, "math.nox");
        Ok(
            "export fn answer() -> int { return 42; }\nfn answer() -> int { return 1; }"
                .to_string(),
        )
    });

    let err = engine
        .eval(
            r#"
                import "math.nox";
                answer();
                "#,
        )
        .unwrap_err();

    assert_eq!(err.code, "module.name-conflict");
    assert!(err.message.contains("name 'answer' redeclared"));
}

#[test]
fn rejects_record_and_value_redeclaration() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
                record User {
                    name: str,
                }
                let User: int = 1;
                User;
                "#,
        )
        .unwrap_err();

    assert_eq!(err.code, "module.name-conflict");
    assert!(err.message.contains("name 'User' redeclared"));
}

#[test]
fn cancels_execution_when_instruction_budget_is_exhausted() {
    let mut engine = Engine::new();
    engine.set_instruction_budget(Some(8));
    let err = engine
        .eval(
            r#"
                let value: int = 0;
                while (value < 100) {
                    value = value + 1;
                }
                value;
                "#,
        )
        .unwrap_err();
    assert!(err.message.contains("instruction budget exhausted"));
}

#[test]
fn engine_can_be_reused_after_budget_exhaustion_when_budget_is_reset() {
    let mut engine = Engine::new();
    engine.set_instruction_budget(Some(8));
    let err = engine
        .eval(
            r#"
                let value: int = 0;
                while (value < 100) {
                    value = value + 1;
                }
                value;
                "#,
        )
        .unwrap_err();
    assert!(err.message.contains("instruction budget exhausted"));

    engine.set_instruction_budget(None);
    assert_eq!(engine.eval("41 + 1;").unwrap(), Value::Int(42));
}

#[test]
fn instruction_budget_resumes_after_host_callback_returns() {
    let mut engine = Engine::new();
    engine
        .register_host_function(HostFunctionBuilder::new("host_tick", Type::Int), |_| {
            Ok(Value::Int(0))
        })
        .unwrap();
    engine.set_instruction_budget(Some(24));

    let err = engine
        .eval(
            r#"
            let value: int = host_tick();
            while (value < 100) {
                value = value + 1;
            }
            value;
            "#,
        )
        .unwrap_err();

    assert!(err.message.contains("instruction budget exhausted"));
}

#[test]
fn session_can_be_reused_after_budget_exhaustion_when_budget_is_reset() {
    let mut session = Session::new();
    session.engine_mut().set_instruction_budget(Some(8));
    let err = session
        .eval(
            r#"
                let value: int = 0;
                while (value < 100) {
                    value = value + 1;
                }
                value;
                "#,
        )
        .unwrap_err();
    assert!(err.message.contains("instruction budget exhausted"));

    session.engine_mut().set_instruction_budget(None);
    session
        .engine_mut()
        .register_host_function(HostFunctionBuilder::new("answer", Type::Int), |_| {
            Ok(Value::Int(42))
        })
        .unwrap();
    assert_eq!(session.eval("answer();").unwrap(), Value::Int(42));
}

#[test]
fn heap_tracks_and_collects_script_string_values() {
    let mut engine = Engine::new();
    let value = engine.eval(r#""hello" + " world";"#).unwrap();
    assert_eq!(value, Value::string("hello world"));
    assert!(engine.heap_object_count() >= 1);

    drop(value);
    engine.collect_garbage();
    assert_eq!(engine.heap_object_count(), 0);
}

#[test]
fn repeated_eval_and_check_collect_transient_heap_values() {
    let mut engine = Engine::new();

    for index in 0..250 {
        let source = format!(
            r#"
            record Item {{
                name: str,
                values: [int],
                scores: map[str, int],
            }}
            let item: Item = Item {{
                name: "item-{index}",
                values: [{index}, {}],
                scores: {{"nox": {index}}},
            }};
            item;
            "#,
            index + 1
        );

        engine.check(&source).unwrap();
        engine.collect_garbage();
        assert_eq!(
            engine.heap_object_count(),
            0,
            "checking should not leave heap values at iteration {index}"
        );

        let value = engine.eval(&source).unwrap();
        assert!(engine.heap_object_count() >= 3);
        drop(value);
        engine.collect_garbage();
        assert_eq!(
            engine.heap_object_count(),
            0,
            "eval result should be collectable at iteration {index}"
        );
    }
}

#[test]
fn host_held_rust_values_keep_heap_objects_until_dropped() {
    let mut engine = Engine::new();
    let mut values = Vec::new();

    for index in 0..40 {
        values.push(
            engine
                .eval(&format!(r#""value-{index}" + "-held";"#))
                .unwrap(),
        );
        values.push(
            engine
                .eval(&format!(
                    "let values: [int] = [{index}, {}]; values;",
                    index + 1
                ))
                .unwrap(),
        );
        values.push(
            engine
                .eval(&format!(
                    r#"let scores: map[str, int] = {{"nox": {index}}}; scores;"#
                ))
                .unwrap(),
        );
        values.push(
            engine
                .eval(&format!(
                    r#"
                    record Item {{
                        name: str,
                        score: int,
                    }}
                    let item: Item = Item {{ name: "nox", score: {index} }};
                    item;
                    "#
                ))
                .unwrap(),
        );
    }

    engine.collect_garbage();
    assert!(
        engine.heap_object_count() >= values.len(),
        "host-held values should keep heap objects alive"
    );

    drop(values);
    engine.collect_garbage();
    assert_eq!(engine.heap_object_count(), 0);
}

#[test]
fn host_held_module_container_values_collect_after_drop() {
    let mut engine = Engine::new();
    engine.set_module_loader(|specifier| match specifier {
        "factory.nox" => Ok(r#"
            export fn labels(seed: int) -> [str] {
                return ["module", "value"];
            }

            export fn scores(seed: int) -> map[str, int] {
                return {"seed": seed};
            }
        "#
        .to_string()),
        other => panic!("unexpected import '{other}'"),
    });

    let mut held = Vec::new();
    for index in 0..80 {
        let value = engine
            .eval(&format!(
                r#"
                import "factory.nox";
                let label_values: [str] = labels({index});
                let score_values: map[str, int] = scores({index});
                let seed: int = score_values["seed"];
                [label_values[0] + label_values[1], "score"];
                "#
            ))
            .unwrap();
        held.push(value);
    }

    engine.collect_garbage();
    assert!(
        engine.heap_object_count() >= held.len(),
        "host-held module container values should keep heap objects alive"
    );

    drop(held);
    engine.collect_garbage();
    assert_eq!(engine.heap_object_count(), 0);
}

#[test]
fn repeated_host_callback_returns_do_not_accumulate_heap_values() {
    let mut engine = Engine::new();
    let calls = Rc::new(Cell::new(0));

    let string_calls = calls.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new("host_name", Type::Str),
            move |_| {
                let next = string_calls.get() + 1;
                string_calls.set(next);
                Ok(Value::string(format!("host-{next}")))
            },
        )
        .unwrap();
    engine
        .register_host_function(
            HostFunctionBuilder::new("host_values", Type::Array(Box::new(Type::Int))),
            |_| Ok(Value::array(Type::Int, vec![Value::Int(1), Value::Int(2)])),
        )
        .unwrap();
    engine
        .register_host_function(
            HostFunctionBuilder::new("host_scores", Type::Map(Box::new(Type::Int))),
            |_| {
                let mut entries = std::collections::BTreeMap::new();
                entries.insert("nox".to_string(), Value::Int(40));
                Ok(Value::map(Type::Int, entries))
            },
        )
        .unwrap();

    for index in 0..200 {
        let value = engine
            .eval(
                r#"
                let label: str = host_name() + "-script";
                let values: [int] = host_values();
                let scores: map[str, int] = host_scores();
                len(label) + len(values) + scores["nox"];
                "#,
            )
            .unwrap();
        assert!(matches!(value, Value::Int(total) if total > 40));
        drop(value);
        engine.collect_garbage();
        assert_eq!(
            engine.heap_object_count(),
            0,
            "host callback return values should not accumulate at iteration {index}"
        );
    }
}

#[test]
fn c_abi_handles_keep_heap_objects_until_freed() {
    let mut engine = c_engine();
    let mut arrays = Vec::new();
    let mut maps = Vec::new();
    let mut records = Vec::new();

    for index in 0..40 {
        let source = std::ffi::CString::new(format!(
            "let values: [int] = [{index}, {}]; values;",
            index + 1
        ))
        .unwrap();
        let mut value = NoxCoreValue::default();
        let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut value) };
        assert_eq!(status, NoxCoreStatus::Ok);
        assert_eq!(value.kind, NoxCoreValueKind::Array);
        arrays.push(value.array_handle);

        let source = std::ffi::CString::new(format!(
            r#"let scores: map[str, int] = {{"nox": {index}}}; scores;"#
        ))
        .unwrap();
        let mut value = NoxCoreValue::default();
        let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut value) };
        assert_eq!(status, NoxCoreStatus::Ok);
        assert_eq!(value.kind, NoxCoreValueKind::Map);
        maps.push(value.map_handle);

        let source = std::ffi::CString::new(format!(
            r#"
            record Item {{
                name: str,
                score: int,
            }}
            let item: Item = Item {{ name: "nox", score: {index} }};
            item;
            "#
        ))
        .unwrap();
        let mut value = NoxCoreValue::default();
        let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut value) };
        assert_eq!(status, NoxCoreStatus::Ok);
        assert_eq!(value.kind, NoxCoreValueKind::Record);
        records.push(value.record_handle);
    }

    engine.engine.collect_garbage();
    assert!(
        engine.engine.heap_object_count() >= arrays.len() + maps.len() + records.len(),
        "C handles should keep heap objects alive"
    );

    for handle in arrays {
        unsafe { nox_core_array_free(handle) };
    }
    for handle in maps {
        unsafe { nox_core_map_free(handle) };
    }
    for handle in records {
        unsafe { nox_core_record_free(handle) };
    }
    engine.engine.collect_garbage();
    assert_eq!(engine.engine.heap_object_count(), 0);
}

#[test]
fn repeated_c_abi_handle_free_collects_nested_heap_values() {
    let mut engine = c_engine();

    for index in 0..120 {
        let source = std::ffi::CString::new(format!(
            r#"
            record Item {{
                name: str,
                values: [int],
                scores: map[str, int],
            }}
            Item {{
                name: "item-{index}",
                values: [{index}, {}],
                scores: {{"nox": {index}}},
            }};
            "#,
            index + 1
        ))
        .unwrap();
        let mut value = NoxCoreValue::default();
        let status = unsafe { nox_core_engine_eval(&mut engine, source.as_ptr(), &mut value) };
        assert_eq!(status, NoxCoreStatus::Ok);
        assert_eq!(value.kind, NoxCoreValueKind::Record);

        engine.engine.collect_garbage();
        assert!(
            engine.engine.heap_object_count() >= 3,
            "C record handle should keep nested heap values alive at iteration {index}"
        );

        unsafe { nox_core_record_free(value.record_handle) };
        engine.engine.collect_garbage();
        assert_eq!(
            engine.engine.heap_object_count(),
            0,
            "freeing C record handle should release nested values at iteration {index}"
        );
    }
}

#[test]
fn c_abi_option_and_result_handles_keep_nested_heap_values_until_freed() {
    let mut engine = c_engine();

    let option_source = std::ffi::CString::new(
        r#"
        let value: option[[str]] = some(["alpha", "beta"]);
        value;
        "#,
    )
    .unwrap();
    let mut option = NoxCoreValue::default();
    let status = unsafe { nox_core_engine_eval(&mut engine, option_source.as_ptr(), &mut option) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(option.kind, NoxCoreValueKind::Option);

    let result_source = std::ffi::CString::new(
        r#"
        let value: result[map[str, str], str] = ok({"nox": "core"});
        value;
        "#,
    )
    .unwrap();
    let mut result = NoxCoreValue::default();
    let status = unsafe { nox_core_engine_eval(&mut engine, result_source.as_ptr(), &mut result) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(result.kind, NoxCoreValueKind::Result);

    engine.engine.collect_garbage();
    assert!(
        engine.engine.heap_object_count() >= 2,
        "C option/result handles should keep nested heap values alive"
    );

    let mut option_payload = NoxCoreValue::default();
    let status = unsafe { nox_core_option_payload(option.option_handle, &mut option_payload) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(option_payload.kind, NoxCoreValueKind::Array);
    unsafe { nox_core_array_free(option_payload.array_handle) };

    let mut result_payload = NoxCoreValue::default();
    let status = unsafe { nox_core_result_payload(result.result_handle, &mut result_payload) };
    assert_eq!(status, NoxCoreStatus::Ok);
    assert_eq!(result_payload.kind, NoxCoreValueKind::Map);
    unsafe { nox_core_map_free(result_payload.map_handle) };

    unsafe { nox_core_option_free(option.option_handle) };
    unsafe { nox_core_result_free(result.result_handle) };
    engine.engine.collect_garbage();
    assert_eq!(engine.engine.heap_object_count(), 0);
}
