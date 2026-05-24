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
fn string_interpolation_stringifies_primitive_values() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let name: str = "nox";
            let count: int = 3;
            let ready: bool = true;
            let ratio: float = 1.5;
            "name=${name}, count=${count + 1}, ready=${ready}, ratio=${ratio}, none=${null}";
            "#,
        )
        .unwrap();
    assert_eq!(
        value,
        Value::string("name=nox, count=4, ready=true, ratio=1.5, none=null")
    );
}

#[test]
fn string_interpolation_rejects_non_stringifiable_values() {
    let mut engine = Engine::new();
    let err = engine.eval(r#""bad=${[1, 2]}";"#).unwrap_err();
    assert_eq!(err.code, "string.interpolation");
    assert!(err
        .message
        .contains("string interpolation cannot stringify [int]"));
}

#[test]
fn string_interpolation_reports_parser_errors_with_stable_code() {
    let mut engine = Engine::new();
    let err = engine.eval(r#""bad=${1 + }";"#).unwrap_err();
    assert_eq!(err.code, "string.interpolation");
    assert!(err.message.contains("expected expression"));
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
fn question_mark_propagation_unwraps_ok_and_some_values() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn read_count() -> result[int, str] {
                return ok(40);
            }

            fn maybe_bonus() -> option[int] {
                return some(2);
            }

            fn result_total() -> result[int, str] {
                let count: int = read_count()?;
                return ok(count + 2);
            }

            fn option_total() -> option[int] {
                let bonus: int = maybe_bonus()?;
                return some(40 + bonus);
            }

            let from_result: result[int, str] = result_total();
            let from_option: option[int] = option_total();
            match (from_result) {
                ok(value) => {
                    match (from_option) {
                        some(bonus) => {
                            value + bonus;
                        }
                        none => {
                            0;
                        }
                    }
                }
                err(message) => {
                    len(message);
                }
            }
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(84));
}

#[test]
fn question_mark_propagation_returns_err_and_none_early() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn fail() -> result[int, str] {
                return err("missing");
            }

            fn chain() -> result[int, str] {
                let value: int = fail()?;
                return ok(value + 1);
            }

            chain();
            "#,
        )
        .unwrap();
    assert_eq!(value.to_string(), "err(missing)");
    assert_eq!(
        value_type(&value),
        Type::Result {
            ok: Box::new(Type::Int),
            err: Box::new(Type::Str),
        }
    );

    let value = engine
        .eval(
            r#"
            fn missing() -> option[int] {
                return none;
            }

            fn chain() -> option[str] {
                let value: int = missing()?;
                return some("value=${value}");
            }

            chain();
            "#,
        )
        .unwrap();
    assert_eq!(value.to_string(), "none");
    assert_eq!(value_type(&value), Type::Option(Box::new(Type::Str)));
}

#[test]
fn question_mark_propagation_rejects_mismatched_contexts() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            fn fail() -> result[int, str] {
                return err("missing");
            }

            fn bad() -> result[int, int] {
                let value: int = fail()?;
                return ok(value);
            }

            bad();
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "result.question-mark.mismatch");
    assert!(err.message.contains("'?' error type mismatch"));

    let err = engine
        .eval(
            r#"
            fn fail() -> result[int, str] {
                return err("missing");
            }

            fail()?;
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "result.question-mark.mismatch");
    assert!(err.message.contains("inside a function"));
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
fn match_extended_patterns_match_number_ranges_and_float_literals() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let code: int = 7;
            let label: str = "";
            match (code) {
                0..5 => {
                    label = "low";
                }
                5..10 => {
                    label = "mid";
                }
                _ => {
                    label = "high";
                }
            }

            let ratio: float = 1.5;
            match (ratio) {
                1.5 => {
                    label = label + ":float";
                }
                _ => {
                    label = label + ":other";
                }
            }
            label;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("mid:float"));
}

#[test]
fn match_extended_patterns_destructure_nested_options() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let input: option[option[int]] = some(some(41));
            let answer: int = 0;
            match (input) {
                some(some(value)) => {
                    answer = value + 1;
                }
                some(none) => {
                    answer = -1;
                }
                none => {
                    answer = -2;
                }
            }
            answer;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn match_extended_patterns_destructure_nested_result_payloads() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let input: result[option[int], str] = ok(some(40));
            let answer: int = 0;
            match (input) {
                ok(some(value)) => {
                    answer = value + 2;
                }
                ok(none) => {
                    answer = -1;
                }
                err(message) => {
                    answer = len(message);
                }
            }
            answer;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn match_extended_rejects_non_exhaustive_nested_option_match() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            let input: option[option[int]] = some(none);
            match (input) {
                some(some(value)) => {}
                none => {}
            }
            "#,
        )
        .unwrap_err();
    assert!(err
        .message
        .contains("option match must cover some and none"));
}

#[test]
fn control_flow_let_patterns_match_option_result_and_enum_values() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            enum LoadState {
                Loading,
                Ready(int),
                Failed(str),
            }

            fn next(index: int) -> option[int] {
                if (index < 3) {
                    return some(index + 1);
                }
                return none;
            }

            fn describe(input: result[LoadState, str]) -> str {
                if let ok(Ready(value)) = input {
                    return "ready:${value}";
                } else if let err(message) = input {
                    return "err:" + message;
                } else {
                    return "other";
                }
            }

            fn require_value(input: option[int]) -> int {
                let some(value) = input else {
                    return 0;
                };
                return value + 1;
            }

            let index: int = 0;
            let total: int = 0;
            while let some(value) = next(index) {
                total = total + value;
                index = index + 1;
            }

            describe(ok(LoadState.Ready(total + require_value(some(35)))));
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("ready:42"));
}

#[test]
fn control_flow_let_patterns_scope_bindings_like_match_cases() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            let input: option[int] = some(1);
            if let some(value) = input {
                value;
            }
            value;
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("undefined variable 'value'"));
}

#[test]
fn control_flow_let_patterns_reject_type_mismatch() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            if let some(value) = 1 {
                value;
            }
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("some(pattern) requires option"));
}

#[test]
fn control_flow_let_patterns_require_let_else_to_exit() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            fn read(input: option[int]) -> int {
                let some(value) = input else {
                    0;
                };
                return value;
            }

            read(some(1));
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "control-flow.let-else-fallthrough");
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
        .contains("match value must be int, float, str, option, result, or enum"));
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
fn tuple_literals_and_destructuring_bind_elements() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let pair: (int, str) = (41, "ok");
            let (count, label) = pair;
            if (label == "ok") {
                count + 1;
            } else {
                0;
            }
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn record_destructuring_binds_fields() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            record Point {
                x: int,
                y: int,
            }

            let point: Point = Point { x: 20, y: 22 };
            let { x, y } = point;
            x + y;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn rejects_tuple_literal_arity_mismatch() {
    let mut engine = Engine::new();
    let err = engine
        .eval(r#"let pair: (int, str) = (1, "ok", 3);"#)
        .unwrap_err();
    assert_eq!(err.code, "tuple.arity-mismatch");
}

#[test]
fn rejects_tuple_literal_element_type_mismatch() {
    let mut engine = Engine::new();
    let err = engine
        .eval(r#"let pair: (int, str) = (1, 2);"#)
        .unwrap_err();
    assert_eq!(err.code, "tuple.element-type-mismatch");
}

#[test]
fn rejects_tuple_destructuring_arity_mismatch() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            let pair: (int, str) = (1, "ok");
            let (only, extra, names) = pair;
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "tuple.arity-mismatch");
}

#[test]
fn heap_tracks_and_collects_script_tuple_values() {
    let mut engine = Engine::new();
    let value = engine
        .eval(r#"let pair: (int, str) = (1, "two"); pair;"#)
        .unwrap();
    assert!(matches!(value, Value::Tuple(_)));
    assert!(engine.heap_object_count() >= 1);

    drop(value);
    engine.collect_garbage();
    assert_eq!(engine.heap_object_count(), 0);
}

#[test]
fn type_alias_expands_named_and_tuple_types() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            type UserId = int;
            type Pair = (UserId, str);
            let pair: Pair = (41, "ok");
            let (id, label) = pair;
            if (label == "ok") {
                id + 1;
            } else {
                0;
            }
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn type_alias_expands_record_fields_and_function_signatures() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            type UserId = int;

            record User {
                id: UserId,
                name: str,
            }

            fn get_id(user: User) -> UserId {
                return user.id;
            }

            let user: User = User { id: 42, name: "nox" };
            get_id(user);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn type_alias_rejects_cycles() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            type A = B;
            type B = A;
            let value: A = 1;
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "type-alias.cyclic");
}

#[test]
fn user_enum_constructs_and_matches_payload_variants() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            enum LoadState {
                Loading,
                Ready(int),
                Failed(str),
            }

            let state: LoadState = LoadState.Ready(41);
            let total: int = 0;
            match (state) {
                Loading => {
                    total = 0;
                }
                Ready(value) => {
                    total = value + 1;
                }
                Failed(message) => {
                    total = len(message);
                }
            }
            total;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn user_enum_rejects_non_exhaustive_matches() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            enum LoadState {
                Loading,
                Ready(int),
            }

            let state: LoadState = LoadState.Loading;
            match (state) {
                Loading => {
                    0;
                }
            }
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "match.non-exhaustive");
}

#[test]
fn user_enum_rejects_missing_variants() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            enum LoadState {
                Loading,
            }

            let state: LoadState = LoadState.Ready(41);
            state;
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "enum.variant-not-found");
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
fn map_intrinsics_return_keys_values_presence_and_size() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let scores: map[str, int] = { "runtime": 22, "core": 20 };
            let keys: [str] = map_keys(scores);
            let values: [int] = map_values(scores);
            let has_core: bool = map_has(scores, "core");
            let has_missing: bool = map_has(scores, "missing");
            let size: int = map_size(scores);
            if (keys[0] == "core" && keys[1] == "runtime" && values[0] == 20 && values[1] == 22 && has_core && !has_missing) {
                size;
            } else {
                0;
            }
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(2));
}

#[test]
fn map_intrinsics_type_check_arguments() {
    let mut engine = Engine::new();

    let err = engine.eval("map_keys([1, 2]);").unwrap_err();
    assert!(err.message.contains("expected map"));

    let err = engine
        .eval(
            r#"
            let scores: map[str, int] = { "a": 1 };
            map_has(scores, 1);
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("expected str, got int"));

    let err = engine
        .eval(
            r#"
            let scores: map[str, int] = { "a": 1 };
            map_size(scores, "extra");
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("expected 1 arguments but got 2"));
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
fn record_method_syntax_rewrites_to_function_call() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            record User {
                name: str,
                score: int,
            }

            fn label(user: User, suffix: str) -> str {
                return "${user.name}:${user.score}${suffix}";
            }

            let user: User = User { name: "nox", score: 42 };
            user.label("!");
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("nox:42!"));
}

#[test]
fn record_method_syntax_reports_missing_method_code() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            record User {
                name: str,
            }

            let user: User = User { name: "nox" };
            user.missing();
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "record.method-not-found");
    assert!(err
        .message
        .contains("record 'User' has no method 'missing'"));
}

#[test]
fn record_method_syntax_requires_matching_receiver_type() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            record User {
                name: str,
            }

            record Project {
                name: str,
            }

            fn label(project: Project) -> str {
                return project.name;
            }

            let user: User = User { name: "nox" };
            user.label();
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "record.method-not-found");
    assert!(err.message.contains("first parameter must be User"));
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
fn generic_function_infers_argument_type() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn id<T>(value: T) -> T {
                return value;
            }
            id(42);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn generic_function_infers_tuple_payloads() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn first<T>(pair: (T, str)) -> T {
                let (value, label) = pair;
                return value;
            }
            first((7, "days"));
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(7));
}

#[test]
fn generic_function_uses_expected_return_type_for_empty_containers() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn empty<T>() -> [T] {
                return [];
            }
            let values: [int] = empty();
            len(values);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(0));
}

#[test]
fn generic_function_reports_conflicting_inference() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            fn choose<T>(left: T, right: T) -> T {
                return left;
            }
            choose(1, "one");
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "generic.infer-failed");
}

#[test]
fn trait_impl_method_call_runs_static_dispatch_mvp() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            trait Display {
                fn to_str(self: Self) -> str;
            }

            record User {
                name: str,
            }

            impl Display for User {
                fn to_str(self: User) -> str {
                    return self.name;
                }
            }

            let user: User = User { name: "nox" };
            user.to_str();
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("nox"));
}

#[test]
fn trait_bound_rejects_unimplemented_type() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            trait Display {
                fn to_str(self: Self) -> str;
            }

            record User {
                name: str,
            }

            fn label<T: Display>(value: T) -> str {
                return value.to_str();
            }

            let user: User = User { name: "nox" };
            label(user);
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "trait.bound-unsatisfied");
}

#[test]
fn trait_impl_requires_all_methods() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            trait Display {
                fn to_str(self: Self) -> str;
            }

            record User {
                name: str,
            }

            impl Display for User {
            }
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "trait.impl-incomplete");
}

#[test]
fn trait_impl_rejects_signature_mismatch() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            trait Display {
                fn to_str(self: Self) -> str;
            }

            record User {
                name: str,
            }

            impl Display for User {
                fn to_str(self: User) -> int {
                    return 1;
                }
            }
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "trait.method-signature-mismatch");
}

#[test]
fn trait_impl_method_dispatch_allows_same_method_name_for_different_types() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            trait Display {
                fn to_str(self: Self) -> str;
            }

            record User {
                name: str,
            }

            record Team {
                name: str,
            }

            impl Display for User {
                fn to_str(self: User) -> str {
                    return self.name;
                }
            }

            impl Display for Team {
                fn to_str(self: Team) -> str {
                    return self.name;
                }
            }

            fn label<T: Display>(value: T) -> str {
                return value.to_str();
            }

            let user: User = User { name: "nox" };
            let team: Team = Team { name: "core" };
            label(user) + ":" + label(team);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("nox:core"));
}

#[test]
fn trait_impl_rejects_top_level_function_collision() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            trait Display {
                fn to_str(self: Self) -> str;
            }

            record User {
                name: str,
            }

            fn to_str(value: str) -> str {
                return value;
            }

            impl Display for User {
                fn to_str(self: User) -> str {
                    return self.name;
                }
            }
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "trait.method-ambiguous");
}

#[test]
fn runtime_stack_trace_records_script_call_frames() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            fn divide(value: int) -> int {
                return value / 0;
            }

            fn wrapper(value: int) -> int {
                return divide(value);
            }

            wrapper(1);
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "runtime.division-by-zero");
    let frames = err
        .stack_frames
        .iter()
        .map(|frame| frame.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(frames, vec!["divide", "wrapper"]);
}

#[test]
fn bitwise_ops_evaluate_int_operands() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let combined: int = (5 & 3) + (4 | 1) + (6 ^ 3) + (1 << 4) + (16 >> 2) + (~0);
            let arithmetic_shift: int = -8 >> 1;
            combined + arithmetic_shift;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(26));
}

#[test]
fn bitwise_ops_reject_non_int_operands() {
    let mut engine = Engine::new();
    let err = engine.eval("1 & true;").unwrap_err();
    assert_eq!(err.code, "type.bitwise-non-int");

    let err = engine.eval("~false;").unwrap_err();
    assert_eq!(err.code, "type.bitwise-non-int");
}

#[test]
fn spread_operator_copies_arrays_and_maps() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let base: [int] = [1, 2];
            let extra: [int] = [4, 5];
            let values: [int] = [0, ...base, 3, ...extra];

            let first: map[str, int] = {"a": 1, "b": 2};
            let second: map[str, int] = {"b": 20, "c": 3};
            let merged: map[str, int] = {...first, ...second, "d": 4};

            values[0] + values[1] + values[2] + values[3] + values[4] + values[5]
                + merged["a"] + merged["b"] + merged["c"] + merged["d"];
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(43));
}

#[test]
fn spread_operator_preserves_nested_array_values() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            record Boxed {
                items: [int],
            }

            let first: [int] = [1, 2];
            let second: [int] = [3, 4];
            let boxes: [Boxed] = [
                Boxed { items: first },
                ...[Boxed { items: second }],
            ];
            boxes[0].items[1] + boxes[1].items[0];
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(5));
}

#[test]
fn spread_operator_rejects_non_container_sources() {
    let mut engine = Engine::new();
    let err = engine.eval("let values: [int] = [...1];").unwrap_err();
    assert_eq!(err.code, "type.spread-mismatch");

    let err = engine
        .eval("let values: map[str, int] = {...1};")
        .unwrap_err();
    assert_eq!(err.code, "type.spread-mismatch");
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

#[test]
fn diagnostic_suggests_similar_variable_name() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            let total: int = 10;
            totl;
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("undefined variable 'totl'"));
    assert!(err.message.contains("did you mean 'total'?"));
}

#[test]
fn diagnostic_suggests_similar_record_field_name() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            record User {
                username: str,
                score: int
            }
            let u: User = User { username: "a", score: 1 };
            u.usrname;
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("record 'User' has no field 'usrname'"));
    assert!(err.message.contains("did you mean 'username'?"));
}

#[test]
fn diagnostic_suggests_similar_enum_variant_name() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            enum Status { Active, Inactive }
            let s: Status = Status.Activ;
            s;
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("enum 'Status' has no variant 'Activ'"));
    assert!(err.message.contains("did you mean 'Active'?"));
}

#[test]
fn diagnostic_suggests_similar_type_name() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            record User { id: int }
            let u: Usr = User { id: 1 };
            u;
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("unknown type 'Usr'"));
    assert!(err.message.contains("did you mean 'User'?"));
}

#[test]
fn diagnostic_omits_suggestion_when_no_similar_name_exists() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            let value: int = 1;
            qwerty;
            "#,
        )
        .unwrap_err();
    assert!(err.message.contains("undefined variable 'qwerty'"));
    assert!(!err.message.contains("did you mean"));
}

#[test]
fn function_type_syntax_binds_named_function_as_value() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn double(x: int) -> int {
                return x * 2;
            }
            let f: fn(int) -> int = double;
            f(7);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(14));
}

#[test]
fn function_value_can_be_passed_as_parameter() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn double(x: int) -> int {
                return x * 2;
            }
            fn apply(f: fn(int) -> int, v: int) -> int {
                return f(v);
            }
            apply(double, 5);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(10));
}

#[test]
fn function_values_can_be_array_elements() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn double(x: int) -> int {
                return x * 2;
            }
            fn triple(x: int) -> int {
                return x * 3;
            }
            let fs: [fn(int) -> int] = [double, triple];
            fs[0](2) + fs[1](2);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(10));
}

#[test]
fn lambda_literal_can_be_bound_and_called() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let twice: fn(int) -> int = fn(x: int) -> int {
                return x * 2;
            };
            twice(7);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(14));
}

#[test]
fn lambda_captures_outer_binding_by_lexical_scope() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            let base: int = 100;
            let add_base: fn(int) -> int = fn(x: int) -> int {
                return x + base;
            };
            add_base(5);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(105));
}

#[test]
fn lambda_can_be_passed_as_first_class_argument() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn apply(f: fn(int) -> int, v: int) -> int {
                return f(v);
            }
            apply(fn(x: int) -> int { return x + 3; }, 4);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(7));
}

#[test]
fn constraint_equatable_accepts_int_array_argument() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn dedupe<T: Equatable>(xs: [T]) -> [T] {
                return xs;
            }
            let r: [int] = dedupe([1, 2, 3]);
            r[0];
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(1));
}

#[test]
fn constraint_unsatisfied_with_function_value_reports_stable_code() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            fn pick<T: Equatable>(x: T) -> T {
                return x;
            }
            fn maker(y: int) -> int {
                return y;
            }
            let f: fn(int) -> int = maker;
            pick(f);
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "generic.constraint-unsatisfied");
    assert!(
        err.message
            .contains("does not implement constraint 'Equatable'"),
        "unexpected message: {}",
        err.message
    );
}

#[test]
fn unknown_trait_bound_is_rejected_with_stable_code() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            fn bad<T: Foo>(x: T) -> T {
                return x;
            }
            bad(1);
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "trait.not-found");
    assert!(err.message.contains("trait 'Foo' is not defined"));
}

#[test]
fn constraint_combinations_with_plus_operator_are_supported() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn echo<T: Equatable + Stringify>(x: T) -> T {
                return x;
            }
            echo("hello");
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("hello"));
}

#[test]
fn reserved_keywords_for_future_exceptions_are_rejected() {
    for keyword in &["try", "catch", "panic", "defer", "finally"] {
        let mut engine = Engine::new();
        let source = format!("let {keyword}: int = 1;");
        let err = engine.eval(&source).unwrap_err();
        assert_eq!(err.code, "parse.reserved-keyword", "keyword: {keyword}");
        assert!(
            err.message.contains(keyword),
            "keyword '{keyword}' missing from message: {}",
            err.message
        );
    }
}

#[test]
fn rejects_deep_recursion_when_max_call_depth_is_set() {
    let mut engine = Engine::new();
    engine.set_max_call_stack_depth(Some(8));
    let err = engine
        .eval(
            r#"
            fn count(value: int) -> int {
                if (value == 0) {
                    return 0;
                }
                return count(value - 1);
            }
            count(100);
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "runtime.call-stack-overflow");
    assert!(err.message.contains("call stack depth"));
}

#[test]
fn lint_flags_duplicate_match_arm_with_equal_pattern() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn classify(value: int) -> str {
                match (value) {
                    1 => { return "one"; }
                    2 => { return "two"; }
                    1 => { return "again"; }
                    other => { return "other"; }
                }
            }
            "#,
        )
        .unwrap();
    assert!(
        warnings.iter().any(|w| w.code == "lint.duplicate-match-arm"
            && w.message.contains("duplicates an earlier arm")),
        "expected lint.duplicate-match-arm, got {warnings:?}"
    );
}

#[test]
fn lint_flags_duplicate_enum_variant_match_arm() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            enum Event {
                Click,
                Quit,
            }
            export fn classify(value: Event) -> str {
                match (value) {
                    Click => { return "click"; }
                    Quit => { return "quit"; }
                    Click => { return "again"; }
                }
                return "fallback";
            }
            "#,
        )
        .unwrap();
    assert!(
        warnings.iter().any(|w| w.code == "lint.duplicate-match-arm"
            && w.message.contains("duplicates an earlier arm")),
        "expected duplicate enum variant arm warning, got {warnings:?}"
    );
}

#[test]
fn lint_does_not_flag_distinct_match_arms() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn classify(value: int) -> str {
                match (value) {
                    1 => { return "one"; }
                    2 => { return "two"; }
                    other => { return "other"; }
                }
            }
            "#,
        )
        .unwrap();
    assert!(
        !warnings
            .iter()
            .any(|w| w.code == "lint.duplicate-match-arm"),
        "did not expect duplicate-match-arm warnings, got {warnings:?}"
    );
}

#[test]
fn max_string_length_rejects_concatenation_beyond_cap() {
    let mut engine = Engine::new();
    engine.set_max_string_length(Some(8));
    let err = engine
        .eval(
            r#"
            let a: str = "hello";
            let b: str = " world";
            let combined: str = a + b;
            combined;
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "runtime.string-length-cap");
    assert!(
        err.message.contains("exceeds configured cap"),
        "expected cap-exceed message, got: {}",
        err.message
    );
}

#[test]
fn max_string_length_allows_concatenation_within_cap() {
    let mut engine = Engine::new();
    engine.set_max_string_length(Some(64));
    let value = engine
        .eval(
            r#"
            let a: str = "hello";
            let b: str = " world";
            a + b;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("hello world"));
}

#[test]
fn max_array_length_rejects_construction_beyond_cap() {
    let mut engine = Engine::new();
    engine.set_max_array_length(Some(3));
    let err = engine
        .eval(
            r#"
            let xs: [int] = [1, 2, 3, 4];
            xs;
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "runtime.array-length-cap");
    assert!(
        err.message.contains("exceeds configured cap"),
        "expected cap-exceed message, got: {}",
        err.message
    );
}

#[test]
fn max_map_entries_rejects_construction_beyond_cap() {
    let mut engine = Engine::new();
    engine.set_max_map_entries(Some(1));
    let err = engine
        .eval(
            r#"
            let m: map[str, int] = {"a": 1, "b": 2};
            m;
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "runtime.map-size-cap");
    assert!(
        err.message.contains("exceeds configured cap"),
        "expected cap-exceed message, got: {}",
        err.message
    );
}

#[test]
fn max_map_entries_rejects_index_assignment_growth_beyond_cap() {
    let mut engine = Engine::new();
    engine.set_max_map_entries(Some(1));
    let err = engine
        .eval(
            r#"
            let m: map[str, int] = {"a": 1};
            m["b"] = 2;
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "runtime.map-size-cap");
}

#[test]
fn max_map_entries_allows_index_assignment_update_within_cap() {
    let mut engine = Engine::new();
    engine.set_max_map_entries(Some(1));
    let value = engine
        .eval(
            r#"
            let m: map[str, int] = {"a": 1};
            m["a"] = 2;
            m["a"];
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(2));
}

#[test]
fn max_heap_objects_rejects_vm_allocations_beyond_cap() {
    let mut engine = Engine::new();
    engine.set_max_heap_objects(Some(2));
    let err = engine.eval(r#"["a", "b"];"#).unwrap_err();
    assert_eq!(err.code, "runtime.heap-object-cap");
}

#[test]
fn max_heap_objects_tracks_host_return_values() {
    let mut engine = Engine::new();
    engine
        .register_host_function(
            HostFunctionBuilder::new("make_array", Type::Array(Box::new(Type::Int))),
            |_| Ok(Value::array(Type::Int, vec![Value::Int(1)])),
        )
        .unwrap();
    engine.set_max_heap_objects(Some(0));
    let err = engine.eval("make_array();").unwrap_err();
    assert_eq!(err.code, "runtime.heap-object-cap");
}

#[test]
fn profile_records_vm_hot_path_operations() {
    let mut engine = Engine::new();
    let (_value, profile) = engine
        .profile(
            r#"
            enum Choice { A(int), B }

            fn pick(value: Choice) -> int {
                match (value) {
                    A(n) => { return n; }
                    B => { return 0; }
                }
            }

            let xs: [int] = [1, 2, 3];
            let pair: (int, int) = (xs[0], pick(Choice.A(4)));
            let data: map[str, int] = {"a": 1};
            data["b"] = 2;
            let value: option[int] = map_get(data, "a");
            map_keys(data);
            map_values(data);
            to_int(1.5);
            pair;
            "#,
        )
        .unwrap();

    for name in [
        "array_literal",
        "tuple_literal",
        "map_literal",
        "index",
        "index_assign",
        "match_pattern",
        "map_get",
        "map_keys",
        "map_values",
        "host_callback",
    ] {
        let count = profile
            .operations
            .get(name)
            .map(|row| row.count)
            .unwrap_or_default();
        assert!(count > 0, "missing profile operation {name}: {profile:?}");
    }
}

#[test]
fn host_function_builder_records_docstring_and_capabilities() {
    let mut engine = Engine::new();
    engine
        .register_host_function(
            HostFunctionBuilder::new("io__read_secret", Type::Str)
                .param("path", Type::Str)
                .docstring("Read the secret stored at `path`. Requires the filesystem capability.")
                .capability("filesystem"),
            |_| Err(Diagnostic::new("host stub", Span { start: 0, end: 0 })),
        )
        .expect("registration should succeed");

    let names = engine.host_function_names();
    assert!(
        names.iter().any(|name| name == "io__read_secret"),
        "expected io__read_secret in {names:?}"
    );

    let signature = engine
        .host_function_signature("io__read_secret")
        .expect("signature should exist");
    assert_eq!(signature.name, "io__read_secret");
    assert_eq!(signature.params.len(), 1);
    assert_eq!(signature.params[0].0, "path");
    assert_eq!(signature.return_type, Type::Str);
    assert_eq!(
        signature.docstring.as_deref(),
        Some("Read the secret stored at `path`. Requires the filesystem capability."),
    );
    assert_eq!(signature.capabilities, vec!["filesystem".to_string()]);
}

#[test]
fn lint_flags_unreachable_code_after_return() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn helper() -> int {
                return 1;
                let dead: int = 2;
            }
            "#,
        )
        .unwrap();
    assert!(
        warnings.iter().any(|w| w.code == "lint.unreachable-code"),
        "expected lint.unreachable-code, got {warnings:?}"
    );
}

#[test]
fn lint_flags_unreachable_code_in_if_branch() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn helper(flag: bool) -> int {
                if (flag) {
                    return 1;
                    let dead: int = 2;
                }
                return 0;
            }
            "#,
        )
        .unwrap();
    assert!(
        warnings.iter().any(|w| w.code == "lint.unreachable-code"),
        "expected lint.unreachable-code in if branch, got {warnings:?}"
    );
}

#[test]
fn lint_does_not_flag_normal_function_body_as_unreachable() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn helper() -> int {
                let value: int = 1;
                return value + 1;
            }
            "#,
        )
        .unwrap();
    assert!(
        !warnings.iter().any(|w| w.code == "lint.unreachable-code"),
        "did not expect lint.unreachable-code, got {warnings:?}"
    );
}

#[test]
fn lint_flags_unreachable_after_break_in_while() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn helper() -> int {
                let counter: int = 0;
                while (counter < 10) {
                    break;
                    counter = counter + 1;
                }
                return counter;
            }
            "#,
        )
        .unwrap();
    assert!(
        warnings.iter().any(|w| w.code == "lint.unreachable-code"),
        "expected lint.unreachable-code after break, got {warnings:?}"
    );
}

#[test]
fn lint_flags_shadowing_in_nested_block() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn helper() -> int {
                let value: int = 1;
                if (value > 0) {
                    let value: int = 2;
                    return value;
                }
                return value;
            }
            "#,
        )
        .unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.code == "lint.shadowed-variable" && w.message.contains("'value'")),
        "expected lint.shadowed-variable for 'value', got {warnings:?}"
    );
}

#[test]
fn lint_does_not_flag_reassignment_in_same_scope_as_shadowing() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn helper() -> int {
                let value: int = 1;
                value = 2;
                return value;
            }
            "#,
        )
        .unwrap();
    assert!(
        !warnings.iter().any(|w| w.code == "lint.shadowed-variable"),
        "did not expect shadowing warning, got {warnings:?}"
    );
}

#[test]
fn lint_does_not_flag_underscore_prefixed_shadowing() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn helper() -> int {
                let _ignored: int = 1;
                if (true) {
                    let _ignored: int = 2;
                    return 0;
                }
                return 0;
            }
            "#,
        )
        .unwrap();
    assert!(
        !warnings.iter().any(|w| w.code == "lint.shadowed-variable"),
        "did not expect underscore-prefixed shadowing warning, got {warnings:?}"
    );
}

#[test]
fn lint_flags_constant_true_in_if_condition() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn helper() -> int {
                if (true) {
                    return 1;
                }
                return 0;
            }
            "#,
        )
        .unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.code == "lint.constant-condition" && w.message.contains("always true")),
        "expected lint.constant-condition for if (true), got {warnings:?}"
    );
}

#[test]
fn lint_flags_constant_false_in_if_condition() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn helper() -> int {
                if (false) {
                    return 1;
                }
                return 0;
            }
            "#,
        )
        .unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.code == "lint.constant-condition" && w.message.contains("always false")),
        "expected lint.constant-condition for if (false), got {warnings:?}"
    );
}

#[test]
fn lint_does_not_flag_while_true_forever_loop() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn helper() -> int {
                while (true) {
                    return 1;
                }
                return 0;
            }
            "#,
        )
        .unwrap();
    assert!(
        !warnings.iter().any(|w| w.code == "lint.constant-condition"),
        "did not expect constant-condition for while (true), got {warnings:?}"
    );
}

#[test]
fn lint_flags_while_false_never_executed() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn helper() -> int {
                while (false) {
                    return 1;
                }
                return 0;
            }
            "#,
        )
        .unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.code == "lint.constant-condition" && w.message.contains("never executes")),
        "expected lint.constant-condition for while (false), got {warnings:?}"
    );
}

#[test]
fn lint_flags_shadowing_of_function_parameter() {
    let mut engine = Engine::new();
    let warnings = engine
        .lint(
            r#"
            export fn helper(value: int) -> int {
                if (value > 0) {
                    let value: int = 2;
                    return value;
                }
                return value;
            }
            "#,
        )
        .unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.code == "lint.shadowed-variable" && w.message.contains("'value'")),
        "expected lint.shadowed-variable for parameter shadowing, got {warnings:?}"
    );
}

#[test]
fn allows_recursion_within_max_call_depth_limit() {
    let mut engine = Engine::new();
    engine.set_max_call_stack_depth(Some(64));
    let value = engine
        .eval(
            r#"
            fn count(value: int) -> int {
                if (value == 0) {
                    return 0;
                }
                return count(value - 1);
            }
            count(10);
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(0));
}

#[test]
fn async_fn_await_runs_ready_task_mvp() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            async fn compute(value: int) -> int {
                return value + 1;
            }

            async fn main() -> int {
                let task: task[int] = compute(41);
                return await task;
            }

            main();
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "async.top-level-task");
}

#[test]
fn async_question_mark_propagates_result_and_option_payloads() {
    let mut engine = Engine::new();
    let value = engine
        .eval(
            r#"
            fn fallible(flag: bool) -> result[int, str] {
                if (flag) {
                    return ok(40);
                }
                return err("missing");
            }

            fn maybe(flag: bool) -> option[int] {
                if (flag) {
                    return some(40);
                }
                return none;
            }

            async fn result_total(flag: bool) -> result[int, str] {
                let count: int = fallible(flag)?;
                return ok(count + 2);
            }

            async fn option_total(flag: bool) -> option[int] {
                let count: int = maybe(flag)?;
                return some(count + 2);
            }

            async fn verify() -> int {
                let result_ok: result[int, str] = await result_total(true);
                match (result_ok) {
                    ok(value) => {
                        if (value != 42) {
                            return 1 / 0;
                        }
                    }
                    err(message) => {
                        return 1 / 0;
                    }
                }

                let result_err: result[int, str] = await result_total(false);
                match (result_err) {
                    ok(value) => {
                        return 1 / 0;
                    }
                    err(message) => {
                        if (message != "missing") {
                            return 1 / 0;
                        }
                    }
                }

                let option_some: option[int] = await option_total(true);
                match (option_some) {
                    some(value) => {
                        if (value != 42) {
                            return 1 / 0;
                        }
                    }
                    none => {
                        return 1 / 0;
                    }
                }

                let option_none: option[int] = await option_total(false);
                match (option_none) {
                    some(value) => {
                        return 1 / 0;
                    }
                    none => {}
                }

                return 99;
            }

            let task: task[int] = verify();
            7;
            "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(7));
}

#[test]
fn async_question_mark_rejects_result_error_type_mismatch() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            fn fail() -> result[int, str] {
                return err("missing");
            }

            async fn bad() -> result[int, int] {
                let value: int = fail()?;
                return ok(value);
            }

            let task: task[result[int, int]] = bad();
            0;
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "result.question-mark.mismatch");
    assert!(err.message.contains("'?' error type mismatch"));
}

#[test]
fn await_outside_async_is_rejected_with_stable_code() {
    let mut engine = Engine::new();
    let err = engine
        .check(
            r#"
            async fn compute() -> int {
                return 1;
            }

            await compute();
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "async.await-outside-async");
}

#[test]
fn await_non_task_is_rejected_with_stable_code() {
    let mut engine = Engine::new();
    let err = engine
        .check(
            r#"
            async fn compute() -> int {
                return await 1;
            }
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "async.await-non-task");
}

#[test]
fn top_level_async_task_is_rejected_with_stable_code() {
    let mut engine = Engine::new();
    let err = engine
        .eval(
            r#"
            async fn compute() -> int {
                return 1;
            }

            compute();
            "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "async.top-level-task");
}
