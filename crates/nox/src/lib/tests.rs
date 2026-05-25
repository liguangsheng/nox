use super::*;
use nox_core::Session;
use std::sync::{Mutex, MutexGuard};

static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

const STD_MODULE_SPECIFIERS: &[&str] = &[
    "std/fs.nox",
    "std/path.nox",
    "std/env.nox",
    "std/process.nox",
    "std/time.nox",
    "std/string.nox",
    "std/json.nox",
    "std/jsonl.nox",
    "std/csv.nox",
    "std/tsv.nox",
    "std/array.nox",
    "std/map.nox",
    "std/option.nox",
    "std/result.nox",
    "std/term.nox",
    "std/bytes.nox",
    "std/encoding.nox",
    "std/hash.nox",
    "std/traits.nox",
    "std/dotenv.nox",
    "std/ini.nox",
    "std/toml.nox",
    "std/yaml.nox",
    "std/xml.nox",
    "std/test.nox",
    "std/task.nox",
    "std/http.nox",
    "std/url.nox",
    "std/random.nox",
];

fn extract_exported_fns(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("export fn ") {
            let end = rest
                .find('(')
                .into_iter()
                .chain(rest.find('<'))
                .min()
                .unwrap_or(rest.len());
            let name = rest[..end].trim();
            if !name.is_empty() {
                names.push(name.to_string());
            }
        }
    }
    names
}

fn doc_mentions_helper(doc: &str, namespace: &str, export: &str) -> bool {
    doc.contains(&format!("`{export}`"))
        || doc.contains(&format!("`{export}(",))
        || doc.contains(&format!("`{export}<"))
        || doc.contains(&format!("`{namespace}.{export}`"))
        || doc.contains(&format!("`{namespace}.{export}("))
        || doc.contains(&format!(" {export} /"))
        || doc.contains(&format!("/ {export} "))
        || doc.contains(&format!("/ {export} /"))
        || doc.contains(&format!("/ {export} |"))
}

fn assert_docs_cover_every_export(runtime_path: &str, index_path: &str, label: &str) {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let runtime_doc = std::fs::read_to_string(manifest.join(runtime_path))
        .unwrap_or_else(|_| panic!("{runtime_path} should exist"));
    let index_doc = std::fs::read_to_string(manifest.join(index_path))
        .unwrap_or_else(|_| panic!("{index_path} should exist"));

    let mut missing: Vec<String> = Vec::new();
    for specifier in STD_MODULE_SPECIFIERS {
        let source = std_module_source(specifier)
            .expect("known std specifier resolves")
            .expect("known std specifier returns source");
        let exports = extract_exported_fns(source);
        assert!(
            !exports.is_empty(),
            "expected at least one export from {specifier}"
        );
        let namespace = specifier_to_namespace(specifier);
        for export in exports {
            let mentioned = doc_mentions_helper(&runtime_doc, namespace, &export)
                || doc_mentions_helper(&index_doc, namespace, &export);
            if !mentioned {
                missing.push(format!("{specifier}::{export}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "the following std helpers are not mentioned in {label}:\n  - {}",
        missing.join("\n  - ")
    );
}

fn assert_stdlib_index_lists_every_module(index_path: &str, label: &str) {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let index_doc = std::fs::read_to_string(manifest.join(index_path))
        .unwrap_or_else(|_| panic!("{index_path} should exist"));
    let mut missing = Vec::new();
    for specifier in STD_MODULE_SPECIFIERS {
        if !index_doc.contains(&format!("`{specifier}`")) {
            missing.push(*specifier);
        }
    }
    assert!(
        missing.is_empty(),
        "the following std modules are missing from {label}:\n  - {}",
        missing.join("\n  - ")
    );
}

#[test]
fn stdlib_index_documents_every_exported_helper() {
    assert_stdlib_index_lists_every_module(
        "../../docs/zh_CN/stdlib-index.md",
        "docs/zh_CN/stdlib-index.md",
    );
    assert_docs_cover_every_export(
        "../../docs/zh_CN/runtime.md",
        "../../docs/zh_CN/stdlib-index.md",
        "docs/zh_CN/{runtime,stdlib-index}.md",
    );
}

#[test]
fn english_stdlib_index_documents_every_exported_helper() {
    assert_stdlib_index_lists_every_module(
        "../../docs/en/stdlib-index.md",
        "docs/en/stdlib-index.md",
    );
    assert_docs_cover_every_export(
        "../../docs/en/runtime.md",
        "../../docs/en/stdlib-index.md",
        "docs/en/{runtime,stdlib-index}.md",
    );
}

fn specifier_to_namespace(specifier: &str) -> &str {
    specifier
        .trim_start_matches("std/")
        .trim_end_matches(".nox")
}

fn env_test_lock() -> MutexGuard<'static, ()> {
    ENV_TEST_LOCK.lock().unwrap_or_else(|err| err.into_inner())
}

#[test]
fn runtime_exposes_minimal_stdlib() {
    let mut runtime = Runtime::new();
    let value = runtime.eval("sqrt(81.0);").unwrap();
    assert_eq!(value, Value::Float(9.0));
}

#[test]
fn math_intrinsics_cover_basic_operations_and_boundaries() {
    let mut runtime = Runtime::new();
    let value = runtime
        .eval(
            r#"
                let total: float = abs(-4.0)
                    + min(2.0, 3.0)
                    + max(2.0, 3.0)
                    + pow(2.0, 3.0)
                    + floor(1.9)
                    + ceil(1.1)
                    + round(1.6)
                    + log(e())
                    + log2(8.0)
                    + sin(0.0)
                    + cos(0.0)
                    + tan(0.0)
                    + pi();
                if (total > 30.14 && total < 30.15) {
                    "math-ok";
                } else {
                    "math-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("math-ok"));

    let sqrt_err = runtime.eval("sqrt(-1.0);").unwrap_err();
    assert!(sqrt_err
        .message
        .contains("sqrt expects a non-negative value"));

    let log_err = runtime.eval("log(0.0);").unwrap_err();
    assert!(log_err.message.contains("log expects a positive value"));
}

#[test]
fn time_intrinsics_format_parse_and_measure_unix_time() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/time.nox" as time;

                let start: int = time.now_unix_ms();
                let end: int = time.now_unix_ms();
                let elapsed: int = time.duration_ms(start, end);
                let text: str = time.format_unix(1704067205, "%Y-%m-%d %H:%M:%S");
                let parsed: result[int, str] = time.parse_unix(text, "%Y-%m-%d %H:%M:%S");
                match (parsed) {
                    ok(ts) => {
                        if (
                            time.now_unix() > 0 &&
                            elapsed >= 0 &&
                            text == "2024-01-01 00:00:05" &&
                            ts == 1704067205
                        ) {
                            "time-ok";
                        } else {
                            "time-bad";
                        }
                    }
                    err(message) => {
                        message;
                    }
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("time-ok"));
}

#[test]
fn time_intrinsics_return_parse_errors_as_result() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/time.nox" as time;

                let parsed: result[int, str] = time.parse_unix("2024-02-30", "%Y-%m-%d");
                match (parsed) {
                    ok(ts) => {
                        to_str_int(ts);
                    }
                    err(message) => {
                        message;
                    }
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("day is out of range for month"));

    let err = runtime
        .eval(
            r#"
                import "std/time.nox" as time;
                time.format_unix(0, "%Q");
                "#,
        )
        .unwrap_err();
    assert!(err.message.contains("unsupported time format token"));
}

#[test]
fn print_output_helpers_stringify_primitive_values() {
    let mut runtime = Runtime::new();
    let value = runtime
        .eval(
            r#"
                let int_text: str = to_str_int(42);
                let float_text: str = to_str_float(4.5);
                let bool_text: str = to_str_bool(true);
                let null_text: str = to_str_null(null);
                let same_text: str = to_str_str("nox");
                int_text + ":" + float_text + ":" + bool_text + ":" + null_text + ":" + same_text;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("42:4.5:true:null:nox"));
}

#[test]
fn string_stdlib_module_exposes_pure_helpers() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/string.nox" as string;

                let text: str = string.to_lower(string.replace(string.trim(" NOX_TYPED "), "_", ":"));
                let parts: [str] = string.split(text, ":");
                let prefix: str = string.substring(text, 0, 3);
                if (
                    len(parts) == 2 &&
                    parts[0] == "nox" &&
                    parts[1] == "typed" &&
                    prefix == "nox" &&
                    string.starts_with(text, "nox") &&
                    string.ends_with(text, "typed") &&
                    string.index_of(text, ":") == 3 &&
                    string.to_upper("ok") == "OK"
                ) {
                    text;
                } else {
                    "bad";
                }
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("nox:typed"));
}

#[test]
fn string_stdlib_second_round_helpers_cover_text_processing() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/string.nox" as string;

                let fields: [str] = string.split("alpha,beta,alpha", ",");
                let joined: str = string.join(fields, "|");
                let parsed_int: result[int, str] = string.parse_int(" 42 ");
                let parsed_float: result[float, str] = string.parse_float("2.5");
                let line_values: [str] = string.lines("first\nsecond\n");
                let int_ok: bool = false;
                let float_ok: bool = false;
                match (parsed_int) {
                    ok(value) => { int_ok = value == 42; }
                    err(message) => { int_ok = false; }
                }
                match (parsed_float) {
                    ok(value) => { float_ok = value == 2.5; }
                    err(message) => { float_ok = false; }
                }
                if (
                    joined == "alpha|beta|alpha" &&
                    string.contains(joined, "beta") &&
                    string.last_index_of(joined, "alpha") == 11 &&
                    string.repeat("ha", 3) == "hahaha" &&
                    string.pad_left("7", 3, "0") == "007" &&
                    string.pad_right("x", 3, ".") == "x.." &&
                    len(line_values) == 2 &&
                    line_values[0] == "first" &&
                    line_values[1] == "second" &&
                    int_ok &&
                    float_ok
                ) {
                    "strings-2-ok";
                } else {
                    "strings-2-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("strings-2-ok"));
}

#[test]
fn string_stdlib_reports_invalid_arguments_without_capability() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                import "std/string.nox" as string;
                string.substring("nox", 2, 2);
                "#,
        )
        .unwrap_err();
    assert!(err.message.contains("substring range is out of bounds"));
}

#[test]
fn json_parse_and_stringify_cover_basic_shapes() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;

                let parsed: result[json, str] = json.parse("{\"name\":\"nox\",\"ok\":true,\"count\":3,\"items\":[null,false,\"x\"]}");
                match (parsed) {
                    ok(value) => {
                        json.stringify(value);
                    }
                    err(message) => {
                        message;
                    }
                }
                "#,
            )
            .unwrap();
    assert_eq!(
        value,
        Value::string(r#"{"count":3,"items":[null,false,"x"],"name":"nox","ok":true}"#)
    );
}

#[test]
fn json_parse_and_stringify_returns_errors_for_malformed_input() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/json.nox" as json;

                let parsed: result[json, str] = json.parse("{\"name\":");
                match (parsed) {
                    ok(value) => {
                        json.stringify(value);
                    }
                    err(message) => {
                        message;
                    }
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("expected JSON value"));
}

#[test]
fn json_helpers_return_structured_results_for_arrays_and_objects() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;

                let parsed: result[json, str] = json.parse("{\"name\":\"nox\",\"items\":[1,2]}");
                match (parsed) {
                    ok(root) => {
                        let name_json: result[json, str] = json.object_get(root, "name");
                        let items_json: result[json, str] = json.object_get(root, "items");
                        match (name_json) {
                            ok(name) => {
                                match (items_json) {
                                    ok(items) => {
                                        let first: result[json, str] = json.array_get(items, 0);
                                        let length: result[int, str] = json.array_len(items);
                                        let has_name: result[bool, str] = json.object_has(root, "name");
                                        match (first) {
                                            ok(first_value) => {
                                                match (length) {
                                                    ok(count) => {
                                                        match (has_name) {
                                                            ok(found) => {
                                                                if (
                                                                    json.kind(root) == "object" &&
                                                                    json.kind(items) == "array" &&
                                                                    json.kind(name) == "string" &&
                                                                    json.stringify(first_value) == "1" &&
                                                                    count == 2 &&
                                                                    found
                                                                ) {
                                                                    "json-helper-ok";
                                                                } else {
                                                                    "json-helper-bad";
                                                                }
                                                            }
                                                            err(message) => { message; }
                                                        }
                                                    }
                                                    err(message) => { message; }
                                                }
                                            }
                                            err(message) => { message; }
                                        }
                                    }
                                    err(message) => { message; }
                                }
                            }
                            err(message) => { message; }
                        }
                    }
                    err(message) => { message; }
                }
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("json-helper-ok"));
}

#[test]
fn delimited_text_helpers_parse_and_format_rows() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/csv.nox" as csv;
                import "std/tsv.nox" as tsv;

                let parsed_csv: result[[str], str] = csv.parse_line("name,\"typed, runtime\",42");
                let parsed_tsv: result[[str], str] = tsv.parse_line("name\ttyped runtime\t42");
                match (parsed_csv) {
                    ok(csv_fields) => {
                        match (parsed_tsv) {
                            ok(tsv_fields) => {
                                let csv_row: str = csv.format_row(csv_fields);
                                let tsv_row: result[str, str] = tsv.format_row(tsv_fields);
                                match (tsv_row) {
                                    ok(tsv_text) => {
                                        if (
                                            len(csv_fields) == 3 &&
                                            csv_fields[1] == "typed, runtime" &&
                                            csv_row == "name,\"typed, runtime\",42" &&
                                            tsv_text == "name\ttyped runtime\t42"
                                        ) {
                                            "delimited-ok";
                                        } else {
                                            "delimited-bad";
                                        }
                                    }
                                    err(message) => { message; }
                                }
                            }
                            err(message) => { message; }
                        }
                    }
                    err(message) => { message; }
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("delimited-ok"));
}

#[test]
fn jsonl_stdlib_parses_and_formats_lines_with_line_errors() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;
                import "std/jsonl.nox" as jsonl;

                let parsed: result[[json], str] = jsonl.parse_lines("{\"id\":1}\n{\"id\":2}");
                match (parsed) {
                    ok(values) => {
                        let first_id_json: result[json, str] = json.object_get(values[0], "id");
                        let bad: result[[json], str] = jsonl.parse_lines("{\"ok\":true}\nnot-json");
                        match (first_id_json) {
                            ok(first_id) => {
                                match (bad) {
                                    ok(_) => {
                                        "jsonl-bad";
                                    }
                                    err(message) => {
                                        let first_id_int: result[int, str] = json.as_int(first_id);
                                        match (first_id_int) {
                                            ok(id) => {
                                                if (
                                                    id == 1 &&
                                                    len(values) == 2 &&
                                                    jsonl.format_lines(values) == "{\"id\":1}\n{\"id\":2}" &&
                                                    message == "line 2: expected JSON literal 'null'"
                                                ) {
                                                    "jsonl-ok";
                                                } else {
                                                    "jsonl-bad";
                                                }
                                            }
                                            err(message) => { message; }
                                        }
                                    }
                                }
                            }
                            err(message) => { message; }
                        }
                    }
                    err(message) => { message; }
                }
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("jsonl-ok"));
}

#[test]
fn delimited_text_helpers_parse_and_format_multiline_rows() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/csv.nox" as csv;
                import "std/tsv.nox" as tsv;

                let parsed_csv: result[[[str]], str] = csv.parse_rows("name,note\nnox,\"typed\nruntime\"");
                let parsed_tsv: result[[[str]], str] = tsv.parse_rows("name\tnote\nnox\ttyped runtime");
                let bad_csv: result[[[str]], str] = csv.parse_rows("ok\n\"bad");
                match (parsed_csv) {
                    ok(csv_rows) => {
                        match (parsed_tsv) {
                            ok(tsv_rows) => {
                                match (bad_csv) {
                                    ok(_) => {
                                        "rows-bad";
                                    }
                                    err(message) => {
                                        let tsv_text: result[str, str] = tsv.format_rows(tsv_rows);
                                        match (tsv_text) {
                                            ok(tsv_formatted) => {
                                                if (
                                                    len(csv_rows) == 2 &&
                                                    csv_rows[1][1] == "typed\nruntime" &&
                                                    csv.format_rows(csv_rows) == "name,note\nnox,\"typed\nruntime\"" &&
                                                    tsv_formatted == "name\tnote\nnox\ttyped runtime" &&
                                                    message == "line 2: unterminated quoted field"
                                                ) {
                                                    "rows-ok";
                                                } else {
                                                    "rows-bad";
                                                }
                                            }
                                            err(message) => { message; }
                                        }
                                    }
                                }
                            }
                            err(message) => { message; }
                        }
                    }
                    err(message) => { message; }
                }
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("rows-ok"));
}

#[test]
fn collection_stdlib_helpers_copy_and_sort_data() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/array.nox" as array;
                import "std/map.nox" as map;

                let numbers: [int] = [3, 1, 2];
                let pushed: [int] = array.push_copy(numbers, 4);
                let joined: [int] = array.concat(numbers, [5, 6]);
                let sliced: result[[int], str] = array.slice_copy(joined, 1, 3);
                let reversed: [int] = array.reverse_copy(numbers);
                let sorted_numbers: [int] = array.sort_copy_int(numbers);
                let sorted_names: [str] = array.sort_copy_str(["beta", "alpha"]);
                let merged: map[str, int] = map.merge({"a": 1, "b": 2}, {"b": 20, "c": 3});
                let removed: map[str, int] = map.remove_copy(merged, "b");
                let entries: [(str, int)] = map.entries(removed);
                let first_entry: (str, int) = entries[0];
                let (first_key, first_value) = first_entry;
                let fallback: int = map.get_or(removed, "b", 99);
                match (sliced) {
                    ok(slice) => {
                        if (
                            array.len(numbers) == 3 &&
                            !array.is_empty(numbers) &&
                            pushed[3] == 4 &&
                            slice[0] == 1 &&
                            slice[2] == 5 &&
                            reversed[0] == 2 &&
                            sorted_numbers[0] == 1 &&
                            sorted_names[0] == "alpha" &&
                            len(map.keys(removed)) == 2 &&
                            len(map.values(removed)) == 2 &&
                            len(entries) == 2 &&
                            first_key == "a" &&
                            first_value == 1 &&
                            fallback == 99
                        ) {
                            "collections-ok";
                        } else {
                            "collections-bad";
                        }
                    }
                    err(message) => { message; }
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("collections-ok"));
}

#[test]
fn array_stdlib_mutates_in_place_and_aliases_observe_changes() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/array.nox" as array;

                let xs: [int] = [10, 20, 30];
                array.append(xs, 40);
                let len_after_append: int = array.len(xs);

                let popped: option[int] = array.pop(xs);
                let len_after_pop: int = array.len(xs);
                let popped_value: int = -1;
                match (popped) {
                    some(v) => {
                        popped_value = v;
                    }
                    none => {}
                }

                let set_ok: result[null, str] = array.set(xs, 0, 99);
                let set_oob: result[null, str] = array.set(xs, 50, 0);
                let first: int = xs[0];

                let alias: [int] = xs;
                array.append(alias, 77);
                let len_via_alias: int = array.len(xs);
                let last_via_alias: int = xs[3];

                let ok_str: str = "fail";
                match (set_ok) {
                    ok(_) => {
                        ok_str = "ok";
                    }
                    err(message) => {
                        ok_str = message;
                    }
                }
                let err_str: str = "fail";
                match (set_oob) {
                    ok(_) => {
                        err_str = "ok";
                    }
                    err(message) => {
                        err_str = message;
                    }
                }

                if (
                    len_after_append == 4 &&
                    popped_value == 40 &&
                    len_after_pop == 3 &&
                    first == 99 &&
                    len_via_alias == 4 &&
                    last_via_alias == 77 &&
                    ok_str == "ok" &&
                    err_str != "ok"
                ) {
                    "array-mutation-ok";
                } else {
                    "array-mutation-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("array-mutation-ok"));
}

#[test]
fn array_index_assignment_syntax_updates_elements_in_place() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                let xs: [int] = [10, 20, 30];
                xs[1] = 99;
                let alias: [int] = xs;
                alias[0] = 7;
                if (xs[0] == 7 && xs[1] == 99 && alias[1] == 99) {
                    "array-index-assign-ok";
                } else {
                    "array-index-assign-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("array-index-assign-ok"));
}

#[test]
fn array_index_assignment_reports_out_of_range_at_runtime() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                let xs: [int] = [1, 2, 3];
                xs[10] = 99;
                "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "runtime.index-out-of-range");
    assert!(
        err.message.contains("out of bounds"),
        "expected out-of-range message, got: {}",
        err.message
    );
}

#[test]
fn array_append_respects_engine_array_length_cap() {
    let mut runtime = Runtime::new();
    runtime.engine_mut().set_max_array_length(Some(1));
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                import "std/array.nox" as array;
                let xs: [int] = [1];
                array.append(xs, 2);
                "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "runtime.array-length-cap");
}

#[test]
fn map_index_assignment_syntax_inserts_and_updates_entries() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                let m: map[str, int] = {"a": 1};
                m["a"] = 100;
                m["b"] = 2;
                let alias: map[str, int] = m;
                alias["c"] = 3;
                if (m["a"] == 100 && m["b"] == 2 && m["c"] == 3 && map_size(m) == 3) {
                    "map-index-assign-ok";
                } else {
                    "map-index-assign-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("map-index-assign-ok"));
}

#[test]
fn map_set_respects_engine_map_entry_cap() {
    let mut runtime = Runtime::new();
    runtime.engine_mut().set_max_map_entries(Some(1));
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                import "std/map.nox" as map;
                let values: map[str, int] = {"a": 1};
                map.set(values, "b", 2);
                "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "runtime.map-size-cap");
}

#[test]
fn term_stdlib_pad_column_and_color_no_color_environment() {
    // Ensure NO_COLOR makes style_color a noop.
    std::env::set_var("NO_COLOR", "1");
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/term.nox" as term;

                let padded: str = term.pad_column("hi", 5);
                let styled: str = term.style_color("hello", "red");
                let enabled: bool = term.color_enabled();
                if (padded == "hi   " && styled == "hello" && !enabled) {
                    "term-ok";
                } else {
                    "term-bad";
                }
                "#,
        )
        .unwrap();
    std::env::remove_var("NO_COLOR");
    assert_eq!(value, Value::string("term-ok"));
}

#[cfg(all(unix, target_os = "linux"))]
#[test]
fn term_disable_echo_reports_invalid_fd() {
    match disable_terminal_echo(-1) {
        Ok(_) => panic!("invalid fd unexpectedly disabled echo"),
        Err(err) => assert!(!err.is_empty()),
    }
}

#[test]
fn process_run_requires_capability() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                import "std/process.nox" as process;
                process.run("echo", ["hi"], "", 1000);
                "#,
        )
        .unwrap_err();
    assert!(
        err.message.contains("process run capability"),
        "expected process run capability diagnostic, got: {}",
        err.message
    );
}

#[test]
fn process_run_captures_stdout_when_allowed() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        process_run: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/process.nox" as process;
                let result_value: result[(int, str, str), str] = process.run("echo", ["hello"], "", 5000);
                let label: str = "fail";
                match (result_value) {
                    ok(parts) => {
                        let (code, out, _) = parts;
                        if (code == 0 && out == "hello\n") {
                            label = "process-ok";
                        } else {
                            label = out;
                        }
                    }
                    err(m) => { label = m; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("process-ok"));
}

#[test]
fn process_run_honours_allowlist() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        process_run: true,
        process_run_allowlist: vec!["true".to_string()],
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/process.nox" as process;
                let r: result[(int, str, str), str] = process.run("echo", ["hi"], "", 1000);
                let label: str = "fail";
                match (r) {
                    ok(_) => { label = "unexpected-ok"; }
                    err(m) => {
                        if (m == "process_run.allowlist-denied: program 'echo' is not in the process_run allowlist") {
                            label = "blocked";
                        } else {
                            label = m;
                        }
                    }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("blocked"));
}

#[test]
fn process_run_with_inherits_cwd_when_empty_and_uses_override_when_set() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        process_run: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let dir = std::env::temp_dir().join(format!("nox-process-run-with-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dir_str = dir.to_string_lossy().to_string();
    let value = runtime
            .eval(&format!(
                r#"
                import "std/process.nox" as process;
                let r: result[(int, str, str), str] = process.run_with("pwd", [], "", 5000, "{dir_str}", []);
                let label: str = "fail";
                match (r) {{
                    ok(parts) => {{
                        let (code, out, _) = parts;
                        if (code == 0) {{
                            label = out;
                        }} else {{
                            label = "non-zero";
                        }}
                    }}
                    err(m) => {{ label = m; }}
                }}
                label;
                "#,
                dir_str = dir_str,
            ))
            .unwrap();
    std::fs::remove_dir_all(&dir).ok();
    let resolved = std::fs::canonicalize(std::path::PathBuf::from(&dir_str))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(dir_str.clone());
    let output = match value {
        Value::String(s) => s.as_ref().to_string(),
        other => panic!("expected string output, got {other:?}"),
    };
    assert!(
        output.trim_end_matches('\n').ends_with(&resolved)
            || output.trim_end_matches('\n').ends_with(&dir_str),
        "expected pwd output to end with {dir_str:?} or {resolved:?}, got {output:?}"
    );
}

#[test]
fn process_run_with_applies_env_override() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        process_run: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/process.nox" as process;
                let pairs: [(str, str)] = [("NOX_TEST_VAR", "hello-from-nox")];
                let r: result[(int, str, str), str] = process.run_with("env", [], "", 5000, "", pairs);
                let label: str = "fail";
                match (r) {
                    ok(parts) => {
                        let (code, out, _) = parts;
                        if (code == 0) {
                            label = out;
                        } else {
                            label = "non-zero";
                        }
                    }
                    err(m) => { label = m; }
                }
                label;
                "#,
            )
            .unwrap();
    let output = match value {
        Value::String(s) => s.as_ref().to_string(),
        other => panic!("expected string output, got {other:?}"),
    };
    assert!(
        output.contains("NOX_TEST_VAR=hello-from-nox"),
        "expected env output to contain NOX_TEST_VAR=hello-from-nox, got {output:?}"
    );
}

#[test]
fn process_run_with_env_pairs_can_unset_and_set_empty_values() {
    unsafe {
        std::env::set_var("NOX_TEST_UNSET_VAR", "from-parent");
    }
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        process_run: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/process.nox" as process;
                import "std/string.nox" as string;

                let pairs: [(str, str)] = [("NOX_TEST_UNSET_VAR", "<unset>"), ("NOX_TEST_EMPTY_VAR", "")];
                let r: result[(int, str, str), str] = process.run_with("env", [], "", 5000, "", pairs);
                let label: str = "fail";
                match (r) {
                    ok(parts) => {
                        let (code, out, _) = parts;
                        if (
                            code == 0 &&
                            !string.contains(out, "NOX_TEST_UNSET_VAR=from-parent") &&
                            string.contains(out, "NOX_TEST_EMPTY_VAR=")
                        ) {
                            label = "env-unset-ok";
                        } else {
                            label = out;
                        }
                    }
                    err(m) => { label = m; }
                }
                label;
                "#,
            )
            .unwrap();
    unsafe {
        std::env::remove_var("NOX_TEST_UNSET_VAR");
    }
    assert_eq!(value, Value::string("env-unset-ok"));
}

#[test]
fn process_run_with_respects_allowlist() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        process_run: true,
        process_run_allowlist: vec!["true".to_string()],
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/process.nox" as process;
                let r: result[(int, str, str), str] = process.run_with("echo", [], "", 1000, "", []);
                let label: str = "fail";
                match (r) {
                    ok(_) => { label = "unexpected-ok"; }
                    err(m) => {
                        if (m == "process_run.allowlist-denied: program 'echo' is not in the process_run allowlist") {
                            label = "blocked";
                        } else {
                            label = m;
                        }
                    }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("blocked"));
}

#[test]
fn process_run_respects_concurrent_limit() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        process_run: true,
        process_run_max_concurrent: Some(0),
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/process.nox" as process;
                import "std/string.nox" as string;

                let r: result[(int, str, str), str] = process.run("true", [], "", 1000);
                match (r) {
                    ok(_) => {
                        "unexpected-ok";
                    }
                    err(m) => {
                        if (string.contains(m, "process_run.concurrent-limit")) {
                            "limit-ok";
                        } else {
                            m;
                        }
                    }
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("limit-ok"));
}

#[test]
fn process_run_releases_concurrent_slot_after_completion() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        process_run: true,
        process_run_max_concurrent: Some(1),
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/process.nox" as process;

                let first: result[(int, str, str), str] = process.run("true", [], "", 1000);
                let second: result[(int, str, str), str] = process.run("true", [], "", 1000);
                let first_ok: bool = false;
                let second_ok: bool = false;
                match (first) {
                    ok(parts) => {
                        let (code, _, _) = parts;
                        if (code == 0) {
                            first_ok = true;
                        }
                    }
                    err(_) => {}
                }
                match (second) {
                    ok(parts) => {
                        let (code, _, _) = parts;
                        if (code == 0) {
                            second_ok = true;
                        }
                    }
                    err(_) => {}
                }
                if (first_ok && second_ok) {
                    "released";
                } else {
                    "blocked";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("released"));
}

#[test]
fn time_stdlib_duration_conversions_are_consistent() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/time.nox" as time;

                let two_minutes_ms: int = time.from_minutes(2);
                let one_hour_ms: int = time.from_hours(1);
                if (
                    two_minutes_ms == 120000 &&
                    one_hour_ms == 3600000 &&
                    time.to_seconds(time.from_seconds(5)) == 5 &&
                    time.to_minutes(time.from_minutes(7)) == 7 &&
                    time.to_hours(time.from_hours(3)) == 3
                ) {
                    "duration-ok";
                } else {
                    "duration-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("duration-ok"));
}

#[test]
fn time_stdlib_iso8601_round_trips() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/time.nox" as time;

                let iso: str = time.iso8601_format(1704067200);
                let parsed: result[int, str] = time.iso8601_parse(iso);
                let label: str = "fail";
                match (parsed) {
                    ok(ts) => {
                        if (iso == "2024-01-01T00:00:00Z" && ts == 1704067200) {
                            label = "iso-ok";
                        } else {
                            label = "iso-bad";
                        }
                    }
                    err(_) => { label = "iso-err"; }
                }
                label;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("iso-ok"));
}

#[test]
fn time_stdlib_iso8601_rejects_non_utc_timezones() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/time.nox" as time;

                let parsed: result[int, str] = time.iso8601_parse("2024-01-01T00:00:00+08:00");
                let label: str = "fail";
                match (parsed) {
                    ok(_) => { label = "unexpected-ok"; }
                    err(_) => { label = "rejected"; }
                }
                label;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("rejected"));
}

#[test]
fn mock_clock_overrides_now_unix_when_set() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_clock_unix(Some(1704067200));
    let value = runtime
        .eval(
            r#"
                import "std/time.nox" as time;

                let unix: int = time.now_unix();
                let unix_ms: int = time.now_unix_ms();
                if (unix == 1704067200 && unix_ms == 1704067200000) {
                    "mock-clock-ok";
                } else {
                    "mock-clock-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("mock-clock-ok"));
}

#[test]
fn json_as_helpers_extract_scalar_values_and_reject_type_mismatches() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;
                let parsed: result[json, str] = json.parse("{\"score\": 42, \"name\": \"alice\", \"active\": true}");
                let label: str = "fail";
                match (parsed) {
                    ok(payload) => {
                        let score_field: result[json, str] = json.object_get(payload, "score");
                        let name_field: result[json, str] = json.object_get(payload, "name");
                        let active_field: result[json, str] = json.object_get(payload, "active");
                        let combined: str = "missing";
                        match (score_field) {
                            ok(score_value) => {
                                match (name_field) {
                                    ok(name_value) => {
                                        match (active_field) {
                                            ok(active_value) => {
                                                let score_outcome: result[int, str] = json.as_int(score_value);
                                                let name_outcome: result[str, str] = json.as_str(name_value);
                                                let active_outcome: result[bool, str] = json.as_bool(active_value);
                                                match (score_outcome) {
                                                    ok(score) => {
                                                        match (name_outcome) {
                                                            ok(name) => {
                                                                match (active_outcome) {
                                                                    ok(active) => {
                                                                        let int_on_string: result[int, str] = json.as_int(name_value);
                                                                        let mismatch: str = "ok-but-no-mismatch";
                                                                        match (int_on_string) {
                                                                            ok(_) => { mismatch = "should-have-failed"; }
                                                                            err(m) => { mismatch = m; }
                                                                        }
                                                                        if (score == 42 && name == "alice" && active && mismatch == "expected JSON number, got string") {
                                                                            combined = "json-as-ok";
                                                                        } else {
                                                                            combined = mismatch;
                                                                        }
                                                                    }
                                                                    err(m) => { combined = m; }
                                                                }
                                                            }
                                                            err(m) => { combined = m; }
                                                        }
                                                    }
                                                    err(m) => { combined = m; }
                                                }
                                            }
                                            err(m) => { combined = m; }
                                        }
                                    }
                                    err(m) => { combined = m; }
                                }
                            }
                            err(m) => { combined = m; }
                        }
                        label = combined;
                    }
                    err(m) => { label = m; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("json-as-ok"));
}

#[test]
fn json_to_json_serializes_record_to_object() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/json.nox" as json;

                record User {
                    name: str,
                    age: int,
                }
                let user: User = User { name: "alice", age: 30 };
                let payload: json = json.to_json(user);
                json.stringify(payload);
                "#,
        )
        .unwrap();
    assert_eq!(
        value,
        Value::string("{\"age\":30,\"name\":\"alice\"}".to_string())
    );
}

#[test]
fn json_to_json_serializes_enum_variants_with_payload() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/json.nox" as json;

                enum Event {
                    Click(int),
                    Quit,
                }
                let click: Event = Event.Click(42);
                let quit: Event = Event.Quit;
                let click_text: str = json.stringify(json.to_json(click));
                let quit_text: str = json.stringify(json.to_json(quit));
                click_text + "|" + quit_text;
                "#,
        )
        .unwrap();
    assert_eq!(
        value,
        Value::string("{\"_variant\":\"Click\",\"payload\":42}|\"Quit\"".to_string())
    );
}

#[test]
fn json_variant_helpers_extract_adjacent_enum_parts() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;
                import "std/result.nox" as result;
                import "std/string.nox" as string;

                let event: result[json, str] = json.parse("{\"_variant\":\"Click\",\"payload\":{\"x\":7}}");
                let empty: result[json, str] = json.parse("\"Quit\"");
                let label: str = "fail";
                match (event) {
                    ok(value) => {
                        match (empty) {
                            ok(no_payload) => {
                                let event_name: result[str, str] = json.variant_name(value);
                                let event_payload: result[json, str] = json.variant_payload(value);
                                let empty_name: result[str, str] = json.variant_name(no_payload);
                                let empty_payload: result[json, str] = json.variant_payload(no_payload);
                                match (event_name) {
                                    ok(name) => {
                                        match (event_payload) {
                                            ok(payload) => {
                                                match (empty_name) {
                                                    ok(no_payload_name) => {
                                                        match (empty_payload) {
                                                            ok(_) => { label = "unexpected-payload"; }
                                                            err(message) => {
                                                                if (
                                                                    name == "Click" &&
                                                                    json.stringify(payload) == "{\"x\":7}" &&
                                                                    no_payload_name == "Quit" &&
                                                                    string.contains(message, "no payload")
                                                                ) {
                                                                    label = "variant-ok";
                                                                } else {
                                                                    label = message;
                                                                }
                                                            }
                                                        }
                                                    }
                                                    err(message) => { label = message; }
                                                }
                                            }
                                            err(message) => { label = message; }
                                        }
                                    }
                                    err(message) => { label = message; }
                                }
                            }
                            err(message) => { label = message; }
                        }
                    }
                    err(message) => { label = message; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("variant-ok"));
}

#[test]
fn json_decode_record3_maps_validated_fields_with_path_errors() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;
                import "std/string.nox" as string;

                record Server {
                    port: int,
                    name: str,
                    enabled: bool,
                }

                fn build_server(port_value: json, name_value: json, enabled_value: json) -> result[Server, str] {
                    let port_result: result[int, str] = json.as_int(port_value);
                    let name_result: result[str, str] = json.as_str(name_value);
                    let enabled_result: result[bool, str] = json.as_bool(enabled_value);
                    match (port_result) {
                        ok(port) => {
                            match (name_result) {
                                ok(name) => {
                                    match (enabled_result) {
                                        ok(enabled) => {
                                            return ok(Server { port: port, name: name, enabled: enabled });
                                        }
                                        err(message) => { return err(message); }
                                    }
                                }
                                err(message) => { return err(message); }
                            }
                        }
                        err(message) => { return err(message); }
                    }
                }

                let parsed: result[json, str] = json.parse("{\"config\":{\"server\":{\"port\":8080,\"name\":\"api\",\"enabled\":true}}}");
                let label: str = "fail";
                match (parsed) {
                    ok(root) => {
                        let decoded: result[Server, str] = json.decode_record3(root, "config.server", "port", "number", "name", "string", "enabled", "bool", build_server);
                        let missing: result[Server, str] = json.decode_record3(root, "config.server", "port", "number", "name", "string", "tls", "bool", build_server);
                        match (decoded) {
                            ok(server) => {
                                match (missing) {
                                    ok(_) => { label = "unexpected-ok"; }
                                    err(message) => {
                                        if (server.port == 8080 && server.name == "api" && server.enabled && string.contains(message, "config.server.tls")) {
                                            label = "record-decode-ok";
                                        } else {
                                            label = message;
                                        }
                                    }
                                }
                            }
                            err(message) => { label = message; }
                        }
                    }
                    err(message) => { label = message; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("record-decode-ok"));
}

#[test]
fn json_decode_adjacent_enum3_dispatches_variants() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;
                import "std/string.nox" as string;

                fn build_click(payload: json) -> result[str, str] {
                    let parsed: result[int, str] = json.as_int(payload);
                    match (parsed) {
                        ok(value) => { return ok("click:${value}"); }
                        err(message) => { return err(message); }
                    }
                }

                fn build_quit(_payload: json) -> result[str, str] {
                    return ok("quit");
                }

                fn build_rename(payload: json) -> result[str, str] {
                    let parsed: result[str, str] = json.as_str(payload);
                    match (parsed) {
                        ok(value) => { return ok("rename:" + value); }
                        err(message) => { return err(message); }
                    }
                }

                let click_json: result[json, str] = json.parse("{\"_variant\":\"Click\",\"payload\":7}");
                let quit_json: result[json, str] = json.parse("\"Quit\"");
                let unknown_json: result[json, str] = json.parse("\"Pause\"");
                let label: str = "fail";
                match (click_json) {
                    ok(click_value) => {
                        match (quit_json) {
                            ok(quit_value) => {
                                match (unknown_json) {
                                    ok(unknown_value) => {
                                        let click: result[str, str] = json.decode_adjacent_enum3(click_value, "action", "Click", build_click, "Quit", build_quit, "Rename", build_rename);
                                        let quit: result[str, str] = json.decode_adjacent_enum3(quit_value, "action", "Click", build_click, "Quit", build_quit, "Rename", build_rename);
                                        let unknown: result[str, str] = json.decode_adjacent_enum3(unknown_value, "action", "Click", build_click, "Quit", build_quit, "Rename", build_rename);
                                        let click_text: str = "";
                                        let quit_text: str = "";
                                        let unknown_message: str = "";
                                        let unknown_failed: bool = false;
                                        match (click) {
                                            ok(value) => { click_text = value; }
                                            err(message) => { label = message; }
                                        }
                                        match (quit) {
                                            ok(value) => { quit_text = value; }
                                            err(message) => { label = message; }
                                        }
                                        match (unknown) {
                                            ok(_) => { label = "unexpected-ok"; }
                                            err(message) => {
                                                unknown_failed = true;
                                                unknown_message = message;
                                            }
                                        }
                                        if (
                                            unknown_failed &&
                                            click_text == "click:7" &&
                                            quit_text == "quit" &&
                                            string.contains(unknown_message, "action: unknown variant Pause")
                                        ) {
                                            label = "enum-decode-ok";
                                        }
                                    }
                                    err(message) => { label = message; }
                                }
                            }
                            err(message) => { label = message; }
                        }
                    }
                    err(message) => { label = message; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("enum-decode-ok"));
}

#[test]
fn json_from_json_decodes_record_from_expected_result_type() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;
                import "std/string.nox" as string;

                record Server {
                    port: int,
                    name: str,
                    enabled: bool,
                }

                let parsed: result[json, str] = json.parse("{\"port\":8080,\"name\":\"api\",\"enabled\":true}");
                let bad: result[json, str] = json.parse("{\"port\":\"bad\",\"name\":\"api\",\"enabled\":true}");
                let label: str = "fail";
                match (parsed) {
                    ok(value) => {
                        match (bad) {
                            ok(bad_value) => {
                                let decoded: result[Server, str] = json.from_json(value);
                                let rejected: result[Server, str] = json.from_json(bad_value);
                                match (decoded) {
                                    ok(server) => {
                                        match (rejected) {
                                            ok(_) => { label = "unexpected-ok"; }
                                            err(message) => {
                                                if (
                                                    server.port == 8080 &&
                                                    server.name == "api" &&
                                                    server.enabled &&
                                                    string.contains(message, "port") &&
                                                    string.contains(message, "expected number")
                                                ) {
                                                    label = "from-json-record-ok";
                                                } else {
                                                    label = message;
                                                }
                                            }
                                        }
                                    }
                                    err(message) => { label = message; }
                                }
                            }
                            err(message) => { label = message; }
                        }
                    }
                    err(message) => { label = message; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("from-json-record-ok"));
}

#[test]
fn json_from_json_decodes_adjacent_enum_from_expected_result_type() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;
                import "std/string.nox" as string;

                enum Action {
                    Click(int),
                    Quit,
                    Rename(str),
                }

                let click_json: result[json, str] = json.parse("{\"_variant\":\"Click\",\"payload\":7}");
                let quit_json: result[json, str] = json.parse("\"Quit\"");
                let unknown_json: result[json, str] = json.parse("\"Pause\"");
                let label: str = "fail";
                match (click_json) {
                    ok(click_value) => {
                        match (quit_json) {
                            ok(quit_value) => {
                                match (unknown_json) {
                                    ok(unknown_value) => {
                                        let click: result[Action, str] = json.from_json(click_value);
                                        let quit: result[Action, str] = json.from_json(quit_value);
                                        let unknown: result[Action, str] = json.from_json(unknown_value);
                                        let click_text: str = json.stringify(json.to_json(click));
                                        let quit_text: str = json.stringify(json.to_json(quit));
                                        let unknown_text: str = json.stringify(json.to_json(unknown));
                                        if (
                                            click_text == "{\"_variant\":\"ok\",\"payload\":{\"_variant\":\"Click\",\"payload\":7}}" &&
                                            quit_text == "{\"_variant\":\"ok\",\"payload\":\"Quit\"}" &&
                                            string.contains(unknown_text, "\"_variant\":\"err\"") &&
                                            string.contains(unknown_text, "unknown variant Pause")
                                        ) {
                                            label = "from-json-enum-ok";
                                        }
                                    }
                                    err(message) => { label = message; }
                                }
                            }
                            err(message) => { label = message; }
                        }
                    }
                    err(message) => { label = message; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("from-json-enum-ok"));
}

#[test]
fn json_to_json_serializes_collection_and_option() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/json.nox" as json;

                let items: [int] = [1, 2, 3];
                let maybe: option[int] = some(7);
                let pair: (str, int) = ("alpha", 99);
                let serialized: str = json.stringify(json.to_json(items)) + "|" +
                    json.stringify(json.to_json(maybe)) + "|" +
                    json.stringify(json.to_json(pair));
                serialized;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("[1,2,3]|7|[\"alpha\",99]".to_string()));
}

#[test]
fn term_progress_renders_ascii_bar_with_percent() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/term.nox" as term;
                term.progress(5, 10, 10);
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("[#####-----] 5/10 (50%)"));
}

#[test]
fn term_progress_clamps_current_to_total() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/term.nox" as term;
                term.progress(20, 10, 4);
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("[####] 10/10 (100%)"));
}

#[test]
fn term_progress_rejects_negative_width() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                import "std/term.nox" as term;
                term.progress(1, 10, -1);
                "#,
        )
        .unwrap_err();
    assert!(err.message.contains("width must be non-negative"));
}

#[test]
fn time_date_arithmetic_helpers_compute_calendar_fields() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/time.nox" as time;
                let epoch: int = 0;
                let y: int = time.year_of(epoch);
                let m: int = time.month_of(epoch);
                let d: int = time.day_of(epoch);
                let wd: int = time.weekday_of(epoch);
                let added_day: int = time.add_days(epoch, 1);
                let added_year: int = time.add_months(epoch, 12);
                let label: str = "calendar-bad";
                if (
                    y == 1970 && m == 1 && d == 1 && wd == 3 &&
                    time.day_of(added_day) == 2 &&
                    time.year_of(added_year) == 1971
                ) {
                    label = "calendar-ok";
                }
                label;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("calendar-ok"));
}

#[test]
fn time_add_months_clamps_day_to_month_length() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/time.nox" as time;
                let jan31: result[int, str] = time.iso8601_parse("1970-01-31T00:00:00Z");
                let label: str = "clamp-bad";
                match (jan31) {
                    ok(ts) => {
                        let feb: int = time.add_months(ts, 1);
                        if (time.year_of(feb) == 1970 && time.month_of(feb) == 2 && time.day_of(feb) == 28) {
                            label = "clamp-ok";
                        }
                    }
                    err(m) => { label = m; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("clamp-ok"));
}

#[test]
fn random_next_int_is_deterministic_for_same_seed() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let first = runtime
        .eval(
            r#"
                import "std/random.nox" as random;
                let result_pair: (int, int) = random.next_int(42, 0, 100);
                let (_, value) = result_pair;
                value;
                "#,
        )
        .unwrap();

    let mut runtime_b = Runtime::new();
    runtime_b.set_import_base(std::env::temp_dir(), Vec::new());
    let second = runtime_b
        .eval(
            r#"
                import "std/random.nox" as random;
                let result_pair: (int, int) = random.next_int(42, 0, 100);
                let (_, value) = result_pair;
                value;
                "#,
        )
        .unwrap();
    assert_eq!(first, second);
    if let Value::Int(v) = first {
        assert!(
            (0..=100).contains(&v),
            "expected next_int result in [0, 100], got {v}"
        );
    } else {
        panic!("expected Int, got {first:?}");
    }
}

#[test]
fn random_next_int_rejects_inverted_range() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                import "std/random.nox" as random;
                random.next_int(1, 10, 0);
                "#,
        )
        .unwrap_err();
    assert!(
        err.message.contains("min <= max"),
        "expected min <= max diagnostic, got: {}",
        err.message
    );
}

#[test]
fn random_next_bool_produces_deterministic_stream() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/random.nox" as random;
                let first: (int, bool) = random.next_bool(99);
                let (seed2, _) = first;
                let second: (int, bool) = random.next_bool(seed2);
                let (_, b2) = second;
                let (_, b1) = first;
                let label: str = "stream-bad";
                if (b1 == b1 && b2 == b2) {
                    label = "stream-ok";
                }
                label;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("stream-ok"));
}

#[test]
fn mock_env_overrides_env_get_try_get_and_list() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        environment: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let mut mocks = BTreeMap::new();
    mocks.insert("NOX_TEST_KEY".to_string(), "mock-value".to_string());
    mocks.insert("OTHER".to_string(), "second".to_string());
    runtime.set_mock_env(Some(mocks));

    let value = runtime
            .eval(
                r#"
                import "std/env.nox" as env;
                let direct: str = env.get("NOX_TEST_KEY");
                let listed: map[str, str] = env.list();
                let absent: option[str] = env.try_get("MISSING_KEY");
                let absent_label: str = "missing-bad";
                match (absent) {
                    none => { absent_label = "missing-ok"; }
                    some(_) => { absent_label = "missing-bad"; }
                }
                if (direct == "mock-value" && map_has(listed, "OTHER") && absent_label == "missing-ok") {
                    "mock-env-ok";
                } else {
                    direct;
                }
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("mock-env-ok"));
}

#[test]
fn mock_env_clears_back_to_real_environment_when_unset() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        environment: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let mut mocks = BTreeMap::new();
    mocks.insert("NOX_MOCK_KEY".to_string(), "mock".to_string());
    runtime.set_mock_env(Some(mocks));
    runtime.set_mock_env(None);

    let probe_key = format!("NOX_MOCK_REAL_PROBE_{}_{}", std::process::id(), line!());
    std::env::set_var(&probe_key, "real-value");
    let value = runtime
        .eval(&format!(
            r#"
                import "std/env.nox" as env;
                env.get("{probe_key}");
                "#,
            probe_key = probe_key,
        ))
        .unwrap();
    std::env::remove_var(&probe_key);
    assert_eq!(value, Value::string("real-value"));
}

#[test]
fn mock_filesystem_drives_read_helpers_after_permission_checks() {
    let dir = std::env::temp_dir().join(format!("nox-mock-fs-{}-{}", std::process::id(), line!()));
    let allowed = dir.join("allowed");
    fs::create_dir_all(&allowed).unwrap();
    let text_path = allowed.join("note.txt");
    let binary_path = allowed.join("raw.bin");
    let nested = allowed.join("nested");
    let nested_file = nested.join("deep.txt");

    let mut runtime =
        Runtime::with_permissions(RuntimePermissions::none().allow_filesystem_read_under(&allowed));
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_filesystem(Some(
        MockFilesystem::new()
            .with_text_file(&text_path, "mock-text")
            .with_binary_file(&binary_path, vec![65, 66, 255])
            .with_text_file(&nested_file, "deep"),
    ));

    let value = runtime
        .eval(&format!(
            r#"
                import "std/fs.nox" as fs;

                let text_path: str = "{}";
                let binary_path: str = "{}";
                let root: str = "{}";
                let nested: str = "{}";

                let loaded: result[str, str] = fs.try_read_text(text_path);
                let read_ok: bool = false;
                match (loaded) {{
                    ok(contents) => {{ read_ok = contents == "mock-text"; }}
                    err(_) => {{ read_ok = false; }}
                }}

                let bytes_ok: bool = false;
                let binary: result[[int], str] = fs.read_binary(binary_path);
                match (binary) {{
                    ok(bytes) => {{
                        bytes_ok = len(bytes) == 3 && bytes[0] == 65 && bytes[2] == 255;
                    }}
                    err(_) => {{ bytes_ok = false; }}
                }}

                let listed_ok: bool = false;
                let listed: result[[str], str] = fs.list_dir(root);
                match (listed) {{
                    ok(entries) => {{
                        listed_ok = len(entries) == 3 &&
                            entries[0] == "nested" &&
                            entries[1] == "note.txt" &&
                            entries[2] == "raw.bin";
                    }}
                    err(_) => {{ listed_ok = false; }}
                }}

                let nested_list_ok: bool = false;
                let nested_list: result[[str], str] = fs.list_dir(nested);
                match (nested_list) {{
                    ok(entries) => {{
                        nested_list_ok = len(entries) == 1 && entries[0] == "deep.txt";
                    }}
                    err(_) => {{ nested_list_ok = false; }}
                }}

                let canonical_ok: bool = false;
                let canonical: result[str, str] = fs.canonicalize(text_path);
                match (canonical) {{
                    ok(resolved) => {{ canonical_ok = resolved == text_path; }}
                    err(_) => {{ canonical_ok = false; }}
                }}

                if (
                    fs.read_text(text_path) == "mock-text" &&
                    read_ok &&
                    fs.exists(text_path) &&
                    fs.is_file(text_path) &&
                    fs.is_dir(root) &&
                    fs.is_dir(nested) &&
                    listed_ok &&
                    nested_list_ok &&
                    bytes_ok &&
                    canonical_ok
                ) {{
                    "mock-fs-ok";
                }} else {{
                    "mock-fs-bad";
                }}
                "#,
            text_path.display(),
            binary_path.display(),
            allowed.display(),
            nested.display(),
        ))
        .unwrap();

    fs::remove_dir_all(&dir).ok();
    assert_eq!(value, Value::string("mock-fs-ok"));
}

#[test]
fn mock_filesystem_does_not_grant_filesystem_capability() {
    let path = std::env::temp_dir().join(format!(
        "nox-mock-fs-deny-{}-{}.txt",
        std::process::id(),
        line!()
    ));
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_filesystem(Some(
        MockFilesystem::new().with_text_file(&path, "mock-text"),
    ));

    let err = runtime
        .eval(&format!(
            r#"import "std/fs.nox" as fs; fs.read_text("{}");"#,
            path.display()
        ))
        .unwrap_err();

    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("filesystem capability"));
}

#[test]
fn async_std_fs_wrappers_use_existing_filesystem_boundary() {
    let dir = std::env::temp_dir().join(format!("nox-async-fs-{}-{}", std::process::id(), line!()));
    fs::create_dir_all(&dir).unwrap();
    let text_path = dir.join("note.txt");
    let binary_path = dir.join("raw.bin");
    let out_path = dir.join("out.bin");

    let permissions = RuntimePermissions::none()
        .allow_filesystem_read_under(&dir)
        .allow_filesystem_write_under(&dir);
    let mut runtime = Runtime::with_permissions(permissions);
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_filesystem(Some(
        MockFilesystem::new()
            .with_text_file(&text_path, "mock-text")
            .with_binary_file(&binary_path, vec![65, 66, 255]),
    ));

    let value = runtime
        .eval(&format!(
            r#"
                import "std/fs.nox" as fs;

                let read_ok: bool = false;
                let binary_ok: bool = false;
                let write_ok: bool = false;
                let canonical_ok: bool = false;

                async fn probe() -> null {{
                    let loaded: result[str, str] = await fs.try_read_text_async("{}");
                    match (loaded) {{
                        ok(contents) => {{ read_ok = contents == "mock-text"; }}
                        err(_) => {{ read_ok = false; }}
                    }}

                    let binary: result[[int], str] = await fs.read_binary_async("{}");
                    match (binary) {{
                        ok(bytes) => {{
                            binary_ok = len(bytes) == 3 && bytes[0] == 65 && bytes[2] == 255;
                        }}
                        err(_) => {{ binary_ok = false; }}
                    }}

                    let wrote: result[null, str] = await fs.write_binary_async("{}", [1, 2, 3]);
                    match (wrote) {{
                        ok(_) => {{ write_ok = true; }}
                        err(_) => {{ write_ok = false; }}
                    }}

                    let canonical: result[str, str] = await fs.canonicalize_async("{}");
                    match (canonical) {{
                        ok(path) => {{ canonical_ok = path == "{}"; }}
                        err(_) => {{ canonical_ok = false; }}
                    }}
                    return null;
                }}

                let task: task[null] = probe();
                if (read_ok && binary_ok && write_ok && canonical_ok) {{
                    "async-fs-ok";
                }} else {{
                    "async-fs-bad";
                }}
                "#,
            text_path.display(),
            binary_path.display(),
            out_path.display(),
            text_path.display(),
            text_path.display(),
        ))
        .unwrap();

    assert_eq!(value, Value::string("async-fs-ok"));
    assert!(!out_path.exists());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn async_std_fs_wrappers_require_filesystem_capability() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                import "std/fs.nox" as fs;

                async fn probe() -> null {
                    let loaded: result[str, str] = await fs.try_read_text_async("none.txt");
                    return null;
                }

                let task: task[null] = probe();
                "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("filesystem capability"));
}

#[test]
fn mock_filesystem_does_not_bypass_read_allowlist() {
    let dir = std::env::temp_dir().join(format!(
        "nox-mock-fs-allow-{}-{}",
        std::process::id(),
        line!()
    ));
    let allowed = dir.join("allowed");
    fs::create_dir_all(&allowed).unwrap();
    let outside = dir.join("outside.txt");

    let mut runtime =
        Runtime::with_permissions(RuntimePermissions::none().allow_filesystem_read_under(&allowed));
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_filesystem(Some(
        MockFilesystem::new().with_text_file(&outside, "outside"),
    ));

    let err = runtime
        .eval(&format!(
            r#"import "std/fs.nox" as fs; fs.read_text("{}");"#,
            outside.display()
        ))
        .unwrap_err();

    fs::remove_dir_all(&dir).ok();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("filesystem read permission denied"));
}

#[test]
fn mock_filesystem_missing_file_does_not_fall_back_to_real_filesystem() {
    let dir = std::env::temp_dir().join(format!(
        "nox-mock-fs-missing-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let real_file = dir.join("real.txt");
    fs::write(&real_file, "real").unwrap();

    let mut runtime =
        Runtime::with_permissions(RuntimePermissions::none().allow_filesystem_read_under(&dir));
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_filesystem(Some(MockFilesystem::new()));

    let value = runtime
        .eval(&format!(
            r#"
                import "std/fs.nox" as fs;
                let loaded: result[str, str] = fs.try_read_text("{}");
                match (loaded) {{
                    ok(contents) => {{ contents; }}
                    err(message) => {{ message; }}
                }}
                "#,
            real_file.display()
        ))
        .unwrap();

    fs::remove_dir_all(&dir).ok();
    let Value::String(message) = value else {
        panic!("expected mock filesystem error string");
    };
    assert!(message.contains("not found in mock filesystem"));
}

#[test]
fn mock_filesystem_captures_text_and_binary_writes() {
    let dir = std::env::temp_dir().join(format!(
        "nox-mock-fs-write-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let text_path = dir.join("out.txt");
    let binary_path = dir.join("out.bin");

    let permissions = RuntimePermissions::none()
        .allow_filesystem_read_under(&dir)
        .allow_filesystem_write_under(&dir);
    let mut runtime = Runtime::with_permissions(permissions);
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_filesystem(Some(MockFilesystem::new()));

    let value = runtime
        .eval(&format!(
            r#"
                import "std/fs.nox" as fs;

                write_text("{}", "mock text");
                let write_binary_result: result[null, str] = fs.write_binary("{}", [7, 8, 255]);
                let binary_written: bool = false;
                match (write_binary_result) {{
                    ok(_) => {{ binary_written = true; }}
                    err(_) => {{ binary_written = false; }}
                }}

                let binary_read_ok: bool = false;
                let binary_read: result[[int], str] = fs.read_binary("{}");
                match (binary_read) {{
                    ok(bytes) => {{
                        binary_read_ok = len(bytes) == 3 && bytes[0] == 7 && bytes[2] == 255;
                    }}
                    err(_) => {{ binary_read_ok = false; }}
                }}

                if (fs.read_text("{}") == "mock text" && binary_written && binary_read_ok) {{
                    "mock-write-ok";
                }} else {{
                    "mock-write-bad";
                }}
                "#,
            text_path.display(),
            binary_path.display(),
            binary_path.display(),
            text_path.display(),
        ))
        .unwrap();

    assert_eq!(value, Value::string("mock-write-ok"));
    assert!(!text_path.exists());
    assert!(!binary_path.exists());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn mock_filesystem_write_does_not_grant_write_capability() {
    let dir = std::env::temp_dir().join(format!(
        "nox-mock-fs-write-deny-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("denied.txt");

    let mut runtime =
        Runtime::with_permissions(RuntimePermissions::none().allow_filesystem_read_under(&dir));
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_filesystem(Some(MockFilesystem::new()));

    let err = runtime
        .eval(&format!(r#"write_text("{}", "denied");"#, target.display()))
        .unwrap_err();

    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("filesystem write capability"));
    assert!(!target.exists());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn mock_filesystem_write_does_not_bypass_write_allowlist() {
    let dir = std::env::temp_dir().join(format!(
        "nox-mock-fs-write-allow-{}-{}",
        std::process::id(),
        line!()
    ));
    let allowed = dir.join("allowed");
    fs::create_dir_all(&allowed).unwrap();
    let outside = dir.join("outside.txt");

    let permissions = RuntimePermissions::none()
        .allow_filesystem_read_under(&allowed)
        .allow_filesystem_write_under(&allowed);
    let mut runtime = Runtime::with_permissions(permissions);
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_filesystem(Some(MockFilesystem::new()));

    let err = runtime
        .eval(&format!(
            r#"write_text("{}", "outside");"#,
            outside.display()
        ))
        .unwrap_err();

    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("filesystem write permission denied"));
    assert!(!outside.exists());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn mock_network_drives_tcp_and_http_after_permission_checks() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        network: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_network(Some(
        MockNetwork::new()
            .with_tcp_connect("example.test", 8080, true)
            .with_http_text_response("GET", "http://example.test/data", 203, "mock-body")
            .with_http_binary_response("POST", "http://example.test/upload", 204, vec![1, 2, 255]),
    ));

    let value = runtime
            .eval(
                r#"
                import "std/http.nox" as http;

                let get_ok: bool = false;
                let get_response: result[(int, str), str] = http.get("http://example.test/data", 1);
                match (get_response) {
                    ok(response) => {
                        let (status, body) = response;
                        get_ok = status == 203 && body == "mock-body";
                    }
                    err(_) => { get_ok = false; }
                }

                let post_ok: bool = false;
                let post_response: result[(int, [int]), str] = http.post_binary("http://example.test/upload", [9, 8], 1);
                match (post_response) {
                    ok(response) => {
                        let (status, body) = response;
                        post_ok = status == 204 && len(body) == 3 && body[2] == 255;
                    }
                    err(_) => { post_ok = false; }
                }

                if (
                    tcp_connect("example.test", 8080) &&
                    !tcp_connect("example.test", 8081) &&
                    get_ok &&
                    post_ok
                ) {
                    "mock-network-ok";
                } else {
                    "mock-network-bad";
                }
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("mock-network-ok"));
}

#[test]
fn async_std_http_wrappers_use_existing_network_boundary() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        network: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_network(Some(
        MockNetwork::new()
            .with_http_text_response("GET", "http://example.test/data", 203, "mock-body")
            .with_http_binary_response("POST", "http://example.test/upload", 204, vec![1, 2, 255]),
    ));

    let value = runtime
            .eval(
                r#"
                import "std/http.nox" as http;

                let get_ok: bool = false;
                let post_ok: bool = false;

                async fn probe() -> null {
                    let get_response: result[(int, str), str] = await http.get_async("http://example.test/data", 1);
                    match (get_response) {
                        ok(response) => {
                            let (status, body) = response;
                            get_ok = status == 203 && body == "mock-body";
                        }
                        err(_) => { get_ok = false; }
                    }

                    let post_response: result[(int, [int]), str] = await http.post_binary_async("http://example.test/upload", [9, 8], 1);
                    match (post_response) {
                        ok(response) => {
                            let (status, body) = response;
                            post_ok = status == 204 && len(body) == 3 && body[2] == 255;
                        }
                        err(_) => { post_ok = false; }
                    }
                    return null;
                }

                let task: task[null] = probe();
                if (get_ok && post_ok) {
                    "async-http-ok";
                } else {
                    "async-http-bad";
                }
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("async-http-ok"));
}

#[test]
fn mock_network_does_not_grant_network_capability() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_network(Some(MockNetwork::new().with_tcp_connect(
        "example.test",
        80,
        true,
    )));

    let err = runtime
        .eval(r#"tcp_connect("example.test", 80);"#)
        .unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("network capability"));
}

#[test]
fn async_std_http_wrappers_require_network_capability() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
            .eval(
                r#"
                import "std/http.nox" as http;

                async fn probe() -> null {
                    let response: result[(int, str), str] = await http.get_async("http://example.test/data", 1);
                    return null;
                }

                let task: task[null] = probe();
                "#,
            )
            .unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("network capability"));
}

#[test]
fn async_await_can_interleave_host_callback_and_runtime_task() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    runtime
        .engine_mut()
        .register_host_function(HostFunctionBuilder::new("host_value", Type::Int), |_| {
            Ok(Value::Int(21))
        })
        .unwrap();

    let value = runtime
        .eval(
            r#"
                let passed: bool = false;

                async fn probe() -> null {
                    let before: int = host_value();
                    await task_sleep(0);
                    let after: int = host_value();
                    passed = before == 21 && after == 21;
                    return null;
                }

                let task: task[null] = probe();
                if (passed) {
                    "async-host-ok";
                } else {
                    "async-host-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("async-host-ok"));
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn mock_network_missing_http_response_does_not_fall_back_to_real_network() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        network: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_network(Some(MockNetwork::new()));

    let value = runtime
        .eval(
            r#"
                import "std/http.nox" as http;
                let response: result[(int, str), str] = http.get("http://127.0.0.1:9/missing", 1);
                match (response) {
                    ok(_) => { "unexpected-ok"; }
                    err(message) => { message; }
                }
                "#,
        )
        .unwrap();

    let Value::String(message) = value else {
        panic!("expected mock network error string");
    };
    assert!(message.contains("mock network has no GET response"));
}

#[test]
fn mock_clock_clears_back_to_real_clock_when_unset() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_clock_unix(Some(42));
    runtime.set_mock_clock_unix(None);
    let value = runtime
        .eval(
            r#"
                import "std/time.nox" as time;

                let unix: int = time.now_unix();
                if (unix > 42) {
                    "real-clock-ok";
                } else {
                    "still-mocked";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("real-clock-ok"));
}

#[test]
fn json_schema_require_field_resolves_paths_and_validates_kind() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;
                import "std/string.nox" as string;

                let doc: result[json, str] = json.parse("{\"server\": {\"port\": 8080}, \"tags\": [\"a\", \"b\"]}");
                let label: str = "fail";
                match (doc) {
                    ok(value) => {
                        let port: result[json, str] = json.require_field(value, "server.port", "number");
                        let tag: result[json, str] = json.require_field(value, "tags[1]", "string");
                        let wrong: result[json, str] = json.require_field(value, "server.port", "string");
                        let port_ok: bool = false;
                        let tag_ok: bool = false;
                        let wrong_msg_ok: bool = false;
                        match (port) { ok(_) => { port_ok = true; } err(_) => {} }
                        match (tag) { ok(_) => { tag_ok = true; } err(_) => {} }
                        match (wrong) {
                            ok(_) => {}
                            err(m) => {
                                if (string.contains(m, "server.port") && string.contains(m, "expected string")) {
                                    wrong_msg_ok = true;
                                }
                            }
                        }
                        if (port_ok && tag_ok && wrong_msg_ok) {
                            label = "json-schema-ok";
                        } else {
                            label = "json-schema-bad";
                        }
                    }
                    err(_) => { label = "json-parse-err"; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("json-schema-ok"));
}

#[test]
fn json_validate_schema_reports_missing_required_fields() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;
                import "std/string.nox" as string;

                let doc: result[json, str] = json.parse("{\"present\": 1}");
                let label: str = "fail";
                match (doc) {
                    ok(value) => {
                        let v: result[null, str] = json.validate_schema(value, ["present", "missing"]);
                        match (v) {
                            ok(_) => { label = "unexpected-ok"; }
                            err(m) => {
                                if (string.contains(m, "missing required field(s): missing")) {
                                    label = "missing-detected";
                                } else {
                                    label = m;
                                }
                            }
                        }
                    }
                    err(_) => { label = "parse-err"; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("missing-detected"));
}

#[test]
fn json_validate_object_reports_missing_and_unknown_fields() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;
                import "std/string.nox" as string;

                let doc: result[json, str] = json.parse("{\"name\":\"nox\",\"extra\":true}");
                let label: str = "fail";
                match (doc) {
                    ok(value) => {
                        let v: result[null, str] = json.validate_object(value, ["name", "version"], ["name", "version"]);
                        match (v) {
                            ok(_) => { label = "unexpected-ok"; }
                            err(m) => {
                                if (string.contains(m, "missing required field(s): version") && string.contains(m, "unknown field(s): extra")) {
                                    label = "object-schema-ok";
                                } else {
                                    label = m;
                                }
                            }
                        }
                    }
                    err(_) => { label = "parse-err"; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("object-schema-ok"));
}

#[test]
fn json_apply_defaults_injects_missing_object_fields() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;
                import "std/string.nox" as string;

                let doc: result[json, str] = json.parse("{\"name\":\"nox\"}");
                let defaults: result[json, str] = json.parse("{\"debug\":false,\"name\":\"fallback\",\"port\":8080}");
                let label: str = "fail";
                match (doc) {
                    ok(value) => {
                        match (defaults) {
                            ok(default_values) => {
                                let applied: result[json, str] = json.apply_defaults(value, default_values);
                                match (applied) {
                                    ok(updated) => {
                                        let text: str = json.stringify(updated);
                                        if (
                                            string.contains(text, "\"name\":\"nox\"") &&
                                            string.contains(text, "\"port\":8080") &&
                                            string.contains(text, "\"debug\":false") &&
                                            !string.contains(text, "fallback")
                                        ) {
                                            label = "defaults-ok";
                                        } else {
                                            label = text;
                                        }
                                    }
                                    err(m) => { label = m; }
                                }
                            }
                            err(_) => { label = "defaults-parse-err"; }
                        }
                    }
                    err(_) => { label = "doc-parse-err"; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("defaults-ok"));
}

#[test]
fn json_apply_defaults_deep_injects_nested_missing_fields() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/json.nox" as json;
                import "std/string.nox" as string;

                let doc: result[json, str] = json.parse("{\"server\":{\"host\":\"localhost\"},\"mode\":\"prod\"}");
                let defaults: result[json, str] = json.parse("{\"server\":{\"host\":\"fallback\",\"port\":8080},\"mode\":\"dev\",\"debug\":false}");
                let label: str = "fail";
                match (doc) {
                    ok(value) => {
                        match (defaults) {
                            ok(default_values) => {
                                let applied: result[json, str] = json.apply_defaults_deep(value, default_values);
                                match (applied) {
                                    ok(updated) => {
                                        let text: str = json.stringify(updated);
                                        if (
                                            string.contains(text, "\"server\":{\"host\":\"localhost\",\"port\":8080}") &&
                                            string.contains(text, "\"mode\":\"prod\"") &&
                                            string.contains(text, "\"debug\":false") &&
                                            !string.contains(text, "fallback")
                                        ) {
                                            label = "defaults-deep-ok";
                                        } else {
                                            label = text;
                                        }
                                    }
                                    err(m) => { label = m; }
                                }
                            }
                            err(_) => { label = "defaults-parse-err"; }
                        }
                    }
                    err(_) => { label = "doc-parse-err"; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("defaults-deep-ok"));
}

#[test]
fn bytes_stdlib_round_trips_utf8_and_encodings() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/bytes.nox" as bytes;

                let utf: [int] = bytes.encode_utf8("hi");
                let decoded: result[str, str] = bytes.decode_utf8(utf);
                let b64: str = bytes.base64_encode(utf);
                let from_b64: result[[int], str] = bytes.base64_decode(b64);
                let hex: str = bytes.hex_encode(utf);
                let from_hex: result[[int], str] = bytes.hex_decode(hex);

                let label: str = "fail";
                match (decoded) {
                    ok(text) => {
                        match (from_b64) {
                            ok(b64_back) => {
                                match (from_hex) {
                                    ok(hex_back) => {
                                        if (
                                            text == "hi" &&
                                            utf[0] == 104 &&
                                            utf[1] == 105 &&
                                            b64 == "aGk=" &&
                                            b64_back[0] == 104 &&
                                            hex == "6869" &&
                                            hex_back[1] == 105
                                        ) {
                                            label = "bytes-ok";
                                        } else {
                                            label = "bytes-bad";
                                        }
                                    }
                                    err(_) => { label = "bytes-hex-err"; }
                                }
                            }
                            err(_) => { label = "bytes-b64-err"; }
                        }
                    }
                    err(_) => { label = "bytes-utf-err"; }
                }
                label;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("bytes-ok"));
}

#[test]
fn hash_stdlib_sha256_and_hmac_hash_text_and_bytes() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/hash.nox" as hash;

                if (
                    hash.sha256_text("abc") == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" &&
                    hash.sha256_hex([97, 98, 99]) == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" &&
                    hash.sha256_text("") == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" &&
                    hash.hmac_sha256_text("key", "The quick brown fox jumps over the lazy dog") == "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8" &&
                    hash.hmac_sha256_hex([107, 101, 121], [84, 104, 101]) == "c42933273b78944b2aab3e7bafa21d5da976d162d69904ded282c6c540f26b2e"
                ) {
                    "hash-ok";
                } else {
                    "hash-bad";
                }
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("hash-ok"));
}

#[test]
fn bytes_stdlib_indexes_slices_and_compares_byte_arrays() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/bytes.nox" as bytes;
                import "std/string.nox" as string;

                let values: [int] = [1, 2, 3, 4];
                let length: int = bytes.len(values);
                let second: result[int, str] = bytes.get(values, 1);
                let missing: result[int, str] = bytes.get(values, 9);
                let middle: result[[int], str] = bytes.slice_copy(values, 1, 2);
                let too_far: result[[int], str] = bytes.slice_copy(values, 3, 9);
                let label: str = "fail";

                match (second) {
                    ok(byte) => {
                        match (missing) {
                            ok(_) => { label = "missing-bad"; }
                            err(missing_message) => {
                                match (middle) {
                                    ok(slice) => {
                                        match (too_far) {
                                            ok(_) => { label = "slice-bad"; }
                                            err(slice_message) => {
                                                if (
                                                    length == 4 &&
                                                    byte == 2 &&
                                                    slice[0] == 2 &&
                                                    slice[1] == 3 &&
                                                    bytes.equal(values, [1, 2, 3, 4]) &&
                                                    !bytes.equal(values, [1, 2]) &&
                                                    string.contains(missing_message, "out of range") &&
                                                    string.contains(slice_message, "out of range")
                                                ) {
                                                    label = "byte-access-ok";
                                                } else {
                                                    label = "byte-access-bad";
                                                }
                                            }
                                        }
                                    }
                                    err(_) => { label = "slice-err"; }
                                }
                            }
                        }
                    }
                    err(_) => { label = "get-err"; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("byte-access-ok"));
}

#[test]
fn bytes_stdlib_rejects_out_of_range_byte_values() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                import "std/bytes.nox" as bytes;
                bytes.base64_encode([300]);
                "#,
        )
        .unwrap_err();
    assert!(
        err.message.contains("out of range"),
        "expected out-of-range diagnostic, got: {}",
        err.message
    );
}

#[test]
fn encoding_stdlib_base64_and_hex_round_trip() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/encoding.nox" as enc;

                let b64: str = enc.base64_encode("hello");
                let decoded: result[str, str] = enc.base64_decode(b64);
                let hex: str = enc.hex_encode("ab");
                let hex_back: result[str, str] = enc.hex_decode(hex);

                let label: str = "fail";
                match (decoded) {
                    ok(text) => {
                        match (hex_back) {
                            ok(back) => {
                                if (b64 == "aGVsbG8=" && text == "hello" && hex == "6162" && back == "ab") {
                                    label = "encoding-ok";
                                } else {
                                    label = "encoding-bad";
                                }
                            }
                            err(_) => {
                                label = "encoding-bad-hex";
                            }
                        }
                    }
                    err(_) => {
                        label = "encoding-bad-b64";
                    }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("encoding-ok"));
}

#[test]
fn encoding_stdlib_rejects_malformed_base64() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/encoding.nox" as enc;

                let r: result[str, str] = enc.base64_decode("not!base64");
                let label: str = "fail";
                match (r) {
                    ok(_) => { label = "unexpected-ok"; }
                    err(_) => { label = "rejected"; }
                }
                label;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("rejected"));
}

#[test]
fn dotenv_stdlib_parses_basic_lines() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/dotenv.nox" as dotenv;

                let env: result[map[str, str], str] = dotenv.parse("FOO=bar\n# comment\nBAZ=\"hello world\"\nQUUX='single quoted'\n");
                let label: str = "fail";
                match (env) {
                    ok(m) => {
                        let foo: option[str] = map_get(m, "FOO");
                        let baz: option[str] = map_get(m, "BAZ");
                        let quux: option[str] = map_get(m, "QUUX");
                        let foo_ok: bool = false;
                        let baz_ok: bool = false;
                        let quux_ok: bool = false;
                        match (foo) { some(v) => { if (v == "bar") { foo_ok = true; } } none => {} }
                        match (baz) { some(v) => { if (v == "hello world") { baz_ok = true; } } none => {} }
                        match (quux) { some(v) => { if (v == "single quoted") { quux_ok = true; } } none => {} }
                        if (foo_ok && baz_ok && quux_ok && map_size(m) == 3) {
                            label = "dotenv-ok";
                        } else {
                            label = "dotenv-bad";
                        }
                    }
                    err(_) => { label = "dotenv-err"; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("dotenv-ok"));
}

#[test]
fn ini_stdlib_parses_sections_and_top_level_keys() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/ini.nox" as ini;

                let parsed: result[map[str, map[str, str]], str] = ini.parse("root = top\n[server]\nport = 8080\nname: nox # comment\n[paths]\nhome = '/tmp/nox'\n");
                let label: str = "fail";
                match (parsed) {
                    ok(config) => {
                        let root_section: option[map[str, str]] = map_get(config, "");
                        let server_section: option[map[str, str]] = map_get(config, "server");
                        let paths_section: option[map[str, str]] = map_get(config, "paths");
                        let root_ok: bool = false;
                        let server_ok: bool = false;
                        let paths_ok: bool = false;
                        match (root_section) {
                            some(section) => {
                                let root: option[str] = map_get(section, "root");
                                match (root) { some(v) => { if (v == "top") { root_ok = true; } } none => {} }
                            }
                            none => {}
                        }
                        match (server_section) {
                            some(section) => {
                                let port: option[str] = map_get(section, "port");
                                let name: option[str] = map_get(section, "name");
                                match (port) {
                                    some(port_value) => {
                                        match (name) {
                                            some(name_value) => {
                                                if (port_value == "8080" && name_value == "nox") {
                                                    server_ok = true;
                                                }
                                            }
                                            none => {}
                                        }
                                    }
                                    none => {}
                                }
                            }
                            none => {}
                        }
                        match (paths_section) {
                            some(section) => {
                                let home: option[str] = map_get(section, "home");
                                match (home) { some(v) => { if (v == "/tmp/nox") { paths_ok = true; } } none => {} }
                            }
                            none => {}
                        }
                        if (root_ok && server_ok && paths_ok) {
                            label = "ini-ok";
                        } else {
                            label = "ini-bad";
                        }
                    }
                    err(message) => { label = message; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("ini-ok"));
}

#[test]
fn ini_stdlib_rejects_bad_section_header() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/ini.nox" as ini;
                import "std/string.nox" as string;

                let parsed: result[map[str, map[str, str]], str] = ini.parse("[missing\nkey=value");
                match (parsed) {
                    ok(_) => {
                        "unexpected-ok";
                    }
                    err(message) => {
                        if (string.contains(message, "unterminated section header")) {
                            "ini-error-ok";
                        } else {
                            message;
                        }
                    }
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("ini-error-ok"));
}

#[test]
fn toml_stdlib_parses_minimal_config_to_json() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/toml.nox" as toml;
                import "std/json.nox" as json;
                import "std/string.nox" as string;

                let parsed: result[json, str] = toml.parse("title = \"Nox\"\n[package]\nname = \"nox\"\nversion = \"0.0.3\"\n[server]\nport = 8080\nenabled = true\ntags = [\"cli\", \"runtime\"]\n");
                let label: str = "fail";
                match (parsed) {
                    ok(config) => {
                        let text: str = json.stringify(config);
                        if (
                            string.contains(text, "\"title\":\"Nox\"") &&
                            string.contains(text, "\"package\"") &&
                            string.contains(text, "\"name\":\"nox\"") &&
                            string.contains(text, "\"port\":8080") &&
                            string.contains(text, "\"enabled\":true") &&
                            string.contains(text, "\"tags\":[\"cli\",\"runtime\"]")
                        ) {
                            label = "toml-ok";
                        } else {
                            label = text;
                        }
                    }
                    err(message) => { label = message; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("toml-ok"));
}

#[test]
fn toml_stdlib_rejects_unsupported_values() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/toml.nox" as toml;
                import "std/string.nox" as string;

                let parsed: result[json, str] = toml.parse("when = 2026-05-24T00:00:00Z");
                match (parsed) {
                    ok(_) => {
                        "unexpected-ok";
                    }
                    err(message) => {
                        if (string.contains(message, "unsupported TOML value")) {
                            "toml-error-ok";
                        } else {
                            message;
                        }
                    }
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("toml-error-ok"));
}

#[test]
fn yaml_stdlib_parses_minimal_config_to_json() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/yaml.nox" as yaml;
                import "std/json.nox" as json;
                import "std/string.nox" as string;

                let parsed: result[json, str] = yaml.parse("name: Nox\nserver:\n  port: 8080\n  enabled: true\ntags:\n  - cli\n  - runtime\nlimits: [1, 2, 3]\n");
                let label: str = "fail";
                match (parsed) {
                    ok(config) => {
                        let text: str = json.stringify(config);
                        if (
                            string.contains(text, "\"name\":\"Nox\"") &&
                            string.contains(text, "\"server\"") &&
                            string.contains(text, "\"port\":8080") &&
                            string.contains(text, "\"enabled\":true") &&
                            string.contains(text, "\"tags\":[\"cli\",\"runtime\"]") &&
                            string.contains(text, "\"limits\":[1,2,3]")
                        ) {
                            label = "yaml-ok";
                        } else {
                            label = text;
                        }
                    }
                    err(message) => { label = message; }
                }
                label;
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("yaml-ok"));
}

#[test]
fn yaml_stdlib_rejects_malformed_indentation() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/yaml.nox" as yaml;
                import "std/string.nox" as string;

                let parsed: result[json, str] = yaml.parse("root:\n  child: ok\n    bad: drift\n");
                match (parsed) {
                    ok(_) => {
                        "unexpected-ok";
                    }
                    err(message) => {
                        if (string.contains(message, "unexpected indentation")) {
                            "yaml-error-ok";
                        } else {
                            message;
                        }
                    }
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("yaml-error-ok"));
}

#[test]
fn xml_stdlib_escapes_text_attrs_and_validates_names() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/xml.nox" as xml;
                import "std/string.nox" as string;

                let escaped_text: str = xml.escape_text("Nox <core> & runtime");
                let escaped_attr: str = xml.escape_attr("quote \" and apostrophe '");
                let element: result[str, str] = xml.text_element("title", "Nox & runtime");
                let attr: result[str, str] = xml.attr("data-id", "nox \"core\"");
                let comment: result[str, str] = xml.comment("generated by Nox");
                let bad_comment: result[str, str] = xml.comment("bad -- marker");
                let bad_comment_tail: result[str, str] = xml.comment("bad-");
                let bad_name: result[str, str] = xml.validate_name("1bad");
                let label: str = "fail";
                match (element) {
                    ok(element_text) => {
                        match (attr) {
                            ok(attr_text) => {
                                match (comment) {
                                    ok(comment_text) => {
                                        match (bad_comment) {
                                            ok(_) => {
                                                label = "xml-bad-comment";
                                            }
                                            err(comment_error) => {
                                                match (bad_comment_tail) {
                                                    ok(_) => {
                                                        label = "xml-bad-comment-tail";
                                                    }
                                                    err(tail_error) => {
                                                        match (bad_name) {
                                                            ok(_) => {
                                                                label = "xml-bad-name";
                                                            }
                                                            err(_) => {
                                                                if (
                                                                    escaped_text == "Nox &lt;core&gt; &amp; runtime" &&
                                                                    string.contains(escaped_attr, "&quot;") &&
                                                                    string.contains(escaped_attr, "&apos;") &&
                                                                    element_text == "<title>Nox &amp; runtime</title>" &&
                                                                    attr_text == "data-id=\"nox &quot;core&quot;\"" &&
                                                                    comment_text == "<!--generated by Nox-->" &&
                                                                    string.contains(comment_error, "must not contain") &&
                                                                    string.contains(tail_error, "must not end")
                                                                ) {
                                                                    label = "xml-ok";
                                                                } else {
                                                                    label = element_text + " " + attr_text + " " + comment_text + " " + comment_error + " " + tail_error;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    err(message) => { label = message; }
                                }
                            }
                            err(message) => { label = message; }
                        }
                    }
                    err(message) => { label = message; }
                }
                label;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("xml-ok"));
}

#[test]
fn xml_stdlib_builds_attrs_empty_and_text_elements() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/xml.nox" as xml;
                import "std/string.nox" as string;

                let attrs: result[str, str] = xml.attrs({"data-id": "nox \"core\""});
                let empty: result[str, str] = xml.empty_element("entry", {"data-id": "42"});
                let element: result[str, str] = xml.text_element_attrs("summary", {"kind": "report"}, "Nox & runtime");
                let bad: result[str, str] = xml.empty_element("bad name", {});
                let label: str = "xml-attrs-fail";
                match (attrs) {
                    ok(attrs_text) => {
                        match (empty) {
                            ok(empty_text) => {
                                match (element) {
                                    ok(element_text) => {
                                        match (bad) {
                                            ok(_) => {
                                                label = "xml-attrs-bad-name";
                                            }
                                            err(message) => {
                                                if (
                                                    string.contains(attrs_text, "data-id=\"nox &quot;core&quot;\"") &&
                                                    empty_text == "<entry data-id=\"42\"/>" &&
                                                    element_text == "<summary kind=\"report\">Nox &amp; runtime</summary>" &&
                                                    string.contains(message, "invalid XML name")
                                                ) {
                                                    label = "xml-attrs-ok";
                                                } else {
                                                    label = attrs_text + " " + empty_text + " " + element_text + " " + message;
                                                }
                                            }
                                        }
                                    }
                                    err(message) => { label = message; }
                                }
                            }
                            err(message) => { label = message; }
                        }
                    }
                    err(message) => { label = message; }
                }
                label;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("xml-attrs-ok"));
}

#[test]
fn xml_stdlib_builds_namespace_names_and_elements() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/xml.nox" as xml;
                import "std/string.nox" as string;

                let qname: result[str, str] = xml.qname("app", "summary");
                let default_qname: result[str, str] = xml.qname("", "summary");
                let namespace_attr: result[str, str] = xml.xmlns("app", "urn:nox");
                let default_namespace: result[str, str] = xml.xmlns_default("urn:default");
                let element: result[str, str] = xml.text_element_ns("app", "summary", {"xmlns:app": "urn:nox"}, "Nox & runtime");
                let empty: result[str, str] = xml.empty_element_ns("app", "entry", {"xmlns:app": "urn:nox"});
                let bad_prefix: result[str, str] = xml.qname("bad:prefix", "summary");
                let label: str = "xml-ns-fail";
                match (qname) {
                    ok(qname_text) => {
                        match (default_qname) {
                            ok(default_qname_text) => {
                                match (namespace_attr) {
                                    ok(namespace_text) => {
                                        match (default_namespace) {
                                            ok(default_namespace_text) => {
                                                match (element) {
                                                    ok(element_text) => {
                                                        match (empty) {
                                                            ok(empty_text) => {
                                                                match (bad_prefix) {
                                                                    ok(_) => {
                                                                        label = "xml-ns-bad-prefix";
                                                                    }
                                                                    err(message) => {
                                                                        if (
                                                                            qname_text == "app:summary" &&
                                                                            default_qname_text == "summary" &&
                                                                            namespace_text == "xmlns:app=\"urn:nox\"" &&
                                                                            default_namespace_text == "xmlns=\"urn:default\"" &&
                                                                            string.contains(element_text, "<app:summary") &&
                                                                            string.contains(element_text, "xmlns:app=\"urn:nox\"") &&
                                                                            string.contains(element_text, ">Nox &amp; runtime</app:summary>") &&
                                                                            string.contains(empty_text, "<app:entry") &&
                                                                            string.contains(empty_text, "/>") &&
                                                                            string.contains(message, "namespace prefix")
                                                                        ) {
                                                                            label = "xml-ns-ok";
                                                                        } else {
                                                                            label = qname_text + " " + default_qname_text + " " + namespace_text + " " + element_text + " " + empty_text + " " + message;
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            err(message) => { label = message; }
                                                        }
                                                    }
                                                    err(message) => { label = message; }
                                                }
                                            }
                                            err(message) => { label = message; }
                                        }
                                    }
                                    err(message) => { label = message; }
                                }
                            }
                            err(message) => { label = message; }
                        }
                    }
                    err(message) => { label = message; }
                }
                label;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("xml-ns-ok"));
}

#[test]
fn url_stdlib_query_encode_round_trips() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/url.nox" as url;

                let raw: str = "hello world+&=";
                let encoded: str = url.query_encode(raw);
                let decoded: result[str, str] = url.query_decode(encoded);
                match (decoded) {
                    ok(text) => {
                        if (text == raw && encoded == "hello%20world%2B%26%3D") {
                            "url-roundtrip-ok";
                        } else {
                            "url-roundtrip-bad";
                        }
                    }
                    err(_) => {
                        "url-roundtrip-error";
                    }
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("url-roundtrip-ok"));
}

#[test]
fn url_stdlib_parse_and_build_recover_components() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/url.nox" as url;

                let parsed: result[(str, str, int, str, str), str] = url.parse("http://example.com:8080/path?a=1");
                let label: str = "parse-err";
                match (parsed) {
                    ok(parts) => {
                        let (scheme, host, port, path, query) = parts;
                        if (scheme == "http" && host == "example.com" && port == 8080 && path == "/path" && query == "a=1") {
                            label = "parse-ok";
                        } else {
                            label = "parse-bad";
                        }
                    }
                    err(_) => {
                        label = "parse-err";
                    }
                }
                let built: str = url.build("http", "example.com", 8080, "/path", "a=1");
                if (label == "parse-ok" && built == "http://example.com:8080/path?a=1") {
                    "url-build-ok";
                } else {
                    "url-build-bad";
                }
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("url-build-ok"));
}

#[test]
fn http_stdlib_requires_network_capability() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                import "std/http.nox" as http;
                http.get("http://localhost:1/x", 100);
                "#,
        )
        .unwrap_err();
    assert!(
        err.message.contains("network capability"),
        "expected network capability diagnostic, got: {}",
        err.message
    );
}

#[test]
fn http_stdlib_rejects_non_http_scheme_when_allowed() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        network: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
            .eval(
                r#"
                import "std/http.nox" as http;

                let result_value: result[(int, str), str] = http.get("ftp://example.com/", 100);
                match (result_value) {
                    ok(_) => {
                        "unexpected-ok";
                    }
                    err(message) => {
                        if (message == "scheme 'ftp' is not supported; only 'http' is implemented") {
                            "scheme-rejected";
                        } else {
                            message;
                        }
                    }
                }
                "#,
            )
            .unwrap();
    assert_eq!(value, Value::string("scheme-rejected"));
}

#[test]
fn http_stdlib_get_against_local_mock_server() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4096];
        let _ = stream.read(&mut buf);
        let body = "hello";
        let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        stream.write_all(response.as_bytes()).unwrap();
    });

    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        network: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let source = format!(
        r#"
            import "std/http.nox" as http;

            let response: result[(int, str), str] = http.get("http://127.0.0.1:{port}/probe", 5000);
            match (response) {{
                ok(parts) => {{
                    let (status, body) = parts;
                    if (status == 200 && body == "hello") {{
                        "http-ok";
                    }} else {{
                        "http-bad";
                    }}
                }}
                err(message) => {{
                    message;
                }}
            }}
            "#
    );
    let value = runtime.eval(&source).unwrap();
    handle.join().unwrap();
    assert_eq!(value, Value::string("http-ok"));
}

#[test]
fn http_stdlib_get_binary_returns_byte_array_body() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4096];
        let _ = stream.read(&mut buf);
        let body: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];
        let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
        response.extend_from_slice(&body);
        stream.write_all(&response).unwrap();
    });

    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        network: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let source = format!(
        r#"
            import "std/http.nox" as http;

            let response: result[(int, [int]), str] = http.get_binary("http://127.0.0.1:{port}/probe", 5000);
            match (response) {{
                ok(parts) => {{
                    let (status, body) = parts;
                    if (status == 200 && len(body) == 4 && body[0] == 222 && body[3] == 239) {{
                        "binary-ok";
                    }} else {{
                        "binary-bad";
                    }}
                }}
                err(message) => {{
                    message;
                }}
            }}
            "#
    );
    let value = runtime.eval(&source).unwrap();
    handle.join().unwrap();
    assert_eq!(value, Value::string("binary-ok"));
}

#[test]
fn http_stdlib_request_sends_headers_body_and_returns_folded_response_headers() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap();
        request.extend_from_slice(&buf[..n]);
        let request_text = String::from_utf8_lossy(&request);
        assert!(request_text.starts_with("PUT /api HTTP/1.1"));
        assert!(request_text.contains("\r\nx-nox-token: abc\r\n"));
        assert!(request_text.ends_with("payload"));
        let response = concat!(
            "HTTP/1.1 201 Created\r\n",
            "Content-Type: application/json\r\n",
            "Set-Cookie: a=1\r\n",
            "Set-Cookie: b=2\r\n",
            "Content-Length: 11\r\n",
            "Connection: close\r\n",
            "\r\n",
            "{\"ok\":true}"
        );
        stream.write_all(response.as_bytes()).unwrap();
    });

    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        network: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let source = format!(
        r#"
            import "std/http.nox" as http;

            let response: result[(int, map[str, str], str), str] = http.request("PUT", "http://127.0.0.1:{port}/api", {{"x-nox-token": "abc"}}, "payload", 5000);
            match (response) {{
                ok(parts) => {{
                    let (status, headers, body) = parts;
                    if (
                        status == 201 &&
                        headers["content-type"] == "application/json" &&
                        headers["set-cookie"] == "a=1, b=2" &&
                        body == "{{\"ok\":true}}"
                    ) {{
                        "request-ok";
                    }} else {{
                        "request-bad";
                    }}
                }}
                err(message) => {{
                    message;
                }}
            }}
            "#
    );
    let value = runtime.eval(&source).unwrap();
    handle.join().unwrap();
    assert_eq!(value, Value::string("request-ok"));
}

#[test]
fn http_stdlib_request_binary_uses_mock_response_headers_and_body() {
    let mut headers = BTreeMap::new();
    headers.insert("X-Mock".to_string(), "yes".to_string());

    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        network: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_network(Some(MockNetwork::new().with_http_binary_response_headers(
        "POST",
        "http://example.test/upload",
        202,
        headers,
        vec![5, 4, 3],
    )));

    let value = runtime
            .eval(
                r#"
                import "std/http.nox" as http;

                let response: result[(int, map[str, str], [int]), str] = http.request_binary("POST", "http://example.test/upload", {"x-client": "ok"}, [9, 8], 1);
                match (response) {
                    ok(parts) => {
                        let (status, headers, body) = parts;
                        if (status == 202 && headers["x-mock"] == "yes" && len(body) == 3 && body[0] == 5 && body[2] == 3) {
                            "request-binary-ok";
                        } else {
                            "request-binary-bad";
                        }
                    }
                    err(message) => {
                        message;
                    }
                }
                "#,
            )
            .unwrap();

    assert_eq!(value, Value::string("request-binary-ok"));
}

#[test]
fn http_stdlib_request_missing_mock_response_does_not_fall_back() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        network: true,
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_network(Some(MockNetwork::new()));

    let value = runtime
            .eval(
                r#"
                import "std/http.nox" as http;
                let response: result[(int, map[str, str], str), str] = http.request("PATCH", "http://127.0.0.1:9/missing", {}, "", 1);
                match (response) {
                    ok(_) => { "unexpected-ok"; }
                    err(message) => { message; }
                }
                "#,
            )
            .unwrap();

    let Value::String(message) = value else {
        panic!("expected mock network error string");
    };
    assert!(message.contains("mock network has no PATCH response"));
}

#[test]
fn array_stdlib_dedupe_and_contains_value_use_equatable_constraint_and_eq_trait() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/array.nox" as array;

                let xs: [int] = [1, 2, 2, 3, 1, 4];
                let d: [int] = array.dedupe(xs);
                let found: bool = array.contains_value(xs, 3);
                let missing: bool = array.contains_value(xs, 99);
                let td: [int] = array.dedupe_equal(xs);
                let tfound: bool = array.contains_equal(xs, 3);
                let tmissing: bool = array.contains_equal(xs, 99);

                if (array.len(d) == 4 &&
                    d[0] == 1 &&
                    d[3] == 4 &&
                    found &&
                    !missing &&
                    array.len(td) == 4 &&
                    td[0] == 1 &&
                    td[3] == 4 &&
                    tfound &&
                    !tmissing) {
                    "equatable-helpers-ok";
                } else {
                    "equatable-helpers-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("equatable-helpers-ok"));
}

#[test]
fn array_stdlib_eq_trait_helpers_accept_user_record_impl() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/array.nox";

                record User {
                    id: int,
                    name: str,
                }

                impl Eq for User {
                    fn equals(self: User, other: User) -> bool {
                        return self.id == other.id;
                    }
                }

                let xs: [User] = [
                    User { id: 1, name: "a" },
                    User { id: 2, name: "b" },
                    User { id: 1, name: "c" },
                ];
                let d: [User] = dedupe_equal(xs);
                let found: bool = contains_equal(xs, User { id: 2, name: "other" });
                if (len(d) == 2 && d[0].name == "a" && d[1].name == "b" && found) {
                    "eq-record-ok";
                } else {
                    "eq-record-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("eq-record-ok"));
}

#[test]
fn array_stdlib_eq_trait_helpers_reject_user_record_without_impl() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                import "std/array.nox" as array;

                record User {
                    id: int,
                }

                let xs: [User] = [
                    User { id: 1 },
                    User { id: 1 },
                ];
                array.dedupe_equal(xs);
                "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "trait.bound-unsatisfied");
    assert!(
        err.message.contains("does not implement"),
        "expected trait bound message, got: {}",
        err.message
    );
}

#[test]
fn traits_stdlib_eq_and_display_helpers_accept_user_impls() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/traits.nox";

                record User {
                    id: int,
                    name: str,
                }

                impl Eq for User {
                    fn equals(self: User, other: User) -> bool {
                        return self.id == other.id;
                    }
                }

                impl Display for User {
                    fn to_str(self: User) -> str {
                        return self.name;
                    }
                }

                let same: bool = equal(
                    User { id: 1, name: "Ada" },
                    User { id: 1, name: "Lovelace" }
                );
                let different: bool = not_equal(
                    User { id: 1, name: "Ada" },
                    User { id: 2, name: "Grace" }
                );
                let label: str = display(User { id: 2, name: "Grace" });
                let tagged: str = display_label("user", User { id: 3, name: "Lin" });
                if (same && different && label == "Grace" && tagged == "user: Lin" && display(42) == "42") {
                    "traits-ok";
                } else {
                    "traits-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("traits-ok"));
}

#[test]
fn array_stdlib_higher_order_helpers_map_filter_reduce_for_each() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/array.nox" as array;

                let xs: [int] = [1, 2, 3, 4];
                let doubled: [int] = array.map_fn(xs, fn(x: int) -> int { return x * 2; });
                let big: [int] = array.filter_fn(xs, fn(x: int) -> bool { return x > 2; });
                let sum: int = array.reduce(xs, 0, fn(acc: int, x: int) -> int { return acc + x; });

                let counter: [int] = [0];
                array.for_each(xs, fn(_: int) -> null {
                    counter[0] = counter[0] + 1;
                    return null;
                });

                if (
                    array.len(doubled) == 4 &&
                    doubled[3] == 8 &&
                    array.len(big) == 2 &&
                    big[0] == 3 &&
                    big[1] == 4 &&
                    sum == 10 &&
                    counter[0] == 4
                ) {
                    "hof-ok";
                } else {
                    "hof-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("hof-ok"));
}

#[test]
fn map_stdlib_mutates_in_place_and_aliases_observe_changes() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/map.nox" as map;

                let m: map[str, int] = {"a": 1};
                map.set(m, "b", 2);
                let len_after_set: int = len(map.keys(m));

                let alias: map[str, int] = m;
                map.set(alias, "c", 3);
                let len_via_alias: int = len(map.keys(m));

                let deleted_existing: bool = map.delete(m, "a");
                let deleted_missing: bool = map.delete(m, "zzz");
                let len_after_delete: int = len(map.keys(m));

                if (
                    len_after_set == 2 &&
                    len_via_alias == 3 &&
                    deleted_existing &&
                    !deleted_missing &&
                    len_after_delete == 2
                ) {
                    "map-mutation-ok";
                } else {
                    "map-mutation-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("map-mutation-ok"));
}

#[test]
fn option_result_stdlib_helpers_cover_status_and_fallbacks() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/option.nox" as option;
                import "std/result.nox" as result;

                let present: option[int] = some(7);
                let missing: option[int] = none;
                let loaded: result[int, str] = ok(9);
                let failed: result[int, str] = err("missing");
                let mapped: result[int, str] = result.map_err_to_str(failed);
                fn double(value: int) -> int {
                    return value * 2;
                }
                fn parse_positive(value: int) -> result[int, str] {
                    if (value > 0) {
                        return ok(value);
                    } else {
                        return err("not-positive");
                    }
                }
                fn as_some(value: int) -> option[int] {
                    return some(value + 1);
                }
                fn prefix(value: str) -> str {
                    return "error:" + value;
                }
                fn fallback() -> int {
                    return 11;
                }
                fn fallback_from_error(value: str) -> int {
                    return len(value);
                }
                fn recover(value: str) -> result[int, str] {
                    if (value == "missing") {
                        return ok(13);
                    } else {
                        return err(value);
                    }
                }
                let option_mapped: option[int] = option.map(present, double);
                let option_chained: option[int] = option.and_then(present, as_some);
                let option_none_mapped: option[int] = option.map(missing, double);
                let option_lazy_some: int = option.unwrap_or_else(present, fallback);
                let option_lazy_none: int = option.unwrap_or_else(missing, fallback);
                let option_ok: result[int, str] = option.ok_or(present, "not-set");
                let option_err: result[int, str] = option.ok_or(missing, "not-set");
                let option_filtered_some: option[int] = option.filter(present, fn(value: int) -> bool { return value > 0; });
                let option_filtered_none: option[int] = option.filter(present, fn(value: int) -> bool { return value > 100; });
                let result_mapped: result[int, str] = result.map(loaded, double);
                let result_chained: result[int, str] = result.and_then(loaded, parse_positive);
                let result_err_mapped: result[int, str] = result.map_err(failed, prefix);
                let result_lazy_ok: int = result.unwrap_or_else(loaded, fallback_from_error);
                let result_lazy_err: int = result.unwrap_or_else(failed, fallback_from_error);
                let result_recovered: result[int, str] = result.or_else(failed, recover);
                let result_map_or_ok: int = result.map_or(loaded, 0, double);
                let result_map_or_err: int = result.map_or(failed, 5, double);

                if (
                    option.is_some(present) &&
                    option.is_none(missing) &&
                    option.unwrap_or(present, 0) == 7 &&
                    option.unwrap_or(missing, 5) == 5 &&
                    option_lazy_some == 7 &&
                    option_lazy_none == 11 &&
                    result.unwrap_or(option_ok, 0) == 7 &&
                    result.is_err(option_err) &&
                    option.unwrap_or(option_mapped, 0) == 14 &&
                    option.unwrap_or(option_chained, 0) == 8 &&
                    option.is_none(option_none_mapped) &&
                    option.is_some(option_filtered_some) &&
                    option.is_none(option_filtered_none) &&
                    result.is_ok(loaded) &&
                    result.is_err(failed) &&
                    result.unwrap_or(loaded, 0) == 9 &&
                    result.unwrap_or(failed, 4) == 4 &&
                    result_lazy_ok == 9 &&
                    result_lazy_err == 7 &&
                    result.unwrap_or(result_recovered, 0) == 13 &&
                    result.unwrap_or(result_mapped, 0) == 18 &&
                    result.unwrap_or(result_chained, 0) == 9 &&
                    result.is_err(result_err_mapped) &&
                    result_map_or_ok == 18 &&
                    result_map_or_err == 5 &&
                    result.is_err(mapped)
                ) {
                    "option-result-ok";
                } else {
                    "option-result-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("option-result-ok"));
}

#[test]
fn process_stdlib_reads_args_stdin_stderr_and_exit_code() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_args(vec!["alpha".to_string(), "beta".to_string()]);
    runtime.set_stdin("input line\n");
    let value = runtime
        .eval(
            r#"
                import "std/process.nox" as process;

                let argv: [str] = process.argv();
                let input: str = process.read_stdin();
                process.print_err("warn:" + argv[0]);
                process.exit(7);
                if (len(argv) == 2 && argv[1] == "beta" && input == "input line\n") {
                    "process-ok";
                } else {
                    "process-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("process-ok"));
    assert_eq!(runtime.take_stderr(), "warn:alpha\n");
    assert_eq!(runtime.exit_code(), Some(7));
}

#[test]
fn mock_stdio_overrides_stdin_and_captures_stdout() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_stdin(Some("mock input\n".to_string()));
    runtime.set_mock_stdout(true);

    let value = runtime
        .eval(
            r#"
                import "std/process.nox" as process;
                let input: str = process.read_stdin();
                print("out:" + input);
                if (input == "mock input\n") {
                    "mock-stdio-ok";
                } else {
                    "mock-stdio-bad";
                }
                "#,
        )
        .unwrap();

    assert_eq!(value, Value::string("mock-stdio-ok"));
    assert_eq!(runtime.take_stdout(), "out:mock input\n\n");
    runtime.set_mock_stdin(None);
    runtime.set_mock_stdout(false);
}

#[test]
fn run_test_file_restores_mock_stdout_capture_state() {
    let dir = std::env::temp_dir().join(format!(
        "nox-runtime-mock-stdout-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("capture_test.nox");
    fs::write(
        &path,
        "fn test_prints() -> bool {\n    print(\"inside-test\");\n    return true;\n}\n",
    )
    .unwrap();

    let mut runtime = Runtime::with_permissions(
        RuntimePermissions::none().allow_filesystem_read_under(dir.clone()),
    );
    runtime.set_import_base(dir.clone(), Vec::new());
    runtime.set_mock_stdout(true);
    let result = runtime.run_test_file(&path).unwrap();

    assert_eq!(result.tests.len(), 1);
    assert_eq!(result.tests[0].stdout, "inside-test\n");
    assert_eq!(runtime.take_stdout(), "inside-test\n");
    runtime.eval("print(\"after-test\");").unwrap();
    assert_eq!(runtime.take_stdout(), "after-test\n");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn run_test_file_cleans_async_tasks_from_failed_test_cases() {
    let dir = std::env::temp_dir().join(format!(
        "nox-runtime-test-async-cleanup-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("async_cleanup_test.nox");
    fs::write(
        &path,
        r#"
            fn test_spawns_and_fails() -> bool {
                task_sleep_ms(60000);
                return false;
            }
        "#,
    )
    .unwrap();

    let mut permissions = RuntimePermissions::none().allow_filesystem_read_under(dir.clone());
    permissions.async_tasks = true;
    let mut runtime = Runtime::with_permissions(permissions);
    runtime.set_import_base(dir.clone(), Vec::new());
    let preexisting = runtime
        .spawn_sleep_task(std::time::Duration::from_millis(60_000))
        .unwrap();
    assert_eq!(runtime.pending_async_task_count(), 1);

    let result = runtime.run_test_file(&path).unwrap();

    assert_eq!(result.tests.len(), 1);
    assert!(!result.tests[0].passed);
    assert_eq!(runtime.pending_async_task_count(), 1);
    runtime.cancel_async_task(preexisting).unwrap();
    assert_eq!(runtime.pending_async_task_count(), 0);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn process_exit_rejects_invalid_exit_codes() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                import "std/process.nox" as process;
                process.exit(300);
                "#,
        )
        .unwrap_err();
    assert!(err.message.contains("exit code must be between 0 and 255"));
}

#[test]
fn path_stdlib_normalizes_and_splits_paths() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/path.nox" as path;

                let joined: str = path.join("logs", "../data/report.txt");
                let normalized: str = path.normalize(joined);
                if (
                    normalized == "data/report.txt" &&
                    path.basename(normalized) == "report.txt" &&
                    path.dirname(normalized) == "data" &&
                    path.extension(normalized) == "txt"
                ) {
                    "path-ok";
                } else {
                    "path-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("path-ok"));
}

#[test]
fn std_fs_lists_and_classifies_allowed_paths() {
    let dir = std::env::temp_dir().join(format!(
        "nox-std-fs-list-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("nested")).unwrap();
    fs::write(dir.join("a.txt"), "alpha").unwrap();
    fs::write(dir.join("b.txt"), "beta").unwrap();

    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(&format!(
            r#"
                import "std/fs.nox" as fs;

                let root: str = "{}";
                let listed: result[[str], str] = fs.list_dir(root);
                match (listed) {{
                    ok(entries) => {{
                        if (
                            fs.is_dir(root) &&
                            fs.is_file(root + "/a.txt") &&
                            len(entries) == 3 &&
                            entries[0] == "a.txt" &&
                            entries[2] == "nested"
                        ) {{
                            "fs-list-ok";
                        }} else {{
                            "fs-list-bad";
                        }}
                    }}
                    err(message) => {{ message; }}
                }}
                "#,
            dir.display()
        ))
        .unwrap();
    assert_eq!(value, Value::string("fs-list-ok"));
}

#[test]
fn std_fs_new_helpers_require_filesystem_capability() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(r#"import "std/fs.nox" as fs; fs.list_dir(".");"#)
        .unwrap_err();
    assert!(err.message.contains("filesystem capability"));
}

#[test]
fn fs_read_binary_returns_byte_array_for_existing_file() {
    let dir = std::env::temp_dir().join(format!(
        "nox-fs-read-binary-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let data_path = dir.join("payload.bin");
    fs::write(&data_path, [0u8, 1, 2, 255, 128]).unwrap();
    let path_str = data_path.to_string_lossy().to_string();

    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(&format!(
            r#"
                import "std/fs.nox" as fs;
                let outcome: result[[int], str] = fs.read_binary("{path_str}");
                let label: str = "fail";
                match (outcome) {{
                    ok(bytes) => {{
                        if (
                            len(bytes) == 5 &&
                            bytes[0] == 0 &&
                            bytes[3] == 255 &&
                            bytes[4] == 128
                        ) {{
                            label = "binary-ok";
                        }} else {{
                            label = "binary-bad";
                        }}
                    }}
                    err(m) => {{ label = m; }}
                }}
                label;
                "#,
            path_str = path_str,
        ))
        .unwrap();
    fs::remove_dir_all(&dir).ok();
    assert_eq!(value, Value::string("binary-ok"));
}

#[test]
fn fs_read_binary_requires_filesystem_capability() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(r#"import "std/fs.nox" as fs; fs.read_binary("placeholder.bin");"#)
        .unwrap_err();
    assert!(
        err.message.contains("filesystem capability"),
        "expected filesystem capability diagnostic, got: {}",
        err.message
    );
}

#[test]
fn fs_write_binary_persists_bytes_with_capability() {
    let dir = std::env::temp_dir().join(format!(
        "nox-fs-write-binary-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let data_path = dir.join("out.bin");
    let path_str = data_path.to_string_lossy().to_string();

    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        filesystem: true,
        filesystem_read_roots: vec![dir.clone()],
        filesystem_write: true,
        filesystem_write_roots: vec![dir.clone()],
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(&format!(
            r#"
                import "std/fs.nox" as fs;
                let outcome: result[null, str] = fs.write_binary("{path_str}", [10, 20, 30]);
                let label: str = "fail";
                match (outcome) {{
                    ok(_) => {{ label = "write-ok"; }}
                    err(m) => {{ label = m; }}
                }}
                label;
                "#,
            path_str = path_str,
        ))
        .unwrap();
    assert_eq!(value, Value::string("write-ok"));
    let written = fs::read(&data_path).unwrap();
    fs::remove_dir_all(&dir).ok();
    assert_eq!(written, vec![10u8, 20, 30]);
}

#[test]
fn fs_canonicalize_resolves_path_when_allowed() {
    let dir = std::env::temp_dir().join(format!(
        "nox-fs-canonicalize-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let data = dir.join("target.txt");
    fs::write(&data, "hello").unwrap();
    let path_str = data.to_string_lossy().to_string();

    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(&format!(
            r#"
                import "std/fs.nox" as fs;
                let outcome: result[str, str] = fs.canonicalize("{path_str}");
                let label: str = "fail";
                match (outcome) {{
                    ok(resolved) => {{ label = resolved; }}
                    err(m) => {{ label = m; }}
                }}
                label;
                "#,
            path_str = path_str,
        ))
        .unwrap();
    let expected = std::fs::canonicalize(&data)
        .unwrap()
        .to_string_lossy()
        .to_string();
    fs::remove_dir_all(&dir).ok();
    match value {
        Value::String(s) => assert_eq!(s.as_ref(), expected),
        other => panic!("expected canonical path string, got {other:?}"),
    }
}

#[test]
fn fs_canonicalize_requires_filesystem_capability() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(r#"import "std/fs.nox" as fs; fs.canonicalize("placeholder.bin");"#)
        .unwrap_err();
    assert!(
        err.message.contains("filesystem capability"),
        "expected filesystem capability diagnostic, got: {}",
        err.message
    );
}

#[test]
fn fs_write_binary_rejects_out_of_range_bytes() {
    let dir = std::env::temp_dir().join(format!(
        "nox-fs-write-binary-range-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let data_path = dir.join("out.bin");
    let path_str = data_path.to_string_lossy().to_string();

    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        filesystem: true,
        filesystem_read_roots: vec![dir.clone()],
        filesystem_write: true,
        filesystem_write_roots: vec![dir.clone()],
        ..RuntimePermissions::default()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(&format!(
            r#"
                import "std/fs.nox" as fs;
                let outcome: result[null, str] = fs.write_binary("{path_str}", [256]);
                let label: str = "ok-unexpected";
                match (outcome) {{
                    ok(_) => {{ label = "ok-unexpected"; }}
                    err(m) => {{ label = m; }}
                }}
                label;
                "#,
            path_str = path_str,
        ))
        .unwrap();
    fs::remove_dir_all(&dir).ok();
    match value {
        Value::String(message) => {
            assert!(
                message.as_ref().contains("256") || message.as_ref().contains("out of range"),
                "expected out-of-range diagnostic, got {}",
                message.as_ref()
            );
        }
        other => panic!("expected string result, got {other:?}"),
    }
}

#[test]
fn runtime_resolves_std_fs_module() {
    let dir = std::env::temp_dir().join(format!("nox-std-fs-{}-{}", std::process::id(), line!()));
    fs::create_dir_all(&dir).unwrap();
    let data = dir.join("message.txt");
    fs::write(&data, "module-ok").unwrap();
    let script = dir.join("main.nox");
    fs::write(
        &script,
        format!(
            "import \"std/fs.nox\" as fs;\n\nfs.read_text(\"{}\");\n",
            data.display()
        ),
    )
    .unwrap();

    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    let value = runtime.eval_file(&script).unwrap();
    assert_eq!(value, Value::string("module-ok"));
}

#[test]
fn runtime_std_fs_try_read_text_returns_ok_for_existing_file() {
    let dir = std::env::temp_dir().join(format!(
        "nox-std-fs-try-ok-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let data = dir.join("message.txt");
    fs::write(&data, "module-ok").unwrap();
    let script = dir.join("main.nox");
    fs::write(
        &script,
        format!(
            r#"import "std/fs.nox" as fs;

fn unwrap_read(path: str) -> str {{
    let loaded: result[str, str] = fs.try_read_text(path);
    match (loaded) {{
        ok(body) => {{
            return body;
        }}
        err(message) => {{
            return message;
        }}
    }}
}}

unwrap_read("{}");
"#,
            data.display()
        ),
    )
    .unwrap();

    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    let value = runtime.eval_file(&script).unwrap();
    assert_eq!(value, Value::string("module-ok"));
}

#[test]
fn runtime_std_fs_try_read_text_returns_err_for_missing_file() {
    let dir = std::env::temp_dir().join(format!(
        "nox-std-fs-try-missing-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let missing = dir.join("missing.txt");
    let script = dir.join("main.nox");
    fs::write(
        &script,
        format!(
            r#"import "std/fs.nox" as fs;

fn describe_read(path: str) -> str {{
    let loaded: result[str, str] = fs.try_read_text(path);
    match (loaded) {{
        ok(body) => {{
            return body;
        }}
        err(message) => {{
            return message;
        }}
    }}
}}

describe_read("{}");
"#,
            missing.display()
        ),
    )
    .unwrap();

    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    let value = runtime.eval_file(&script).unwrap();
    let Value::String(message) = value else {
        panic!("expected string error message");
    };
    assert!(message.contains("failed to read"), "{message}");
    assert!(message.contains("missing.txt"), "{message}");
}

#[test]
fn std_module_import_does_not_grant_runtime_permissions() {
    let dir = std::env::temp_dir().join(format!(
        "nox-std-permission-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("main.nox");
    fs::write(
        &script,
        "import \"std/env.nox\" as env;\n\nenv.get(\"NOX_MISSING_PERMISSION\");\n",
    )
    .unwrap();

    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    let err = runtime.eval_file(&script).unwrap_err();
    assert!(err.message.contains("environment capability is required"));
}

#[test]
fn std_env_try_get_returns_option_when_allowed() {
    let dir = std::env::temp_dir().join(format!(
        "nox-std-env-try-get-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("main.nox");
    fs::write(
        &script,
        r#"
            import "std/env.nox" as env;

            let path: option[str] = env.try_get("PATH");
            match (path) {
                some(value) => {
                    "some";
                }
                none => {
                    "none";
                }
            }

            let missing: option[str] = env.try_get("__NOX_TEST_MISSING_ENV__");
            missing;
            "#,
    )
    .unwrap();

    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        filesystem: true,
        environment: true,
        ..RuntimePermissions::none()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime.eval_file(&script).unwrap();
    assert_eq!(value.to_string(), "none");
}

#[test]
fn std_env_try_get_requires_environment_capability() {
    let dir = std::env::temp_dir().join(format!(
        "nox-std-env-try-get-permission-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("main.nox");
    fs::write(
        &script,
        r#"
            import "std/env.nox" as env;

            env.try_get("PATH");
            "#,
    )
    .unwrap();

    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    let err = runtime.eval_file(&script).unwrap_err();
    assert!(err.message.contains("environment capability"));
}

#[test]
fn session_and_runtime_can_coexist_without_permission_leakage() {
    let mut session = Session::new();
    session
        .engine_mut()
        .register_host_function(HostFunctionBuilder::new("host_value", Type::Int), |_| {
            Ok(Value::Int(21))
        })
        .unwrap();
    session.set_module_loader(|specifier| {
        if specifier == "math.nox" {
            Ok("fn double(value: int) -> int { return value * 2; }\n".to_string())
        } else {
            Err(Diagnostic::new(
                format!("session module '{specifier}' not found"),
                Span { start: 0, end: 0 },
            ))
        }
    });

    assert_eq!(
        session
            .eval("import \"math.nox\";\n\ndouble(host_value());\n")
            .unwrap(),
        Value::Int(42)
    );

    let dir = std::env::temp_dir().join(format!(
        "nox-session-runtime-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let data = dir.join("message.txt");
    fs::write(&data, "runtime-ok").unwrap();
    let script = dir.join("main.nox");
    fs::write(
        &script,
        format!(
            "import \"std/fs.nox\" as fs;\n\nfs.read_text(\"{}\");\n",
            data.display()
        ),
    )
    .unwrap();

    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    assert_eq!(
        runtime.eval_file(&script).unwrap(),
        Value::string("runtime-ok")
    );

    let err = session
        .eval("import \"std/fs.nox\" as fs;\n\nfs.exists(\"message.txt\");\n")
        .unwrap_err();
    assert!(
        err.message
            .contains("session module 'std/fs.nox' not found"),
        "{}",
        err.message
    );
}

#[test]
fn environment_stdlib_requires_capability() {
    let mut runtime = Runtime::new();
    let err = runtime.eval(r#"env_get("PATH");"#).unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("environment capability"));
}

#[test]
fn environment_stdlib_reads_when_allowed() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        filesystem: true,
        environment: true,
        ..RuntimePermissions::none()
    });
    let value = runtime.eval(r#"env_get("PATH");"#).unwrap();
    assert!(matches!(value, Value::String(_)));
}

#[test]
fn env_list_requires_capability() {
    let mut runtime = Runtime::new();
    let err = runtime.eval("env_list();").unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("environment capability"));
}

#[test]
fn env_list_returns_environment_map_when_allowed() {
    let _guard = env_test_lock();
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        environment: true,
        ..RuntimePermissions::none()
    });
    let value = runtime.eval(r#"contains(env_list(), "PATH");"#).unwrap();
    assert_eq!(value, Value::Bool(true));
}

#[cfg(unix)]
#[test]
fn environment_non_utf8_values_are_diagnostics() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let _guard = env_test_lock();
    let key = format!("NOX_NON_UTF8_ENV_{}_{}", std::process::id(), line!());
    let previous = env::var_os(&key);
    unsafe {
        env::set_var(&key, OsString::from_vec(vec![0xff]));
    }

    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        filesystem: true,
        environment: true,
        ..RuntimePermissions::none()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());

    let env_get_message = runtime
        .eval(&format!(r#"env_get("{key}");"#))
        .unwrap_err()
        .message;

    let try_get_message = runtime
        .eval(&format!(
            r#"import "std/env.nox" as env; env.try_get("{key}");"#
        ))
        .unwrap_err()
        .message;

    match previous {
        Some(value) => unsafe { env::set_var(&key, value) },
        None => unsafe { env::remove_var(&key) },
    }

    assert!(
        env_get_message.contains("failed to read environment variable"),
        "{}",
        env_get_message
    );
    assert!(env_get_message.contains(&key), "{}", env_get_message);
    assert!(
        try_get_message.contains("failed to read environment variable"),
        "{}",
        try_get_message
    );
    assert!(try_get_message.contains(&key), "{}", try_get_message);
}

#[cfg(unix)]
#[test]
fn env_list_reports_non_utf8_values_without_panicking() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let _guard = env_test_lock();
    let key = format!("NOX_NON_UTF8_LIST_ENV_{}_{}", std::process::id(), line!());
    let previous = env::var_os(&key);
    unsafe {
        env::set_var(&key, OsString::from_vec(vec![0xfe]));
    }

    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        environment: true,
        ..RuntimePermissions::none()
    });
    let message = runtime.eval("env_list();").unwrap_err().message;

    match previous {
        Some(value) => unsafe { env::set_var(&key, value) },
        None => unsafe { env::remove_var(&key) },
    }

    assert!(message.contains("failed to read environment variable"));
    assert!(message.contains(&key));
}

#[test]
fn args_defaults_to_empty_array() {
    let mut runtime = Runtime::new();
    let value = runtime.eval("len(args());").unwrap();
    assert_eq!(value, Value::Int(0));
}

#[test]
fn args_returns_injected_arguments_without_permission() {
    let mut runtime = Runtime::new();
    runtime.set_args(vec!["alpha".to_string(), "beta".to_string()]);
    let value = runtime.eval(r#"args()[0] + ":" + args()[1];"#).unwrap();
    assert_eq!(value, Value::string("alpha:beta"));
}

#[test]
fn timer_stdlib_requires_capability() {
    let mut runtime = Runtime::new();
    let err = runtime.eval("sleep_ms(0);").unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("timer capability"));
}

#[test]
fn timer_stdlib_runs_when_allowed() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        timers: true,
        ..RuntimePermissions::none()
    });
    let value = runtime.eval("sleep_ms(0);").unwrap();
    assert_eq!(value, Value::Null);
}

#[test]
fn network_stdlib_requires_capability() {
    let mut runtime = Runtime::new();
    let err = runtime.eval(r#"tcp_connect("127.0.0.1", 1);"#).unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("network capability"));
}

#[test]
fn network_stdlib_validates_port_when_allowed() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        network: true,
        ..RuntimePermissions::none()
    });
    let err = runtime
        .eval(r#"tcp_connect("127.0.0.1", 70000);"#)
        .unwrap_err();
    assert!(err.message.contains("integer port"));
}

#[test]
fn network_stdlib_reports_loopback_connectivity_when_allowed() {
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let accept = thread::spawn(move || listener.accept().map(|_| ()).unwrap());

    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        network: true,
        ..RuntimePermissions::none()
    });
    let value = runtime
        .eval(&format!(r#"tcp_connect("127.0.0.1", {port});"#))
        .unwrap();

    assert_eq!(value, Value::Bool(true));
    accept.join().unwrap();
}

#[test]
fn network_stdlib_returns_false_for_refused_loopback_when_allowed() {
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        network: true,
        ..RuntimePermissions::none()
    });
    let value = runtime
        .eval(&format!(r#"tcp_connect("127.0.0.1", {port});"#))
        .unwrap();

    assert_eq!(value, Value::Bool(false));
}

#[test]
fn async_task_stdlib_requires_capability() {
    let mut runtime = Runtime::new();
    let err = runtime.eval("task_sleep_ms(0);").unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("async task capability"));
}

#[test]
fn async_task_stdlib_spawns_and_polls_when_allowed() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    assert_eq!(runtime.pending_async_task_count(), 0);
    let value = runtime
        .eval(
            r#"
                let task: int = task_sleep_ms(0);
                task_ready(task);
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Bool(true));
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_await_sleep_task_uses_runtime_task_table() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    runtime.set_runtime_trace_enabled(true);
    let value = runtime
        .eval(
            r#"
                async fn pause() -> int {
                    await task_sleep(0);
                    return 42;
                }

                let task: task[int] = pause();
                7;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(7));
    assert_eq!(runtime.pending_async_task_count(), 0);
    let events = runtime.take_runtime_trace_events();
    assert!(events.iter().any(|event| {
        event.event == "task"
            && matches!(
                event.fields.get("operation"),
                Some(RuntimeTraceValue::String(operation)) if operation == "await"
            )
    }));
}

#[test]
fn async_await_std_task_sleep_wrapper_uses_runtime_task_table() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/task.nox" as task;

                async fn pause() -> int {
                    await task.sleep(0);
                    return 42;
                }

                let pending: task[int] = pause();
                9;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Int(9));
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_task_stdlib_delay_and_join_helpers_compose_awaitable_tasks() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_stdout(true);
    let value = runtime
        .eval(
            r#"
                import "std/task.nox" as task;

                async fn run() -> null {
                    let pair: (int, str) = await task.join2(task.delay(0, 7), task.delay(0, "joined"));
                    let (count, label) = pair;
                    let triple: (int, str, bool) = await task.join3(task.delay(0, 1), task.delay(0, "three"), task.delay(0, true));
                    let (left, middle, right) = triple;
                    if (count == 7 && label == "joined" && left == 1 && middle == "three" && right) {
                        print("task-combinators-ok");
                    } else {
                        print("task-combinators-bad");
                    }
                    return null;
                }

                let ignored: task[null] = run();
                null;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Null);
    assert_eq!(runtime.take_stdout(), "task-combinators-ok\n");
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_task_stdlib_map_and_then_compose_awaitable_tasks() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_stdout(true);
    let value = runtime
        .eval(
            r#"
                import "std/task.nox" as task;

                fn double(value: int) -> int {
                    return value * 2;
                }

                fn label(value: int) -> task[str] {
                    return task.delay(0, "mapped-${value}");
                }

                async fn run() -> null {
                    let doubled: int = await task.map(task.delay(0, 21), double);
                    let chained: str = await task.and_then(task.delay(0, doubled), label);
                    if (doubled == 42 && chained == "mapped-42") {
                        print("task-map-and-then-ok");
                    } else {
                        print("task-map-and-then-bad");
                    }
                    return null;
                }

                let ignored: task[null] = run();
                null;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Null);
    assert_eq!(runtime.take_stdout(), "task-map-and-then-ok\n");
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn task_stdlib_join_helpers_do_not_require_async_permission_for_ready_tasks() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_stdout(true);
    let value = runtime
        .eval(
            r#"
                import "std/task.nox" as task;

                async fn ready_int() -> int {
                    return 3;
                }

                async fn ready_str() -> str {
                    return "ready";
                }

                async fn run() -> null {
                    let pair: (int, str) = await task.join2(ready_int(), ready_str());
                    let (count, label) = pair;
                    if (count == 3 && label == "ready") {
                        print("ready-join-ok");
                    } else {
                        print("ready-join-bad");
                    }
                    return null;
                }

                let ignored: task[null] = run();
                null;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Null);
    assert_eq!(runtime.take_stdout(), "ready-join-ok\n");
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn task_stdlib_map_and_then_do_not_require_async_permission_for_ready_tasks() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    runtime.set_mock_stdout(true);
    let value = runtime
        .eval(
            r#"
                import "std/task.nox" as task;

                async fn ready_int() -> int {
                    return 5;
                }

                fn inc(value: int) -> int {
                    return value + 1;
                }

                fn ready_label(value: int) -> task[str] {
                    return label(value);
                }

                async fn label(value: int) -> str {
                    return "ready-${value}";
                }

                async fn run() -> null {
                    let mapped: int = await task.map(ready_int(), inc);
                    let chained: str = await task.and_then(ready_int(), ready_label);
                    if (mapped == 6 && chained == "ready-5") {
                        print("ready-map-and-then-ok");
                    } else {
                        print("ready-map-and-then-bad");
                    }
                    return null;
                }

                let ignored: task[null] = run();
                null;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Null);
    assert_eq!(runtime.take_stdout(), "ready-map-and-then-ok\n");
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_await_sleep_task_requires_capability() {
    let mut runtime = Runtime::new();
    let err = runtime
        .eval(
            r#"
                async fn pause() -> null {
                    await task_sleep(0);
                    return null;
                }

                let task: task[null] = pause();
                0;
                "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("async task capability"));
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_await_sleep_task_pending_cap_cleans_created_tasks() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        async_task_max_pending: Some(1),
        ..RuntimePermissions::none()
    });
    let err = runtime
        .eval(
            r#"
                async fn spawn_two() -> null {
                    let first: task[null] = task_sleep(60000);
                    let second: task[null] = task_sleep(60000);
                    return null;
                }

                let task: task[null] = spawn_two();
                0;
                "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "runtime.task-pending-cap");
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_await_sleep_task_failed_eval_cleans_unawaited_tasks_created_by_that_eval() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });

    let err = runtime
        .eval(
            r#"
                async fn fail_after_spawn() -> null {
                    let pending: task[null] = task_sleep(60000);
                    task_sleep(-1);
                    return null;
                }

                let task: task[null] = fail_after_spawn();
                0;
                "#,
        )
        .unwrap_err();

    assert!(err.message.contains("non-negative duration"));
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_await_sleep_task_failed_eval_preserves_preexisting_tasks() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });

    runtime.eval("task_sleep_ms(60000);").unwrap();
    assert_eq!(runtime.pending_async_task_count(), 1);

    let err = runtime
        .eval(
            r#"
                async fn fail_after_spawn() -> null {
                    let pending: task[null] = task_sleep(60000);
                    task_sleep(-1);
                    return null;
                }

                let task: task[null] = fail_after_spawn();
                0;
                "#,
        )
        .unwrap_err();

    assert!(err.message.contains("non-negative duration"));
    assert_eq!(runtime.pending_async_task_count(), 1);
}

#[test]
fn async_await_sleep_task_error_keeps_script_stack_frame() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });

    let err = runtime
        .eval(
            r#"
                async fn pause() -> null {
                    await task_sleep(-1);
                    return null;
                }

                let task: task[null] = pause();
                0;
                "#,
        )
        .unwrap_err();

    assert!(err.message.contains("non-negative duration"));
    let frames = err
        .stack_frames
        .iter()
        .map(|frame| frame.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(frames, vec!["task_sleep", "pause"]);
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_task_rust_api_spawns_polls_and_cancels_sleep_tasks() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    let ready = runtime.spawn_sleep_task(Duration::from_millis(0)).unwrap();
    assert_eq!(
        runtime.poll_async_task(ready).unwrap(),
        AsyncTaskPoll::Ready
    );
    assert_eq!(runtime.pending_async_task_count(), 0);

    let pending = runtime
        .spawn_sleep_task(Duration::from_millis(60_000))
        .unwrap();
    assert_eq!(
        runtime.poll_async_task(pending).unwrap(),
        AsyncTaskPoll::Pending
    );
    assert_eq!(runtime.pending_async_task_count(), 1);
    runtime.cancel_async_task(pending).unwrap();
    assert_eq!(runtime.pending_async_task_count(), 0);
    let err = runtime.poll_async_task(pending).unwrap_err();
    assert!(err.message.contains("unknown async task id"));
}

#[test]
fn async_task_rust_api_preserves_host_created_tasks_after_eval_failure() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    let host_task = runtime
        .spawn_sleep_task(Duration::from_millis(60_000))
        .unwrap();
    let err = runtime
        .eval(
            r#"
                task_sleep_ms(60000);
                task_ready(999);
                "#,
        )
        .unwrap_err();
    assert!(err.message.contains("unknown async task id"));
    assert_eq!(runtime.pending_async_task_count(), 1);
    runtime.cancel_async_task(host_task).unwrap();
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_task_rust_api_respects_permissions_and_pending_cap() {
    let mut denied = Runtime::new();
    let err = denied
        .spawn_sleep_task(Duration::from_millis(0))
        .unwrap_err();
    assert_eq!(err.code, "permission.denied");

    let mut capped = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        async_task_max_pending: Some(1),
        ..RuntimePermissions::none()
    });
    capped
        .spawn_sleep_task(Duration::from_millis(60_000))
        .unwrap();
    let err = capped
        .spawn_sleep_task(Duration::from_millis(60_000))
        .unwrap_err();
    assert_eq!(err.code, "runtime.task-pending-cap");
    assert_eq!(capped.pending_async_task_count(), 1);
}

#[test]
fn async_task_ready_clears_completed_task_and_rejects_second_poll() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    let value = runtime
        .eval("let task: int = task_sleep_ms(0); task_ready(task);")
        .unwrap();
    assert_eq!(value, Value::Bool(true));
    assert_eq!(runtime.pending_async_task_count(), 0);
    let err = runtime
        .eval("let task: int = task_sleep_ms(0); task_ready(task); task_ready(task);")
        .unwrap_err();
    assert!(err.message.contains("unknown async task id"));
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_task_ready_can_be_polled_repeatedly_until_deadline() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    let value = runtime
        .eval(
            r#"
                let task: int = task_sleep_ms(60000);
                let first: bool = task_ready(task);
                let second: bool = task_ready(task);
                first;
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::Bool(false));
    assert_eq!(runtime.pending_async_task_count(), 1);
}

#[test]
fn async_task_sleep_respects_pending_task_cap() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        async_task_max_pending: Some(1),
        ..RuntimePermissions::none()
    });
    let err = runtime
        .eval(
            r#"
                task_sleep_ms(60000);
                task_sleep_ms(60000);
                "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "runtime.task-pending-cap");
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_task_ready_on_unknown_id_returns_diagnostic() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    let err = runtime.eval("task_ready(999);").unwrap_err();
    assert!(err.message.contains("unknown async task id"));
}

#[test]
fn async_task_cancel_releases_pending_task() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    let err = runtime
        .eval(
            r#"
                let task: int = task_sleep_ms(60000);
                task_cancel(task);
                task_ready(task);
                "#,
        )
        .unwrap_err();
    assert!(err.message.contains("unknown async task id"));
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_task_cancel_rejects_unknown_id() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    let err = runtime.eval("task_cancel(7);").unwrap_err();
    assert!(err.message.contains("unknown async task id"));
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn task_stdlib_wait_returns_true_when_sleep_completes() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/task.nox" as task;

                let id: int = task.sleep_ms(5);
                let finished: bool = task.wait(id);
                let remaining: int = task.pending_count();
                if (finished && remaining == 0) {
                    "task-wait-ok";
                } else {
                    "task-wait-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("task-wait-ok"));
}

#[test]
fn task_stdlib_wait_or_timeout_cancels_long_sleep() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let value = runtime
        .eval(
            r#"
                import "std/task.nox" as task;

                let id: int = task.sleep_ms(60000);
                let finished: bool = task.wait_or_timeout(id, 10);
                let remaining: int = task.pending_count();
                if (!finished && remaining == 0) {
                    "task-timeout-ok";
                } else {
                    "task-timeout-bad";
                }
                "#,
        )
        .unwrap();
    assert_eq!(value, Value::string("task-timeout-ok"));
}

#[test]
fn task_stdlib_requires_async_capability() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                import "std/task.nox" as task;
                task.sleep_ms(0);
                "#,
        )
        .unwrap_err();
    assert!(
        err.message.contains("async task capability"),
        "expected async task diagnostic, got: {}",
        err.message
    );
}

#[test]
fn async_task_lifecycle_releases_many_completed_and_cancelled_tasks() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });

    for _ in 0..100 {
        runtime
            .eval("let task: int = task_sleep_ms(0); task_ready(task);")
            .unwrap();
    }
    assert_eq!(runtime.pending_async_task_count(), 0);

    for _ in 0..100 {
        runtime
            .eval("let task: int = task_sleep_ms(60000); task_cancel(task);")
            .unwrap();
    }
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_task_failed_eval_cleans_tasks_created_by_that_eval() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });

    let err = runtime
        .eval(
            r#"
                let task: int = task_sleep_ms(60000);
                task_ready(999);
                "#,
        )
        .unwrap_err();

    assert!(err.message.contains("unknown async task id"));
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn async_task_failed_eval_preserves_preexisting_pending_tasks() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });

    runtime.eval("task_sleep_ms(60000);").unwrap();
    assert_eq!(runtime.pending_async_task_count(), 1);

    let err = runtime
        .eval(
            r#"
                let task: int = task_sleep_ms(60000);
                task_ready(999);
                "#,
        )
        .unwrap_err();

    assert!(err.message.contains("unknown async task id"));
    assert_eq!(runtime.pending_async_task_count(), 1);
}

#[test]
fn async_task_budget_exhaustion_cleans_tasks_created_by_that_eval() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    runtime.set_instruction_budget(Some(20));

    let err = runtime
        .eval(
            r#"
                let task: int = task_sleep_ms(60000);
                let value: int = 0;
                while (value < 100) {
                    value = value + 1;
                }
                task_ready(task);
                "#,
        )
        .unwrap_err();

    assert!(err.message.contains("instruction budget exhausted"));
    assert_eq!(runtime.pending_async_task_count(), 0);
}

#[test]
fn runtime_checks_and_inspects_files() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    let example = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("examples/hello.nox");
    runtime.check_file(&example).unwrap();
    let bytecode = runtime.inspect_bytecode_file(&example).unwrap();
    assert!(bytecode.contains("Function"));
    assert!(bytecode.contains("double"));
}

#[test]
fn file_evaluation_requires_filesystem_capability() {
    let mut runtime = Runtime::new();
    let err = runtime.eval_file("examples/hello.nox").unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("filesystem capability"));
}

#[test]
fn read_text_requires_filesystem_capability() {
    let mut runtime = Runtime::new();
    let err = runtime.eval(r#"read_text("none.txt");"#).unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("filesystem capability"));
}

#[test]
fn std_fs_try_read_text_requires_filesystem_capability() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(r#"import "std/fs.nox" as fs; fs.try_read_text("none.txt");"#)
        .unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(
        err.message.contains("filesystem capability"),
        "{}",
        err.message
    );
}

#[test]
fn question_mark_does_not_capture_permission_diagnostics_as_result_err() {
    let mut runtime = Runtime::new();
    runtime.set_import_base(std::env::temp_dir(), Vec::new());
    let err = runtime
        .eval(
            r#"
                import "std/fs.nox" as fs;

                fn load() -> result[str, str] {
                    let value: str = fs.try_read_text("none.txt")?;
                    return ok(value);
                }

                load();
                "#,
        )
        .unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(
        err.message.contains("filesystem capability"),
        "{}",
        err.message
    );
}

#[test]
fn read_text_reads_existing_file() {
    let dir = std::env::temp_dir().join(format!("nox-rt-read-{}-{}", std::process::id(), line!()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("hello.txt");
    fs::write(&file, "ok").unwrap();
    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    let source = format!(r#"read_text("{}");"#, file.display());
    let value = runtime.eval(&source).unwrap();
    assert_eq!(value, Value::string("ok"));
}

#[test]
fn exists_reports_presence_under_read_capability() {
    let dir =
        std::env::temp_dir().join(format!("nox-rt-exists-{}-{}", std::process::id(), line!()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("there.txt");
    fs::write(&file, "x").unwrap();
    let missing = dir.join("nope.txt");

    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    let value = runtime
        .eval(&format!(r#"exists("{}");"#, file.display()))
        .unwrap();
    assert_eq!(value, Value::Bool(true));
    let value = runtime
        .eval(&format!(r#"exists("{}");"#, missing.display()))
        .unwrap();
    assert_eq!(value, Value::Bool(false));
}

#[test]
fn filesystem_read_allowlist_allows_inside_and_denies_escape() {
    let dir = std::env::temp_dir().join(format!(
        "nox-rt-read-allow-{}-{}",
        std::process::id(),
        line!()
    ));
    let allowed = dir.join("allowed");
    fs::create_dir_all(&allowed).unwrap();
    let inside = allowed.join("inside.txt");
    let outside = dir.join("outside.txt");
    fs::write(&inside, "inside").unwrap();
    fs::write(&outside, "outside").unwrap();

    let mut runtime =
        Runtime::with_permissions(RuntimePermissions::none().allow_filesystem_read_under(&allowed));
    let value = runtime
        .eval(&format!(r#"read_text("{}");"#, inside.display()))
        .unwrap();
    assert_eq!(value, Value::string("inside"));

    let escaped = allowed.join("../outside.txt");
    let err = runtime
        .eval(&format!(r#"read_text("{}");"#, escaped.display()))
        .unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("filesystem read permission denied"));
}

#[test]
fn std_fs_try_read_text_denies_allowlist_escape() {
    let dir = std::env::temp_dir().join(format!(
        "nox-rt-try-read-allow-{}-{}",
        std::process::id(),
        line!()
    ));
    let allowed = dir.join("allowed");
    fs::create_dir_all(&allowed).unwrap();
    let outside = dir.join("outside.txt");
    fs::write(&outside, "outside").unwrap();

    let mut runtime =
        Runtime::with_permissions(RuntimePermissions::none().allow_filesystem_read_under(&allowed));
    runtime.set_import_base(allowed.clone(), Vec::new());
    let escaped = allowed.join("../outside.txt");
    let err = runtime
        .eval(&format!(
            r#"import "std/fs.nox" as fs; fs.try_read_text("{}");"#,
            escaped.display()
        ))
        .unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(
        err.message.contains("filesystem read permission denied"),
        "{}",
        err.message
    );
}

#[test]
fn filesystem_read_allowlist_reports_missing_inside_but_denies_outside_exists() {
    let dir = std::env::temp_dir().join(format!(
        "nox-rt-read-missing-{}-{}",
        std::process::id(),
        line!()
    ));
    let allowed = dir.join("allowed");
    fs::create_dir_all(&allowed).unwrap();
    let missing_inside = allowed.join("missing.txt");
    let missing_outside = dir.join("missing.txt");

    let mut runtime =
        Runtime::with_permissions(RuntimePermissions::none().allow_filesystem_read_under(&allowed));
    let value = runtime
        .eval(&format!(r#"exists("{}");"#, missing_inside.display()))
        .unwrap();
    assert_eq!(value, Value::Bool(false));

    let err = runtime
        .eval(&format!(r#"exists("{}");"#, missing_outside.display()))
        .unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("filesystem read permission denied"));
}

#[test]
fn filesystem_empty_paths_are_invalid() {
    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    let err = runtime.eval(r#"read_text("");"#).unwrap_err();
    assert!(err.message.contains("invalid filesystem path"));
}

#[test]
fn write_text_requires_distinct_write_capability() {
    let dir = std::env::temp_dir().join(format!(
        "nox-rt-write-deny-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("out.txt");

    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    let err = runtime
        .eval(&format!(r#"write_text("{}", "hi");"#, target.display()))
        .unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("filesystem write capability"));
    assert!(!target.exists());
}

#[test]
fn write_text_writes_when_allowed() {
    let dir = std::env::temp_dir().join(format!(
        "nox-rt-write-ok-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("out.txt");

    let mut runtime = Runtime::with_permissions(RuntimePermissions {
        filesystem: true,
        filesystem_write: true,
        ..RuntimePermissions::none()
    });
    let value = runtime
        .eval(&format!(r#"write_text("{}", "stored");"#, target.display()))
        .unwrap();
    assert_eq!(value, Value::Null);
    assert_eq!(fs::read_to_string(&target).unwrap(), "stored");
}

#[test]
fn filesystem_write_allowlist_allows_inside_and_denies_escape() {
    let dir = std::env::temp_dir().join(format!(
        "nox-rt-write-allow-{}-{}",
        std::process::id(),
        line!()
    ));
    let allowed = dir.join("allowed");
    fs::create_dir_all(&allowed).unwrap();
    let inside = allowed.join("out.txt");
    let escaped = allowed.join("../outside.txt");

    let mut runtime = Runtime::with_permissions(
        RuntimePermissions::none().allow_filesystem_write_under(&allowed),
    );
    let value = runtime
        .eval(&format!(r#"write_text("{}", "stored");"#, inside.display()))
        .unwrap();
    assert_eq!(value, Value::Null);
    assert_eq!(fs::read_to_string(&inside).unwrap(), "stored");

    let err = runtime
        .eval(&format!(
            r#"write_text("{}", "outside");"#,
            escaped.display()
        ))
        .unwrap_err();
    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("filesystem write permission denied"));
    assert!(!dir.join("outside.txt").exists());
}

#[cfg(unix)]
#[test]
fn filesystem_write_allowlist_denies_missing_file_under_symlink_escape() {
    use std::os::unix::fs::symlink;

    let dir = std::env::temp_dir().join(format!(
        "nox-rt-write-symlink-{}-{}",
        std::process::id(),
        line!()
    ));
    let allowed = dir.join("allowed");
    let outside = dir.join("outside");
    fs::create_dir_all(&allowed).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let link = allowed.join("link-out");
    symlink(&outside, &link).unwrap();
    let target = link.join("created.txt");

    let mut runtime = Runtime::with_permissions(
        RuntimePermissions::none().allow_filesystem_write_under(&allowed),
    );
    let err = runtime
        .eval(&format!(
            r#"write_text("{}", "outside");"#,
            target.display()
        ))
        .unwrap_err();

    assert_eq!(err.code, "permission.denied");
    assert!(err.message.contains("filesystem write permission denied"));
    assert!(!outside.join("created.txt").exists());
}
