use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn nox_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nox"))
}

#[test]
fn version_prints_package_version() {
    let output = nox_command().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("nox {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}

fn lsp_frame(body: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{body}", body.len())
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn lsp_position(source: &str, byte_offset: usize) -> (usize, usize) {
    let prefix = &source[..byte_offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let character = prefix
        .rsplit_once('\n')
        .map(|(_, tail)| tail.len())
        .unwrap_or(prefix.len());
    (line, character)
}

#[test]
fn run_prints_final_value() {
    for (path, expected) in [
        ("examples/arrays.nox", "40\n"),
        ("examples/bench-containers.nox", "containers-ok\n"),
        ("examples/bench-fib.nox", "fib-ok\n"),
        ("examples/bench-loop.nox", "loop-ok\n"),
        ("examples/bench-modules.nox", "modules-ok\n"),
        ("examples/hello.nox", "84\n"),
        ("examples/control-flow.nox", "sum-ok\n"),
        ("examples/constants.nox", "const-ok\n"),
        ("examples/conversions.nox", "42\n"),
        ("examples/else-if.nox", "mid\n"),
        ("examples/export-main.nox", "42\n"),
        ("examples/for-range.nox", "10\n"),
        ("examples/logical.nox", "logic-ok\n"),
        ("examples/loop-break-continue.nox", "loop-ok\n"),
        ("examples/maps.nox", "42\n"),
        ("examples/match.nox", "two-2\n"),
        ("examples/numeric-boundaries.nox", "numeric-ok\n"),
        ("examples/recursion.nox", "21\n"),
        ("examples/records.nox", "42\n"),
        ("examples/scopes.nox", "10\n"),
        ("examples/string-escapes.nox", "escape-ok\n"),
        ("examples/string-and-map-builtins.nox", "builtins-ok\n"),
        ("examples/strings.nox", "nox:typed\n"),
        ("examples/stdlib.nox", "sqrt-ok\n"),
    ] {
        let output = nox_command()
            .args(["run", fixture(path).to_str().unwrap()])
            .output()
            .unwrap();

        assert!(output.status.success(), "{path}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{path}");
        assert!(output.stderr.is_empty(), "{path}");
    }
}

#[test]
fn run_passes_script_arguments_to_args_builtin() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-run-args-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("args.nox");
    fs::write(
        &path,
        r#"fn joined() -> str {
    if (len(args()) == 2) {
        return args()[0] + ":" + args()[1];
    }
    return "bad";
}

joined();
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["run", path.to_str().unwrap(), "alpha", "beta"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "alpha:beta\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn check_reports_ok_without_running() {
    for path in [
        "examples/arrays.nox",
        "examples/hello.nox",
        "examples/control-flow.nox",
        "examples/constants.nox",
        "examples/conversions.nox",
        "examples/else-if.nox",
        "examples/export-main.nox",
        "examples/for-range.nox",
        "examples/logical.nox",
        "examples/loop-break-continue.nox",
        "examples/maps.nox",
        "examples/match.nox",
        "examples/numeric-boundaries.nox",
        "examples/recursion.nox",
        "examples/records.nox",
        "examples/scopes.nox",
        "examples/string-escapes.nox",
        "examples/string-and-map-builtins.nox",
        "examples/strings.nox",
        "examples/stdlib.nox",
    ] {
        let path = fixture(path);
        let output = nox_command()
            .args(["check", path.to_str().unwrap()])
            .output()
            .unwrap();

        assert!(output.status.success(), "{}", path.display());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("{}: ok\n", path.display())
        );
        assert!(output.stderr.is_empty(), "{}", path.display());
    }
}

#[test]
fn check_accepts_multiple_files() {
    let first = fixture("examples/hello.nox");
    let second = fixture("examples/records.nox");
    let output = nox_command()
        .args(["check", first.to_str().unwrap(), second.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("{}: ok", first.display())));
    assert!(stdout.contains(&format!("{}: ok", second.display())));
    assert!(output.stderr.is_empty());
}

#[test]
fn check_json_reports_success_without_human_ok_lines() {
    let path = fixture("examples/hello.nox");
    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!(
            "{{\"schema\":\"nox.check.v1\",\"ok\":true,\"diagnostics\":[],\"files\":[{{\"path\":\"{}\",\"ok\":true,\"diagnostic_count\":0}}],\"summary\":{{\"checked\":1,\"passed\":1,\"failed\":0,\"diagnostic_count\":0}}}}\n",
            path.display()
        )
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn check_json_reports_all_failures_on_stdout() {
    let first = fixture("examples/type-error.nox");
    let second = fixture("examples/type-error-record-field-access.nox");
    let output = nox_command()
        .args([
            "check",
            "--json",
            first.to_str().unwrap(),
            second.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("{\"schema\":\"nox.check.v1\",\"ok\":false,\"diagnostics\":["));
    assert!(stdout.contains(&format!("\"file\":\"{}\"", first.display())));
    assert!(stdout.contains(&format!("\"file\":\"{}\"", second.display())));
    assert!(stdout.contains(&format!(
        "\"files\":[{{\"path\":\"{}\",\"ok\":false,\"diagnostic_count\":2}},{{\"path\":\"{}\",\"ok\":false,\"diagnostic_count\":1}}]",
        first.display(),
        second.display()
    )));
    assert!(stdout
        .contains("\"summary\":{\"checked\":2,\"passed\":0,\"failed\":2,\"diagnostic_count\":3}"));
    assert!(stdout.contains("\"code\":\"type.mismatch\""));
    assert!(stdout.contains("\"message\":\"expected int, got str\""));
    assert!(stdout.contains("\"message\":\"record 'User' has no field 'score'\""));
    assert!(stdout.contains("\"span\":{\"start\":"));
    assert!(stdout.contains("\"source\":{\"name\":"));
}

#[test]
fn check_json_reports_parser_code() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-parse-code-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("parse.nox");
    fs::write(&path, "let value = 1;\n").unwrap();

    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"parse.expected-token\""));
    assert!(stdout.contains(&format!("\"file\":\"{}\"", path.display())));
    assert!(stdout.contains("\"span\":{\"start\":"));
    assert!(stdout.contains("\"source\":{\"name\":"));
}

#[test]
fn check_json_and_lsp_report_matching_precise_ranges() {
    let dir = std::env::temp_dir().join(format!("nox-cli-range-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("range.nox");
    let source = "let values: [int] = [1, 2];\nvalues[0.0];\n";
    fs::write(&path, source).unwrap();

    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"type.mismatch\""));
    assert!(stdout.contains("\"message\":\"expected int, got float\""));
    assert!(stdout.contains("\"source\":{\"name\":"));
    assert!(stdout.contains("\"line\":2,\"column\":8"));
    assert!(stdout
        .contains("\"summary\":{\"checked\":1,\"passed\":0,\"failed\":1,\"diagnostic_count\":1}"));

    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///range.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"type.mismatch\""));
    assert!(stdout.contains(
        "\"range\":{\"start\":{\"line\":1,\"character\":7},\"end\":{\"line\":1,\"character\":10}}"
    ));
}

#[test]
fn check_json_and_lsp_report_multiple_type_errors_in_one_file() {
    let dir = std::env::temp_dir().join(format!("nox-cli-multi-type-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("multi-type.nox");
    let source = "let first: int = \"bad\";\nlet second: bool = 1;\n";
    fs::write(&path, source).unwrap();

    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert_eq!(stdout.matches("\"code\":\"type.mismatch\"").count(), 2);
    assert!(stdout.contains("\"message\":\"expected int, got str\""));
    assert!(stdout.contains("\"message\":\"expected bool, got int\""));
    assert!(stdout
        .contains("\"summary\":{\"checked\":1,\"passed\":0,\"failed\":1,\"diagnostic_count\":2}"));

    let output = nox_command()
        .args(["check", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("expected int, got str"));
    assert!(stderr.contains("expected bool, got int"));

    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///multi-type.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.matches("\"code\":\"type.mismatch\"").count(), 2);
    assert!(stdout.contains("\"message\":\"expected int, got str\""));
    assert!(stdout.contains("\"message\":\"expected bool, got int\""));
}

#[test]
fn check_json_and_lsp_report_parser_code() {
    let source = "let value = 1;\n";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///parse-code.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"method\":\"textDocument/publishDiagnostics\""));
    assert!(stdout.contains("\"uri\":\"file:///parse-code.nox\""));
    assert!(stdout.contains("\"code\":\"parse.expected-token\""));
}

#[test]
fn check_multiple_files_reports_all_failures() {
    let first = fixture("examples/type-error.nox");
    let second = fixture("examples/type-error-record-field-access.nox");
    let output = nox_command()
        .args(["check", first.to_str().unwrap(), second.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("expected int, got str"));
    assert!(stderr.contains("record 'User' has no field 'score'"));
}

#[test]
fn check_json_and_lsp_report_module_member_code() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-member-code-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.nox");
    let helper = dir.join("helper.nox");
    fs::write(&helper, "export fn answer() -> int {\n    return 42;\n}\n").unwrap();
    let source = "import \"helper.nox\" as helper;\n\nhelper.missing();\n";
    fs::write(&main, source).unwrap();

    let output = nox_command()
        .args(["check", "--json", main.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"module.member-not-found\""));
    assert!(stdout.contains(&format!("\"file\":\"{}\"", main.display())));
    assert!(stdout.contains("\"span\":{\"start\":"));

    let uri = format!("file://{}", main.display());
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(&uri),
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"method\":\"textDocument/publishDiagnostics\""));
    assert!(stdout.contains("\"code\":\"module.member-not-found\""));
}

#[test]
fn check_json_reports_module_name_conflicts() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-module-conflict-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.nox");
    let helper = dir.join("helper.nox");
    fs::write(&helper, "export fn answer() -> int {\n    return 42;\n}\n").unwrap();
    fs::write(
        &main,
        "import \"helper.nox\";\n\nlet answer: int = 1;\nanswer;\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["check", "--json", main.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"module.name-conflict\""));
    assert!(stdout.contains("\"message\":\"name 'answer' redeclared\""));
}

#[test]
fn check_without_paths_uses_manifest_project_files() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-check-manifest-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::create_dir_all(dir.join("other")).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.2\"\n\n[entrypoints]\nmain = \"src/main.nox\"\n\n[modules]\nsource_dirs = [\"src\"]\ntest_dirs = [\"tests\"]\n",
    )
    .unwrap();
    fs::write(dir.join("src/main.nox"), "let value: int = 1;\nvalue;\n").unwrap();
    fs::write(
        dir.join("tests/math_test.nox"),
        "fn test_ok() -> bool {\n    return true;\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("other/ignored.nox"),
        "let value: str = 1;\nvalue;\n",
    )
    .unwrap();

    let output = nox_command()
        .current_dir(&dir)
        .args(["check", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"summary\":{\"checked\":2,\"passed\":2,\"failed\":0"));
    assert!(stdout.contains("src/main.nox"));
    assert!(stdout.contains("tests/math_test.nox"));
    assert!(!stdout.contains("other/ignored.nox"));
}

#[test]
fn check_without_paths_uses_manifest_main_when_no_dirs() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-check-manifest-main-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.2\"\n\n[entrypoints]\nmain = \"src/main.nox\"\n",
    )
    .unwrap();
    fs::write(dir.join("src/main.nox"), "let value: int = 1;\nvalue;\n").unwrap();
    fs::write(dir.join("src/ignored.nox"), "let value: str = 1;\nvalue;\n").unwrap();

    let output = nox_command()
        .current_dir(&dir)
        .arg("check")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("src/main.nox"));
    assert!(!stdout.contains("ignored.nox"));
}

#[test]
fn check_explicit_path_overrides_manifest_project_files() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-check-explicit-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("tools")).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.2\"\n\n[modules]\nsource_dirs = [\"src\"]\n",
    )
    .unwrap();
    fs::write(dir.join("src/bad.nox"), "let value: str = 1;\nvalue;\n").unwrap();
    let explicit = dir.join("tools/good.nox");
    fs::write(&explicit, "let value: int = 1;\nvalue;\n").unwrap();

    let output = nox_command()
        .current_dir(&dir)
        .args(["check", explicit.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tools/good.nox"));
    assert!(!stdout.contains("src/bad.nox"));
}

#[test]
fn test_runs_explicit_test_file() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-pass-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("math_test.nox");
    fs::write(
        &path,
        "fn test_add() -> bool {\n    return 1 + 1 == 2;\n}\n\nfn test_string() -> bool {\n    return len(\"nox\") == 3;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["test", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("{}::test_add PASS", path.display())));
    assert!(stdout.contains(&format!("{}::test_string PASS", path.display())));
    assert!(stdout.contains("summary: 2 tests, 2 passed, 0 failed"));
    assert!(output.stderr.is_empty());
}

#[test]
fn test_reports_bool_false_as_failure() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-false-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("math_test.nox");
    fs::write(
        &path,
        "fn test_add() -> bool {\n    return 1 + 1 == 3;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["test", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("{}::test_add FAIL", path.display())));
    assert!(stdout.contains("summary: 1 tests, 0 passed, 1 failed"));
    assert!(output.stderr.is_empty());
}

#[test]
fn test_reports_runtime_error_as_failure() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-runtime-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("runtime_test.nox");
    fs::write(
        &path,
        "fn test_division() -> bool {\n    let value: int = 1 / 0;\n    return value == 0;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["test", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("{}::test_division FAIL", path.display())));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("division by zero"));
}

#[test]
fn test_json_reports_results_on_stdout() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-json-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("json_test.nox");
    fs::write(
        &path,
        "fn test_pass() -> bool {\n    return true;\n}\n\nfn test_fail() -> bool {\n    return false;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("{\"schema\":\"nox.test.v1\",\"ok\":false,\"tests\":["));
    assert!(stdout.contains(&format!("\"file\":\"{}\"", path.display())));
    assert!(stdout.contains("\"name\":\"test_pass\",\"ok\":true,\"diagnostic\":null"));
    assert!(stdout.contains("\"name\":\"test_fail\",\"ok\":false,\"diagnostic\":null"));
    assert!(stdout.contains("\"summary\":{\"tests\":2,\"passed\":1,\"failed\":1}"));
}

#[test]
fn test_json_reports_runtime_diagnostic_code() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-runtime-json-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("runtime_test.nox");
    fs::write(
        &path,
        "fn test_division() -> bool {\n    let value: int = 1 / 0;\n    return value == 0;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.test.v1\""));
    assert!(stdout.contains("\"name\":\"test_division\""));
    assert!(stdout.contains("\"code\":\"runtime.division-by-zero\""));
    assert!(stdout.contains("\"span\":{\"start\":"));
    assert!(stdout.contains("\"source\":{\"name\":"));
}

#[test]
fn test_json_reports_permission_denied_code() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-permission-json-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permission_test.nox");
    fs::write(
        &path,
        "fn test_env() -> bool {\n    env_get(\"PATH\");\n    return true;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.test.v1\""));
    assert!(stdout.contains("\"name\":\"test_env\""));
    assert!(stdout.contains("\"code\":\"permission.denied\""));
    assert!(stdout.contains("environment capability is required to call env_get"));
}

#[test]
fn test_discovers_test_files_under_directory() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-dir-{}-{}",
        std::process::id(),
        line!()
    ));
    let nested = dir.join("nested");
    fs::create_dir_all(&nested).unwrap();
    let first = dir.join("first_test.nox");
    let second = nested.join("second_test.nox");
    fs::write(&first, "fn test_first() -> bool {\n    return true;\n}\n").unwrap();
    fs::write(&second, "fn test_second() -> bool {\n    return true;\n}\n").unwrap();
    fs::write(
        dir.join("helper.nox"),
        "fn test_ignored() -> bool {\n    return false;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["test", dir.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("{}::test_first PASS", first.display())));
    assert!(stdout.contains(&format!("{}::test_second PASS", second.display())));
    assert!(!stdout.contains("test_ignored"));
    assert!(stdout.contains("summary: 2 tests, 2 passed, 0 failed"));
}

#[test]
fn test_without_paths_uses_manifest_source_dirs() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-manifest-{}-{}",
        std::process::id(),
        line!()
    ));
    let src = dir.join("src");
    let other = dir.join("other");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&other).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"test-package\"\nversion = \"0.0.1\"\n\n[modules]\nsource_dirs = [\"src\"]\n",
    )
    .unwrap();
    let included = src.join("included_test.nox");
    fs::write(
        &included,
        "fn test_included() -> bool {\n    return true;\n}\n",
    )
    .unwrap();
    fs::write(
        other.join("ignored_test.nox"),
        "fn test_ignored() -> bool {\n    return false;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .arg("test")
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("{}::test_included PASS", included.display())));
    assert!(!stdout.contains("test_ignored"));
    assert!(stdout.contains("summary: 1 tests, 1 passed, 0 failed"));
}

#[test]
fn test_without_paths_prefers_manifest_test_dirs() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-manifest-test-dirs-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.2\"\n\n[modules]\nsource_dirs = [\"src\"]\ntest_dirs = [\"tests\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/ignored_test.nox"),
        "fn test_ignored() -> bool { return false; }\n",
    )
    .unwrap();
    fs::write(
        dir.join("tests/active_test.nox"),
        "fn test_active() -> bool { return true; }\n",
    )
    .unwrap();

    let output = nox_command()
        .current_dir(&dir)
        .arg("test")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("active_test.nox::test_active PASS"));
    assert!(!stdout.contains("ignored_test.nox"));
}

#[test]
fn run_without_path_uses_manifest_main() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-run-manifest-main-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.2\"\n\n[entrypoints]\nmain = \"src/main.nox\"\n",
    )
    .unwrap();
    fs::write(dir.join("src/main.nox"), "42;\n").unwrap();

    let output = nox_command().current_dir(&dir).arg("run").output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "42\n");
}

#[test]
fn sample_project_supports_project_workflow() {
    let project = fixture("examples/projects/scoreboard");

    let run = nox_command()
        .current_dir(&project)
        .arg("run")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "score:high\n");

    let check = nox_command()
        .current_dir(&project)
        .args(["check", "--json"])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"summary\":{\"checked\":6,\"passed\":6,\"failed\":0"));
    assert!(stdout.contains("src/main.nox"));
    assert!(stdout.contains("src/runtime_info.nox"));
    assert!(stdout.contains("src/scoring.nox"));
    assert!(stdout.contains("tests/scoring_test.nox"));
    assert!(stdout.contains("tests/runtime_info_test.nox"));

    let test = nox_command()
        .current_dir(&project)
        .arg("test")
        .output()
        .unwrap();
    assert!(
        test.status.success(),
        "{}",
        String::from_utf8_lossy(&test.stderr)
    );
    let stdout = String::from_utf8_lossy(&test.stdout);
    assert!(stdout.contains("scoring_test.nox::test_total PASS"));
    assert!(stdout.contains("scoring_test.nox::test_sqrt_bonus PASS"));
    assert!(stdout.contains("runtime_info_test.nox::test_manifest_present PASS"));
    assert!(stdout.contains("runtime_info_test.nox::test_manifest_result PASS"));
    assert!(stdout.contains("runtime_info_test.nox::test_optional_description PASS"));
    assert!(stdout.contains("summary: 5 tests, 5 passed, 0 failed"));

    let fmt = nox_command()
        .current_dir(&project)
        .args(["fmt", "--check"])
        .output()
        .unwrap();
    assert!(
        fmt.status.success(),
        "{}",
        String::from_utf8_lossy(&fmt.stderr)
    );
    assert!(fmt.stdout.is_empty());
}

#[test]
fn sample_project_manifest_defaults_match_explicit_paths() {
    let project = fixture("examples/projects/scoreboard")
        .canonicalize()
        .unwrap();
    let main = project.join("src/main.nox");
    let labels = project.join("src/labels.nox");
    let runtime_info = project.join("src/runtime_info.nox");
    let scoring = project.join("src/scoring.nox");
    let runtime_test = project.join("tests/runtime_info_test.nox");
    let scoring_test = project.join("tests/scoring_test.nox");

    let manifest_check = nox_command()
        .current_dir(&project)
        .args(["check", "--json"])
        .output()
        .unwrap();
    assert!(
        manifest_check.status.success(),
        "{}",
        String::from_utf8_lossy(&manifest_check.stderr)
    );
    let manifest_stdout = String::from_utf8_lossy(&manifest_check.stdout);
    assert!(manifest_stdout.contains("\"summary\":{\"checked\":6,\"passed\":6,\"failed\":0"));

    let explicit_check = nox_command()
        .current_dir(&project)
        .args([
            "check",
            "--json",
            main.to_str().unwrap(),
            labels.to_str().unwrap(),
            runtime_info.to_str().unwrap(),
            scoring.to_str().unwrap(),
            runtime_test.to_str().unwrap(),
            scoring_test.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        explicit_check.status.success(),
        "{}",
        String::from_utf8_lossy(&explicit_check.stderr)
    );
    let explicit_stdout = String::from_utf8_lossy(&explicit_check.stdout);
    assert!(explicit_stdout.contains("\"summary\":{\"checked\":6,\"passed\":6,\"failed\":0"));
    for path in [
        &main,
        &labels,
        &runtime_info,
        &scoring,
        &runtime_test,
        &scoring_test,
    ] {
        let path = path.display().to_string();
        assert!(
            manifest_stdout.contains(&path),
            "manifest check missing {path}"
        );
        assert!(
            explicit_stdout.contains(&path),
            "explicit check missing {path}"
        );
    }

    let manifest_test = nox_command()
        .current_dir(&project)
        .args(["test", "--json"])
        .output()
        .unwrap();
    assert!(
        manifest_test.status.success(),
        "{}",
        String::from_utf8_lossy(&manifest_test.stderr)
    );
    let manifest_test_stdout = String::from_utf8_lossy(&manifest_test.stdout);
    assert!(manifest_test_stdout.contains("\"summary\":{\"tests\":5,\"passed\":5,\"failed\":0}"));

    let explicit_test = nox_command()
        .current_dir(&project)
        .args([
            "test",
            "--json",
            runtime_test.to_str().unwrap(),
            scoring_test.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        explicit_test.status.success(),
        "{}",
        String::from_utf8_lossy(&explicit_test.stderr)
    );
    let explicit_test_stdout = String::from_utf8_lossy(&explicit_test.stdout);
    assert!(explicit_test_stdout.contains("\"summary\":{\"tests\":5,\"passed\":5,\"failed\":0}"));
    for name in [
        "test_manifest_present",
        "test_manifest_result",
        "test_optional_description",
        "test_total",
        "test_sqrt_bonus",
    ] {
        assert!(
            manifest_test_stdout.contains(name),
            "manifest test missing {name}"
        );
        assert!(
            explicit_test_stdout.contains(name),
            "explicit test missing {name}"
        );
    }

    let project_check = nox_command()
        .current_dir(&project)
        .args(["project", "check", "--json"])
        .output()
        .unwrap();
    assert!(
        project_check.status.success(),
        "{}",
        String::from_utf8_lossy(&project_check.stderr)
    );
    let project_stdout = String::from_utf8_lossy(&project_check.stdout);
    assert!(project_stdout.contains("\"schema\":\"nox.project-check.v1\""));
    assert!(project_stdout.contains("\"name\":\"check\",\"ok\":true,\"status\":0"));
    assert!(project_stdout.contains("\"name\":\"test\",\"ok\":true,\"status\":0"));
    assert!(project_stdout.contains("\"name\":\"fmt\",\"ok\":true,\"status\":0"));
}

#[test]
fn project_check_runs_project_workflow() {
    let project = fixture("examples/projects/scoreboard");

    for cwd in [&project, &project.join("src")] {
        let output = nox_command()
            .current_dir(cwd)
            .args(["project", "check"])
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("project check: check"));
        assert!(stdout.contains("summary: 5 tests, 5 passed, 0 failed"));
        assert!(stdout.contains("project check: fmt --check"));
        assert!(stdout.contains("project check: ok"));
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn project_check_json_reports_manifest_boundary_and_steps() {
    let project = fixture("examples/projects/scoreboard");
    let project_root = project.canonicalize().unwrap();

    for cwd in [&project, &project.join("src")] {
        let output = nox_command()
            .current_dir(cwd)
            .args(["project", "check", "--json"])
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.starts_with("{\"schema\":\"nox.project-check.v1\""));
        assert!(stdout.contains("\"ok\":true"));
        assert!(stdout.contains(&format!("\"root\":\"{}\"", project_root.display())));
        assert!(stdout.contains("\"package\":{\"name\":\"scoreboard\",\"version\":\"0.0.3\"}"));
        assert!(stdout.contains("\"name\":\"check\",\"ok\":true,\"status\":0"));
        assert!(stdout.contains("\"name\":\"test\",\"ok\":true,\"status\":0"));
        assert!(stdout.contains("\"name\":\"fmt\",\"ok\":true,\"status\":0"));
        assert!(stdout.contains("\"summary\":{\"steps\":3,\"passed\":3,\"failed\":0}"));
        assert!(stdout.contains("\\\"name\\\":\\\"test_manifest_present\\\""));
    }
}

#[test]
fn run_resolves_std_fs_module() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-run-std-fs-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let data_path = dir.join("message.txt");
    fs::write(&data_path, "std-ok").unwrap();
    let script_path = dir.join("main.nox");
    fs::write(
        &script_path,
        format!(
            "import \"std/fs.nox\" as fs;\n\nfs.read_text(\"{}\");\n",
            json_escape(&data_path.display().to_string())
        ),
    )
    .unwrap();

    let output = nox_command()
        .args(["run", script_path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "std-ok\n");
}

#[test]
fn check_resolves_std_env_and_time_modules() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-check-std-modules-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.nox");
    fs::write(
        &path,
        r#"import "std/env.nox" as env;
import "std/time.nox" as time;

let values: map[str, str] = env.list();
time.sleep_ms(0);
values;
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["check", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("ok"));
}

#[test]
fn check_json_reports_std_env_try_get_option_type() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-check-env-try-get-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.nox");
    fs::write(
        &path,
        r#"import "std/env.nox" as env;

let value: str = env.try_get("PATH");
value;
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"type.mismatch\""));
    assert!(stdout.contains("expected str, got option[str]"));
}

#[test]
fn check_json_reports_std_fs_try_read_text_result_type() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-check-fs-try-read-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.nox");
    fs::write(
        &path,
        r#"import "std/fs.nox" as fs;

let value: str = fs.try_read_text("missing.txt");
value;
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"type.mismatch\""));
    assert!(stdout.contains("expected str, got result[str, str]"));
}

#[test]
fn check_json_reports_map_get_option_type() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-check-map-get-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.nox");
    fs::write(
        &path,
        r#"let scores: map[str, int] = { "a": 1 };
let value: int = map_get(scores, "a");
value;
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"type.mismatch\""));
    assert!(stdout.contains("expected int, got option[int]"));
}

#[test]
fn test_resolves_std_fs_module() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-std-fs-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let test_path = dir.join("std_test.nox");
    fs::write(
        &test_path,
        r#"import "std/fs.nox" as fs;

fn test_manifest_absent() -> bool {
    return !fs.exists("definitely-missing-nox-test-file");
}
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["test", test_path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("std_test.nox::test_manifest_absent PASS")
    );
}

#[test]
fn check_reports_unknown_std_module_without_filesystem_fallback() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-check-unknown-std-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("std")).unwrap();
    fs::write(
        dir.join("std/missing.nox"),
        "export fn answer() -> int { return 42; }\n",
    )
    .unwrap();
    let path = dir.join("main.nox");
    fs::write(
        &path,
        "import \"std/missing.nox\" as missing;\n\nmissing.answer();\n",
    )
    .unwrap();

    let output = nox_command()
        .current_dir(&dir)
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"module.not-found\""), "{stdout}");
    assert!(
        stdout.contains("standard module 'std/missing.nox' is not provided"),
        "{stdout}"
    );
}

#[test]
fn check_json_and_lsp_report_relative_module_not_found_code() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-relative-module-missing-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.nox");
    let source = "import \"missing.nox\" as missing;\n\nmissing.answer();\n";
    fs::write(&main, source).unwrap();

    let output = nox_command()
        .args(["check", "--json", main.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"module.not-found\""), "{stdout}");
    assert!(stdout.contains("failed to load module"), "{stdout}");
    assert!(stdout.contains(&format!("\"file\":\"{}\"", main.display())));

    let uri = format!("file://{}", main.display());
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(&uri),
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"method\":\"textDocument/publishDiagnostics\""));
    assert!(stdout.contains(&format!("\"uri\":\"{}\"", uri)));
    assert!(stdout.contains("\"code\":\"module.not-found\""), "{stdout}");
    assert!(stdout.contains("failed to load module"), "{stdout}");
}

#[test]
fn project_check_fails_when_formatting_is_not_stable() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-project-check-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(
        dir.join("nox.toml"),
        r#"[package]
name = "project-check"
version = "0.0.1"

[entrypoints]
main = "src/main.nox"

[modules]
source_dirs = ["src"]
test_dirs = ["tests"]
"#,
    )
    .unwrap();
    fs::write(dir.join("src/main.nox"), "let value:int=1;value;").unwrap();
    fs::write(
        dir.join("tests/main_test.nox"),
        "fn test_value() -> bool { return true; }\n",
    )
    .unwrap();

    let output = nox_command()
        .current_dir(&dir)
        .args(["project", "check"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("project check: check"));
    assert!(stdout.contains("project check: test"));
    assert!(stdout.contains("project check: fmt --check"));
    assert!(stdout.contains("src/main.nox"));
    assert!(output.stderr.is_empty());
}

#[test]
fn run_explicit_path_overrides_manifest_main() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-run-manifest-override-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.2\"\n\n[entrypoints]\nmain = \"src/main.nox\"\n",
    )
    .unwrap();
    fs::write(dir.join("src/main.nox"), "1;\n").unwrap();
    fs::write(dir.join("src/other.nox"), "2;\n").unwrap();

    let output = nox_command()
        .current_dir(&dir)
        .args(["run", "src/other.nox"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n");
}

#[test]
fn test_rejects_invalid_test_signature() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-signature-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bad_test.nox");
    fs::write(
        &path,
        "fn test_bad(value: int) -> bool {\n    return value == 1;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["test", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("{}::<module> FAIL", path.display())));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("test function 'test_bad' must not take parameters"));
}

#[test]
fn test_json_reports_invalid_signature_code() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-signature-json-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bad_test.nox");
    fs::write(
        &path,
        "fn test_bad(value: int) -> bool {\n    return value == 1;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.test.v1\""));
    assert!(stdout.contains("\"name\":\"<module>\""));
    assert!(stdout.contains("\"code\":\"test.signature\""));
    assert!(stdout.contains("\"span\":{\"start\":0,\"end\":58}"));
    assert!(stdout.contains("\"source\":{\"name\":"));
}

#[test]
fn inspect_bytecode_prints_bytecode_module() {
    let output = nox_command()
        .args([
            "inspect-bytecode",
            fixture("examples/hello.nox").to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Function"));
    assert!(stdout.contains("double"));
    assert!(output.stderr.is_empty());
}

#[test]
fn inspect_bytecode_compact_prints_numbered_instruction_stream() {
    let output = nox_command()
        .args([
            "inspect-bytecode",
            "--compact",
            fixture("examples/hello.nox").to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout
        .lines()
        .next()
        .is_some_and(|line| line.starts_with("0000 ")));
    assert!(stdout.contains("Function"));
    assert!(!stdout.contains("BytecodeModule {"));
    assert!(output.stderr.is_empty());
}

#[test]
fn fmt_prints_stable_formatted_source() {
    let dir = std::env::temp_dir().join(format!("nox-cli-fmt-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("messy.nox");
    fs::write(
        &path,
        r#"export fn double(value:int)->int{return value*2;}let result:int=double(21);result;"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["fmt", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "export fn double(value: int) -> int {\n    return value * 2;\n}\n\nlet result: int = double(21);\n\nresult;\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn fmt_preserves_integral_float_literals() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-fmt-float-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("float.nox");
    fs::write(&path, r#"let result:float=42.0*2.0;result;"#).unwrap();

    let output = nox_command()
        .args(["fmt", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "let result: float = 42.0 * 2.0;\n\nresult;\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn fmt_prints_namespace_imports() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-fmt-namespace-import-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.nox");
    fs::write(&path, r#"import "math.nox" as math;math.double(21);"#).unwrap();

    let output = nox_command()
        .args(["fmt", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "import \"math.nox\" as math;\n\nmath.double(21);\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn fmt_prints_std_namespace_imports() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-fmt-std-import-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.nox");
    fs::write(&path, r#"import "std/fs.nox" as fs;fs.exists("nox.toml");"#).unwrap();

    let output = nox_command()
        .args(["fmt", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "import \"std/fs.nox\" as fs;\n\nfs.exists(\"nox.toml\");\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn fmt_prints_match_statement() {
    let dir = std::env::temp_dir().join(format!("nox-cli-fmt-match-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("match.nox");
    fs::write(
        &path,
        r#"let value:int=1;match(value){1=>{value=2;}_=>{value=0;}}value;"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["fmt", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "let value: int = 1;\n\nmatch (value) {\n    1 => {\n        value = 2;\n    }\n    _ => {\n        value = 0;\n    }\n}\n\nvalue;\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn fmt_golden_fixture_is_idempotent() {
    let path = fixture("examples/formatter-golden.nox");
    let first = nox_command()
        .args(["fmt", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(first.status.success());
    assert!(first.stderr.is_empty());
    let source = fs::read_to_string(&path).unwrap();
    let formatted = String::from_utf8(first.stdout).unwrap();
    assert_eq!(formatted, source);

    let dir = std::env::temp_dir().join(format!(
        "nox-cli-fmt-golden-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let copy = dir.join("copy.nox");
    fs::write(&copy, &formatted).unwrap();

    let second = nox_command()
        .args(["fmt", copy.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(second.status.success());
    assert!(second.stderr.is_empty());
    assert_eq!(String::from_utf8(second.stdout).unwrap(), formatted);
}

#[test]
fn fmt_check_reports_inconsistent_files_and_exits_nonzero() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-fmt-check-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let messy = dir.join("messy.nox");
    let tidy = dir.join("tidy.nox");
    fs::write(&messy, r#"let value:int=1;value;"#).unwrap();
    fs::write(&tidy, "let value: int = 1;\n\nvalue;\n").unwrap();

    let output = nox_command()
        .args([
            "fmt",
            "--check",
            messy.to_str().unwrap(),
            tidy.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(messy.to_str().unwrap()));
    assert!(!stdout.contains(&format!("{}\n", tidy.display())));
    assert!(output.stderr.is_empty());

    let messy_after = fs::read_to_string(&messy).unwrap();
    assert_eq!(messy_after, "let value:int=1;value;");
}

#[test]
fn fmt_check_and_write_expand_directories() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-fmt-dir-{}-{}",
        std::process::id(),
        line!()
    ));
    let nested = dir.join("nested");
    fs::create_dir_all(&nested).unwrap();
    let messy = dir.join("messy.nox");
    let tidy = nested.join("tidy.nox");
    fs::write(&messy, "let value:int=1;value;").unwrap();
    fs::write(&tidy, "let other: int = 2;\n\nother;\n").unwrap();

    let check = nox_command()
        .args(["fmt", "--check", dir.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!check.status.success());
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains(messy.to_str().unwrap()));
    assert!(!stdout.contains(tidy.to_str().unwrap()));
    assert!(check.stderr.is_empty());

    let write = nox_command()
        .args(["fmt", "--write", dir.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(write.status.success());
    assert!(write.stdout.is_empty());
    assert!(write.stderr.is_empty());
    assert_eq!(
        fs::read_to_string(&messy).unwrap(),
        "let value: int = 1;\n\nvalue;\n"
    );

    let recheck = nox_command()
        .args(["fmt", "--check", dir.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(recheck.status.success());
    assert!(recheck.stdout.is_empty());
    assert!(recheck.stderr.is_empty());
}

#[test]
fn fmt_check_without_paths_uses_manifest_project_files() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-fmt-manifest-{}-{}",
        std::process::id(),
        line!()
    ));
    let src = dir.join("src");
    let tests = dir.join("tests");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&tests).unwrap();
    fs::write(
        dir.join("nox.toml"),
        r#"[package]
name = "fmt-project"
version = "0.0.1"

[entrypoints]
main = "src/main.nox"

[modules]
source_dirs = ["src"]
test_dirs = ["tests"]
"#,
    )
    .unwrap();
    let main = src.join("main.nox");
    let helper = src.join("helper.nox");
    let test = tests.join("helper_test.nox");
    fs::write(&main, "let value:int=1;value;").unwrap();
    fs::write(&helper, "fn value() -> int {\n    return 2;\n}\n").unwrap();
    fs::write(&test, "fn test_value() -> bool {\n    return true;\n}\n").unwrap();

    let check = nox_command()
        .current_dir(&dir)
        .args(["fmt", "--check"])
        .output()
        .unwrap();

    assert!(!check.status.success());
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("src/main.nox"));
    assert!(!stdout.contains("src/helper.nox"));
    assert!(!stdout.contains("tests/helper_test.nox"));
    assert!(check.stderr.is_empty());

    let write = nox_command()
        .current_dir(&dir)
        .args(["fmt", "--write"])
        .output()
        .unwrap();

    assert!(write.status.success());
    assert!(write.stdout.is_empty());
    assert!(write.stderr.is_empty());
    assert_eq!(
        fs::read_to_string(&main).unwrap(),
        "let value: int = 1;\n\nvalue;\n"
    );
}

#[test]
fn fmt_check_succeeds_when_all_files_already_formatted() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-fmt-check-ok-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let tidy = dir.join("tidy.nox");
    fs::write(&tidy, "let value: int = 1;\n\nvalue;\n").unwrap();

    let output = nox_command()
        .args(["fmt", "--check", tidy.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn fmt_write_rewrites_files_in_place() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-fmt-write-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let first = dir.join("first.nox");
    let second = dir.join("second.nox");
    fs::write(&first, "let a:int=1;a;").unwrap();
    fs::write(&second, "let b: int = 2;\n\nb;\n").unwrap();
    let second_before = fs::metadata(&second).unwrap().modified().ok();

    let output = nox_command()
        .args([
            "fmt",
            "--write",
            first.to_str().unwrap(),
            second.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert_eq!(
        fs::read_to_string(&first).unwrap(),
        "let a: int = 1;\n\na;\n"
    );
    assert_eq!(
        fs::read_to_string(&second).unwrap(),
        "let b: int = 2;\n\nb;\n"
    );
    if let (Some(before), Ok(after)) = (second_before, fs::metadata(&second).unwrap().modified()) {
        assert_eq!(
            before, after,
            "already-formatted file should not be rewritten"
        );
    }
}

#[test]
fn fmt_default_rejects_multiple_files() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-fmt-multi-stdout-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let first = dir.join("first.nox");
    let second = dir.join("second.nox");
    fs::write(&first, "let a: int = 1;\n\na;\n").unwrap();
    fs::write(&second, "let b: int = 2;\n\nb;\n").unwrap();

    let output = nox_command()
        .args(["fmt", first.to_str().unwrap(), second.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--check") && stderr.contains("--write"));
}

#[test]
fn fmt_check_and_write_are_mutually_exclusive() {
    let output = nox_command()
        .args(["fmt", "--check", "--write", "ignored.nox"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("mutually exclusive"));
}

#[test]
fn lsp_publishes_diagnostics_for_open_document() {
    let source = r#"let value: int = "bad";"#;
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///bad.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"method\":\"textDocument/publishDiagnostics\""));
    assert!(stdout.contains("\"uri\":\"file:///bad.nox\""));
    assert!(stdout.contains("\"code\":\"type.mismatch\""));
    assert!(stdout.contains("\"message\":\"expected int, got str\""));
}

#[test]
fn lsp_reports_std_env_try_get_option_type() {
    let source = r#"import "std/env.nox" as env;

let value: str = env.try_get("PATH");
value;
"#;
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///env-try-get.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"uri\":\"file:///env-try-get.nox\""));
    assert!(stdout.contains("\"code\":\"type.mismatch\""));
    assert!(stdout.contains("expected str, got option[str]"));
}

#[test]
fn lsp_reports_std_fs_try_read_text_result_type() {
    let source = r#"import "std/fs.nox" as fs;

let value: str = fs.try_read_text("missing.txt");
value;
"#;
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///fs-try-read.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"uri\":\"file:///fs-try-read.nox\""));
    assert!(stdout.contains("\"code\":\"type.mismatch\""));
    assert!(stdout.contains("expected str, got result[str, str]"));
}

#[test]
fn lsp_reports_map_get_option_type() {
    let source = r#"let scores: map[str, int] = { "a": 1 };
let value: int = map_get(scores, "a");
value;
"#;
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///map-get.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"uri\":\"file:///map-get.nox\""));
    assert!(stdout.contains("\"code\":\"type.mismatch\""));
    assert!(stdout.contains("expected int, got option[int]"));
}

#[test]
fn lsp_reports_module_not_found_code() {
    let source = "import \"std/missing.nox\" as missing;\n\nmissing.answer();\n";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///missing-std.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"method\":\"textDocument/publishDiagnostics\""));
    assert!(stdout.contains("\"uri\":\"file:///missing-std.nox\""));
    assert!(stdout.contains("\"code\":\"module.not-found\""));
}

#[test]
fn lsp_did_change_updates_diagnostics() {
    let original = r#"let value: int = 1;"#;
    let updated = r#"let value: int = "bad";"#;
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///live.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(original)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"file:///live.nox","version":2}},"contentChanges":[{{"text":"{}"}}]}}}}"#,
            json_escape(updated)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let occurrences = stdout
        .matches("\"method\":\"textDocument/publishDiagnostics\"")
        .count();
    assert_eq!(occurrences, 2, "expected didOpen and didChange diagnostics");
    let first_clean = stdout
        .find("\"diagnostics\":[]")
        .expect("first diagnostics should be empty");
    let mismatch_offset = stdout
        .find("\"code\":\"type.mismatch\"")
        .expect("second diagnostics should report the mismatch");
    assert!(
        first_clean < mismatch_offset,
        "didChange diagnostics must follow didOpen diagnostics"
    );
}

#[test]
fn lsp_formatting_returns_full_document_edit() {
    let source = "let value:int=1+2;value;";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///fmt.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/formatting","params":{"textDocument":{"uri":"file:///fmt.nox"},"options":{"tabSize":4,"insertSpaces":true}}}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"id\":2"));
    assert!(stdout.contains("\"newText\":\"let value: int = 1 + 2;\\n\\nvalue;\\n\""));
    assert!(stdout.contains("\"start\":{\"line\":0,\"character\":0}"));
}

#[test]
fn lsp_uses_open_document_for_import_resolution() {
    let dir =
        std::env::temp_dir().join(format!("nox-lsp-import-{}-{}", std::process::id(), line!()));
    fs::create_dir_all(&dir).unwrap();
    let helper_path = dir.join("helper.nox");
    let main_path = dir.join("main.nox");
    fs::write(
        &helper_path,
        "export fn answer() -> int {\n    return 0;\n}\n",
    )
    .unwrap();
    let helper_uri = format!("file://{}", helper_path.display());
    let main_uri = format!("file://{}", main_path.display());
    let helper_text = "export fn answer() -> str {\n    return \"42\";\n}\n";
    let main_text = "import \"helper.nox\";\n\nlet value: int = answer();\nvalue;\n";

    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            helper_uri,
            json_escape(helper_text)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            main_uri,
            json_escape(main_text)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let main_diagnostic_offset = stdout
        .find(&format!("\"uri\":\"{main_uri}\""))
        .expect("main document should have diagnostics");
    let after_main = &stdout[main_diagnostic_offset..];
    assert!(
        after_main.contains("\"code\":\"type.mismatch\""),
        "expected overlay import to surface main's type error: {after_main}"
    );
    assert!(
        after_main.contains("expected int, got str"),
        "expected mismatch message from open document"
    );
}

#[test]
fn lsp_rechecks_importers_when_imported_open_document_changes() {
    let dir = std::env::temp_dir().join(format!(
        "nox-lsp-import-refresh-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let helper_path = dir.join("helper.nox");
    let main_path = dir.join("main.nox");
    let helper_uri = format!("file://{}", helper_path.display());
    let main_uri = format!("file://{}", main_path.display());
    let helper_ok = "export fn answer() -> int {\n    return 42;\n}\n";
    let helper_bad = "export fn answer() -> str {\n    return \"bad\";\n}\n";
    let main_text = "import \"helper.nox\";\n\nlet value: int = answer();\nvalue;\n";

    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            helper_uri,
            json_escape(helper_ok)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            main_uri,
            json_escape(main_text)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{}","version":2}},"contentChanges":[{{"text":"{}"}}]}}}}"#,
            helper_uri,
            json_escape(helper_bad)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let main_blocks = stdout
        .match_indices(&format!("\"uri\":\"{main_uri}\""))
        .map(|(index, _)| &stdout[index..])
        .collect::<Vec<_>>();
    assert!(
        main_blocks
            .iter()
            .any(|block| block.contains("\"diagnostics\":[]")),
        "main should be clean before helper changes: {stdout}"
    );
    assert!(
        main_blocks
            .iter()
            .any(|block| block.contains("expected int, got str")),
        "main should be rechecked after helper changes: {stdout}"
    );
}

#[test]
fn lsp_completion_includes_keywords_and_scope_identifiers() {
    let source = "let answer: int = 42;\nlet ";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///complete.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":"file:///complete.nox"},"position":{"line":1,"character":4}}}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"id\":2"));
    assert!(stdout.contains("\"label\":\"let\""));
    assert!(stdout.contains("\"label\":\"break\""));
    assert!(stdout.contains("\"label\":\"continue\""));
    assert!(stdout.contains("\"label\":\"len\""));
    assert!(stdout.contains("\"label\":\"contains\""));
    assert!(stdout.contains("\"label\":\"map_get\""));
    assert!(stdout.contains("\"label\":\"answer\""));
}

#[test]
fn lsp_completion_includes_namespace_members_from_open_import() {
    let dir = std::env::temp_dir().join(format!(
        "nox-lsp-namespace-complete-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let helper_path = dir.join("math.nox");
    let main_path = dir.join("main.nox");
    let helper_uri = format!("file://{}", helper_path.display());
    let main_uri = format!("file://{}", main_path.display());
    let helper_text = "export fn double(value: int) -> int {\n    return value * 2;\n}\n\nfn helper(value: int) -> int {\n    return value;\n}\n";
    let main_text = "import \"math.nox\" as math;\n\nmath.";

    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            helper_uri,
            json_escape(helper_text)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            main_uri,
            json_escape(main_text)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":2,"character":5}}}}}}"#,
            main_uri
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let completion = stdout
        .split("\"id\":2")
        .nth(1)
        .expect("completion response should be present");
    assert!(completion.contains("\"label\":\"double\""), "{stdout}");
    assert!(!completion.contains("\"label\":\"helper\""), "{stdout}");
}

#[test]
fn lsp_completion_includes_std_module_members() {
    let source = "import \"std/fs.nox\" as fs;\n\nfs.exists(\"nox.toml\");\n";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///std-main.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":"file:///std-main.nox"},"position":{"line":2,"character":3}}}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"diagnostics\":[]"), "{stdout}");
    let completion = stdout
        .split("\"id\":2")
        .nth(1)
        .expect("std module completion response should be present");
    assert!(completion.contains("\"label\":\"read_text\""), "{stdout}");
    assert!(
        completion.contains("\"label\":\"try_read_text\""),
        "{stdout}"
    );
    assert!(completion.contains("\"label\":\"exists\""), "{stdout}");
    assert!(completion.contains("\"label\":\"write_text\""), "{stdout}");
}

#[test]
fn lsp_supports_scoreboard_project_workflow() {
    let main_path = fixture("examples/projects/scoreboard/src/main.nox");
    let main_uri = format!("file://{}", main_path.display());
    let main_source = fs::read_to_string(&main_path).unwrap();
    let completion_offset = main_source.find("scoring.").unwrap() + "scoring.".len();
    let (line, character) = lsp_position(&main_source, completion_offset);
    let runtime_test_path = fixture("examples/projects/scoreboard/tests/runtime_info_test.nox");
    let runtime_test_uri = format!("file://{}", runtime_test_path.display());
    let runtime_test_source = fs::read_to_string(&runtime_test_path).unwrap();
    let runtime_completion_offset =
        runtime_test_source.find("runtime_info.").unwrap() + "runtime_info.".len();
    let (runtime_line, runtime_character) =
        lsp_position(&runtime_test_source, runtime_completion_offset);

    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            main_uri,
            json_escape(&main_source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            runtime_test_uri,
            json_escape(&runtime_test_source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{},"character":{}}}}}}}"#,
            main_uri, line, character
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"textDocument/formatting","params":{{"textDocument":{{"uri":"{}"}},"options":{{"tabSize":4,"insertSpaces":true}}}}}}"#,
            main_uri
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":4,"method":"textDocument/completion","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{},"character":{}}}}}}}"#,
            runtime_test_uri, runtime_line, runtime_character
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":5,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let main_diagnostics = stdout
        .split(&format!("\"uri\":\"{main_uri}\""))
        .nth(1)
        .expect("scoreboard main diagnostics should be published");
    assert!(
        main_diagnostics.contains("\"diagnostics\":[]"),
        "scoreboard project imports should resolve through manifest source_dirs: {stdout}"
    );
    let runtime_diagnostics = stdout
        .split(&format!("\"uri\":\"{runtime_test_uri}\""))
        .nth(1)
        .expect("scoreboard runtime_info diagnostics should be published");
    assert!(
        runtime_diagnostics.contains("\"diagnostics\":[]"),
        "scoreboard test imports should resolve through manifest test_dirs and source_dirs: {stdout}"
    );

    let completion = stdout
        .split("\"id\":2")
        .nth(1)
        .expect("scoreboard completion response should be present");
    assert!(completion.contains("\"label\":\"total\""), "{stdout}");
    assert!(completion.contains("\"label\":\"sqrt_bonus\""), "{stdout}");
    assert!(
        !completion.contains("\"label\":\"score_label\""),
        "{stdout}"
    );

    let formatting = stdout
        .split("\"id\":3")
        .nth(1)
        .expect("scoreboard formatting response should be present");
    assert!(formatting.contains("\"result\":[]"), "{stdout}");

    let runtime_completion = stdout
        .split("\"id\":4")
        .nth(1)
        .expect("scoreboard runtime_info completion response should be present");
    assert!(
        runtime_completion.contains("\"label\":\"has_manifest\""),
        "{stdout}"
    );
    assert!(
        runtime_completion.contains("\"label\":\"try_manifest\""),
        "{stdout}"
    );
    assert!(
        runtime_completion.contains("\"label\":\"describe_optional\""),
        "{stdout}"
    );
    assert!(
        runtime_completion.contains("\"label\":\"optional_env\""),
        "{stdout}"
    );
}

#[test]
fn lsp_reports_invalid_manifest_before_module_resolution() {
    let dir = std::env::temp_dir().join(format!(
        "nox-lsp-invalid-manifest-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("nox.toml"), "[package]\nname = \"demo\"\n").unwrap();
    let main_path = dir.join("src/main.nox");
    let main_uri = format!("file://{}", main_path.display());
    let source = "import \"missing.nox\";\n0;\n";

    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            main_uri,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics = stdout
        .split(&format!("\"uri\":\"{main_uri}\""))
        .nth(1)
        .expect("main diagnostics should be published");
    assert!(
        diagnostics.contains("\"code\":\"manifest.invalid\""),
        "{stdout}"
    );
    assert!(
        diagnostics.contains("missing required key 'version'"),
        "{stdout}"
    );
    assert!(
        !diagnostics.contains("\"code\":\"module.not-found\""),
        "manifest errors should not be hidden behind module resolution: {stdout}"
    );
}

#[test]
fn lsp_hover_returns_expression_type() {
    let source = "let value: int = 42;\nvalue;\n";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///hover.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///hover.nox"},"position":{"line":1,"character":1}}}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"id\":2"));
    assert!(stdout.contains("\"kind\":\"plaintext\""));
    assert!(stdout.contains("\"value\":\"int\""));
}

#[test]
fn check_exits_nonzero_for_static_type_error() {
    for (path, expected) in [
        ("examples/type-error.nox", "expected int, got str"),
        (
            "examples/type-error-array-element.nox",
            "expected int, got str",
        ),
        (
            "examples/type-error-array-index.nox",
            "expected int, got float",
        ),
        (
            "examples/type-error-array-len.nox",
            "expected array or str, got int",
        ),
        (
            "examples/type-error-int-float.nox",
            "'+' is not defined for int and float",
        ),
        (
            "examples/type-error-for-range.nox",
            "expected int, got float",
        ),
        (
            "examples/type-error-const-assignment.nox",
            "cannot assign to constant 'value'",
        ),
        (
            "examples/type-error-break-outside-loop.nox",
            "'break' is only allowed inside a 'while' or 'for' loop",
        ),
        (
            "examples/type-error-continue-outside-loop.nox",
            "'continue' is only allowed inside a 'while' or 'for' loop",
        ),
        ("examples/type-error-logical.nox", "expected bool, got int"),
        ("examples/type-error-map-index.nox", "expected str, got int"),
        ("examples/type-error-map-key.nox", "expected str, got int"),
        ("examples/type-error-map-value.nox", "expected int, got str"),
        (
            "examples/type-error-record-duplicate-field.nox",
            "duplicate field 'name'",
        ),
        (
            "examples/type-error-record-extra-field.nox",
            "record 'User' has no field 'score'",
        ),
        (
            "examples/type-error-record-field-access.nox",
            "record 'User' has no field 'score'",
        ),
        (
            "examples/type-error-record-field-type.nox",
            "expected int, got str",
        ),
        (
            "examples/type-error-record-missing-field.nox",
            "missing field 'score'",
        ),
        (
            "examples/type-error-sqrt-int.nox",
            "expected float, got int",
        ),
        (
            "examples/type-error-sleep-float.nox",
            "expected int, got float",
        ),
    ] {
        let output = nox_command()
            .args(["check", fixture(path).to_str().unwrap()])
            .output()
            .unwrap();

        assert!(!output.status.success(), "{path}");
        assert!(output.stdout.is_empty(), "{path}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{path}"
        );
    }
}

#[test]
fn run_exits_nonzero_for_runtime_error() {
    for (path, expected) in [
        ("examples/runtime-error-divide-zero.nox", "division by zero"),
        (
            "examples/runtime-error-array-bounds.nox",
            "array index out of bounds",
        ),
        ("examples/runtime-error-map-key.nox", "map key not found"),
    ] {
        let output = nox_command()
            .args(["run", fixture(path).to_str().unwrap()])
            .output()
            .unwrap();

        assert!(!output.status.success(), "{path}");
        assert!(output.stdout.is_empty(), "{path}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{path}"
        );
    }
}

#[test]
fn check_exits_nonzero_for_syntax_error() {
    let path = "examples/syntax-error-string-escape.nox";
    let output = nox_command()
        .args(["check", fixture(path).to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success(), "{path}");
    assert!(output.stdout.is_empty(), "{path}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unsupported escape sequence '\\r'"),
        "{path}"
    );
}

#[test]
fn check_prints_multiple_syntax_errors() {
    let output = nox_command()
        .args([
            "check",
            fixture("examples/syntax-errors.nox").to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches("expected ':'").count(), 2);
}

#[test]
fn check_reports_cyclic_imports() {
    let dir = std::env::temp_dir().join(format!("nox-cli-import-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.nox"), r#"import "a.nox";"#).unwrap();
    fs::write(dir.join("a.nox"), r#"import "b.nox";"#).unwrap();
    fs::write(dir.join("b.nox"), r#"import "a.nox";"#).unwrap();

    let output = nox_command()
        .args(["check", dir.join("main.nox").to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cyclic import detected for 'a.nox'"));
}

#[test]
fn run_resolves_imports_relative_to_entrypoint_directory() {
    let dir = std::env::temp_dir().join(format!("nox-cli-relative-import-{}", std::process::id()));
    let modules = dir.join("modules");
    fs::create_dir_all(&modules).unwrap();
    fs::write(
        dir.join("main.nox"),
        r#"
        import "modules/math.nox";
        double(21);
        "#,
    )
    .unwrap();
    fs::write(
        modules.join("math.nox"),
        r#"
        fn double(value: int) -> int {
            return value * 2;
        }
        "#,
    )
    .unwrap();

    let output = nox_command()
        .args(["run", dir.join("main.nox").to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "42\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn check_rejects_private_imported_declarations() {
    let dir = std::env::temp_dir().join(format!("nox-cli-export-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("main.nox"),
        r#"
        import "math.nox";
        helper(21);
        "#,
    )
    .unwrap();
    fs::write(
        dir.join("math.nox"),
        r#"
        export fn double(value: int) -> int {
            return helper(value);
        }

        fn helper(value: int) -> int {
            return value * 2;
        }
        "#,
    )
    .unwrap();

    let output = nox_command()
        .args(["check", dir.join("main.nox").to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("undefined variable 'helper'"));
}

#[test]
fn run_resolves_imports_through_manifest_source_dirs() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-manifest-source-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src/lib")).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[modules]\nsource_dirs = [\"src\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/lib/math.nox"),
        "export fn triple(value: int) -> int {\n    return value * 3;\n}\n",
    )
    .unwrap();
    let entry = dir.join("src/main.nox");
    fs::write(
        &entry,
        "import \"lib/math.nox\";\n\nlet value: int = triple(7);\nvalue;\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["run", entry.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "21\n");
}

#[test]
fn relative_imports_still_resolve_without_manifest() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-manifest-none-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("helper.nox"),
        "export fn quadruple(value: int) -> int {\n    return value * 4;\n}\n",
    )
    .unwrap();
    let entry = dir.join("main.nox");
    fs::write(
        &entry,
        "import \"helper.nox\";\n\nlet value: int = quadruple(5);\nvalue;\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["run", entry.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "20\n");
}

#[test]
fn run_reports_invalid_manifest() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-manifest-bad-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("nox.toml"), "[package]\nname = \"demo\"\n").unwrap();
    let entry = dir.join("main.nox");
    fs::write(&entry, "0;\n").unwrap();

    let output = nox_command()
        .args(["run", entry.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("version"),
        "expected stderr to mention missing version: {stderr}"
    );
}

#[test]
fn check_json_reports_invalid_manifest_code() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-manifest-json-bad-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("nox.toml"), "[package]\nname = \"demo\"\n").unwrap();
    let entry = dir.join("main.nox");
    fs::write(&entry, "0;\n").unwrap();

    let output = nox_command()
        .args(["check", "--json", entry.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""), "{stdout}");
    assert!(stdout.contains("\"code\":\"manifest.invalid\""), "{stdout}");
    assert!(
        stdout.contains("missing required key 'version'"),
        "{stdout}"
    );
    assert!(stdout.contains("\"diagnostic_count\":1"), "{stdout}");
}

#[test]
fn check_json_reports_manifest_module_dirs_outside_project_root() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-manifest-json-boundary-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[modules]\nsource_dirs = [\"../src\"]\n",
    )
    .unwrap();
    let entry = dir.join("src/main.nox");
    fs::write(&entry, "0;\n").unwrap();

    let output = nox_command()
        .args(["check", "--json", entry.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""), "{stdout}");
    assert!(stdout.contains("\"code\":\"manifest.invalid\""), "{stdout}");
    assert!(
        stdout.contains("must stay within the project root"),
        "{stdout}"
    );
    assert!(stdout.contains("\"diagnostic_count\":1"), "{stdout}");
}

#[test]
fn check_json_reports_missing_project_discovery_code() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-project-discovery-missing-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();

    let output = nox_command()
        .current_dir(&dir)
        .args(["check", "--json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""), "{stdout}");
    assert!(
        stdout.contains("\"code\":\"project.discovery\""),
        "{stdout}"
    );
    assert!(stdout.contains("no nox.toml was found"), "{stdout}");
    assert!(stdout.contains("\"checked\":1"), "{stdout}");
    assert!(stdout.contains("\"diagnostic_count\":1"), "{stdout}");
}

#[test]
fn check_json_reports_missing_manifest_project_dir() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-project-discovery-dir-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[modules]\nsource_dirs = [\"src\"]\n",
    )
    .unwrap();

    let output = nox_command()
        .current_dir(&dir)
        .args(["check", "--json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""), "{stdout}");
    assert!(
        stdout.contains("\"code\":\"project.discovery\""),
        "{stdout}"
    );
    assert!(stdout.contains("path '"), "{stdout}");
    assert!(stdout.contains("src' does not exist"), "{stdout}");
    assert!(stdout.contains("\"diagnostic_count\":1"), "{stdout}");
}

#[test]
fn manifest_without_main_does_not_block_explicit_entry() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-manifest-no-main-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n",
    )
    .unwrap();
    let entry = dir.join("main.nox");
    fs::write(&entry, "9;\n").unwrap();

    let output = nox_command()
        .args(["run", entry.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "9\n");
}
