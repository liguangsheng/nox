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

fn dap_frame(body: &str) -> String {
    lsp_frame(body)
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
        ("examples/bitwise.nox", "bitwise-ok\n"),
        ("tests/benchmarks/bench-containers.nox", "containers-ok\n"),
        ("tests/benchmarks/bench-fib.nox", "fib-ok\n"),
        ("tests/benchmarks/bench-loop.nox", "loop-ok\n"),
        ("tests/benchmarks/bench-modules.nox", "modules-ok\n"),
        ("examples/hello.nox", "84\n"),
        ("examples/control-flow.nox", "sum-ok\n"),
        (
            "examples/control-flow-let-patterns.nox",
            "let-patterns:42\n",
        ),
        ("examples/constants.nox", "const-ok\n"),
        ("examples/conversions.nox", "42\n"),
        ("examples/else-if.nox", "mid\n"),
        ("examples/enums.nox", "ready:42\n"),
        ("examples/export-main.nox", "42\n"),
        ("examples/for-range.nox", "10\n"),
        ("examples/generic-functions.nox", "generic:42\n"),
        ("examples/logical.nox", "logic-ok\n"),
        ("examples/loop-break-continue.nox", "loop-ok\n"),
        ("examples/maps.nox", "42\n"),
        ("examples/math-intrinsics.nox", "math-ok\n"),
        ("examples/match.nox", "two-range-2-nested-2\n"),
        ("examples/numeric-boundaries.nox", "numeric-ok\n"),
        ("examples/print.nox", "42\n"),
        ("examples/recursion.nox", "21\n"),
        ("examples/records.nox", "42\n"),
        ("examples/result-chain.nox", "nox:42\n"),
        ("examples/scopes.nox", "10\n"),
        ("examples/spread.nox", "spread-ok\n"),
        ("examples/string-escapes.nox", "escape-ok\n"),
        (
            "tests/fixtures/string-and-map-builtins.nox",
            "builtins-ok\n",
        ),
        ("examples/strings.nox", "nox:typed\n"),
        ("examples/time.nox", "time-ok\n"),
        ("examples/type-alias.nox", "nox:42\n"),
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
fn run_process_stdlib_handles_argv_stdin_stderr_and_exit_code() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-process-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("process.nox");
    fs::write(
        &path,
        r#"import "std/process.nox" as process;

let argv: [str] = process.argv();
let input: str = process.read_stdin();
process.print_err("stderr:" + argv[0]);
process.exit(7);
if (len(argv) == 2 && argv[1] == "beta" && input == "from stdin\n") {
    "process-ok";
} else {
    "process-bad";
}
"#,
    )
    .unwrap();

    let mut child = nox_command()
        .args(["run", path.to_str().unwrap(), "alpha", "beta"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"from stdin\n")
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(7));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "process-ok\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "stderr:alpha\n");
}

#[test]
fn repl_evaluates_lines_and_keeps_session_state() {
    let mut child = nox_command()
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"let answer: int = 41;\nanswer + 1\n:quit\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "42\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn profile_and_coverage_report_function_rows() {
    let path = fixture("tests/benchmarks/bench-fib.nox");

    let profile = nox_command()
        .args(["profile", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(profile.status.success());
    let stdout = String::from_utf8_lossy(&profile.stdout);
    assert!(stdout.contains("function\tcall_count\ttotal_us"));
    let fib_count = stdout
        .lines()
        .find_map(|line| line.strip_prefix("fib\t"))
        .and_then(|line| line.split('\t').next())
        .and_then(|count| count.parse::<u64>().ok())
        .unwrap_or(0);
    assert!(fib_count > 1, "{stdout}");
    assert!(stdout.contains("<module>\t1\t"));
    assert!(stdout.contains("fib-ok"));

    let coverage = nox_command()
        .args(["coverage", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(coverage.status.success());
    let stdout = String::from_utf8_lossy(&coverage.stdout);
    assert!(stdout.contains("coverage\tfunction\tcovered"));
    assert!(stdout.contains("coverage\tfib\ttrue"));
}

#[test]
fn dap_emits_initialized_event() {
    let path = fixture("examples/hello.nox");
    let input = [
        dap_frame(r#"{"seq":1,"type":"request","command":"initialize","arguments":{}}"#),
        dap_frame(&format!(
            r#"{{"seq":2,"type":"request","command":"setBreakpoints","arguments":{{"source":{{"path":"{}"}},"breakpoints":[{{"line":1,"condition":"result == 84"}}]}}}}"#,
            path.display()
        )),
        dap_frame(r#"{"seq":3,"type":"request","command":"setExceptionBreakpoints","arguments":{"filters":["raised"]}}"#),
        dap_frame(&format!(
            r#"{{"seq":4,"type":"request","command":"launch","arguments":{{"program":"{}"}}}}"#,
            path.display()
        )),
        dap_frame(r#"{"seq":5,"type":"request","command":"configurationDone","arguments":{}}"#),
        dap_frame(r#"{"seq":6,"type":"request","command":"threads","arguments":{}}"#),
        dap_frame(r#"{"seq":7,"type":"request","command":"stackTrace","arguments":{"threadId":1}}"#),
        dap_frame(r#"{"seq":8,"type":"request","command":"scopes","arguments":{"frameId":1}}"#),
        dap_frame(r#"{"seq":9,"type":"request","command":"variables","arguments":{"variablesReference":1,"maxDepth":0}}"#),
        dap_frame(r#"{"seq":10,"type":"request","command":"variables","arguments":{"variablesReference":1,"maxDepth":2}}"#),
        dap_frame(r#"{"seq":11,"type":"request","command":"variables","arguments":{"variablesReference":2,"maxDepth":2}}"#),
        dap_frame(r#"{"seq":12,"type":"request","command":"next","arguments":{"threadId":1}}"#),
        dap_frame(r#"{"seq":13,"type":"request","command":"disconnect","arguments":{}}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("dap")
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
    assert!(stdout.contains("Content-Length:"), "{stdout}");
    assert!(stdout.contains(r#""command":"initialize""#), "{stdout}");
    assert!(
        stdout.contains(r#""supportsConditionalBreakpoints":true"#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""event":"initialized""#), "{stdout}");
    assert!(stdout.contains(r#""command":"setBreakpoints""#), "{stdout}");
    assert!(stdout.contains(r#""condition":"result == 84""#), "{stdout}");
    assert!(
        stdout.contains(r#""command":"setExceptionBreakpoints""#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""event":"stopped""#), "{stdout}");
    assert!(stdout.contains(r#""reason":"breakpoint""#), "{stdout}");
    assert!(stdout.contains(r#""command":"stackTrace""#), "{stdout}");
    assert!(stdout.contains(r#""name":"result""#), "{stdout}");
    assert!(stdout.contains(r#""name":"exceptionFilters""#), "{stdout}");
    assert!(
        stdout.contains(r#""name":"maxDepth","value":"0""#),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            r#""name":"debugState","value":"depth limit reached","variablesReference":0"#
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains(r#""name":"maxDepth","value":"2""#),
        "{stdout}"
    );
    assert!(
        stdout.contains(r#""name":"debugState","value":"expanded","variablesReference":2"#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""name":"resultPreview""#), "{stdout}");
    assert!(
        stdout.contains(r#""name":"conditionChecks","value":"1""#),
        "{stdout}"
    );
    assert!(
        stdout.contains(r#""name":"conditionMatches","value":"1""#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""command":"next""#), "{stdout}");
    assert!(stdout.contains(r#""event":"terminated""#), "{stdout}");
}

#[test]
fn dap_exception_breakpoint_pauses_on_launch_error() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-dap-exception-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("boom.nox");
    fs::write(&path, "let value: int = 1 / 0;\nvalue;\n").unwrap();
    let input = [
        dap_frame(r#"{"seq":1,"type":"request","command":"initialize","arguments":{}}"#),
        dap_frame(r#"{"seq":2,"type":"request","command":"setExceptionBreakpoints","arguments":{"filters":["raised"]}}"#),
        dap_frame(&format!(
            r#"{{"seq":3,"type":"request","command":"launch","arguments":{{"program":"{}"}}}}"#,
            path.display()
        )),
        dap_frame(r#"{"seq":4,"type":"request","command":"configurationDone","arguments":{}}"#),
        dap_frame(r#"{"seq":5,"type":"request","command":"variables","arguments":{"variablesReference":2,"maxDepth":2}}"#),
        dap_frame(r#"{"seq":6,"type":"request","command":"disconnect","arguments":{}}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("dap")
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
    assert!(stdout.contains(r#""event":"stopped""#), "{stdout}");
    assert!(stdout.contains(r#""reason":"exception""#), "{stdout}");
    assert!(
        stdout.contains(r#""description":"raised error""#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""name":"exceptionMessage""#), "{stdout}");
    assert!(stdout.contains("division by zero"), "{stdout}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn dap_conditional_breakpoint_false_condition_terminates() {
    let path = fixture("examples/hello.nox");
    let input = [
        dap_frame(r#"{"seq":1,"type":"request","command":"initialize","arguments":{}}"#),
        dap_frame(&format!(
            r#"{{"seq":2,"type":"request","command":"setBreakpoints","arguments":{{"source":{{"path":"{}"}},"breakpoints":[{{"line":1,"condition":"result == 0"}}]}}}}"#,
            path.display()
        )),
        dap_frame(&format!(
            r#"{{"seq":3,"type":"request","command":"launch","arguments":{{"program":"{}"}}}}"#,
            path.display()
        )),
        dap_frame(r#"{"seq":4,"type":"request","command":"configurationDone","arguments":{}}"#),
        dap_frame(r#"{"seq":5,"type":"request","command":"variables","arguments":{"variablesReference":2,"maxDepth":2}}"#),
        dap_frame(r#"{"seq":6,"type":"request","command":"disconnect","arguments":{}}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("dap")
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
    assert!(!stdout.contains(r#""event":"stopped""#), "{stdout}");
    assert!(stdout.contains(r#""event":"terminated""#), "{stdout}");
    assert!(
        stdout.contains(r#""name":"conditionChecks","value":"1""#),
        "{stdout}"
    );
    assert!(
        stdout.contains(r#""name":"conditionMatches","value":"0""#),
        "{stdout}"
    );
}

#[test]
fn check_reports_ok_without_running() {
    for path in [
        "examples/arrays.nox",
        "examples/bitwise.nox",
        "examples/hello.nox",
        "examples/control-flow.nox",
        "examples/control-flow-let-patterns.nox",
        "examples/constants.nox",
        "examples/conversions.nox",
        "examples/else-if.nox",
        "examples/export-main.nox",
        "examples/for-range.nox",
        "examples/generic-functions.nox",
        "examples/logical.nox",
        "examples/loop-break-continue.nox",
        "examples/maps.nox",
        "examples/match.nox",
        "examples/numeric-boundaries.nox",
        "examples/print.nox",
        "examples/recursion.nox",
        "examples/records.nox",
        "examples/result-chain.nox",
        "examples/scopes.nox",
        "examples/spread.nox",
        "examples/string-escapes.nox",
        "tests/fixtures/string-and-map-builtins.nox",
        "examples/strings.nox",
        "examples/enums.nox",
        "examples/stdlib.nox",
        "examples/type-alias.nox",
        "tests/fixtures/enums.nox",
        "tests/fixtures/bitwise.nox",
        "tests/fixtures/control-flow-let-patterns.nox",
        "tests/fixtures/generic-functions.nox",
        "tests/fixtures/spread.nox",
        "tests/fixtures/type-alias.nox",
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
    let first = fixture("tests/fixtures/type-error.nox");
    let second = fixture("tests/fixtures/type-error-record-field-access.nox");
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
fn check_json_reports_string_interpolation_code() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-interpolation-code-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("interpolation.nox");
    fs::write(&path, r#""bad=${[1, 2]}";"#).unwrap();

    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"string.interpolation\""));
    assert!(stdout.contains("string interpolation cannot stringify"));
}

#[test]
fn check_json_reports_question_mark_mismatch_code() {
    let path = fixture("tests/fixtures/type-error-question-mark-mismatch.nox");
    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"result.question-mark.mismatch\""));
    assert!(stdout.contains("'?' error type mismatch"));
}

#[test]
fn check_json_reports_record_method_not_found_code() {
    let path = fixture("tests/fixtures/type-error-record-method-not-found.nox");
    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"record.method-not-found\""));
    assert!(stdout.contains("record 'User' has no method 'missing'"));
}

#[test]
fn check_json_and_lsp_report_trait_codes() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-trait-code-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.nox");
    let source = r#"fn label<T: Display>(value: T) -> str {
    return value.to_str();
}
"#;
    fs::write(&path, source).unwrap();

    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"trait.not-found\""));

    let uri = format!("file://{}", path.display());
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
    assert!(stdout.contains("\"code\":\"trait.not-found\""));
}

#[test]
fn check_json_and_lsp_report_async_codes() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-async-code-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.nox");
    let source = r#"async fn compute() -> int {
    return 1;
}

await compute();
"#;
    fs::write(&path, source).unwrap();

    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"async.await-outside-async\""));

    let uri = format!("file://{}", path.display());
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
    assert!(stdout.contains("\"code\":\"async.await-outside-async\""));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn check_json_reports_match_non_exhaustive_code() {
    let path = fixture("tests/fixtures/type-error-match-non-exhaustive.nox");
    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"match.non-exhaustive\""));
    assert!(stdout.contains("option match must cover some and none"));
}

#[test]
fn check_json_reports_tuple_codes() {
    let arity = fixture("tests/fixtures/type-error-tuple-arity.nox");
    let output = nox_command()
        .args(["check", "--json", arity.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"tuple.arity-mismatch\""));

    let element = fixture("tests/fixtures/type-error-tuple-element.nox");
    let output = nox_command()
        .args(["check", "--json", element.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"tuple.element-type-mismatch\""));
}

#[test]
fn check_json_reports_type_alias_cyclic_code() {
    let path = fixture("tests/fixtures/type-error-type-alias-cyclic.nox");
    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"type-alias.cyclic\""));
}

#[test]
fn check_json_reports_enum_codes() {
    let non_exhaustive = fixture("tests/fixtures/type-error-enum-non-exhaustive.nox");
    let output = nox_command()
        .args(["check", "--json", non_exhaustive.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"match.non-exhaustive\""));

    let missing_variant = fixture("tests/fixtures/type-error-enum-variant-not-found.nox");
    let output = nox_command()
        .args(["check", "--json", missing_variant.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"enum.variant-not-found\""));
}

#[test]
fn check_json_reports_generic_infer_failed_code() {
    let path = fixture("tests/fixtures/type-error-generic-infer-failed.nox");
    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"generic.infer-failed\""));
}

#[test]
fn check_json_reports_bitwise_non_int_code() {
    let path = fixture("tests/fixtures/type-error-bitwise-non-int.nox");
    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"type.bitwise-non-int\""));
}

#[test]
fn check_json_reports_let_else_fallthrough_code() {
    let path = fixture("tests/fixtures/type-error-let-else-fallthrough.nox");
    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"control-flow.let-else-fallthrough\""));
}

#[test]
fn check_json_reports_spread_mismatch_code() {
    let path = fixture("tests/fixtures/type-error-spread-mismatch.nox");
    let output = nox_command()
        .args(["check", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.check.v1\""));
    assert!(stdout.contains("\"code\":\"type.spread-mismatch\""));
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
    let first = fixture("tests/fixtures/type-error.nox");
    let second = fixture("tests/fixtures/type-error-record-field-access.nox");
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
    assert!(stdout.contains(
        "\"name\":\"test_pass\",\"ok\":true,\"attempts\":1,\"retried\":false,\"duration_us\":"
    ));
    assert!(stdout.contains("\"kind\":\"unit\""));
    assert!(stdout.contains("\"mock_events\":[]"));
    assert!(stdout.contains(
        "\"name\":\"test_fail\",\"ok\":false,\"attempts\":1,\"retried\":false,\"duration_us\":"
    ));
    assert!(stdout.contains(&format!(
        "\"suites\":[{{\"file\":\"{}\",\"cases\":[\"test_pass\",\"test_fail\"]}}]",
        path.display()
    )));
    assert!(stdout.contains("\"summary\":{\"tests\":2,\"passed\":1,\"failed\":1}"));
}

#[test]
fn test_json_captures_stdout_and_stderr_per_case() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-json-output-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("output_test.nox");
    fs::write(
        &path,
        "import \"std/process.nox\" as process;\n\nfn test_alpha() -> bool {\n    print(\"alpha out\");\n    process.print_err(\"alpha err\");\n    return true;\n}\n\nfn test_beta() -> bool {\n    print(\"beta out\");\n    return true;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\":\"test_alpha\""));
    assert!(stdout.contains("\"stdout\":\"alpha out\\n\""));
    assert!(stdout.contains("\"stderr\":\"alpha err\\n\""));
    assert!(stdout.contains("\"name\":\"test_beta\""));
    assert!(stdout.contains("\"stdout\":\"beta out\\n\""));
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
fn test_json_reports_runtime_stack_frames() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-stack-json-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("stack_test.nox");
    fs::write(
        &path,
        "fn divide(value: int) -> int {\n    return value / 0;\n}\n\nfn wrapper(value: int) -> int {\n    return divide(value);\n}\n\nfn test_stack() -> bool {\n    return wrapper(1) == 0;\n}\n",
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
    assert!(stdout.contains("\"code\":\"runtime.division-by-zero\""));
    assert!(stdout.contains("\"stack_frames\":["));
    assert!(stdout.contains("\"name\":\"divide\""));
    assert!(stdout.contains("\"name\":\"wrapper\""));
    assert!(stdout.contains("\"name\":\"test_stack\""));
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
fn new_creates_project_that_passes_project_commands() {
    let dir = std::env::temp_dir().join(format!("nox-cli-new-{}-{}", std::process::id(), line!()));
    let project = dir.join("demo_app");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let output = nox_command()
        .current_dir(&dir)
        .args(["new", "demo_app"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(project.join("nox.toml").is_file());
    assert!(project.join("src/main.nox").is_file());
    assert!(project.join("tests/main_test.nox").is_file());
    assert!(project.join("README.md").is_file());

    let project_check = nox_command()
        .current_dir(&project)
        .args(["project", "check"])
        .output()
        .unwrap();
    assert!(
        project_check.status.success(),
        "{}",
        String::from_utf8_lossy(&project_check.stderr)
    );

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
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hello, nox\n");

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
    assert!(String::from_utf8_lossy(&test.stdout).contains("summary: 1 tests, 1 passed, 0 failed"));

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
}

#[test]
fn new_supports_explicit_dir_and_rejects_non_empty_target_without_force() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-new-dir-{}-{}",
        std::process::id(),
        line!()
    ));
    let project = dir.join("custom-target");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("notes.txt"), "keep me").unwrap();

    let reject = nox_command()
        .args(["new", "demo_app", "--dir", project.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(reject.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&reject.stderr).contains("already exists and is not empty"));
    assert!(!project.join("nox.toml").exists());

    let create = nox_command()
        .args([
            "new",
            "demo_app",
            "--dir",
            project.to_str().unwrap(),
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );
    assert_eq!(
        fs::read_to_string(project.join("notes.txt")).unwrap(),
        "keep me"
    );
    assert!(project.join("nox.toml").is_file());
}

#[test]
fn new_force_overwrites_only_scaffold_files() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-new-force-{}-{}",
        std::process::id(),
        line!()
    ));
    let project = dir.join("demo_app");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(project.join("src/main.nox"), "broken").unwrap();
    fs::write(project.join("custom.txt"), "custom").unwrap();

    let output = nox_command()
        .args([
            "new",
            "demo_app",
            "--dir",
            project.to_str().unwrap(),
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fs::read_to_string(project.join("src/main.nox"))
        .unwrap()
        .contains("export fn greet"));
    assert_eq!(
        fs::read_to_string(project.join("custom.txt")).unwrap(),
        "custom"
    );
}

#[test]
fn new_rejects_invalid_package_name() {
    let output = nox_command().args(["new", "Bad/Name"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("package name must start with an ASCII lowercase letter"));
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
        assert!(stdout.contains(
            "\"schema_validation\":{\"ok\":true,\"manifest_sections\":[\"package\",\"entrypoints\",\"modules\",\"dependencies\",\"runtime\"],\"unknown_sections\":\"rejected\",\"unknown_keys\":\"rejected\"}"
        ));
        assert!(stdout.contains(&format!(
            "\"entrypoints\":{{\"main\":\"{}\",\"named\":[",
            project_root.join("src/main.nox").display()
        )));
        assert!(stdout.contains("\"capabilities\":{\"declared\":[]}"));
        assert!(stdout.contains("\"dependencies\":{\"declared\":[],\"lockfile\":{"));
        assert!(stdout.contains("\"ok\":true,\"status\":\"not_required\",\"diagnostics\":[]"));
        assert!(stdout.contains(&format!(
            "\"module_graph\":{{\"roots\":[\"{}\"],\"files\":[",
            project_root.join("src").display()
        )));
        for module in [
            "src/labels.nox",
            "src/main.nox",
            "src/runtime_info.nox",
            "src/scoring.nox",
        ] {
            assert!(
                stdout.contains(&format!("\"{}\"", project_root.join(module).display())),
                "missing module graph file {module} in {stdout}"
            );
        }
        assert!(stdout.contains("\"name\":\"check\",\"ok\":true,\"status\":0"));
        assert!(stdout.contains("\"name\":\"test\",\"ok\":true,\"status\":0"));
        assert!(stdout.contains("\"name\":\"fmt\",\"ok\":true,\"status\":0"));
        assert!(stdout.contains("\"summary\":{\"steps\":3,\"passed\":3,\"failed\":0}"));
        assert!(stdout.contains("\\\"name\\\":\\\"test_manifest_present\\\""));
    }
}

#[test]
fn project_check_json_reports_pinned_dependencies() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-project-check-deps-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("nox.toml"),
        r#"[package]
name = "project-deps"
version = "0.0.1"

[entrypoints]
main = "src/main.nox"

[modules]
source_dirs = ["src"]

[dependencies]
mathx = { github = "owner/mathx", rev = "0123456789abcdef0123456789abcdef01234567" }
tools = { git = "https://github.com/owner/tools.git", tag = "v0.2.0" }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("nox.lock"),
        r#"[lock]
version = "1"

[dependencies.mathx]
source_kind = "github"
source = "owner/mathx"
pin_kind = "rev"
pin = "0123456789abcdef0123456789abcdef01234567"
resolved = "0123456789abcdef0123456789abcdef01234567"
content_hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
cache_key = "github-owner-mathx-0123456789abcdef0123456789abcdef01234567"
tool = "nox 0.0.4"

[dependencies.tools]
source_kind = "git"
source = "https://github.com/owner/tools.git"
pin_kind = "tag"
pin = "v0.2.0"
resolved = "fedcba9876543210fedcba9876543210fedcba98"
content_hash = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
cache_key = "git-owner-tools-fedcba9876543210fedcba9876543210fedcba98"
tool = "nox 0.0.4"
"#,
    )
    .unwrap();
    fs::write(dir.join("src/main.nox"), "1;\n").unwrap();

    let output = nox_command()
        .current_dir(&dir)
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
    assert!(stdout.contains("\"dependencies\":{\"declared\":["));
    assert!(stdout.contains(
        "\"name\":\"mathx\",\"source\":{\"kind\":\"github\",\"value\":\"owner/mathx\"},\"pin\":{\"kind\":\"rev\",\"value\":\"0123456789abcdef0123456789abcdef01234567\"}"
    ));
    assert!(stdout.contains(
        "\"name\":\"tools\",\"source\":{\"kind\":\"git\",\"value\":\"https://github.com/owner/tools.git\"},\"pin\":{\"kind\":\"tag\",\"value\":\"v0.2.0\"}"
    ));
    assert!(stdout.contains("\"lockfile\":{"));
    assert!(stdout.contains("\"ok\":true,\"status\":\"ok\",\"diagnostics\":[]"));
}

#[test]
fn project_check_json_reports_missing_dependency_lockfile() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-project-check-missing-lock-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("nox.toml"),
        r#"[package]
name = "project-deps"
version = "0.0.1"

[entrypoints]
main = "src/main.nox"

[modules]
source_dirs = ["src"]

[dependencies]
mathx = { github = "owner/mathx", rev = "0123456789abcdef0123456789abcdef01234567" }
"#,
    )
    .unwrap();
    fs::write(dir.join("src/main.nox"), "1;\n").unwrap();

    let output = nox_command()
        .current_dir(&dir)
        .args(["project", "check", "--json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"status\":\"missing\""));
    assert!(stdout.contains("\"code\":\"lockfile.invalid\""));
    assert!(stdout.contains("nox.lock is required"));
}

#[test]
fn fetch_writes_lockfile_and_reuses_cache_offline() {
    let dir =
        std::env::temp_dir().join(format!("nox-cli-fetch-{}-{}", std::process::id(), line!()));
    let remote = create_local_bare_git_dependency(&dir);
    let project = dir.join("project");
    let cache = dir.join("cache");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(
        project.join("nox.toml"),
        format!(
            r#"[package]
name = "fetch-demo"
version = "0.0.1"

[entrypoints]
main = "src/main.nox"

[modules]
source_dirs = ["src"]

[dependencies]
dep = {{ git = "file://{}", tag = "v0.1.0" }}
"#,
            remote.display()
        ),
    )
    .unwrap();
    fs::write(project.join("src/main.nox"), "1;\n").unwrap();

    let output = nox_command()
        .current_dir(&project)
        .args(["fetch", "--cache-dir", cache.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("fetch: wrote"));
    let lockfile = fs::read_to_string(project.join("nox.lock")).unwrap();
    assert!(lockfile.contains("[dependencies.dep]"));
    assert!(lockfile.contains("source_kind = \"git\""));
    assert!(lockfile.contains("pin_kind = \"tag\""));
    assert!(lockfile.contains("pin = \"v0.1.0\""));
    assert!(lockfile.contains("content_hash = \"sha256:"));

    let offline = nox_command()
        .current_dir(&project)
        .args(["fetch", "--offline", "--cache-dir", cache.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        offline.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&offline.stdout),
        String::from_utf8_lossy(&offline.stderr)
    );
    assert!(String::from_utf8_lossy(&offline.stdout).contains("fetch: dep"));

    let check = nox_command()
        .current_dir(&project)
        .args(["project", "check", "--json"])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(String::from_utf8_lossy(&check.stdout).contains("\"status\":\"ok\""));
}

#[test]
fn fetch_offline_reports_cache_miss() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-fetch-offline-miss-{}-{}",
        std::process::id(),
        line!()
    ));
    let remote = create_local_bare_git_dependency(&dir);
    let project = dir.join("project");
    let cache = dir.join("cache");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("nox.toml"),
        format!(
            "[package]\nname = \"fetch-demo\"\nversion = \"0.0.1\"\n\n[dependencies]\ndep = {{ git = \"file://{}\", tag = \"v0.1.0\" }}\n",
            remote.display()
        ),
    )
    .unwrap();

    let output = nox_command()
        .current_dir(&project)
        .args(["fetch", "--offline", "--cache-dir", cache.to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cache miss in offline mode"));
    assert!(!project.join("nox.lock").exists());
}

#[test]
fn fetch_offline_reports_corrupt_cache() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-fetch-corrupt-cache-{}-{}",
        std::process::id(),
        line!()
    ));
    let remote = create_local_bare_git_dependency(&dir);
    let project = dir.join("project");
    let cache = dir.join("cache");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("nox.toml"),
        format!(
            "[package]\nname = \"fetch-demo\"\nversion = \"0.0.1\"\n\n[dependencies]\ndep = {{ git = \"file://{}\", tag = \"v0.1.0\" }}\n",
            remote.display()
        ),
    )
    .unwrap();

    let online = nox_command()
        .current_dir(&project)
        .args(["fetch", "--cache-dir", cache.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(online.status.success());
    let cache_entry = fs::read_dir(&cache)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::remove_dir_all(cache_entry.join("objects")).unwrap();

    let offline = nox_command()
        .current_dir(&project)
        .args(["fetch", "--offline", "--cache-dir", cache.to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(offline.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&offline.stderr).contains("failed to resolve pin"));
}

#[test]
fn external_dependency_imports_resolve_from_lockfile_cache() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-external-import-{}-{}",
        std::process::id(),
        line!()
    ));
    let remote = create_local_bare_git_dependency(&dir);
    let project = dir.join("project");
    let cache = dir.join("cache");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::create_dir_all(project.join("tests")).unwrap();
    fs::write(
        project.join("nox.toml"),
        format!(
            r#"[package]
name = "external-import-demo"
version = "0.0.1"

[entrypoints]
main = "src/main.nox"

[modules]
source_dirs = ["src"]
test_dirs = ["tests"]

[dependencies]
dep = {{ git = "file://{}", tag = "v0.1.0" }}
"#,
            remote.display()
        ),
    )
    .unwrap();
    fs::write(
        project.join("src/main.nox"),
        "import \"dep/math.nox\" as math;\n\nmath.answer();\n",
    )
    .unwrap();
    fs::write(
        project.join("tests/main_test.nox"),
        "import \"dep/math.nox\" as math;\n\nfn test_external_answer() -> bool { return math.answer() == 42; }\n",
    )
    .unwrap();

    let fetch = nox_command()
        .current_dir(&project)
        .args(["fetch", "--cache-dir", cache.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        fetch.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&fetch.stdout),
        String::from_utf8_lossy(&fetch.stderr)
    );

    let check = nox_command()
        .current_dir(&project)
        .env("NOX_MODULE_CACHE", &cache)
        .args(["check"])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = nox_command()
        .current_dir(&project)
        .env("NOX_MODULE_CACHE", &cache)
        .args(["run"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "42");

    let test = nox_command()
        .current_dir(&project)
        .env("NOX_MODULE_CACHE", &cache)
        .args(["test"])
        .output()
        .unwrap();
    assert!(
        test.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&test.stdout),
        String::from_utf8_lossy(&test.stderr)
    );
    assert!(String::from_utf8_lossy(&test.stdout).contains("test_external_answer PASS"));
}

#[test]
fn external_dependency_import_reports_missing_cache() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-external-import-cache-miss-{}-{}",
        std::process::id(),
        line!()
    ));
    let remote = create_local_bare_git_dependency(&dir);
    let project = dir.join("project");
    let cache = dir.join("cache");
    let missing_cache = dir.join("missing-cache");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(
        project.join("nox.toml"),
        format!(
            "[package]\nname = \"external-import-demo\"\nversion = \"0.0.1\"\n\n[entrypoints]\nmain = \"src/main.nox\"\n\n[modules]\nsource_dirs = [\"src\"]\n\n[dependencies]\ndep = {{ git = \"file://{}\", tag = \"v0.1.0\" }}\n",
            remote.display()
        ),
    )
    .unwrap();
    fs::write(
        project.join("src/main.nox"),
        "import \"dep/math.nox\" as math;\n\nmath.answer();\n",
    )
    .unwrap();
    let fetch = nox_command()
        .current_dir(&project)
        .args(["fetch", "--cache-dir", cache.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(fetch.status.success());

    let output = nox_command()
        .current_dir(&project)
        .env("NOX_MODULE_CACHE", &missing_cache)
        .args(["check"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cache is missing"),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        stderr
    );
    assert!(
        stderr.contains("run nox fetch"),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        stderr
    );

    let json = nox_command()
        .current_dir(&project)
        .env("NOX_MODULE_CACHE", &missing_cache)
        .args(["check", "--json"])
        .output()
        .unwrap();

    assert_eq!(json.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&json.stdout);
    assert!(
        stdout.contains("\"code\":\"module.cache-missing\""),
        "stdout:\n{}\nstderr:\n{}",
        stdout,
        String::from_utf8_lossy(&json.stderr)
    );
    assert!(stdout.contains("run nox fetch"));
}

#[test]
fn external_dependency_import_reports_corrupt_cache_json() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-external-import-corrupt-cache-{}-{}",
        std::process::id(),
        line!()
    ));
    let remote = create_local_bare_git_dependency(&dir);
    let project = dir.join("project");
    let cache = dir.join("cache");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(
        project.join("nox.toml"),
        format!(
            "[package]\nname = \"external-import-demo\"\nversion = \"0.0.1\"\n\n[entrypoints]\nmain = \"src/main.nox\"\n\n[modules]\nsource_dirs = [\"src\"]\n\n[dependencies]\ndep = {{ git = \"file://{}\", tag = \"v0.1.0\" }}\n",
            remote.display()
        ),
    )
    .unwrap();
    fs::write(
        project.join("src/main.nox"),
        "import \"dep/math.nox\" as math;\n\nmath.answer();\n",
    )
    .unwrap();
    let fetch = nox_command()
        .current_dir(&project)
        .args(["fetch", "--cache-dir", cache.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        fetch.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&fetch.stdout),
        String::from_utf8_lossy(&fetch.stderr)
    );
    let cache_entry = fs::read_dir(&cache)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::remove_dir_all(cache_entry.join("objects")).unwrap();

    let output = nox_command()
        .current_dir(&project)
        .env("NOX_MODULE_CACHE", &cache)
        .args(["check", "--json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"code\":\"module.cache-corrupt\""),
        "stdout:\n{}\nstderr:\n{}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("cache is corrupt"));
}

#[test]
fn external_dependency_import_reports_hash_mismatch() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-external-import-hash-mismatch-{}-{}",
        std::process::id(),
        line!()
    ));
    let remote = create_local_bare_git_dependency(&dir);
    let project = dir.join("project");
    let cache = dir.join("cache");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(
        project.join("nox.toml"),
        format!(
            "[package]\nname = \"external-import-demo\"\nversion = \"0.0.1\"\n\n[entrypoints]\nmain = \"src/main.nox\"\n\n[modules]\nsource_dirs = [\"src\"]\n\n[dependencies]\ndep = {{ git = \"file://{}\", tag = \"v0.1.0\" }}\n",
            remote.display()
        ),
    )
    .unwrap();
    fs::write(
        project.join("src/main.nox"),
        "import \"dep/math.nox\" as math;\n\nmath.answer();\n",
    )
    .unwrap();
    let fetch = nox_command()
        .current_dir(&project)
        .args(["fetch", "--cache-dir", cache.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        fetch.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&fetch.stdout),
        String::from_utf8_lossy(&fetch.stderr)
    );

    let lock_path = project.join("nox.lock");
    let lockfile = fs::read_to_string(&lock_path).unwrap();
    let bad_lockfile = lockfile
        .lines()
        .map(|line| {
            if line.starts_with("content_hash = \"sha256:") {
                "content_hash = \"sha256:0000000000000000000000000000000000000000000000000000000000000000\""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&lock_path, format!("{bad_lockfile}\n")).unwrap();

    let output = nox_command()
        .current_dir(&project)
        .env("NOX_MODULE_CACHE", &cache)
        .args(["check", "--json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"code\":\"module.hash-mismatch\""),
        "stdout:\n{}\nstderr:\n{}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("content hash mismatch"));
}

fn create_local_bare_git_dependency(root: &PathBuf) -> PathBuf {
    let work = root.join("dep-work");
    let remote = root.join("dep.git");
    fs::create_dir_all(&work).unwrap();
    fs::create_dir_all(root).unwrap();
    run_git_test(["init"], &work);
    fs::write(
        work.join("math.nox"),
        "export fn answer() -> int { return 42; }\n",
    )
    .unwrap();
    run_git_test(["add", "math.nox"], &work);
    run_git_test(
        [
            "-c",
            "user.name=Nox Test",
            "-c",
            "user.email=nox@example.invalid",
            "commit",
            "-m",
            "initial",
        ],
        &work,
    );
    run_git_test(["tag", "v0.1.0"], &work);
    run_git_test(["init", "--bare", remote.to_str().unwrap()], root);
    run_git_test(["remote", "add", "origin", remote.to_str().unwrap()], &work);
    run_git_test(["push", "--tags", "origin", "HEAD:main"], &work);
    remote
}

fn run_git_test<const N: usize>(args: [&str; N], cwd: &PathBuf) {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git failed in {}:\nstdout:\n{}\nstderr:\n{}",
        cwd.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
fn fmt_prints_trait_and_impl_blocks() {
    let dir = std::env::temp_dir().join(format!("nox-cli-fmt-trait-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("trait.nox");
    fs::write(
        &path,
        r#"trait Display{fn to_str(self:Self)->str;}record User{name:str,}impl Display for User{fn to_str(self:User)->str{return self.name;}}"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["fmt", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("trait Display {\n    fn to_str(self: Self) -> str;"));
    assert!(stdout.contains("impl Display for User {\n    fn to_str(self: User) -> str {"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn fmt_prints_async_fn_and_await() {
    let dir = std::env::temp_dir().join(format!("nox-cli-fmt-async-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("async.nox");
    fs::write(
        &path,
        r#"async fn compute(value:int)->int{return await value;}"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["fmt", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "async fn compute(value: int) -> int {\n    return await value;\n}\n"
    );
    assert!(output.stderr.is_empty());
    fs::remove_dir_all(&dir).ok();
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
    let path = fixture("tests/fixtures/formatter-golden.nox");
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
    assert!(stdout.contains("\"data\":{\"trace_id\":\"lsp-"));
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
    let original = r#"let value: int = 1; value;"#;
    let updated = r#"let value: int = "bad"; value;"#;
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
fn lsp_emits_code_lens_for_test_functions() {
    let source = "fn helper() -> int { return 0; }\nfn test_helper_returns_zero() -> bool { return helper() == 0; }\nfn test_helper_is_idempotent() -> bool { return helper() == helper(); }\n";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///lens.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/codeLens","params":{"textDocument":{"uri":"file:///lens.nox"}}}"#),
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
    // initialize result should declare codeLensProvider
    assert!(stdout.contains("\"codeLensProvider\""));
    // helper should NOT have a lens
    assert!(!stdout.contains("\"title\":\"Run helper\""));
    // both test_* functions should have lenses
    assert!(stdout.contains("\"title\":\"Run test_helper_returns_zero\""));
    assert!(stdout.contains("\"title\":\"Run test_helper_is_idempotent\""));
    assert!(stdout.contains("\"command\":\"nox.runTest\""));
}

#[test]
fn lsp_discovers_test_functions_for_editors() {
    let source = "fn helper() -> int { return 0; }\nfn test_one() -> bool { return true; }\nfn test_two() -> bool { return helper() == 0; }\n";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///discover.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"nox/testDiscovery","params":{"textDocument":{"uri":"file:///discover.nox"}}}"#),
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
    assert!(stdout.contains("\"uri\":\"file:///discover.nox\""));
    assert!(stdout.contains("\"name\":\"test_one\""));
    assert!(stdout.contains("\"name\":\"test_two\""));
    assert!(!stdout.contains("\"name\":\"helper\""));
    assert!(stdout.contains("\"range\":{\"start\":{\"line\":1"));
}

#[test]
fn lsp_hover_includes_doc_comment_for_top_level_function() {
    let source = "/// Doubles the input value.\n/// Useful for tests.\nexport fn double(x: int) -> int { return x * 2; }\ndouble(2);";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///hover.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        // hover on the second `double` (the call site, line 3 character 0..)
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///hover.nox"},"position":{"line":3,"character":0}}}"#),
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
    assert!(
        stdout.contains("Doubles the input value."),
        "expected hover to include doc comment, got: {stdout}"
    );
    assert!(stdout.contains("Useful for tests."));
}

#[test]
fn lsp_hover_on_import_alias_shows_module_surface() {
    let dir = std::env::temp_dir().join(format!(
        "nox-lsp-module-hover-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("helper.nox"),
        "export fn answer() -> int { return 42; }\nfn private() -> int { return 0; }\n",
    )
    .unwrap();
    let main_path = dir.join("main.nox");
    let main_uri = format!("file://{}", main_path.display());
    let source = "import \"std/fs.nox\" as fs;\nimport \"helper.nox\" as helper;\nfs.exists(\"x\");\nhelper.answer();\n";
    let (std_line, std_character) = lsp_position(source, source.find("fs;").unwrap());
    let (helper_line, helper_character) = lsp_position(source, source.find("helper;").unwrap());
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            main_uri,
            json_escape(source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{std_line},"character":{std_character}}}}}}}"#,
            main_uri
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{helper_line},"character":{helper_character}}}}}}}"#,
            main_uri
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":4,"method":"shutdown","params":null}"#),
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
    let std_hover = stdout
        .split("\"id\":2")
        .nth(1)
        .expect("std module hover response should be present");
    assert!(std_hover.contains("module std/fs.nox"), "{stdout}");
    assert!(std_hover.contains("exports: read_text"), "{stdout}");
    assert!(std_hover.contains("exists"), "{stdout}");
    let helper_hover = stdout
        .split("\"id\":3")
        .nth(1)
        .expect("project module hover response should be present");
    assert!(helper_hover.contains("module helper.nox"), "{stdout}");
    assert!(helper_hover.contains("exports: answer"), "{stdout}");
    assert!(!helper_hover.contains("private"), "{stdout}");
}

#[test]
fn lsp_publishes_lint_warnings_with_severity_2() {
    let source = "let unused: int = 1; let used: int = 2; used;";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///lint.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
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
    assert!(stdout.contains("\"code\":\"lint.unused-variable\""));
    assert!(stdout.contains("\"severity\":2"));
    assert!(stdout.contains("\"data\":{\"trace_id\":\"lsp-"));
    assert!(stdout.contains("'unused'"));
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
    assert!(stdout.contains("\"label\":\"async\""));
    assert!(stdout.contains("\"label\":\"await\""));
    assert!(stdout.contains("\"label\":\"break\""));
    assert!(stdout.contains("\"label\":\"continue\""));
    assert!(stdout.contains("\"label\":\"len\""));
    assert!(stdout.contains("\"label\":\"contains\""));
    assert!(stdout.contains("\"label\":\"map_get\""));
    assert!(stdout.contains("\"label\":\"read_text\""));
    assert!(stdout.contains("fn read_text(path: str) -> str"));
    assert!(stdout.contains("Read a UTF-8 text file through the host filesystem boundary."));
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
fn lsp_completion_suggests_import_specifiers() {
    let dir = std::env::temp_dir().join(format!(
        "nox-lsp-import-complete-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src/nested")).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"import-complete\"\nversion = \"0.0.1\"\n\n[modules]\nsource_dirs = [\"src\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/math.nox"),
        "export fn answer() -> int { return 42; }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/nested/util.nox"),
        "export fn ok() -> bool { return true; }\n",
    )
    .unwrap();
    let main_path = dir.join("src/main.nox");
    let main_uri = format!("file://{}", main_path.display());
    let source = "import \"std/";

    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            main_uri,
            json_escape(source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":0,"character":12}}}}}}"#,
            main_uri
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{}","version":2}},"contentChanges":[{{"text":"import \"n"}}]}}}}"#,
            main_uri
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":0,"character":9}}}}}}"#,
            main_uri
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":4,"method":"shutdown","params":null}"#),
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
    let std_completion = stdout
        .split("\"id\":2")
        .nth(1)
        .expect("std import completion response should be present");
    assert!(
        std_completion.contains("\"label\":\"std/fs.nox\""),
        "{stdout}"
    );
    assert!(
        std_completion.contains("\"label\":\"std/json.nox\""),
        "{stdout}"
    );
    let project_completion = stdout
        .split("\"id\":3")
        .nth(1)
        .expect("project import completion response should be present");
    assert!(
        project_completion.contains("\"label\":\"nested/util.nox\""),
        "{stdout}"
    );
}

#[test]
fn lsp_completion_includes_project_top_level_symbols() {
    let dir = std::env::temp_dir().join(format!(
        "nox-lsp-project-complete-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"project-complete\"\nversion = \"0.0.1\"\n\n[modules]\nsource_dirs = [\"src\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/model.nox"),
        "export trait Display {\n    fn to_str(self: Self) -> str;\n}\n\nexport record Profile {\n    name: str,\n}\n\ntype ProjectId = int;\nlet hidden_value: int = 1;\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/service.nox"),
        "export fn project_answer() -> int {\n    return 42;\n}\n",
    )
    .unwrap();
    let main_path = dir.join("src/main.nox");
    let main_uri = format!("file://{}", main_path.display());
    let root_uri = format!("file://{}", dir.display());

    let input = [
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"rootUri":"{}"}}}}"#,
            root_uri
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"pro"}}}}}}"#,
            main_uri
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":0,"character":3}}}}}}"#,
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
        .expect("project completion response should be present");
    assert!(
        completion.contains("\"label\":\"project_answer\""),
        "{stdout}"
    );
    assert!(completion.contains("\"label\":\"Profile\""), "{stdout}");
    assert!(completion.contains("\"label\":\"Display\""), "{stdout}");
    assert!(
        completion.contains("\"label\":\"Display\",\"kind\":8"),
        "{stdout}"
    );
    assert!(completion.contains("\"label\":\"ProjectId\""), "{stdout}");
    assert!(
        !completion.contains("\"label\":\"hidden_value\""),
        "{stdout}"
    );
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
    assert!(
        completion.contains("\"label\":\"read_text_async\""),
        "{stdout}"
    );
    assert!(
        completion.contains("\"label\":\"write_binary_async\""),
        "{stdout}"
    );
}

#[test]
fn lsp_completion_includes_trait_and_record_methods() {
    let source = "trait Display {\n    fn to_str(self: Self) -> str;\n}\n\nrecord User {\n    name: str,\n}\n\nimpl Display for User {\n    fn to_str(self: User) -> str {\n        return self.name;\n    }\n}\n\nfn label(self: User) -> str {\n    return self.name;\n}\n\nlet user: User = User { name: \"Ada\" };\nuser.";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///method-complete.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":"file:///method-complete.nox"},"position":{"line":19,"character":5}}}"#),
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
        .expect("method completion response should be present");
    assert!(completion.contains("\"label\":\"to_str\""), "{stdout}");
    assert!(completion.contains("\"label\":\"label\""), "{stdout}");
}

#[test]
fn lsp_completion_includes_std_string_members() {
    let source = "import \"std/string.nox\" as string;\n\nstring.split(\"a,b\", \",\");\n";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///std-string-main.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":"file:///std-string-main.nox"},"position":{"line":2,"character":7}}}"#),
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
        .expect("std string completion response should be present");
    for label in [
        "split",
        "substring",
        "trim",
        "replace",
        "starts_with",
        "ends_with",
        "index_of",
        "to_upper",
        "to_lower",
    ] {
        assert!(
            completion.contains(&format!("\"label\":\"{label}\"")),
            "{stdout}"
        );
    }
}

#[test]
fn lsp_supports_scoreboard_project_workflow() {
    let main_path = fixture("examples/projects/scoreboard/src/main.nox");
    let main_uri = format!("file://{}", main_path.display());
    let main_source = fs::read_to_string(&main_path).unwrap();
    let completion_offset = main_source.find("scoring.total").unwrap() + "scoring.".len();
    let (line, character) = lsp_position(&main_source, completion_offset);
    let runtime_test_path = fixture("examples/projects/scoreboard/tests/runtime_info_test.nox");
    let runtime_test_uri = format!("file://{}", runtime_test_path.display());
    let runtime_test_source = fs::read_to_string(&runtime_test_path).unwrap();
    let runtime_completion_offset = runtime_test_source
        .find("runtime_info.has_manifest")
        .unwrap()
        + "runtime_info.".len();
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
fn lsp_hover_and_signature_show_async_call_return() {
    let source = "/// Computes asynchronously.\nasync fn compute(value: int) -> int {\n    return value + 1;\n}\nlet task: task[int] = compute(41);\n";
    let hover_offset = source.rfind("compute").unwrap();
    let (hover_line, hover_character) = lsp_position(source, hover_offset);
    let signature_offset = source.find("41").unwrap();
    let (signature_line, signature_character) = lsp_position(source, signature_offset);
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///async-hover.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{{"textDocument":{{"uri":"file:///async-hover.nox"}},"position":{{"line":{hover_line},"character":{hover_character}}}}}}}"#
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"textDocument/signatureHelp","params":{{"textDocument":{{"uri":"file:///async-hover.nox"}},"position":{{"line":{signature_line},"character":{signature_character}}}}}}}"#
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":4,"method":"shutdown","params":null}"#),
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
    let hover = stdout
        .split("\"id\":2")
        .nth(1)
        .expect("async hover response should be present");
    assert!(hover.contains("task[int]"), "{stdout}");
    assert!(
        hover.contains("async fn compute(value: int) -\\u003e int (task[int])")
            || hover.contains("async fn compute(value: int) -> int (task[int])"),
        "{stdout}"
    );
    assert!(hover.contains("Computes asynchronously."), "{stdout}");
    let signature = stdout
        .split("\"id\":3")
        .nth(1)
        .expect("async signature response should be present");
    assert!(
        signature.contains("async fn compute(value: int) -\\u003e int (task[int])")
            || signature.contains("async fn compute(value: int) -> int (task[int])"),
        "{stdout}"
    );
}

#[test]
fn lsp_signature_help_returns_function_parameters() {
    let source =
        "fn add(left: int, right: int) -> int {\n    return left + right;\n}\nadd(1, 2);\n";
    let (line, character) = lsp_position(source, source.find('2').unwrap());
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///signature.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/signatureHelp","params":{{"textDocument":{{"uri":"file:///signature.nox"}},"position":{{"line":{line},"character":{character}}}}}}}"#
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
    assert!(stdout.contains("\"signatureHelpProvider\""), "{stdout}");
    assert!(stdout.contains("\"definitionProvider\":true"), "{stdout}");
    assert!(
        stdout.contains("\"documentSymbolProvider\":true"),
        "{stdout}"
    );
    assert!(
        stdout.contains("fn add(left: int, right: int) -> int"),
        "{stdout}"
    );
    assert!(stdout.contains("\"activeParameter\":1"), "{stdout}");
}

#[test]
fn lsp_uses_host_metadata_for_signature_and_hover() {
    let source = "read_text(\"input.txt\");\n";
    let signature_offset = source.find("input").unwrap();
    let (signature_line, signature_character) = lsp_position(source, signature_offset);
    let hover_offset = source.find("read_text").unwrap();
    let (hover_line, hover_character) = lsp_position(source, hover_offset);
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///host-metadata.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/signatureHelp","params":{{"textDocument":{{"uri":"file:///host-metadata.nox"}},"position":{{"line":{signature_line},"character":{signature_character}}}}}}}"#
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{{"textDocument":{{"uri":"file:///host-metadata.nox"}},"position":{{"line":{hover_line},"character":{hover_character}}}}}}}"#
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":4,"method":"shutdown","params":null}"#),
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
    assert!(
        stdout.contains("fn read_text(path: str) -> str"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Read a UTF-8 text file through the host filesystem boundary."),
        "{stdout}"
    );
    assert!(stdout.contains("capabilities: filesystem"), "{stdout}");
}

#[test]
fn host_metadata_json_reports_registered_host_docs_and_capabilities() {
    let output = nox_command()
        .args(["host-metadata", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"schema\":\"nox.host-metadata.v1\""),
        "{stdout}"
    );
    assert!(stdout.contains("\"name\":\"read_text\""), "{stdout}");
    assert!(stdout.contains("\"return_type\":\"str\""), "{stdout}");
    assert!(
        stdout.contains("Read a UTF-8 text file through the host filesystem boundary."),
        "{stdout}"
    );
    assert!(
        stdout.contains("\"capabilities\":[\"filesystem\"]"),
        "{stdout}"
    );
}

#[test]
fn lsp_document_symbol_returns_top_level_declarations() {
    let source = "export trait Display {\n    fn to_str(self: Self) -> str;\n}\n\nexport record User {\n    name: str,\n}\n\nasync fn answer() -> int {\n    return 42;\n}\n\nlet value: task[int] = answer();\n";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///symbols.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":"file:///symbols.nox"}}}"#),
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
    assert!(
        stdout.contains("\"documentSymbolProvider\":true"),
        "{stdout}"
    );
    assert!(stdout.contains("\"id\":2"), "{stdout}");
    assert!(
        stdout.contains("\"name\":\"Display\",\"kind\":11"),
        "{stdout}"
    );
    assert!(stdout.contains("\"name\":\"User\",\"kind\":23"), "{stdout}");
    assert!(
        stdout.contains("\"name\":\"answer\",\"kind\":12"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\"name\":\"value\",\"kind\":13"),
        "{stdout}"
    );
}

#[test]
fn lsp_workspace_symbol_returns_project_top_level_declarations() {
    let dir = std::env::temp_dir().join(format!(
        "nox-lsp-workspace-symbol-{}-{}",
        std::process::id(),
        line!()
    ));
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"symbols\"\nversion = \"0.0.1\"\n\n[entrypoints]\nmain = \"src/main.nox\"\n\n[modules]\nsource_dirs = [\"src\"]\n",
    )
    .unwrap();
    fs::write(
        src.join("main.nox"),
        "export record User {\n    name: str,\n}\n\nfn answer() -> int {\n    return 42;\n}\n",
    )
    .unwrap();
    fs::write(
        src.join("events.nox"),
        "export trait Display {\n    fn to_str(self: Self) -> str;\n}\n\nexport enum Event {\n    Started,\n}\n\ntype UserId = int;\nlet hidden: int = 1;\n",
    )
    .unwrap();
    let root_uri = format!("file://{}", dir.display());

    let input = [
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"rootUri":"{}"}}}}"#,
            root_uri
        )),
        lsp_frame(
            r#"{"jsonrpc":"2.0","id":2,"method":"workspace/symbol","params":{"query":"user"}}"#,
        ),
        lsp_frame(r#"{"jsonrpc":"2.0","id":3,"method":"workspace/symbol","params":{"query":""}}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","id":4,"method":"shutdown","params":null}"#),
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
    assert!(
        stdout.contains("\"workspaceSymbolProvider\":true"),
        "{stdout}"
    );
    assert!(stdout.contains("\"id\":2"), "{stdout}");
    assert!(stdout.contains("\"name\":\"User\",\"kind\":23"), "{stdout}");
    assert!(
        stdout.contains("\"name\":\"UserId\",\"kind\":5"),
        "{stdout}"
    );
    assert!(stdout.contains("\"id\":3"), "{stdout}");
    assert!(
        stdout.contains("\"name\":\"answer\",\"kind\":12"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\"name\":\"Event\",\"kind\":10"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\"name\":\"Display\",\"kind\":11"),
        "{stdout}"
    );
    assert!(!stdout.contains("\"name\":\"hidden\""), "{stdout}");
}

#[test]
fn lsp_symbol_graph_cache_updates_after_did_change() {
    let dir = std::env::temp_dir().join(format!(
        "nox-lsp-symbol-cache-{}-{}",
        std::process::id(),
        line!()
    ));
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"symbol-cache\"\nversion = \"0.0.1\"\n\n[modules]\nsource_dirs = [\"src\"]\n",
    )
    .unwrap();
    let main_path = src.join("main.nox");
    let main_uri = format!("file://{}", main_path.display());
    let root_uri = format!("file://{}", dir.display());
    let first_source = "fn before_change() -> int { return 1; }\n";
    let second_source = "fn after_change() -> int { return 2; }\n";

    let input = [
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"rootUri":"{}"}}}}"#,
            root_uri
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            main_uri,
            json_escape(first_source)
        )),
        lsp_frame(
            r#"{"jsonrpc":"2.0","id":2,"method":"workspace/symbol","params":{"query":"change"}}"#,
        ),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{}","version":2}},"contentChanges":[{{"text":"{}"}}]}}}}"#,
            main_uri,
            json_escape(second_source)
        )),
        lsp_frame(
            r#"{"jsonrpc":"2.0","id":3,"method":"workspace/symbol","params":{"query":"change"}}"#,
        ),
        lsp_frame(r#"{"jsonrpc":"2.0","id":4,"method":"shutdown","params":null}"#),
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
    let first_symbols = stdout
        .split("\"id\":2")
        .nth(1)
        .expect("first workspace symbol response should be present");
    assert!(
        first_symbols.contains("\"name\":\"before_change\""),
        "{stdout}"
    );
    let second_symbols = stdout
        .split("\"id\":3")
        .nth(1)
        .expect("second workspace symbol response should be present");
    assert!(
        second_symbols.contains("\"name\":\"after_change\""),
        "{stdout}"
    );
    assert!(
        !second_symbols.contains("\"name\":\"before_change\""),
        "{stdout}"
    );
}

#[test]
fn lsp_definition_returns_current_document_top_level_declaration() {
    let source = "fn answer() -> int {\n    return 42;\n}\n\nlet value: int = answer();\n";
    let call_offset = source.rfind("answer();").unwrap() + 2;
    let (line, character) = lsp_position(source, call_offset);
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///definition.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{{"textDocument":{{"uri":"file:///definition.nox"}},"position":{{"line":{line},"character":{character}}}}}}}"#
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
    assert!(stdout.contains("\"definitionProvider\":true"), "{stdout}");
    assert!(stdout.contains("\"id\":2"), "{stdout}");
    assert!(
        stdout.contains("\"uri\":\"file:///definition.nox\""),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "\"range\":{\"start\":{\"line\":0,\"character\":3},\"end\":{\"line\":0,\"character\":9}}"
        ),
        "{stdout}"
    );
}

#[test]
fn lsp_definition_returns_imported_namespace_member_declaration() {
    let dir = std::env::temp_dir().join(format!(
        "nox-lsp-definition-namespace-{}-{}",
        std::process::id(),
        line!()
    ));
    let app = dir.join("app");
    let src = dir.join("src");
    fs::create_dir_all(&app).unwrap();
    fs::create_dir_all(&src).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"definition\"\nversion = \"0.0.1\"\n\n[entrypoints]\nmain = \"app/main.nox\"\n\n[modules]\nsource_dirs = [\"app\", \"src\"]\n",
    )
    .unwrap();
    let lib_source = "export fn answer() -> int {\n    return 42;\n}\n";
    let main_source = "import \"lib.nox\" as lib;\n\nlet value: int = lib.answer();\n";
    fs::write(src.join("lib.nox"), lib_source).unwrap();
    fs::write(app.join("main.nox"), main_source).unwrap();
    let member_offset = main_source.rfind("answer").unwrap() + 2;
    let (line, character) = lsp_position(main_source, member_offset);
    let main_uri = format!("file://{}", app.join("main.nox").display());
    let lib_uri = format!("file://{}", src.join("lib.nox").display());
    let root_uri = format!("file://{}", dir.display());

    let input = [
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"rootUri":"{}"}}}}"#,
            root_uri
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            main_uri,
            json_escape(main_source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{line},"character":{character}}}}}}}"#,
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
    assert!(stdout.contains("\"id\":2"), "{stdout}");
    assert!(
        stdout.contains(&format!("\"uri\":\"{}\"", json_escape(&lib_uri))),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "\"range\":{\"start\":{\"line\":0,\"character\":10},\"end\":{\"line\":0,\"character\":16}}"
        ),
        "{stdout}"
    );
}

#[test]
fn lsp_definition_returns_direct_imported_type_declaration_from_open_overlay() {
    let dir = std::env::temp_dir().join(format!(
        "nox-lsp-definition-direct-{}-{}",
        std::process::id(),
        line!()
    ));
    let app = dir.join("app");
    let src = dir.join("src");
    fs::create_dir_all(&app).unwrap();
    fs::create_dir_all(&src).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"definition\"\nversion = \"0.0.1\"\n\n[entrypoints]\nmain = \"app/main.nox\"\n\n[modules]\nsource_dirs = [\"app\", \"src\"]\n",
    )
    .unwrap();
    let disk_model_source = "export type OldId = int;\n";
    let open_model_source = "export type UserId = int;\n";
    let main_source = "import \"model.nox\";\n\nlet id: UserId = 1;\n";
    fs::write(src.join("model.nox"), disk_model_source).unwrap();
    fs::write(app.join("main.nox"), main_source).unwrap();
    let symbol_offset = main_source.rfind("UserId").unwrap() + 2;
    let (line, character) = lsp_position(main_source, symbol_offset);
    let main_uri = format!("file://{}", app.join("main.nox").display());
    let model_uri = format!("file://{}", src.join("model.nox").display());
    let root_uri = format!("file://{}", dir.display());

    let input = [
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"rootUri":"{}"}}}}"#,
            root_uri
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            main_uri,
            json_escape(main_source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            model_uri,
            json_escape(open_model_source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{line},"character":{character}}}}}}}"#,
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
    assert!(stdout.contains("\"id\":2"), "{stdout}");
    assert!(
        stdout.contains(&format!("\"uri\":\"{}\"", json_escape(&model_uri))),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "\"range\":{\"start\":{\"line\":0,\"character\":12},\"end\":{\"line\":0,\"character\":18}}"
        ),
        "{stdout}"
    );
    assert!(!stdout.contains("OldId"), "{stdout}");
}

#[test]
fn lsp_definition_does_not_jump_to_std_module_member() {
    let source =
        "import \"std/string.nox\" as string;\n\nlet value: str = string.trim(\" hi \");\n";
    let member_offset = source.rfind("trim").unwrap() + 1;
    let (line, character) = lsp_position(source, member_offset);
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///std-definition.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{{"textDocument":{{"uri":"file:///std-definition.nox"}},"position":{{"line":{line},"character":{character}}}}}}}"#
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
    assert!(stdout.contains("\"id\":2"), "{stdout}");
    assert!(stdout.contains("\"result\":null"), "{stdout}");
}

#[test]
fn lsp_external_dependency_import_is_diagnostic_clean_and_definition_is_conservative() {
    let dir = std::env::temp_dir().join(format!(
        "nox-lsp-external-import-{}-{}",
        std::process::id(),
        line!()
    ));
    let remote = create_local_bare_git_dependency(&dir);
    let project = dir.join("project");
    let cache = dir.join("cache");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(
        project.join("nox.toml"),
        format!(
            r#"[package]
name = "lsp-external-import"
version = "0.0.1"

[entrypoints]
main = "src/main.nox"

[modules]
source_dirs = ["src"]

[dependencies]
dep = {{ git = "file://{}", tag = "v0.1.0" }}
"#,
            remote.display()
        ),
    )
    .unwrap();
    let main_source = "import \"dep/math.nox\" as dep;\n\nlet value: int = dep.answer();\nvalue;\n";
    let main_path = project.join("src/main.nox");
    fs::write(&main_path, main_source).unwrap();
    let fetch = nox_command()
        .current_dir(&project)
        .args(["fetch", "--cache-dir", cache.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        fetch.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&fetch.stdout),
        String::from_utf8_lossy(&fetch.stderr)
    );

    let member_offset = main_source.rfind("answer").unwrap() + 2;
    let (line, character) = lsp_position(main_source, member_offset);
    let main_uri = format!("file://{}", main_path.display());
    let root_uri = format!("file://{}", project.display());
    let input = [
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"rootUri":"{}"}}}}"#,
            root_uri
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            main_uri,
            json_escape(main_source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{line},"character":{character}}}}}}}"#,
            main_uri
        )),
        lsp_frame(
            r#"{"jsonrpc":"2.0","id":3,"method":"workspace/symbol","params":{"query":"answer"}}"#,
        ),
        lsp_frame(r#"{"jsonrpc":"2.0","id":4,"method":"shutdown","params":null}"#),
        lsp_frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#),
    ]
    .join("");

    let mut child = nox_command()
        .arg("lsp")
        .env("NOX_MODULE_CACHE", &cache)
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
    assert!(!stdout.contains("module.not-found"), "{stdout}");
    assert!(stdout.contains("\"id\":2"), "{stdout}");
    assert!(stdout.contains("\"result\":null"), "{stdout}");
    let workspace_response = stdout
        .split("\"id\":3")
        .nth(1)
        .expect("workspace symbol response should be present");
    assert!(
        !workspace_response.contains("\"name\":\"answer\""),
        "{stdout}"
    );
}

#[test]
fn lsp_rename_renames_current_file_top_level_symbol() {
    let source = "fn answer() -> int {\n    return 42;\n}\n\nlet value: int = answer();\n";
    let call_offset = source.rfind("answer();").unwrap() + 2;
    let (line, character) = lsp_position(source, call_offset);
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///rename.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/prepareRename","params":{{"textDocument":{{"uri":"file:///rename.nox"}},"position":{{"line":{line},"character":{character}}}}}}}"#
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"textDocument/rename","params":{{"textDocument":{{"uri":"file:///rename.nox"}},"position":{{"line":{line},"character":{character}}},"newName":"total"}}}}"#
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":4,"method":"shutdown","params":null}"#),
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
    assert!(
        stdout.contains("\"renameProvider\":{\"prepareProvider\":true}"),
        "{stdout}"
    );
    assert!(stdout.contains("\"id\":2"), "{stdout}");
    assert!(stdout.contains("\"placeholder\":\"answer\""), "{stdout}");
    assert!(stdout.contains("\"id\":3"), "{stdout}");
    assert!(
        stdout.contains("\"changes\":{\"file:///rename.nox\""),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "\"range\":{\"start\":{\"line\":0,\"character\":3},\"end\":{\"line\":0,\"character\":9}}"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "\"range\":{\"start\":{\"line\":4,\"character\":17},\"end\":{\"line\":4,\"character\":23}}"
        ),
        "{stdout}"
    );
    assert_eq!(
        stdout.matches("\"newText\":\"total\"").count(),
        2,
        "{stdout}"
    );
}

#[test]
fn lsp_rename_rejects_shadowed_current_file_symbol() {
    let source = "fn answer() -> int {\n    let answer: int = 1;\n    return answer;\n}\n\nlet value: int = answer();\n";
    let call_offset = source.rfind("answer();").unwrap() + 2;
    let (line, character) = lsp_position(source, call_offset);
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///rename-shadow.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/prepareRename","params":{{"textDocument":{{"uri":"file:///rename-shadow.nox"}},"position":{{"line":{line},"character":{character}}}}}}}"#
        )),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"textDocument/rename","params":{{"textDocument":{{"uri":"file:///rename-shadow.nox"}},"position":{{"line":{line},"character":{character}}},"newName":"total"}}}}"#
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":4,"method":"shutdown","params":null}"#),
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
    assert!(stdout.contains("\"id\":2,\"result\":null"), "{stdout}");
    assert!(stdout.contains("\"id\":3,\"result\":null"), "{stdout}");
}

#[test]
fn lsp_code_action_returns_source_action() {
    let source = "let answer: int = 42;\nanswer;\n";
    let input = [
        lsp_frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        lsp_frame(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///action.nox","languageId":"nox","version":1,"text":"{}"}}}}}}"#,
            json_escape(source)
        )),
        lsp_frame(r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/codeAction","params":{"textDocument":{"uri":"file:///action.nox"},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"context":{"diagnostics":[]}}}"#),
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
    assert!(stdout.contains("\"codeActionProvider\":true"), "{stdout}");
    assert!(stdout.contains("Run nox check"), "{stdout}");
}

#[test]
fn check_exits_nonzero_for_static_type_error() {
    for (path, expected) in [
        ("tests/fixtures/type-error.nox", "expected int, got str"),
        (
            "tests/fixtures/type-error-array-element.nox",
            "expected int, got str",
        ),
        (
            "tests/fixtures/type-error-array-index.nox",
            "expected int, got float",
        ),
        (
            "tests/fixtures/type-error-array-len.nox",
            "expected array or str, got int",
        ),
        (
            "tests/fixtures/type-error-int-float.nox",
            "'+' is not defined for int and float",
        ),
        (
            "tests/fixtures/type-error-for-range.nox",
            "expected int, got float",
        ),
        (
            "tests/fixtures/type-error-const-assignment.nox",
            "cannot assign to constant 'value'",
        ),
        (
            "tests/fixtures/type-error-break-outside-loop.nox",
            "'break' is only allowed inside a 'while' or 'for' loop",
        ),
        (
            "tests/fixtures/type-error-continue-outside-loop.nox",
            "'continue' is only allowed inside a 'while' or 'for' loop",
        ),
        (
            "tests/fixtures/type-error-logical.nox",
            "expected bool, got int",
        ),
        (
            "tests/fixtures/type-error-map-index.nox",
            "expected str, got int",
        ),
        (
            "tests/fixtures/type-error-map-key.nox",
            "expected str, got int",
        ),
        (
            "tests/fixtures/type-error-map-value.nox",
            "expected int, got str",
        ),
        (
            "tests/fixtures/type-error-record-duplicate-field.nox",
            "duplicate field 'name'",
        ),
        (
            "tests/fixtures/type-error-record-extra-field.nox",
            "record 'User' has no field 'score'",
        ),
        (
            "tests/fixtures/type-error-record-field-access.nox",
            "record 'User' has no field 'score'",
        ),
        (
            "tests/fixtures/type-error-record-field-type.nox",
            "expected int, got str",
        ),
        (
            "tests/fixtures/type-error-record-missing-field.nox",
            "missing field 'score'",
        ),
        (
            "tests/fixtures/type-error-sqrt-int.nox",
            "expected float, got int",
        ),
        (
            "tests/fixtures/type-error-sleep-float.nox",
            "expected int, got float",
        ),
        (
            "tests/fixtures/type-error-generic-infer-failed.nox",
            "could not infer generic type 'T'",
        ),
        (
            "tests/fixtures/type-error-bitwise-non-int.nox",
            "bitwise operator expects int",
        ),
        (
            "tests/fixtures/type-error-let-else-fallthrough.nox",
            "let-else branch must return",
        ),
        (
            "tests/fixtures/type-error-spread-mismatch.nox",
            "array spread expects array",
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
        (
            "tests/fixtures/runtime-error-divide-zero.nox",
            "division by zero",
        ),
        (
            "tests/fixtures/runtime-error-array-bounds.nox",
            "array index out of bounds",
        ),
        (
            "tests/fixtures/runtime-error-map-key.nox",
            "map key not found",
        ),
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
fn run_prints_runtime_stack_trace() {
    let path = fixture("tests/fixtures/runtime-error-stack-trace.nox");
    let output = nox_command()
        .args(["run", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("division by zero"));
    assert!(stderr.contains("  at divide [script] ("));
    assert!(stderr.contains("  at wrapper [script] ("));
}

#[test]
fn check_exits_nonzero_for_syntax_error() {
    let path = "tests/fixtures/syntax-error-string-escape.nox";
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
            fixture("tests/fixtures/syntax-errors.nox")
                .to_str()
                .unwrap(),
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

#[test]
fn watch_reports_missing_subcommand() {
    let output = nox_command().arg("watch").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("expected one of check / test / run"),
        "stderr: {stderr}"
    );
}

#[test]
fn watch_rejects_unknown_subcommand() {
    let output = nox_command().args(["watch", "fmt"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported subcommand 'fmt'"),
        "stderr: {stderr}"
    );
}

#[test]
fn watch_reports_invalid_interval_argument() {
    let output = nox_command()
        .args(["watch", "--interval-ms", "0", "run"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--interval-ms"), "stderr: {stderr}");
}

#[test]
fn watch_reports_missing_path_with_stable_code() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-watch-missing-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("nox.toml"),
        "[package]\nname = \"watch-missing\"\nversion = \"0.0.1\"\n[modules]\nsource_dirs = [\"nonexistent\"]\n",
    )
    .unwrap();

    let output = nox_command()
        .current_dir(&dir)
        .args(["watch", "check"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[watch.path-not-found]"),
        "expected watch.path-not-found code, got stderr: {stderr}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_filter_limits_to_matching_test_names() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-filter-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("filtering_test.nox");
    fs::write(
        &path,
        "fn test_alpha() -> bool { return true; }\nfn test_beta() -> bool { return true; }\nfn test_gamma() -> bool { return true; }\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--filter", "beta", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test_beta PASS"), "stdout: {stdout}");
    assert!(!stdout.contains("test_alpha"), "stdout: {stdout}");
    assert!(!stdout.contains("test_gamma"), "stdout: {stdout}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_assertion_helpers_pass_and_fail_with_stable_code() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-assertions-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("assertions_test.nox");
    fs::write(
        &path,
        "import \"std/test.nox\" as test;\n\nfn test_passes() -> null {\n    test.assert_eq(1 + 1, 2, \"math\");\n    test.assert_contains(\"hello world\", \"world\", \"contains\");\n    return null;\n}\n\nfn test_fails() -> null {\n    test.assert_eq(1, 2, \"oops\");\n    return null;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\":\"test_passes\""));
    assert!(stdout.contains("\"name\":\"test_fails\""));
    assert!(stdout.contains("\"code\":\"test.assertion-failed\""));
    assert!(stdout.contains("assert_eq failed"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn lint_reports_unused_top_level_variables() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-lint-unused-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("unused.nox");
    fs::write(
        &path,
        "let unused: int = 42;\nlet used: int = 1;\nprint(to_str_int(used));\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["lint", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.lint.v1\""));
    assert!(stdout.contains("\"code\":\"lint.unused-variable\""));
    assert!(stdout.contains("\"message\":\"variable 'unused' is declared but never used\""));
    assert!(!stdout.contains("'used'"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn lint_skips_underscore_prefixed_variables() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-lint-underscore-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("underscore.nox");
    fs::write(
        &path,
        "let _ignored: int = 0;\nlet used: int = 1;\nprint(to_str_int(used));\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["lint", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("summary: 1 files, 0 warnings"),
        "stdout: {stdout}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn lint_reports_capability_summary_from_imports() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-lint-caps-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("caps.nox");
    fs::write(
        &path,
        "import \"std/fs.nox\" as fs;\nimport \"std/process.nox\" as process;\nlet contents: str = fs.read_text(\"placeholder.txt\");\nfs.write_text(\"out.txt\", contents);\nprocess.run(\"echo\", [], \"\", 100);\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["lint", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"capabilities\":["));
    assert!(stdout.contains("\"filesystem\""));
    assert!(stdout.contains("\"filesystem_write\""));
    assert!(stdout.contains("\"process_run\""));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn lint_does_not_report_filesystem_write_for_read_only_imports() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-lint-caps-read-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("caps-read.nox");
    fs::write(
        &path,
        "import \"std/fs.nox\" as fs;\nlet contents: str = fs.read_text(\"placeholder.txt\");\nprint(contents);\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["lint", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"filesystem\""));
    assert!(!stdout.contains("\"filesystem_write\""));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn lint_reports_duplicate_match_arm() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-lint-dup-match-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("dup.nox");
    fs::write(
        &path,
        "export fn classify(value: int) -> str {\n    match (value) {\n        1 => { return \"one\"; }\n        2 => { return \"two\"; }\n        1 => { return \"again\"; }\n        _ => { return \"other\"; }\n    }\n}\nprint(classify(1));\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["lint", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"lint.duplicate-match-arm\""));
    assert!(stdout.contains("duplicates an earlier arm"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn lint_reports_constant_if_condition() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-lint-const-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("const.nox");
    fs::write(
        &path,
        "export fn helper() -> int {\n    if (true) {\n        return 1;\n    }\n    return 0;\n}\nprint(to_str_int(helper()));\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["lint", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"lint.constant-condition\""));
    assert!(stdout.contains("always true"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn lint_reports_shadowed_variables_in_nested_blocks() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-lint-shadow-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("shadow.nox");
    fs::write(
        &path,
        "export fn helper() -> int {\n    let value: int = 1;\n    if (value > 0) {\n        let value: int = 2;\n        return value;\n    }\n    return value;\n}\nprint(to_str_int(helper()));\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["lint", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"lint.shadowed-variable\""));
    assert!(stdout.contains("shadows an outer binding"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn lint_reports_unreachable_code_after_return() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-lint-unreachable-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("dead.nox");
    fs::write(
        &path,
        "export fn helper() -> int {\n    return 1;\n    let dead: int = 2;\n}\nprint(to_str_int(helper()));\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["lint", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"lint.unreachable-code\""));
    assert!(stdout.contains("statement is unreachable"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn profile_json_emits_nox_profile_schema() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-profile-json-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.nox");
    fs::write(
        &path,
        "fn add(a: int, b: int) -> int { return a + b; }\nlet values: [int] = [1, 2, 3];\nprint(to_str_int(add(values[0], 3)));\n",
    )
    .unwrap();
    let output = nox_command()
        .args(["profile", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.profile.v1\""));
    assert!(stdout.contains("\"name\":\"add\""));
    assert!(stdout.contains("\"operations\":["));
    assert!(stdout.contains("\"name\":\"array_literal\""));
    assert!(stdout.contains("\"name\":\"index\""));
    assert!(stdout.contains("\"name\":\"host_callback\""));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn profile_ndjson_emits_one_event_per_function() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-profile-ndjson-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.nox");
    fs::write(
        &path,
        "fn add(a: int, b: int) -> int { return a + b; }\nfn mul(a: int, b: int) -> int { return a * b; }\nprint(to_str_int(add(2, 3)));\nprint(to_str_int(mul(2, 3)));\n",
    )
    .unwrap();
    let output = nox_command()
        .args(["profile", "--ndjson", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let event_lines: Vec<&str> = stdout
        .lines()
        .filter(|line| line.contains("\"schema\":\"nox.profile.event.v1\""))
        .collect();
    assert!(
        event_lines.len() >= 2,
        "expected at least 2 NDJSON events, got {event_lines:?} from:\n{stdout}"
    );
    assert!(event_lines.iter().any(|l| l.contains("\"name\":\"add\"")));
    assert!(event_lines.iter().any(|l| l.contains("\"name\":\"mul\"")));
    assert!(
        stdout.contains("\"kind\":\"operation\""),
        "expected operation profile events in:\n{stdout}"
    );
    assert!(stdout.contains("\"name\":\"host_callback\""));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn profile_rejects_combined_json_and_ndjson_flags() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-profile-conflict-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.nox");
    fs::write(&path, "print(\"ok\");\n").unwrap();
    let output = nox_command()
        .args(["profile", "--json", "--ndjson", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("mutually exclusive"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn trace_ndjson_emits_events_and_captures_stdout() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-trace-ndjson-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("trace.nox");
    fs::write(
        &path,
        "import \"std/time.nox\" as time;\n// sleep_ms(0) marks timers as a static trace requirement without granting it.\nfn add(a: int, b: int) -> int { return a + b; }\nlet values: [int] = [1, 2, 3];\nprint(to_str_int(add(values[0], 3)));\n",
    )
    .unwrap();
    let output = nox_command()
        .args(["trace", "--ndjson", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.trace.event.v1\""));
    assert!(stdout.contains("\"event\":\"run_start\""));
    assert!(stdout.contains("\"event\":\"permission_summary\""));
    assert!(stdout.contains("\"event\":\"permission_check\""));
    assert!(stdout.contains("\"event\":\"io\""));
    assert!(stdout.contains("\"operation\":\"write\""));
    assert!(stdout.contains("\"stream\":\"stdout\""));
    assert!(stdout.contains("\"event\":\"stdout\""));
    assert!(stdout.contains("\"text\":\"4\\n\""));
    assert!(stdout.contains("\"event\":\"function_profile\""));
    assert!(stdout.contains("\"name\":\"add\""));
    assert!(stdout.contains("\"event\":\"operation_profile\""));
    assert!(stdout.contains("\"event\":\"host_callback\""));
    assert!(stdout.contains("\"name\":\"host_callback\""));
    assert!(stdout.contains("\"event\":\"host_callback_call\""));
    assert!(stdout.contains("\"phase\":\"enter\""));
    assert!(stdout.contains("\"phase\":\"exit\""));
    assert!(stdout.contains("\"status\":\"ok\""));
    assert!(stdout.contains("\"name\":\"array_literal\""));
    assert!(stdout.contains("\"event\":\"run_finish\""));
    assert!(stdout.contains("\"status\":\"ok\""));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn trace_ndjson_emits_runtime_task_and_timer_events() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-trace-runtime-events-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let timer_path = dir.join("timer_trace.nox");
    fs::write(&timer_path, "sleep_ms(0);\n").unwrap();
    let timer = nox_command()
        .args(["trace", "--ndjson", timer_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!timer.status.success());
    let timer_stdout = String::from_utf8_lossy(&timer.stdout);
    assert!(timer_stdout.contains("\"event\":\"timer\""));
    assert!(timer_stdout.contains("\"operation\":\"sleep\""));
    assert!(timer_stdout.contains("\"allowed\":false"));

    let task_path = dir.join("task_trace.nox");
    fs::write(&task_path, "task_sleep_ms(0);\n").unwrap();
    let task = nox_command()
        .args(["trace", "--ndjson", task_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!task.status.success());
    let task_stdout = String::from_utf8_lossy(&task.stdout);
    assert!(task_stdout.contains("\"event\":\"task\""));
    assert!(task_stdout.contains("\"operation\":\"spawn\""));
    assert!(task_stdout.contains("\"allowed\":false"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn trace_ndjson_emits_stdin_and_filesystem_io_events() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-trace-io-events-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let data_path = dir.join("data.txt");
    fs::write(&data_path, "abc").unwrap();

    let read_path = dir.join("read_trace.nox");
    fs::write(
        &read_path,
        format!(
            "import \"std/fs.nox\" as fs;\nimport \"std/process.nox\" as process;\nlet input: str = process.read_stdin();\nlet data: str = read_text(\"{data}\");\nlet tried: result[str, str] = fs.try_read_text(\"{data}\");\nlet binary: result[[int], str] = fs.read_binary(\"{data}\");\nlet listed: result[[str], str] = fs.list_dir(\"{dir}\");\nlet canonical: result[str, str] = fs.canonicalize(\"{data}\");\nprint(input + data);\n",
            data = data_path.display(),
            dir = dir.display()
        ),
    )
    .unwrap();
    let mut read_cmd = nox_command();
    let mut child = read_cmd
        .args(["trace", "--ndjson", read_path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(b"stdin-").unwrap();
    let read = child.wait_with_output().unwrap();
    assert!(read.status.success());
    let read_stdout = String::from_utf8_lossy(&read.stdout);
    assert!(read_stdout.contains("\"stream\":\"stdin\""));
    assert!(read_stdout.contains("\"operation\":\"read\""));
    assert!(read_stdout.contains("\"stream\":\"filesystem\""));
    assert!(read_stdout.contains("\"operation\":\"read_text\""));
    assert!(read_stdout.contains("\"operation\":\"try_read_text\""));
    assert!(read_stdout.contains("\"operation\":\"read_binary\""));
    assert!(read_stdout.contains("\"operation\":\"list_dir\""));
    assert!(read_stdout.contains("\"operation\":\"canonicalize\""));
    assert!(read_stdout.contains("\"status\":\"ok\""));
    assert!(read_stdout.contains("\"bytes\":3"));

    let write_path = dir.join("write_trace.nox");
    fs::write(
        &write_path,
        "import \"std/fs.nox\" as fs;\nfs.write_binary(\"blocked.bin\", [1, 2, 3]);\n",
    )
    .unwrap();
    let write = nox_command()
        .args(["trace", "--ndjson", write_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!write.status.success());
    let write_stdout = String::from_utf8_lossy(&write.stdout);
    assert!(write_stdout.contains("\"stream\":\"filesystem\""));
    assert!(write_stdout.contains("\"operation\":\"write_binary\""));
    assert!(write_stdout.contains("\"allowed\":false"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn trace_ndjson_emits_diagnostic_event_on_runtime_error() {
    let path = fixture("tests/fixtures/runtime-error-stack-trace.nox");
    let output = nox_command()
        .args(["trace", "--ndjson", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.trace.event.v1\""));
    assert!(stdout.contains("\"trace_id\":\"trace-"));
    assert!(stdout.contains("\"event\":\"diagnostic\""));
    assert!(stdout.contains("\"code\":\"runtime.division-by-zero\""));
    assert!(stdout.contains("\"span\":{\"start\":"));
    assert!(stdout.contains("\"source\":{\"name\":"));
    assert!(stdout.contains("\"stack_frames\":["));
    assert!(stdout.contains("\"name\":\"wrapper\""));
    assert!(stdout.contains("\"event\":\"run_finish\""));
    assert!(stdout.contains("\"status\":\"error\""));
}

#[test]
fn coverage_json_emits_nox_coverage_schema() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-coverage-json-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("c.nox");
    fs::write(
        &path,
        "fn choose(value: int) -> int {\n    if (value > 0) {\n        return 1;\n    } else {\n        return 0;\n    }\n}\nprint(to_str_int(choose(1)));\nprint(to_str_int(choose(0)));\n",
    )
    .unwrap();
    let output = nox_command()
        .args(["coverage", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.coverage.v1\""));
    assert!(stdout.contains("\"statements\":["));
    assert!(stdout.contains("\"execution_count\":"));
    assert!(stdout.contains("\"source\":{\"line\":"));
    assert!(stdout.contains("\"branches\":["));
    assert!(stdout.contains("\"true_count\":"));
    assert!(stdout.contains("\"false_count\":"));
    assert!(stdout.contains("\"covered\":"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn coverage_ndjson_emits_statement_and_branch_events() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-coverage-ndjson-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("branches.nox");
    fs::write(
        &path,
        "fn choose(value: int) -> int {\n    if (value > 0) {\n        return 1;\n    } else {\n        return 0;\n    }\n}\nprint(to_str_int(choose(1)));\nprint(to_str_int(choose(0)));\n",
    )
    .unwrap();
    let output = nox_command()
        .args(["coverage", "--ndjson", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema\":\"nox.coverage.event.v1\""));
    assert!(stdout.contains("\"kind\":\"statement\""));
    assert!(stdout.contains("\"execution_count\":"));
    assert!(stdout.contains("\"kind\":\"branch\""));
    assert!(stdout.contains("\"true_count\":1"));
    assert!(stdout.contains("\"false_count\":1"));
    assert!(stdout.contains("\"covered\":true"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_retry_marks_attempts_in_json() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-retry-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("retry_test.nox");
    fs::write(
        &path,
        "fn test_pass() -> bool { return true; }\nfn test_fail() -> bool { return false; }\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--json", "--retry", "2", path.to_str().unwrap()])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"name\":\"test_fail\",\"ok\":false,\"attempts\":3,\"retried\":true"),
        "stdout: {stdout}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_before_each_runs_before_every_test() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-lifecycle-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("lifecycle_test.nox");
    fs::write(
        &path,
        r#"let counter: [int] = [0];

fn before_each() -> null {
    counter[0] = counter[0] + 1;
    return null;
}

fn test_first() -> bool {
    return counter[0] == 1;
}

fn test_second() -> bool {
    return counter[0] == 2;
}
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["test", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test_first PASS"));
    assert!(stdout.contains("test_second PASS"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_after_each_failure_marks_test_failed() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-after-each-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("after_each_test.nox");
    fs::write(
        &path,
        r#"import "std/test.nox" as test;

fn after_each() -> null {
    test.fail("teardown", "after_each rejected");
    return null;
}

fn test_pass() -> null {
    return null;
}
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["test", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test_pass FAIL"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_assert_snapshot_emits_diff_on_mismatch() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-snapshot-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("snapshot_test.nox");
    fs::write(
        &path,
        r#"import "std/test.nox" as test;

fn test_match() -> null {
    test.assert_snapshot("match", "hello", "hello");
    return null;
}

fn test_mismatch() -> null {
    test.assert_snapshot("mismatch", "hello", "world");
    return null;
}
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\":\"test_match\",\"ok\":true"));
    assert!(stdout.contains("\"name\":\"test_mismatch\",\"ok\":false"));
    assert!(stdout.contains("\"code\":\"test.assertion-failed\""));
    assert!(stdout.contains("snapshot mismatch"));
    assert!(
        stdout.contains(
            "\"snapshot_diff\":{\"label\":\"mismatch\",\"actual\":\"hello\",\"expected\":\"world\"}"
        ),
        "stdout: {stdout}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_assert_table_row_runs_table_driven_cases() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-table-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("table_test.nox");
    fs::write(
        &path,
        r#"import "std/test.nox" as test;

fn double(value: int) -> int {
    return value * 2;
}

fn test_double_table() -> null {
    let cases: [(int, int)] = [(1, 2), (2, 4), (3, 6)];
    let i: int = 0;
    let n: int = 3;
    while (i < n) {
        let pair: (int, int) = cases[i];
        let (input, expected) = pair;
        test.assert_table_row("double", i, double(input), expected);
        i = i + 1;
    }
    return null;
}
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["test", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test_double_table PASS"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_property_int_reports_seed_shrink_and_replay_metadata() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-property-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("property_test.nox");
    fs::write(
        &path,
        r#"import "std/test.nox" as test;

fn test_property_passes() -> null {
    test.assert_property_int("non-negative", 3, 8, 0, 20, fn(value: int) -> bool {
        return value >= 0;
    });
    return null;
}

fn test_property_fails() -> null {
    test.assert_property_int("negative-rejected", 3, 20, -20, 20, fn(value: int) -> bool {
        return value >= 0;
    });
    return null;
}
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\":\"test_property_passes\",\"ok\":true"));
    assert!(stdout.contains("\"name\":\"test_property_fails\",\"ok\":false"));
    assert!(
        stdout.contains("property failed seed=3 case="),
        "stdout: {stdout}"
    );
    assert!(stdout.contains(" value="), "stdout: {stdout}");
    assert!(stdout.contains(" minimized="), "stdout: {stdout}");
    assert!(
        stdout.contains(" replay=\\\"negative-rejected:"),
        "stdout: {stdout}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_property_int_array_reports_structural_shrink_metadata() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-property-array-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("property_array_test.nox");
    fs::write(
        &path,
        r#"import "std/test.nox" as test;

fn test_array_property_fails() -> null {
    test.assert_property_int_array("array-negative-rejected", 5, 20, 4, -20, 20, fn(values: [int]) -> bool {
        let i: int = 0;
        while (i < len(values)) {
            if (values[i] < 0) {
                return false;
            }
            i = i + 1;
        }
        return true;
    });
    return null;
}
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\":\"test_array_property_fails\",\"ok\":false"));
    assert!(
        stdout.contains("property failed seed=5 case="),
        "stdout: {stdout}"
    );
    assert!(stdout.contains(" value_len=4"), "stdout: {stdout}");
    assert!(stdout.contains(" minimized_len="), "stdout: {stdout}");
    assert!(stdout.contains(" minimized_first="), "stdout: {stdout}");
    assert!(
        stdout.contains(" replay=\\\"array-negative-rejected:len="),
        "stdout: {stdout}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_property_int_map_reports_structural_shrink_metadata() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-property-map-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("property_map_test.nox");
    fs::write(
        &path,
        r#"import "std/map.nox" as map;
import "std/test.nox" as test;

fn test_map_property_fails() -> null {
    test.assert_property_int_map("map-negative-rejected", 5, 20, 4, -20, 20, fn(values: map[str, int]) -> bool {
        let i: int = 0;
        while (i < len(map.keys(values))) {
            let found: option[int] = map_get(values, "k${i}");
            match (found) {
                some(value) => {
                    if (value < 0) {
                        return false;
                    }
                }
                none => {}
            }
            i = i + 1;
        }
        return true;
    });
    return null;
}
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\":\"test_map_property_fails\",\"ok\":false"));
    assert!(
        stdout.contains("property failed seed=5 case="),
        "stdout: {stdout}"
    );
    assert!(stdout.contains(" value_len=4"), "stdout: {stdout}");
    assert!(stdout.contains(" minimized_len="), "stdout: {stdout}");
    assert!(stdout.contains(" minimized_first="), "stdout: {stdout}");
    assert!(
        stdout.contains(" replay=\\\"map-negative-rejected:len="),
        "stdout: {stdout}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_property_record3_reports_structural_shrink_metadata() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-property-record-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("property_record_test.nox");
    fs::write(
        &path,
        r#"import "std/test.nox" as test;

record User {
    id: int,
    name: str,
    active: bool,
}

fn make_user(id: int, name: str, active: bool) -> User {
    return User { id: id, name: name, active: active };
}

fn test_record_property_fails() -> null {
    test.assert_property_record3("record-user-valid", 5, 20, -20, 20, make_user, fn(user: User) -> bool {
        return user.id >= 0 && len(user.name) > 0 && user.active;
    });
    return null;
}
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\":\"test_record_property_fails\",\"ok\":false"));
    assert!(
        stdout.contains("property failed seed=5 case="),
        "stdout: {stdout}"
    );
    assert!(stdout.contains(" record_fields=3"), "stdout: {stdout}");
    assert!(stdout.contains(" minimized_int="), "stdout: {stdout}");
    assert!(stdout.contains(" minimized_str_len="), "stdout: {stdout}");
    assert!(stdout.contains(" minimized_bool="), "stdout: {stdout}");
    assert!(
        stdout.contains(" replay=\\\"record-user-valid:record3:"),
        "stdout: {stdout}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_property_enum3_reports_variant_shrink_metadata() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-property-enum-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("property_enum_test.nox");
    fs::write(
        &path,
        r#"import "std/test.nox" as test;

enum Sample {
    Number(int),
    Name(str),
    Flag(bool),
}

fn sample_number(value: int) -> Sample {
    return Sample.Number(value);
}

fn sample_name(value: str) -> Sample {
    return Sample.Name(value);
}

fn sample_flag(value: bool) -> Sample {
    return Sample.Flag(value);
}

fn reject_sample(value: Sample) -> bool {
    return false;
}

fn test_enum_property_fails() -> null {
    test.assert_property_enum3("enum-always-fails", 5, 20, -20, 20, 8, sample_number, sample_name, sample_flag, reject_sample);
    return null;
}
"#,
    )
    .unwrap();

    let output = nox_command()
        .args(["test", "--json", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\":\"test_enum_property_fails\",\"ok\":false"));
    assert!(
        stdout.contains("property failed seed=5 case="),
        "stdout: {stdout}"
    );
    assert!(stdout.contains(" enum_variant="), "stdout: {stdout}");
    assert!(stdout.contains(" minimized_variant="), "stdout: {stdout}");
    assert!(stdout.contains(" minimized_int="), "stdout: {stdout}");
    assert!(stdout.contains(" minimized_str_len="), "stdout: {stdout}");
    assert!(stdout.contains(" minimized_bool="), "stdout: {stdout}");
    assert!(
        stdout.contains(" replay=\\\"enum-always-fails:enum3:"),
        "stdout: {stdout}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_export_failures_writes_property_failure_corpus() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-export-failures-{}-{}",
        std::process::id(),
        line!()
    ));
    let export_dir = dir.join("corpus");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("property_test.nox");
    fs::write(
        &path,
        r#"import "std/test.nox" as test;

fn test_property_fails() -> null {
    test.assert_property_int("negative-rejected", 3, 20, -20, 20, fn(value: int) -> bool {
        return value >= 0;
    });
    return null;
}
"#,
    )
    .unwrap();

    let output = nox_command()
        .args([
            "test",
            "--export-failures",
            export_dir.to_str().unwrap(),
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let exported: Vec<_> = fs::read_dir(&export_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(exported.len(), 1);
    let contents = fs::read_to_string(&exported[0]).unwrap();
    assert!(contents.contains("Exported by nox test --export-failures"));
    assert!(contents.contains("// test: test_property_fails"));
    assert!(contents.contains("property failed seed=3 case="));
    assert!(contents.contains("replay=\"negative-rejected:"));
    assert!(contents.contains("fn test_property_fails() -> null"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_export_failures_classified_writes_malformed_parser_corpus() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-test-export-classified-{}-{}",
        std::process::id(),
        line!()
    ));
    let export_dir = dir.join("classified");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("broken_test.nox");
    fs::write(&path, "fn test_broken( -> bool {\n    return true;\n}\n").unwrap();

    let output = nox_command()
        .args([
            "test",
            "--export-failures-classified",
            export_dir.to_str().unwrap(),
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let parser_dir = export_dir.join("parser");
    let exported: Vec<_> = fs::read_dir(&parser_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(exported.len(), 1);
    let contents = fs::read_to_string(&exported[0]).unwrap();
    assert!(contents.contains("Exported by nox test --export-failures"));
    assert!(contents.contains("// classification: parser"));
    assert!(contents.contains("// test: <module>"));
    assert!(contents.contains("fn test_broken("));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn doc_emits_markdown_for_exported_functions() {
    let dir = std::env::temp_dir().join(format!("nox-cli-doc-{}-{}", std::process::id(), line!()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("api.nox");
    fs::write(
        &path,
        "/// Doubles a value.\nexport fn double(x: int) -> int {\n    return x * 2;\n}\n\nfn local_helper() -> int {\n    return 1;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["doc", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("# `{}`", path.display())));
    assert!(stdout.contains("## export fn double(x: int) -> int"));
    assert!(stdout.contains("Kind: **fn**. Visibility: **exported**"));
    assert!(stdout.contains("Doubles a value."));
    assert!(stdout.contains("## fn local_helper() -> int"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn doc_emits_markdown_for_records_enums_and_type_aliases() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-doc-types-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("types.nox");
    fs::write(
        &path,
        "/// A 2D point.\nexport record Point {\n    x: int,\n    y: int,\n}\n\n/// An event emitted by the system.\nexport enum Event {\n    Click(int),\n    Quit,\n}\n\n/// Alias for entity ids.\nexport type EntityId = int;\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["doc", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("## export record Point"));
    assert!(stdout.contains("Kind: **record**. Visibility: **exported**"));
    assert!(stdout.contains("A 2D point."));
    assert!(stdout.contains("## export enum Event"));
    assert!(stdout.contains("Kind: **enum**. Visibility: **exported**"));
    assert!(stdout.contains("An event emitted by the system."));
    assert!(stdout.contains("## export type EntityId = int;"));
    assert!(stdout.contains("Kind: **type**. Visibility: **exported**"));
    assert!(stdout.contains("Alias for entity ids."));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn doc_emits_markdown_for_traits() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-doc-traits-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("traits.nox");
    fs::write(
        &path,
        "/// Displayable values.\nexport trait Display {\n    fn to_str(self: Self) -> str;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["doc", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("## export trait Display"));
    assert!(stdout.contains("Kind: **trait**. Visibility: **exported**"));
    assert!(stdout.contains("Displayable values."));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn doc_emits_async_function_call_return_type() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-doc-async-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("async.nox");
    fs::write(
        &path,
        "/// Computes a value asynchronously.\nexport async fn compute() -> result[int, str] {\n    return ok(42);\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["doc", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("## export async fn compute() -> result[int, str]"));
    assert!(stdout.contains("Kind: **fn**. Visibility: **exported**"));
    assert!(stdout.contains("Call return: **task[result[int, str]]**."));
    assert!(stdout.contains("Computes a value asynchronously."));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn doc_uses_structured_top_level_declarations_and_doc_blocks() {
    let dir = std::env::temp_dir().join(format!(
        "nox-cli-doc-structured-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("structured.nox");
    fs::write(
        &path,
        "/// Builds a value.\n/// This doc block stays attached.\nexport fn build(\n    value: int,\n) -> int {\n    fn nested() -> int {\n        return value;\n    }\n    return nested();\n}\n\n// This ordinary comment does not attach.\nfn helper() -> int {\n    return 1;\n}\n",
    )
    .unwrap();

    let output = nox_command()
        .args(["doc", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("## export fn build( value: int, ) -> int"));
    assert!(stdout.contains("Kind: **fn**. Visibility: **exported**"));
    assert!(stdout.contains("Builds a value."));
    assert!(stdout.contains("This doc block stays attached."));
    assert!(!stdout.contains("nested()"));
    assert!(stdout.contains("## fn helper() -> int"));
    assert!(!stdout.contains("This ordinary comment does not attach."));
    fs::remove_dir_all(&dir).ok();
}
