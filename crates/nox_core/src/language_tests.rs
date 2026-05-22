use super::*;

#[test]
fn parses_and_runs_variables_arithmetic_and_assignment() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
                let value: int = 10;
                value = value + 7 * 2;
                value;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(24));
}

#[test]
fn runs_functions_and_return() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
                fn add(left: int, right: int) -> int {
                    return left + right;
                }
                add(20, 22);
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn distinguishes_int_and_float_values() {
    let mut engine = Engine::new();
    let int_value = engine.eval("let value: int = 21; value * 2;").unwrap();
    assert_eq!(int_value, Value::Int(42));

    let float_value = engine
        .eval("let value: float = 21.0; value * 2.0;")
        .unwrap();
    assert_eq!(float_value, Value::Float(42.0));
}

#[test]
fn rejects_mixed_int_and_float_arithmetic() {
    let mut engine = Engine::new();
    let err = engine.eval("1 + 2.0;").unwrap_err();
    assert!(err.message.contains("'+' is not defined for int and float"));
}

#[test]
fn runs_explicit_numeric_conversions() {
    let mut engine = Engine::new();
    let float_value = engine.eval("to_float(21) * 2.0;").unwrap();
    assert_eq!(float_value, Value::Float(42.0));

    let int_value = engine.eval("to_int(42.9);").unwrap();
    assert_eq!(int_value, Value::Int(42));
}

#[test]
fn rejects_numeric_runtime_errors() {
    let mut engine = Engine::new();

    let value = engine.eval("7 / 2;").unwrap();
    assert_eq!(value, Value::Int(3));

    let value = engine.eval("(0 - 7) / 2;").unwrap();
    assert_eq!(value, Value::Int(-3));

    let err = engine.eval("1 / 0;").unwrap_err();
    assert_eq!(err.code, "runtime.division-by-zero");
    assert!(err.message.contains("division by zero"));

    let err = engine.eval("9223372036854775807 + 1;").unwrap_err();
    assert!(err.message.contains("integer overflow"));

    let err = engine
        .eval("(0 - 9223372036854775807 - 1) / (0 - 1);")
        .unwrap_err();
    assert!(err.message.contains("integer overflow"));

    let err = engine.eval("1.0 / 0.0;").unwrap_err();
    assert_eq!(err.code, "runtime.division-by-zero");
    assert!(err.message.contains("division by zero"));

    let err = engine
        .eval(
            r#"
            let value: float = to_float(9223372036854775807);
            value = value * value;
            value = value * value;
            value = value * value;
            value = value * value;
            value * value;
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("float result is not finite"));
}

#[test]
fn runs_short_circuit_logical_operators() {
    let mut engine = Engine::new();
    engine
        .register_host_function(HostFunctionBuilder::new("fail_bool", Type::Bool), |_| {
            Err(Diagnostic::new(
                "fail_bool should not be evaluated",
                Span { start: 0, end: 0 },
            ))
        })
        .unwrap();
    let value = engine
        .eval(
            r#"
            let left: bool = false && fail_bool();
            let right: bool = true || fail_bool();
            left == false && right == true;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn rejects_non_bool_logical_operands() {
    let mut engine = Engine::new();
    let err = engine.eval("true && 1;").unwrap_err();
    assert!(err.message.contains("expected bool, got int"));
}

#[test]
fn runs_int_and_string_match_statements() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let code: int = 2;
            let label: str = "";
            match (code) {
                1 => {
                    label = "one";
                }
                2 => {
                    label = "two";
                }
                _ => {
                    label = "other";
                }
            }

            let suffix: str = "";
            match (label) {
                "one" => {
                    suffix = "-1";
                }
                "two" => {
                    suffix = "-2";
                }
                _ => {
                    suffix = "-x";
                }
            }
            label + suffix;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("two-2"));
}

#[test]
fn match_branches_have_independent_scopes() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            match (1) {
                1 => {
                    let hidden: int = 42;
                }
                _ => {}
            }
            hidden;
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("undefined variable 'hidden'"));
}

#[test]
fn match_branches_can_satisfy_function_return() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn classify(value: int) -> str {
                match (value) {
                    1 => {
                        return "one";
                    }
                    _ => {
                        return "other";
                    }
                }
            }
            classify(1);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("one"));
}

#[test]
fn option_match_binds_payload_and_none_branch() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn describe(input: option[int]) -> int {
                match (input) {
                    some(value) => {
                        return value + 1;
                    }
                    none => {
                        return 0;
                    }
                }
            }
            describe(some(41)) + describe(none);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn result_match_binds_ok_and_err_payloads() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn describe(input: result[int, str]) -> str {
                match (input) {
                    ok(value) => {
                        return "ok";
                    }
                    err(message) => {
                        return message;
                    }
                }
            }
            describe(ok(1)) + ":" + describe(err("bad"));
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("ok:bad"));
}

#[test]
fn option_match_payload_is_scoped_to_case() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            let input: option[int] = some(1);
            match (input) {
                some(value) => {}
                none => {}
            }
            value;
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("undefined variable 'value'"));
}

#[test]
fn rejects_duplicate_match_cases() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            match (1) {
                1 => {}
                1 => {}
                _ => {}
            }
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("duplicate match case"));
}

#[test]
fn rejects_match_without_default() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            match (1) {
                1 => {}
            }
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("match requires '_' default case"));
}

#[test]
fn rejects_duplicate_match_default() {
    let tokens = lex(r#"
        match (1) {
            _ => {}
            _ => {}
        }
        "#)
    .unwrap();
    let err = parse(tokens).unwrap_err();
    assert!(err
        .message
        .contains("match default case can only appear once"));
}

#[test]
fn rejects_match_case_type_mismatch() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            match (1) {
                "one" => {}
                _ => {}
            }
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("expected int, got str"));
}

#[test]
fn rejects_match_on_unsupported_type() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            match (true) {
                1 => {}
                _ => {}
            }
            "#,
        )
        .unwrap_err();
    assert!(err
        .message
        .contains("match value must be int, str, option, or result"));
}

#[test]
fn rejects_non_exhaustive_option_match() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            let input: option[int] = some(1);
            match (input) {
                some(value) => {}
            }
            "#,
        )
        .unwrap_err();
    assert!(err
        .message
        .contains("option match must cover some and none"));
}

#[test]
fn rejects_wrong_option_match_case() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            let input: option[int] = some(1);
            match (input) {
                ok(value) => {}
                none => {}
            }
            "#,
        )
        .unwrap_err();
    assert!(err
        .message
        .contains("option match only accepts some(name) and none cases"));
}

#[test]
fn rejects_result_match_default_case() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            let input: result[int, str] = ok(1);
            match (input) {
                ok(value) => {}
                err(message) => {}
                _ => {}
            }
            "#,
        )
        .unwrap_err();
    assert!(err
        .message
        .contains("result match does not accept '_' default case"));
}

#[test]
fn runs_for_range_loop() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let total: int = 0;
            for i in 0..5 {
                total = total + i;
            }
            total;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(10));
}

#[test]
fn rejects_non_int_for_range_bounds() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            for i in 0.0..3 {
                i;
            }
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("expected int, got float"));
}

#[test]
fn keeps_for_loop_variable_scoped_to_body() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            for i in 0..1 {
                i;
            }
            i;
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("undefined variable 'i'"));
}

#[test]
fn for_loop_does_not_guarantee_function_return() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            fn maybe() -> int {
                for i in 0..0 {
                    return i;
                }
            }
            maybe();
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("function 'maybe' must return int"));
}

#[test]
fn runs_array_literals_and_indexing() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let values: [int] = [10, 20, 30];
            values[0] + values[2];
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(40));
}

#[test]
fn runs_array_len_builtin() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let values: [int] = [10, 20, 30];
            let empty: [int] = [];
            len(values) + len(empty);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(3));
}

#[test]
fn runs_empty_array_literals_with_expected_type() {
    let mut engine = Engine::new();
    let value = engine.eval("let values: [int] = []; values;").unwrap();
    assert_eq!(format!("{value}"), "[]");
}

#[test]
fn heap_tracks_and_collects_script_array_values() {
    let mut engine = Engine::new();
    let value = engine.eval("let values: [int] = [1, 2]; values;").unwrap();
    assert!(matches!(value, Value::Array(_)));
    assert!(engine.heap_object_count() >= 1);

    drop(value);
    engine.collect_garbage();
    assert_eq!(engine.heap_object_count(), 0);
}

#[test]
fn rejects_mixed_array_element_types() {
    let mut engine = Engine::new();
    let err = engine
        .eval(r#"let values: [int] = [1, "two"];"#)
        .unwrap_err();
    assert_eq!(err.code, "type.mismatch");
    assert!(err.message.contains("expected int, got str"));
}

#[test]
fn rejects_non_int_array_index() {
    let mut engine = Engine::new();
    let source = r#"let values: [int] = [1, 2]; values[0.0];"#;
    let err = engine.eval(source).unwrap_err();
    assert!(err.message.contains("expected int, got float"));
    assert_eq!(&source[err.span.start..err.span.end], "0.0");
}

#[test]
fn rejects_len_on_non_array_or_str_value() {
    let mut engine = Engine::new();
    let err = engine.eval("len(1);").unwrap_err();
    assert!(err.message.contains("expected array or str, got int"));
}

#[test]
fn len_returns_character_count_for_strings() {
    let mut engine = Engine::new();
    let value = engine.eval(r#"len("nox");"#).unwrap();
    assert_eq!(value, Value::Int(3));
    let value = engine.eval(r#"len("中文");"#).unwrap();
    assert_eq!(value, Value::Int(2));
    let value = engine.eval(r#"len("");"#).unwrap();
    assert_eq!(value, Value::Int(0));
}

#[test]
fn contains_returns_whether_map_has_key() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let scores: map[str, int] = { "a": 1, "b": 2 };
            contains(scores, "a");
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Bool(true));
    let value = engine
        .eval(
            r#"
            let scores: map[str, int] = { "a": 1 };
            contains(scores, "missing");
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Bool(false));
}

#[test]
fn contains_requires_map_first_argument() {
    let mut engine = Engine::new();
    let err = engine.eval(r#"contains([1, 2], "a");"#).unwrap_err();
    assert!(err.message.contains("expected map"));
}

#[test]
fn contains_requires_str_key() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            let scores: map[str, int] = { "a": 1 };
            contains(scores, 1);
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("expected str, got int"));
}

#[test]
fn map_get_returns_option_for_present_and_missing_keys() {
    let mut engine = Engine::new();
    let present = engine
        .eval(
            r#"
            let scores: map[str, int] = { "a": 1, "b": 2 };
            map_get(scores, "a");
            "#,
        )
        .unwrap();
    assert_eq!(present.to_string(), "some(1)");
    assert_eq!(value_type(&present), Type::Option(Box::new(Type::Int)));

    let missing = engine
        .eval(
            r#"
            let scores: map[str, int] = { "a": 1 };
            map_get(scores, "missing");
            "#,
        )
        .unwrap();
    assert_eq!(missing.to_string(), "none");
    assert_eq!(value_type(&missing), Type::Option(Box::new(Type::Int)));
}

#[test]
fn map_get_result_can_be_matched() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn describe(scores: map[str, int], key: str) -> int {
                let score: option[int] = map_get(scores, key);
                match (score) {
                    some(value) => {
                        return value;
                    }
                    none => {
                        return 0;
                    }
                }
            }

            let scores: map[str, int] = { "a": 7 };
            describe(scores, "a") + describe(scores, "missing");
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(7));
}

#[test]
fn map_get_requires_map_first_argument() {
    let mut engine = Engine::new();
    let err = engine.eval(r#"map_get([1, 2], "a");"#).unwrap_err();
    assert!(err.message.contains("expected map"));
}

#[test]
fn map_get_requires_str_key() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            let scores: map[str, int] = { "a": 1 };
            map_get(scores, 1);
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("expected str, got int"));
}

#[test]
fn rejects_array_index_out_of_bounds() {
    let mut engine = Engine::new();
    let err = engine
        .eval(r#"let values: [int] = [1, 2]; values[2];"#)
        .unwrap_err();
    assert!(err.message.contains("array index out of bounds"));
}

#[test]
fn rejects_array_equality_until_semantics_are_designed() {
    let mut engine = Engine::new();
    let err = engine.eval("[1] == [1];").unwrap_err();
    assert!(err.message.contains("container equality is not supported"));
}

#[test]
fn runs_map_literals_and_string_indexing() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let scores: map[str, int] = {"nox": 40, "core": 2};
            scores["nox"] + scores["core"];
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn runs_empty_map_literals_with_expected_type() {
    let mut engine = Engine::new();
    let value = engine
        .eval(r#"let scores: map[str, int] = {}; scores;"#)
        .unwrap();
    assert_eq!(format!("{value}"), "{}");
}

#[test]
fn heap_tracks_and_collects_script_map_values() {
    let mut engine = Engine::new();
    let value = engine
        .eval(r#"let scores: map[str, int] = {"nox": 1}; scores;"#)
        .unwrap();
    assert!(matches!(value, Value::Map(_)));
    assert!(engine.heap_object_count() >= 1);

    drop(value);
    engine.collect_garbage();
    assert_eq!(engine.heap_object_count(), 0);
}

#[test]
fn rejects_non_string_map_key_type() {
    let mut engine = Engine::new();
    let err = engine
        .eval(r#"let scores: map[int, int] = {"nox": 1};"#)
        .unwrap_err();
    assert!(err.message.contains("map key type must be str"));
}

#[test]
fn rejects_non_string_map_literal_key() {
    let mut engine = Engine::new();
    let err = engine
        .eval(r#"let scores: map[str, int] = {1: 2};"#)
        .unwrap_err();
    assert!(err.message.contains("expected str, got int"));
}

#[test]
fn rejects_mixed_map_value_types() {
    let mut engine = Engine::new();
    let err = engine
        .eval(r#"let scores: map[str, int] = {"nox": 1, "core": "two"};"#)
        .unwrap_err();
    assert!(err.message.contains("expected int, got str"));
}

#[test]
fn rejects_non_string_map_index() {
    let mut engine = Engine::new();
    let source = r#"let scores: map[str, int] = {"nox": 1}; scores[0];"#;
    let err = engine.eval(source).unwrap_err();
    assert!(err.message.contains("expected str, got int"));
    assert_eq!(&source[err.span.start..err.span.end], "0");
}

#[test]
fn rejects_missing_map_key() {
    let mut engine = Engine::new();
    let err = engine
        .eval(r#"let scores: map[str, int] = {"nox": 1}; scores["missing"];"#)
        .unwrap_err();
    assert!(err.message.contains("map key not found"));
}

#[test]
fn rejects_map_equality_until_semantics_are_designed() {
    let mut engine = Engine::new();
    let err = engine
        .eval(r#"let left: map[str, int] = {"nox": 1}; left == left;"#)
        .unwrap_err();
    assert!(err.message.contains("container equality is not supported"));
}

#[test]
fn runs_record_literals_and_field_access() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            record User {
                name: str,
                score: int,
            }
            let user: User = User { name: "nox", score: 40 };
            user.score + 2;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn heap_tracks_and_collects_script_record_values() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
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
    assert!(matches!(value, Value::Record(_)));
    assert!(engine.heap_object_count() >= 1);

    drop(value);
    engine.collect_garbage();
    assert_eq!(engine.heap_object_count(), 0);
}

#[test]
fn heap_keeps_nested_record_container_fields_alive() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            record Bag {
                values: [int],
                tags: map[str, int],
            }
            let bag: Bag = Bag {
                values: [20, 22],
                tags: {"nox": 42},
            };
            bag;
            "#,
        )
        .unwrap();
    assert!(matches!(value, Value::Record(_)));
    assert!(engine.heap_object_count() >= 3);

    drop(value);
    engine.collect_garbage();
    assert_eq!(engine.heap_object_count(), 0);
}

#[test]
fn heap_tracks_and_collects_script_function_values() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn answer() -> int {
                return 42;
            }
            answer;
            "#,
        )
        .unwrap();
    assert!(matches!(value, Value::Function(_)));
    assert!(engine.heap_object_count() >= 1);

    drop(value);
    engine.collect_garbage();
    assert_eq!(engine.heap_object_count(), 0);
}

#[test]
fn rejects_duplicate_record_fields() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            record User {
                name: str,
                name: int,
            }
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("duplicate field 'name'"));
}

#[test]
fn rejects_missing_record_literal_fields() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            record User {
                name: str,
                score: int,
            }
            let user: User = User { name: "nox" };
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("missing field 'score'"));
}

#[test]
fn rejects_extra_record_literal_fields() {
    let mut engine = Engine::new();
    let source = r#"
            record User {
                name: str,
            }
            let user: User = User { name: "nox", score: 42 };
            "#;
    let err = engine.eval(source).unwrap_err();
    assert!(err.message.contains("record 'User' has no field 'score'"));
    assert_eq!(&source[err.span.start..err.span.end], "score");
}

#[test]
fn rejects_wrong_record_literal_field_type() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            record User {
                score: int,
            }
            let user: User = User { score: "forty two" };
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("expected int, got str"));
}

#[test]
fn rejects_unknown_record_field_access() {
    let mut engine = Engine::new();
    let source = r#"
            record User {
                name: str,
            }
            let user: User = User { name: "nox" };
            user.score;
            "#;
    let err = engine.eval(source).unwrap_err();
    assert!(err.message.contains("record 'User' has no field 'score'"));
    assert_eq!(&source[err.span.start..err.span.end], "score");
}

#[test]
fn rejects_wrong_function_argument_count_at_call_paren() {
    let mut engine = Engine::new();
    let source = r#"fn one(value: int) -> int { return value; } one();"#;
    let err = engine.eval(source).unwrap_err();
    assert!(err.message.contains("expected 1 arguments but got 0"));
    assert_eq!(&source[err.span.start..err.span.end], ")");
}

#[test]
fn rejects_field_access_on_non_record_values() {
    let mut engine = Engine::new();
    let err = engine.eval("42.name;").unwrap_err();
    assert!(err.message.contains("field access requires a record value"));
}

#[test]
fn rejects_record_equality_until_semantics_are_designed() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            record User {
                name: str,
            }
            let user: User = User { name: "nox" };
            user == user;
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("container equality is not supported"));
}

#[test]
fn runs_basic_control_flow() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let i: int = 0;
            let total: int = 0;
            while (i < 5) {
                total = total + i;
                i = i + 1;
            }
            if (total == 10) {
                "ok";
            } else {
                "bad";
            }
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("ok"));
}

#[test]
fn const_binding_is_evaluated_at_runtime() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            const limit: int = 10;
            limit + 1;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(11));
}

#[test]
fn const_assignment_is_a_static_error() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            const answer: int = 42;
            answer = 0;
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("constant"));
    assert!(err.message.contains("answer"));
}

#[test]
fn const_inside_block_is_immutable() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            {
                const half: int = 21;
                half = 0;
            }
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("constant"));
}

#[test]
fn const_inside_block_can_be_shadowed_by_let() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            const base: int = 1;
            {
                let base: int = 2;
                base = 3;
                base;
            }
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(3));
}

#[test]
fn while_loop_break_exits_immediately() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let i: int = 0;
            let total: int = 0;
            while (i < 10) {
                if (i == 4) {
                    break;
                }
                total = total + i;
                i = i + 1;
            }
            total;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(1 + 2 + 3));
}

#[test]
fn while_loop_continue_skips_to_condition() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let i: int = 0;
            let total: int = 0;
            while (i < 5) {
                i = i + 1;
                if (i == 3) {
                    continue;
                }
                total = total + i;
            }
            total;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(1 + 2 + 4 + 5));
}

#[test]
fn for_loop_break_exits_and_releases_loop_var() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let total: int = 0;
            for i in 0..10 {
                if (i == 3) {
                    break;
                }
                total = total + i;
            }
            total;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(1 + 2));
}

#[test]
fn for_loop_continue_advances_iteration() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let total: int = 0;
            for i in 0..5 {
                if (i == 2) {
                    continue;
                }
                total = total + i;
            }
            total;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(1 + 3 + 4));
}

#[test]
fn nested_loops_break_only_affects_innermost() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let total: int = 0;
            for i in 0..3 {
                for j in 0..5 {
                    if (j == 2) {
                        break;
                    }
                    total = total + 1;
                }
                total = total + 100;
            }
            total;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(2 + 2 + 2 + 100 + 100 + 100));
}

#[test]
fn break_outside_loop_is_a_static_error() {
    let mut engine = Engine::new();
    let err = engine.eval("break;").unwrap_err();
    assert!(err.message.contains("'break'"));
}

#[test]
fn continue_outside_loop_is_a_static_error() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            fn helper() -> int {
                continue;
                return 0;
            }
            helper();
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("'continue'"));
}

#[test]
fn runs_else_if_control_flow() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let value: int = 2;
            if (value == 1) {
                "one";
            } else if (value == 2) {
                "two";
            } else {
                "other";
            }
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("two"));
}

#[test]
fn rejects_static_type_mismatch_before_execution() {
    let mut engine = Engine::new();
    let err = engine.eval("let answer: int = \"forty two\";").unwrap_err();
    assert!(err.message.contains("expected int, got str"));
}

#[test]
fn check_diagnostics_collects_independent_top_level_type_errors() {
    let mut engine = Engine::new();
    let diagnostics = engine
        .check_diagnostics(
            r#"
            let first: int = "bad";
            let second: bool = 1;
            "#,
        )
        .unwrap_err();

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].code, "type.mismatch");
    assert_eq!(diagnostics[1].code, "type.mismatch");
    assert!(diagnostics[0].message.contains("expected int, got str"));
    assert!(diagnostics[1].message.contains("expected bool, got int"));
}

#[test]
fn validates_option_and_result_type_annotations() {
    let mut engine = Engine::new();
    engine
        .check(
            r#"
            record Box {
                found: option[int],
                loaded: result[str, str],
            }
            null;
            "#,
        )
        .unwrap();
}

#[test]
fn rejects_unknown_option_payload_type() {
    let mut engine = Engine::new();
    let err = engine
        .check(
            r#"
            record Box {
                found: option[Missing],
            }
            null;
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("unknown type 'Missing'"));
}

#[test]
fn rejects_assigning_plain_value_to_option_type() {
    let mut engine = Engine::new();
    let err = engine
        .check(
            r#"
            let value: option[int] = 1;
            value;
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "type.mismatch");
    assert!(err.message.contains("expected option[int], got int"));
}

#[test]
fn evaluates_option_constructors() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let value: option[int] = some(42);
            value;
            "#,
        )
        .unwrap();
    assert_eq!(value.to_string(), "some(42)");
    assert_eq!(value_type(&value), Type::Option(Box::new(Type::Int)));

    let missing = engine
        .eval(
            r#"
            let value: option[str] = none;
            value;
            "#,
        )
        .unwrap();
    assert_eq!(missing.to_string(), "none");
    assert_eq!(value_type(&missing), Type::Option(Box::new(Type::Str)));
}

#[test]
fn evaluates_result_constructors() {
    let mut engine = Engine::new();
    let ok = engine
        .eval(
            r#"
            let value: result[int, str] = ok(7);
            value;
            "#,
        )
        .unwrap();
    assert_eq!(ok.to_string(), "ok(7)");
    assert_eq!(
        value_type(&ok),
        Type::Result {
            ok: Box::new(Type::Int),
            err: Box::new(Type::Str),
        }
    );

    let err = engine
        .eval(
            r#"
            let value: result[int, str] = err("missing");
            value;
            "#,
        )
        .unwrap();
    assert_eq!(err.to_string(), "err(missing)");
}

#[test]
fn rejects_untyped_none_ok_and_err_constructors() {
    let mut engine = Engine::new();
    let err = engine.check("none;").unwrap_err();
    assert!(err.message.contains("expected option type"));

    let err = engine.check("ok(1);").unwrap_err();
    assert!(err.message.contains("expected result type"));

    let err = engine.check("err(\"bad\");").unwrap_err();
    assert!(err.message.contains("expected result type"));
}

#[test]
fn rejects_wrong_result_constructor_payload() {
    let mut engine = Engine::new();
    let err = engine
        .check(
            r#"
            let value: result[int, str] = ok("bad");
            value;
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "type.mismatch");
    assert!(err.message.contains("expected int, got str"));
}

#[test]
fn heap_tracks_and_collects_option_result_values() {
    let mut engine = Engine::new();
    let option = engine
        .eval("let value: option[int] = some(1); value;")
        .unwrap();
    let result = engine
        .eval(r#"let value: result[int, str] = err("bad"); value;"#)
        .unwrap();

    assert!(engine.heap_object_count() >= 2);
    drop(option);
    drop(result);
    engine.collect_garbage();
    assert_eq!(engine.heap_object_count(), 0);
}

#[test]
fn reports_undefined_variables_with_span() {
    let mut engine = Engine::new();
    let err = engine.eval("missing;").unwrap_err();
    assert_eq!(err.span, Span { start: 0, end: 7 });
    assert!(err.message.contains("undefined variable"));
}

#[test]
fn rejects_non_bool_conditions() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            let value: int = 1;
            if (value) {
                value;
            }
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("expected bool, got int"));
}

#[test]
fn rejects_function_without_static_return_path() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            fn maybe(value: bool) -> int {
                if (value) {
                    return 1;
                }
            }
            maybe(true);
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("function 'maybe' must return int"));
}

#[test]
fn rejects_wrong_function_argument_count() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            fn id(value: int) -> int {
                return value;
            }
            id();
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("expected 1 arguments but got 0"));
}

#[test]
fn rejects_mixed_string_and_int_addition() {
    let mut engine = Engine::new();
    let err = engine.eval(r#""nox" + 1;"#).unwrap_err();
    assert!(err.message.contains("'+' is not defined for str and int"));
}

#[test]
fn rejects_calling_non_function_value() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            let value: int = 1;
            value();
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("called value is not a function"));
}

#[test]
fn rejects_return_outside_function() {
    let mut engine = Engine::new();
    let err = engine.eval("return 1;").unwrap_err();
    assert!(err.message.contains("return outside function"));
}

#[test]
fn supports_block_scope_shadowing() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let value: int = 10;
            {
                let value: int = 20;
                value = value + 1;
            }
            value;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(10));
}

#[test]
fn supports_string_typed_functions() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn label(name: str, status: str) -> str {
                return name + ":" + status;
            }
            label("nox", "typed");
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("nox:typed"));
}

#[test]
fn runs_recursive_function_pressure() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn factorial(value: int) -> int {
                if (value <= 1) {
                    return 1;
                } else {
                    return value * factorial(value - 1);
                }
            }
            factorial(6);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(720));
}

#[test]
fn runs_loop_pressure() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let total: int = 0;
            for i in 0..100 {
                total = total + i;
            }
            total;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(4950));
}

#[test]
fn runs_deep_scope_pressure() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let value: int = 1;
            {
                let value: int = value + 1;
                {
                    let value: int = value + 1;
                    {
                        let value: int = value + 1;
                        value;
                    }
                }
            }
            value;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(1));
}

#[test]
fn cancels_recursive_execution_when_instruction_budget_is_exhausted() {
    let mut engine = Engine::new();
    engine.set_instruction_budget(Some(32));
    let err = engine
        .eval(
            r#"
            fn count(value: int) -> int {
                return count(value + 1);
            }
            count(0);
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("instruction budget exhausted"));
}
