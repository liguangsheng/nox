use std::{
    env,
    fmt::Write,
    fs, io,
    path::{Path, PathBuf},
    process::{self, Command},
};

use nox::{manifest::Manifest, Runtime, RuntimePermissions};
use nox_core::Diagnostic;

struct CheckFileReport {
    path: String,
    ok: bool,
    diagnostic_count: usize,
}

struct TestReport {
    path: String,
    name: String,
    ok: bool,
    diagnostic: Option<Diagnostic>,
}

struct ProjectStepReport {
    name: &'static str,
    status: i32,
    stdout: String,
    stderr: String,
}

fn main() {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_usage_and_exit();
    };

    match command.as_str() {
        "--version" | "version" => {
            println!("nox {}", env!("CARGO_PKG_VERSION"));
        }
        "run" => {
            let Some(path) = args.next() else {
                let path = match manifest_main_from_current_dir() {
                    Ok(path) => path,
                    Err(err) => {
                        eprintln!("run: {err}");
                        process::exit(2);
                    }
                };
                let script_args = args.collect();
                let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
                runtime.set_args(script_args);
                match runtime.eval_file(&path) {
                    Ok(value) => println!("{value}"),
                    Err(err) => {
                        print_diagnostic(&path.display().to_string(), &err);
                        process::exit(1);
                    }
                }
                return;
            };
            let script_args = args.collect();
            let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
            runtime.set_args(script_args);
            match runtime.eval_file(&path) {
                Ok(value) => println!("{value}"),
                Err(err) => {
                    print_diagnostic(&path, &err);
                    process::exit(1);
                }
            }
        }
        "check" => process::exit(run_check(args.collect())),
        "test" => process::exit(run_test(args.collect())),
        "fmt" => process::exit(run_fmt(args.collect())),
        "project" => process::exit(run_project(args.collect())),
        "lsp" => {
            if let Err(err) = nox::lsp::run_stdio() {
                eprintln!("lsp error: {err}");
                process::exit(1);
            }
        }
        "inspect-bytecode" => {
            let mut compact = false;
            let mut paths = args.filter(|arg| {
                if arg == "--compact" {
                    compact = true;
                    false
                } else {
                    true
                }
            });
            let Some(path) = paths.next() else {
                eprintln!("missing script path");
                process::exit(2);
            };
            let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
            let result = if compact {
                runtime.inspect_bytecode_file_compact(&path)
            } else {
                runtime.inspect_bytecode_file(&path)
            };
            match result {
                Ok(bytecode) => println!("{bytecode}"),
                Err(err) => {
                    print_diagnostic(&path, &err);
                    process::exit(1);
                }
            }
        }
        _ => print_usage_and_exit(),
    }
}

fn print_usage_and_exit() -> ! {
    eprintln!("usage: nox --version");
    eprintln!("usage: nox run [file.nox]");
    eprintln!("       nox check [--json] <file.nox> [file.nox ...]");
    eprintln!("       nox test [--json] [file-or-dir ...]");
    eprintln!("       nox fmt [--check | --write] <file.nox> [file.nox ...]");
    eprintln!("       nox project check [--json]");
    eprintln!("       nox lsp");
    eprintln!("       nox inspect-bytecode [--compact] <file.nox>");
    process::exit(2);
}

fn manifest_main_from_current_dir() -> Result<PathBuf, String> {
    let manifest = Manifest::discover(Path::new("."))
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "missing script path and no nox.toml was found".to_string())?;
    manifest.main_path().ok_or_else(|| {
        format!(
            "missing script path and '{}' has no [entrypoints].main",
            manifest
                .root
                .join(nox::manifest::MANIFEST_FILE_NAME)
                .display()
        )
    })
}

fn run_check(raw_args: Vec<String>) -> i32 {
    let mut json = false;
    let paths = raw_args
        .into_iter()
        .filter(|arg| {
            if arg == "--json" {
                json = true;
                false
            } else {
                true
            }
        })
        .collect::<Vec<_>>();
    let paths = match discover_check_files(&paths) {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("check: {err}");
            return 2;
        }
    };
    let mut failed = false;
    let mut all_diagnostics = Vec::new();
    let mut file_reports = Vec::new();
    for path in paths {
        let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
        match runtime.check_file_diagnostics(&path) {
            Ok(()) => {
                if !json {
                    println!("{path}: ok");
                }
                file_reports.push(CheckFileReport {
                    path,
                    ok: true,
                    diagnostic_count: 0,
                });
            }
            Err(diagnostics) => {
                let diagnostic_count = diagnostics.len();
                if json {
                    all_diagnostics.extend(
                        diagnostics
                            .into_iter()
                            .map(|diagnostic| (path.clone(), diagnostic)),
                    );
                } else {
                    print_diagnostics(&path, &diagnostics);
                }
                file_reports.push(CheckFileReport {
                    path,
                    ok: false,
                    diagnostic_count,
                });
                failed = true;
            }
        }
    }
    if json {
        println!(
            "{}",
            diagnostics_json(!failed, &all_diagnostics, &file_reports)
        );
    }
    if failed {
        1
    } else {
        0
    }
}

fn run_project(raw_args: Vec<String>) -> i32 {
    let json = match raw_args.as_slice() {
        [command] if command == "check" => false,
        [command, flag] if command == "check" && flag == "--json" => true,
        [] => {
            eprintln!("project: missing subcommand 'check'");
            return 2;
        }
        [command, ..] => {
            eprintln!("project: unknown subcommand '{command}'");
            return 2;
        }
    };

    let manifest = match Manifest::discover(Path::new(".")) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
            eprintln!("project check: no nox.toml was found");
            return 2;
        }
        Err(err) => {
            eprintln!("project check: {err}");
            return 2;
        }
    };

    if json {
        return run_project_check_json(&manifest);
    }

    println!("project check: check");
    let check_status = run_check(Vec::new());
    println!("project check: test");
    let test_status = run_test(Vec::new());
    println!("project check: fmt --check");
    let fmt_status = run_fmt(vec!["--check".to_string()]);

    if check_status == 0 && test_status == 0 && fmt_status == 0 {
        println!("project check: ok");
        0
    } else if check_status == 2 || test_status == 2 || fmt_status == 2 {
        2
    } else {
        1
    }
}

fn run_project_check_json(manifest: &Manifest) -> i32 {
    let exe = match env::current_exe() {
        Ok(exe) => exe,
        Err(err) => {
            eprintln!("project check: failed to resolve current executable: {err}");
            return 2;
        }
    };
    let steps = [
        ("check", vec!["check", "--json"]),
        ("test", vec!["test", "--json"]),
        ("fmt", vec!["fmt", "--check"]),
    ]
    .into_iter()
    .map(|(name, args)| {
        let output = Command::new(&exe)
            .current_dir(&manifest.root)
            .args(args)
            .output();
        match output {
            Ok(output) => ProjectStepReport {
                name,
                status: output.status.code().unwrap_or(1),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            },
            Err(err) => ProjectStepReport {
                name,
                status: 2,
                stdout: String::new(),
                stderr: format!("failed to run project check step '{name}': {err}"),
            },
        }
    })
    .collect::<Vec<_>>();

    let ok = steps.iter().all(|step| step.status == 0);
    println!("{}", project_check_json(manifest, &steps));
    if ok {
        0
    } else if steps.iter().any(|step| step.status == 2) {
        2
    } else {
        1
    }
}

fn run_test(raw_args: Vec<String>) -> i32 {
    let mut json = false;
    let mut paths = Vec::new();
    for arg in raw_args {
        match arg.as_str() {
            "--json" => json = true,
            other if other.starts_with("--") => {
                eprintln!("test: unknown flag '{other}'");
                return 2;
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    let test_files = match discover_test_files(&paths) {
        Ok(files) => files,
        Err(err) => {
            eprintln!("test: {err}");
            return 2;
        }
    };

    let mut reports = Vec::new();
    let mut failed = false;
    for path in test_files {
        let path_string = path.display().to_string();
        let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
        match runtime.run_test_file(&path) {
            Ok(result) => {
                for test in result.tests {
                    let ok = test.passed;
                    if !ok {
                        failed = true;
                    }
                    if !json {
                        println!(
                            "{}::{} {}",
                            path.display(),
                            test.name,
                            if ok { "PASS" } else { "FAIL" }
                        );
                        if let Some(diagnostic) = &test.diagnostic {
                            print_diagnostic(&path_string, diagnostic);
                        }
                    }
                    reports.push(TestReport {
                        path: path_string.clone(),
                        name: test.name,
                        ok,
                        diagnostic: test.diagnostic,
                    });
                }
            }
            Err(diagnostic) => {
                failed = true;
                if !json {
                    println!("{}::<module> FAIL", path.display());
                    print_diagnostic(&path_string, &diagnostic);
                }
                reports.push(TestReport {
                    path: path_string,
                    name: "<module>".to_string(),
                    ok: false,
                    diagnostic: Some(diagnostic),
                });
            }
        }
    }

    if json {
        println!("{}", tests_json(!failed, &reports));
    } else {
        let passed = reports.iter().filter(|report| report.ok).count();
        let failed_count = reports.len() - passed;
        println!(
            "summary: {} tests, {passed} passed, {failed_count} failed",
            reports.len()
        );
    }

    if failed {
        1
    } else {
        0
    }
}

fn discover_check_files(paths: &[String]) -> Result<Vec<String>, String> {
    if !paths.is_empty() {
        return Ok(paths.to_vec());
    }

    let manifest = Manifest::discover(Path::new("."))
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "missing script path and no nox.toml was found".to_string())?;
    let mut roots = Vec::new();
    if let Some(main) = manifest.main_path() {
        roots.push(main);
    }
    roots.extend(manifest.source_dirs());
    roots.extend(manifest.test_dirs());
    if roots.is_empty() {
        roots.push(manifest.root);
    }

    let mut files = Vec::new();
    for root in roots {
        if root.is_file() {
            if is_nox_file(&root) {
                files.push(root);
            } else {
                return Err(format!("'{}' is not a .nox file", root.display()));
            }
        } else if root.is_dir() {
            collect_nox_files(&root, &mut files).map_err(|err| {
                format!("failed to discover files under '{}': {err}", root.display())
            })?;
        } else {
            return Err(format!("path '{}' does not exist", root.display()));
        }
    }

    files.sort_by(|left, right| left.to_string_lossy().cmp(&right.to_string_lossy()));
    files.dedup();
    Ok(files
        .into_iter()
        .map(|path| path.display().to_string())
        .collect())
}

fn discover_test_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut roots = Vec::new();
    if paths.is_empty() {
        match Manifest::discover(Path::new(".")) {
            Ok(Some(manifest)) => {
                let test_dirs = manifest.test_dirs();
                if !test_dirs.is_empty() {
                    roots.extend(test_dirs);
                } else {
                    let source_dirs = manifest.source_dirs();
                    if source_dirs.is_empty() {
                        roots.push(manifest.root);
                    } else {
                        roots.extend(source_dirs);
                    }
                }
            }
            Ok(None) => roots.push(env::current_dir().map_err(|err| err.to_string())?),
            Err(err) => return Err(err.to_string()),
        }
    } else {
        roots.extend(paths.iter().cloned());
    }

    let mut files = Vec::new();
    for root in roots {
        if root.is_file() {
            if is_test_file(&root) {
                files.push(root);
            } else {
                return Err(format!(
                    "'{}' is not a test file; expected '*_test.nox'",
                    root.display()
                ));
            }
        } else if root.is_dir() {
            collect_test_files(&root, &mut files).map_err(|err| {
                format!("failed to discover tests under '{}': {err}", root.display())
            })?;
        } else {
            return Err(format!("path '{}' does not exist", root.display()));
        }
    }

    files.sort_by(|left, right| left.to_string_lossy().cmp(&right.to_string_lossy()));
    files.dedup();
    Ok(files)
}

fn collect_nox_files(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_nox_files(&path, files)?;
        } else if is_nox_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn collect_test_files(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_test_files(&path, files)?;
        } else if is_test_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_nox_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension == "nox")
}

fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("_test.nox"))
}

enum FmtMode {
    Stdout,
    Check,
    Write,
}

fn run_fmt(raw_args: Vec<String>) -> i32 {
    let mut mode = FmtMode::Stdout;
    let mut paths = Vec::new();
    for arg in raw_args {
        match arg.as_str() {
            "--check" => {
                if matches!(mode, FmtMode::Write) {
                    eprintln!("fmt: --check and --write are mutually exclusive");
                    return 2;
                }
                mode = FmtMode::Check;
            }
            "--write" => {
                if matches!(mode, FmtMode::Check) {
                    eprintln!("fmt: --check and --write are mutually exclusive");
                    return 2;
                }
                mode = FmtMode::Write;
            }
            other if other.starts_with("--") => {
                eprintln!("fmt: unknown flag '{other}'");
                return 2;
            }
            _ => paths.push(arg),
        }
    }

    if matches!(mode, FmtMode::Stdout) && paths.is_empty() {
        eprintln!("missing script path");
        return 2;
    }

    if matches!(mode, FmtMode::Stdout) && paths.len() > 1 {
        eprintln!(
            "fmt: writing multiple files to stdout is not supported; pass --check or --write"
        );
        return 2;
    }

    let paths = match discover_fmt_files(&paths, &mode) {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("fmt: {err}");
            return 2;
        }
    };

    let mut failed = false;
    let mut needs_format = Vec::new();
    for path in &paths {
        let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
        let original = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("{path}: failed to read: {err}");
                failed = true;
                continue;
            }
        };
        let formatted = match runtime.format_file(path) {
            Ok(formatted) => formatted,
            Err(err) => {
                print_diagnostic(path, &err);
                failed = true;
                continue;
            }
        };

        match mode {
            FmtMode::Stdout => print!("{formatted}"),
            FmtMode::Check => {
                if formatted != original {
                    println!("{path}");
                    needs_format.push(path.clone());
                }
            }
            FmtMode::Write => {
                if formatted == original {
                    continue;
                }
                if let Err(err) = fs::write(path, &formatted) {
                    eprintln!("{path}: failed to write: {err}");
                    failed = true;
                }
            }
        }
    }

    if failed {
        return 1;
    }
    if matches!(mode, FmtMode::Check) && !needs_format.is_empty() {
        return 1;
    }
    0
}

fn discover_fmt_files(paths: &[String], mode: &FmtMode) -> Result<Vec<String>, String> {
    if matches!(mode, FmtMode::Stdout) {
        let path = Path::new(&paths[0]);
        if !path.is_file() {
            return Err(format!(
                "stdout formatting expects one .nox file, got '{}'",
                path.display()
            ));
        }
        if !is_nox_file(path) {
            return Err(format!("'{}' is not a .nox file", path.display()));
        }
        return Ok(paths.to_vec());
    }

    let roots = if paths.is_empty() {
        let manifest = Manifest::discover(Path::new("."))
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "missing script path and no nox.toml was found".to_string())?;
        let mut roots = Vec::new();
        if let Some(main) = manifest.main_path() {
            roots.push(main);
        }
        roots.extend(manifest.source_dirs());
        roots.extend(manifest.test_dirs());
        if roots.is_empty() {
            roots.push(manifest.root);
        }
        roots
    } else {
        paths.iter().map(PathBuf::from).collect()
    };

    let mut files = Vec::new();
    for root in roots {
        if root.is_file() {
            if is_nox_file(&root) {
                files.push(root);
            } else {
                return Err(format!("'{}' is not a .nox file", root.display()));
            }
        } else if root.is_dir() {
            collect_nox_files(&root, &mut files).map_err(|err| {
                format!("failed to discover files under '{}': {err}", root.display())
            })?;
        } else {
            return Err(format!("path '{}' does not exist", root.display()));
        }
    }

    files.sort_by(|left, right| left.to_string_lossy().cmp(&right.to_string_lossy()));
    files.dedup();
    Ok(files
        .into_iter()
        .map(|path| path.display().to_string())
        .collect())
}

fn print_diagnostic(path: &str, diagnostic: &Diagnostic) {
    if let Some(source) = &diagnostic.source {
        eprintln!(
            "{}:{}:{}: {}",
            source.name, source.line, source.column, diagnostic.message
        );
    } else {
        eprintln!("{path}: {diagnostic}");
    }
}

fn print_diagnostics(path: &str, diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        print_diagnostic(path, diagnostic);
    }
}

fn diagnostics_json(
    ok: bool,
    diagnostics: &[(String, Diagnostic)],
    files: &[CheckFileReport],
) -> String {
    let mut json = String::new();
    write!(
        &mut json,
        "{{\"schema\":\"nox.check.v1\",\"ok\":{ok},\"diagnostics\":["
    )
    .expect("writing to String cannot fail");
    for (index, (path, diagnostic)) in diagnostics.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        write!(
            &mut json,
            "{{\"file\":\"{}\",\"code\":\"{}\",\"message\":\"{}\",\"span\":{{\"start\":{},\"end\":{}}}",
            json_escape(path),
            json_escape(diagnostic.code),
            json_escape(&diagnostic.message),
            diagnostic.span.start,
            diagnostic.span.end
        )
        .expect("writing to String cannot fail");
        if let Some(source) = &diagnostic.source {
            write!(
                &mut json,
                ",\"source\":{{\"name\":\"{}\",\"line\":{},\"column\":{}}}",
                json_escape(&source.name),
                source.line,
                source.column
            )
            .expect("writing to String cannot fail");
        } else {
            json.push_str(",\"source\":null");
        }
        json.push('}');
    }
    json.push_str("],\"files\":[");
    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        write!(
            &mut json,
            "{{\"path\":\"{}\",\"ok\":{},\"diagnostic_count\":{}}}",
            json_escape(&file.path),
            file.ok,
            file.diagnostic_count
        )
        .expect("writing to String cannot fail");
    }
    let checked = files.len();
    let failed = files.iter().filter(|file| !file.ok).count();
    let passed = checked - failed;
    let diagnostic_count = diagnostics.len();
    write!(
        &mut json,
        "],\"summary\":{{\"checked\":{checked},\"passed\":{passed},\"failed\":{failed},\"diagnostic_count\":{diagnostic_count}}}}}"
    )
    .expect("writing to String cannot fail");
    json
}

fn tests_json(ok: bool, reports: &[TestReport]) -> String {
    let mut json = String::new();
    write!(
        &mut json,
        "{{\"schema\":\"nox.test.v1\",\"ok\":{ok},\"tests\":["
    )
    .expect("writing to String cannot fail");
    for (index, report) in reports.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        write!(
            &mut json,
            "{{\"file\":\"{}\",\"name\":\"{}\",\"ok\":{}",
            json_escape(&report.path),
            json_escape(&report.name),
            report.ok
        )
        .expect("writing to String cannot fail");
        match &report.diagnostic {
            Some(diagnostic) => {
                write!(
                    &mut json,
                    ",\"diagnostic\":{{\"code\":\"{}\",\"message\":\"{}\",\"span\":{{\"start\":{},\"end\":{}}}",
                    json_escape(diagnostic.code),
                    json_escape(&diagnostic.message),
                    diagnostic.span.start,
                    diagnostic.span.end
                )
                .expect("writing to String cannot fail");
                if let Some(source) = &diagnostic.source {
                    write!(
                        &mut json,
                        ",\"source\":{{\"name\":\"{}\",\"line\":{},\"column\":{}}}",
                        json_escape(&source.name),
                        source.line,
                        source.column
                    )
                    .expect("writing to String cannot fail");
                } else {
                    json.push_str(",\"source\":null");
                }
                json.push('}');
            }
            None => json.push_str(",\"diagnostic\":null"),
        }
        json.push('}');
    }
    let total = reports.len();
    let failed = reports.iter().filter(|report| !report.ok).count();
    let passed = total - failed;
    write!(
        &mut json,
        "],\"summary\":{{\"tests\":{total},\"passed\":{passed},\"failed\":{failed}}}}}"
    )
    .expect("writing to String cannot fail");
    json
}

fn project_check_json(manifest: &Manifest, steps: &[ProjectStepReport]) -> String {
    let mut json = String::new();
    let ok = steps.iter().all(|step| step.status == 0);
    write!(
        &mut json,
        "{{\"schema\":\"nox.project-check.v1\",\"ok\":{ok},\"manifest\":{{\"root\":\"{}\",\"package\":{{\"name\":\"{}\",\"version\":\"{}\"}}}},\"steps\":[",
        json_escape(&manifest.root.display().to_string()),
        json_escape(&manifest.package.name),
        json_escape(&manifest.package.version)
    )
    .expect("writing to String cannot fail");
    for (index, step) in steps.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        write!(
            &mut json,
            "{{\"name\":\"{}\",\"ok\":{},\"status\":{},\"stdout\":\"{}\",\"stderr\":\"{}\"}}",
            json_escape(step.name),
            step.status == 0,
            step.status,
            json_escape(&step.stdout),
            json_escape(&step.stderr)
        )
        .expect("writing to String cannot fail");
    }
    let total = steps.len();
    let failed = steps.iter().filter(|step| step.status != 0).count();
    let passed = total - failed;
    write!(
        &mut json,
        "],\"summary\":{{\"steps\":{total},\"passed\":{passed},\"failed\":{failed}}}}}"
    )
    .expect("writing to String cannot fail");
    json
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                write!(&mut escaped, "\\u{:04x}", character as u32)
                    .expect("writing to String cannot fail");
            }
            character => escaped.push(character),
        }
    }
    escaped
}
