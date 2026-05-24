use std::{
    collections::BTreeMap,
    env,
    fmt::Write,
    fs, io,
    io::{BufRead, Read},
    path::{Path, PathBuf},
    process::{self, Command},
};

use nox::{
    manifest::{
        validate_lockfile_for_manifest, DependencyPin, DependencySource, LockedDependency,
        Lockfile, LockfileValidation, Manifest,
    },
    Runtime, RuntimePermissions, RuntimeTraceEvent, RuntimeTraceValue,
};
use nox_core::{Diagnostic, Value};

struct CheckFileReport {
    path: String,
    ok: bool,
    diagnostic_count: usize,
}

struct TestReport {
    path: String,
    name: String,
    kind: &'static str,
    ok: bool,
    diagnostic: Option<Diagnostic>,
    attempts: usize,
    retried: bool,
    duration_us: u128,
    stdout: String,
    stderr: String,
    mock_events: Vec<String>,
}

struct TestAttemptResult {
    name: String,
    ok: bool,
    diagnostic: Option<Diagnostic>,
    attempts: usize,
    duration_us: u128,
    stdout: String,
    stderr: String,
    mock_events: Vec<String>,
}

struct ProjectStepReport {
    name: &'static str,
    status: i32,
    stdout: String,
    stderr: String,
}

struct ProjectLockfileReport {
    path: String,
    ok: bool,
    status: &'static str,
    diagnostics: Vec<Diagnostic>,
}

struct ProjectModuleGraph {
    roots: Vec<String>,
    files: Vec<String>,
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
                prepare_process_stdin(&mut runtime);
                match runtime.eval_file(&path) {
                    Ok(value) => {
                        print_process_stderr(&mut runtime);
                        print_run_value(&value);
                        if let Some(code) = runtime.exit_code() {
                            process::exit(code as i32);
                        }
                    }
                    Err(err) => {
                        print_process_stderr(&mut runtime);
                        print_diagnostic(&path.display().to_string(), &err);
                        process::exit(1);
                    }
                }
                return;
            };
            let script_args = args.collect();
            let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
            runtime.set_args(script_args);
            prepare_process_stdin(&mut runtime);
            match runtime.eval_file(&path) {
                Ok(value) => {
                    print_process_stderr(&mut runtime);
                    print_run_value(&value);
                    if let Some(code) = runtime.exit_code() {
                        process::exit(code as i32);
                    }
                }
                Err(err) => {
                    print_process_stderr(&mut runtime);
                    print_diagnostic(&path, &err);
                    process::exit(1);
                }
            }
        }
        "check" => process::exit(run_check(args.collect())),
        "test" => process::exit(run_test(args.collect())),
        "fmt" => process::exit(run_fmt(args.collect())),
        "new" => process::exit(run_new(args.collect())),
        "fetch" => process::exit(run_fetch(args.collect())),
        "project" => process::exit(run_project(args.collect())),
        "repl" => process::exit(run_repl()),
        "lsp" => {
            if let Err(err) = nox::lsp::run_stdio() {
                eprintln!("lsp error: {err}");
                process::exit(1);
            }
        }
        "dap" => process::exit(run_dap()),
        "profile" => process::exit(run_profile(args.collect(), false)),
        "coverage" => process::exit(run_profile(args.collect(), true)),
        "trace" => process::exit(run_trace(args.collect())),
        "watch" => process::exit(run_watch(args.collect())),
        "lint" => process::exit(run_lint(args.collect())),
        "doc" => process::exit(run_doc(args.collect())),
        "host-metadata" => process::exit(run_host_metadata(args.collect())),
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
    eprintln!(
        "       nox test [--json] [--filter <substr>] [--retry <N>] [--export-failures <dir>] [--export-failures-classified <dir>] [file-or-dir ...]"
    );
    eprintln!("       nox fmt [--check | --write] <file.nox> [file.nox ...]");
    eprintln!("       nox new <name> [--dir <path>] [--force]");
    eprintln!("       nox fetch [--offline] [--cache-dir <dir>]");
    eprintln!("       nox project check [--json]");
    eprintln!("       nox repl");
    eprintln!("       nox lsp");
    eprintln!("       nox dap");
    eprintln!("       nox profile [--json] <file.nox>");
    eprintln!("       nox coverage [--json] <file.nox>");
    eprintln!("       nox trace [--ndjson] <file.nox>");
    eprintln!("       nox inspect-bytecode [--compact] <file.nox>");
    eprintln!("       nox watch [--interval-ms <ms>] (check|test|run) [args...]");
    eprintln!("       nox lint [--json] <file.nox> [file.nox ...]");
    eprintln!("       nox doc <file.nox>");
    eprintln!("       nox host-metadata [--json]");
    process::exit(2);
}

fn run_new(raw_args: Vec<String>) -> i32 {
    let mut name = None;
    let mut target_dir = None;
    let mut force = false;
    let mut args = raw_args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--force" => force = true,
            "--dir" => {
                let Some(path) = args.next() else {
                    eprintln!("new: --dir requires a path");
                    return 2;
                };
                if target_dir.replace(PathBuf::from(path)).is_some() {
                    eprintln!("new: --dir was provided more than once");
                    return 2;
                }
            }
            other if other.starts_with("--") => {
                eprintln!("new: unknown flag '{other}'");
                return 2;
            }
            other => {
                if name.replace(other.to_string()).is_some() {
                    eprintln!("new: unexpected argument '{other}'");
                    return 2;
                }
            }
        }
    }

    let Some(name) = name else {
        eprintln!("new: missing project name");
        return 2;
    };
    if let Err(err) = validate_new_package_name(&name) {
        eprintln!("new: {err}");
        return 2;
    }

    let target = target_dir.unwrap_or_else(|| PathBuf::from(&name));
    match create_new_project(&name, &target, force) {
        Ok(()) => {
            println!("created Nox project '{}' at {}", name, target.display());
            0
        }
        Err(err) => {
            eprintln!("new: {err}");
            2
        }
    }
}

fn run_fetch(raw_args: Vec<String>) -> i32 {
    let mut offline = false;
    let mut cache_dir: Option<PathBuf> = None;
    let mut args = raw_args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--offline" => offline = true,
            "--cache-dir" => {
                let Some(path) = args.next() else {
                    eprintln!("fetch: --cache-dir requires a path");
                    return 2;
                };
                if cache_dir.replace(PathBuf::from(path)).is_some() {
                    eprintln!("fetch: --cache-dir was provided more than once");
                    return 2;
                }
            }
            other => {
                eprintln!("fetch: unknown argument '{other}'");
                return 2;
            }
        }
    }

    let manifest = match Manifest::discover(Path::new(".")) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
            eprintln!("fetch: no nox.toml was found");
            return 2;
        }
        Err(err) => {
            eprintln!("fetch: {err}");
            return 2;
        }
    };

    if manifest.dependencies.is_empty() {
        println!("fetch: no dependencies");
        return 0;
    }

    let cache_dir = cache_dir.unwrap_or_else(default_module_cache_dir);
    if let Err(err) = fs::create_dir_all(&cache_dir) {
        eprintln!(
            "fetch: failed to create module cache '{}': {err}",
            cache_dir.display()
        );
        return 2;
    }

    let mut locked = Vec::new();
    for dependency in &manifest.dependencies {
        match fetch_dependency(dependency, &cache_dir, offline) {
            Ok(entry) => {
                println!("fetch: {} {}", dependency.name, entry.resolved);
                locked.push(entry);
            }
            Err(message) => {
                eprintln!("fetch: {message}");
                return 1;
            }
        }
    }

    let lockfile = Lockfile {
        dependencies: locked,
    };
    let lock_path = manifest.root.join(nox::manifest::LOCK_FILE_NAME);
    if let Err(err) = fs::write(&lock_path, lockfile.to_source()) {
        eprintln!(
            "fetch: failed to write lockfile '{}': {err}",
            lock_path.display()
        );
        return 1;
    }
    println!("fetch: wrote {}", lock_path.display());
    0
}

fn default_module_cache_dir() -> PathBuf {
    if let Ok(path) = env::var("NOX_MODULE_CACHE") {
        return PathBuf::from(path);
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("nox")
            .join("modules");
    }
    env::temp_dir().join("nox").join("modules")
}

fn fetch_dependency(
    dependency: &nox::manifest::DependencyDecl,
    cache_dir: &Path,
    offline: bool,
) -> Result<LockedDependency, String> {
    let source_url = dependency_source_url(&dependency.source);
    let cache_key = dependency_cache_key(dependency);
    let cache_path = cache_dir.join(&cache_key);

    if cache_path.exists() {
        if !offline {
            run_git(
                [
                    "--git-dir",
                    &cache_path.display().to_string(),
                    "fetch",
                    "--tags",
                    "origin",
                ],
                None,
            )
            .map_err(|err| {
                format!(
                    "dependency '{}' failed to update cache '{}': {err}",
                    dependency.name,
                    cache_path.display()
                )
            })?;
        }
    } else if offline {
        return Err(format!(
            "dependency '{}' cache miss in offline mode at '{}'",
            dependency.name,
            cache_path.display()
        ));
    } else {
        run_git(
            [
                "clone",
                "--bare",
                &source_url,
                &cache_path.display().to_string(),
            ],
            None,
        )
        .map_err(|err| {
            format!(
                "dependency '{}' failed to clone '{}': {err}",
                dependency.name, source_url
            )
        })?;
    }

    let resolved = resolve_dependency_commit(&cache_path, &dependency.pin).map_err(|err| {
        format!(
            "dependency '{}' failed to resolve pin: {err}",
            dependency.name
        )
    })?;
    let archive = nox::run_git_capture(
        &[
            "--git-dir",
            &cache_path.display().to_string(),
            "archive",
            "--format=tar",
            &resolved,
        ],
        None,
    )
    .map_err(|err| {
        format!(
            "dependency '{}' failed to archive resolved commit: {err}",
            dependency.name
        )
    })?;
    let content_hash = format!("sha256:{}", nox::sha256_hex_bytes(&archive));

    Ok(LockedDependency {
        name: dependency.name.clone(),
        source: dependency.source.clone(),
        pin: dependency.pin.clone(),
        resolved,
        content_hash,
        cache_key,
        tool: format!("nox {}", env!("CARGO_PKG_VERSION")),
    })
}

fn dependency_source_url(source: &DependencySource) -> String {
    match source {
        DependencySource::GitHub(repo) => format!("https://github.com/{repo}.git"),
        DependencySource::Git(url) => url.clone(),
    }
}

fn dependency_cache_key(dependency: &nox::manifest::DependencyDecl) -> String {
    format!(
        "{}-{}-{}",
        sanitize_cache_component(&dependency.name),
        dependency.source.kind(),
        sanitize_cache_component(dependency.source.value())
    )
}

fn sanitize_cache_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn resolve_dependency_commit(cache_path: &Path, pin: &DependencyPin) -> Result<String, String> {
    let rev = format!("{}^{{commit}}", pin.value());
    let output = nox::run_git_capture(
        &[
            "--git-dir",
            &cache_path.display().to_string(),
            "rev-parse",
            &rev,
        ],
        None,
    )?;
    Ok(String::from_utf8_lossy(&output).trim().to_string())
}

fn run_git<const N: usize>(args: [&str; N], cwd: Option<&Path>) -> Result<(), String> {
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .args(args)
        .output()
        .map_err(|err| format!("failed to run git: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn validate_new_package_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err("package name cannot be empty".to_string());
    };
    if !first.is_ascii_lowercase() {
        return Err("package name must start with an ASCII lowercase letter".to_string());
    }
    if !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_') {
        return Err(
            "package name may contain only ASCII lowercase letters, digits, '-' and '_'"
                .to_string(),
        );
    }
    Ok(())
}

fn create_new_project(name: &str, target: &Path, force: bool) -> Result<(), String> {
    if target.exists() {
        if !target.is_dir() {
            return Err(format!(
                "target path '{}' is not a directory",
                target.display()
            ));
        }
        if !force
            && target
                .read_dir()
                .map_err(|err| {
                    format!(
                        "failed to read target directory '{}': {err}",
                        target.display()
                    )
                })?
                .next()
                .is_some()
        {
            return Err(format!(
                "target directory '{}' already exists and is not empty; pass --force to overwrite scaffold files",
                target.display()
            ));
        }
    }

    fs::create_dir_all(target).map_err(|err| {
        format!(
            "failed to create target directory '{}': {err}",
            target.display()
        )
    })?;

    let files = [
        ("nox.toml", new_manifest_template(name)),
        ("src/main.nox", new_main_template()),
        ("tests/main_test.nox", new_test_template()),
        ("README.md", new_readme_template(name)),
    ];

    for (relative, contents) in files {
        write_scaffold_file(&target.join(relative), contents)?;
    }
    Ok(())
}

fn write_scaffold_file(path: &Path, contents: String) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("scaffold path '{}' has no parent", path.display()))?;
    if parent.exists() && !parent.is_dir() {
        return Err(format!(
            "cannot create '{}' because '{}' is not a directory",
            path.display(),
            parent.display()
        ));
    }
    fs::create_dir_all(parent)
        .map_err(|err| format!("failed to create directory '{}': {err}", parent.display()))?;
    fs::write(path, contents)
        .map_err(|err| format!("failed to write scaffold file '{}': {err}", path.display()))
}

fn new_manifest_template(name: &str) -> String {
    format!(
        "[package]\nname = \"{}\"\nversion = \"0.0.1\"\ndescription = \"Nox project\"\n\n[entrypoints]\nmain = \"src/main.nox\"\n\n[modules]\nsource_dirs = [\"src\"]\ntest_dirs = [\"tests\"]\n",
        name
    )
}

fn new_main_template() -> String {
    "export fn greet(name: str) -> str {\n    return \"hello, \" + name;\n}\n\ngreet(\"nox\");\n"
        .to_string()
}

fn new_test_template() -> String {
    "import \"main.nox\" as app;\n\nfn test_greet() -> bool {\n    return app.greet(\"nox\") == \"hello, nox\";\n}\n"
        .to_string()
}

fn new_readme_template(name: &str) -> String {
    format!("# {name}\n\n```sh\nnox project check\nnox run\nnox test\nnox fmt --check\n```\n")
}

fn run_host_metadata(raw_args: Vec<String>) -> i32 {
    let mut json = false;
    for arg in raw_args {
        match arg.as_str() {
            "--json" => json = true,
            other if other.starts_with("--") => {
                eprintln!("host-metadata: unknown flag '{other}'");
                return 2;
            }
            other => {
                eprintln!("host-metadata: unexpected argument '{other}'");
                return 2;
            }
        }
    }

    let runtime = Runtime::new();
    let signatures = runtime
        .engine()
        .host_function_names()
        .into_iter()
        .filter_map(|name| runtime.engine().host_function_signature(&name))
        .filter(|signature| !signature.name.starts_with("__"))
        .collect::<Vec<_>>();

    if json {
        let functions = signatures
            .iter()
            .map(|signature| {
                let params = signature
                    .params
                    .iter()
                    .map(|(name, ty)| {
                        format!(
                            "{{\"name\":\"{}\",\"type\":\"{}\"}}",
                            json_escape(name),
                            json_escape(&ty.to_string())
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let capabilities = signature
                    .capabilities
                    .iter()
                    .map(|capability| format!("\"{}\"", json_escape(capability)))
                    .collect::<Vec<_>>()
                    .join(",");
                let docstring = signature
                    .docstring
                    .as_ref()
                    .map(|doc| format!("\"{}\"", json_escape(doc)))
                    .unwrap_or_else(|| "null".to_string());
                format!(
                    "{{\"name\":\"{}\",\"params\":[{}],\"return_type\":\"{}\",\"docstring\":{},\"capabilities\":[{}]}}",
                    json_escape(&signature.name),
                    params,
                    json_escape(&signature.return_type.to_string()),
                    docstring,
                    capabilities
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!("{{\"schema\":\"nox.host-metadata.v1\",\"functions\":[{functions}]}}");
    } else {
        for signature in signatures {
            let params = signature
                .params
                .iter()
                .map(|(name, ty)| format!("{name}: {ty}"))
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "fn {}({params}) -> {}",
                signature.name, signature.return_type
            );
            if let Some(doc) = signature.docstring {
                println!("  {doc}");
            }
            if !signature.capabilities.is_empty() {
                println!("  capabilities: {}", signature.capabilities.join(", "));
            }
        }
    }
    0
}

fn run_doc(raw_args: Vec<String>) -> i32 {
    use std::fmt::Write as _;

    let Some(path) = raw_args.first() else {
        eprintln!("doc: expected a file path");
        return 2;
    };
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("{path}: failed to read: {err}");
            return 1;
        }
    };

    let mut out = String::new();
    writeln!(&mut out, "# `{}`", path).unwrap();
    writeln!(&mut out).unwrap();

    let declarations = doc_declarations(&source);
    for declaration in &declarations {
        writeln!(&mut out, "## {}", declaration.signature).unwrap();
        writeln!(&mut out).unwrap();
        writeln!(
            &mut out,
            "Kind: **{}**. Visibility: **{}**.",
            declaration.kind, declaration.visibility
        )
        .unwrap();
        if let Some(call_return_type) = &declaration.call_return_type {
            writeln!(&mut out, "Call return: **{call_return_type}**.").unwrap();
        }
        writeln!(&mut out).unwrap();
        if !declaration.docs.is_empty() {
            for doc in &declaration.docs {
                writeln!(&mut out, "{doc}").unwrap();
            }
            writeln!(&mut out).unwrap();
        }
    }

    if declarations.is_empty() {
        writeln!(
            &mut out,
            "_No fn / record / enum / type declarations found._"
        )
        .unwrap();
    }

    print!("{out}");
    0
}

struct DocDeclaration {
    signature: String,
    kind: &'static str,
    visibility: &'static str,
    call_return_type: Option<String>,
    docs: Vec<String>,
}

fn doc_declarations(source: &str) -> Vec<DocDeclaration> {
    let bytes = source.as_bytes();
    let mut declarations = Vec::new();
    let mut pending_docs = Vec::new();
    let mut index = 0;
    let mut depth = 0usize;
    let mut line_start = true;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                pending_docs.clear();
                line_start = false;
                index = skip_string_literal(source, index);
            }
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'/' => {
                let line_end = source[index..]
                    .find('\n')
                    .map(|offset| index + offset)
                    .unwrap_or(source.len());
                if depth == 0 && line_start && bytes.get(index + 2) == Some(&b'/') {
                    let doc = source[index + 3..line_end]
                        .strip_prefix(' ')
                        .unwrap_or(&source[index + 3..line_end])
                        .to_string();
                    pending_docs.push(doc);
                } else if !source[index..line_end].trim().is_empty() {
                    pending_docs.clear();
                }
                index = line_end;
            }
            b'{' => {
                pending_docs.clear();
                depth += 1;
                line_start = false;
                index += 1;
            }
            b'}' => {
                pending_docs.clear();
                depth = depth.saturating_sub(1);
                line_start = false;
                index += 1;
            }
            b'\n' => {
                line_start = true;
                index += 1;
            }
            byte if byte.is_ascii_whitespace() => {
                index += 1;
            }
            byte if depth == 0 && is_identifier_start_byte(byte) => {
                let Some((declaration, next)) = parse_doc_declaration(source, index, &pending_docs)
                else {
                    pending_docs.clear();
                    index = skip_identifier(source, index);
                    line_start = false;
                    continue;
                };
                declarations.push(declaration);
                pending_docs.clear();
                index = next;
                line_start = false;
            }
            _ => {
                pending_docs.clear();
                line_start = false;
                index += 1;
            }
        }
    }
    declarations
}

fn parse_doc_declaration(
    source: &str,
    start: usize,
    docs: &[String],
) -> Option<(DocDeclaration, usize)> {
    let bytes = source.as_bytes();
    let (first, mut index) = read_identifier(source, start)?;
    let exported = first == "export";
    let kind_start = if exported {
        index = skip_ws_bytes(source, index);
        index
    } else {
        start
    };
    let (first_kind, after_first_kind) = read_identifier(source, kind_start)?;
    let (kind, after_kind, is_async) = if first_kind == "async" {
        let fn_start = skip_ws_bytes(source, after_first_kind);
        let (kind, after_kind) = read_identifier(source, fn_start)?;
        (kind, after_kind, true)
    } else {
        (first_kind, after_first_kind, false)
    };
    let kind = match kind.as_str() {
        "fn" | "record" | "enum" | "type" | "trait" => kind,
        _ => return None,
    };
    if is_async && kind != "fn" {
        return None;
    }
    let mut end = after_kind;
    while end < bytes.len() {
        match bytes[end] {
            b'"' => end = skip_string_literal(source, end),
            b'{' if kind != "type" => break,
            b';' if kind == "type" => {
                end += 1;
                break;
            }
            b'\n' | b'\r' | b'\t' | b' ' => end += 1,
            _ => end += 1,
        }
    }
    if end <= after_kind || end > source.len() {
        return None;
    }
    let signature_source = source[start..end].trim().trim_end_matches('{');
    let signature = signature_source
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let call_return_type = if is_async {
        async_doc_call_return_type(signature_source)
    } else {
        None
    };
    Some((
        DocDeclaration {
            signature,
            kind: match kind.as_str() {
                "fn" => "fn",
                "record" => "record",
                "enum" => "enum",
                "type" => "type",
                "trait" => "trait",
                _ => return None,
            },
            visibility: if exported { "exported" } else { "local" },
            call_return_type,
            docs: docs.to_vec(),
        },
        end,
    ))
}

fn async_doc_call_return_type(signature_source: &str) -> Option<String> {
    let return_type = signature_source.rsplit_once("->")?.1.trim();
    if return_type.is_empty() {
        return None;
    }
    Some(format!("task[{return_type}]"))
}

fn skip_string_literal(source: &str, mut index: usize) -> usize {
    let bytes = source.as_bytes();
    index += 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 1 < bytes.len() {
            index += 2;
        } else if bytes[index] == b'"' {
            index += 1;
            break;
        } else {
            index += 1;
        }
    }
    index
}

fn read_identifier(source: &str, start: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    if start >= bytes.len() || !is_identifier_start_byte(bytes[start]) {
        return None;
    }
    let mut end = start + 1;
    while end < bytes.len() && is_identifier_continue_byte(bytes[end]) {
        end += 1;
    }
    Some((source[start..end].to_string(), end))
}

fn skip_identifier(source: &str, start: usize) -> usize {
    read_identifier(source, start)
        .map(|(_, end)| end)
        .unwrap_or(start + 1)
}

fn skip_ws_bytes(source: &str, mut index: usize) -> usize {
    let bytes = source.as_bytes();
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn is_identifier_start_byte(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn collect_required_capabilities(source: &str) -> Vec<&'static str> {
    use std::collections::BTreeSet;

    let mut imports: BTreeSet<&'static str> = BTreeSet::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("import") {
            continue;
        }
        for specifier in [
            "std/fs.nox",
            "std/env.nox",
            "std/time.nox",
            "std/task.nox",
            "std/http.nox",
            "std/process.nox",
        ] {
            if trimmed.contains(&format!("\"{specifier}\"")) {
                imports.insert(specifier);
            }
        }
    }

    let mut caps: BTreeSet<&'static str> = BTreeSet::new();
    if imports.contains("std/fs.nox") {
        caps.insert("filesystem");
        if source.contains("write_text(") || source.contains("write_binary(") {
            caps.insert("filesystem_write");
        }
    }
    if imports.contains("std/env.nox") {
        caps.insert("environment");
    }
    if imports.contains("std/time.nox") && source.contains("sleep_ms(") {
        caps.insert("timers");
    }
    if imports.contains("std/task.nox") {
        caps.insert("async_tasks");
    }
    if imports.contains("std/http.nox") {
        caps.insert("network");
    }
    if imports.contains("std/process.nox")
        && (source.contains("process.run(") || source.contains("process.run_with("))
    {
        caps.insert("process_run");
    }
    caps.into_iter().collect()
}

fn run_lint(raw_args: Vec<String>) -> i32 {
    use std::fmt::Write as _;

    let mut json = false;
    let mut paths: Vec<String> = Vec::new();
    for arg in raw_args {
        match arg.as_str() {
            "--json" => json = true,
            other if other.starts_with("--") => {
                eprintln!("lint: unknown flag '{other}'");
                return 2;
            }
            _ => paths.push(arg),
        }
    }
    if paths.is_empty() {
        eprintln!("lint: expected at least one file");
        return 2;
    }

    let mut overall_status = 0;
    let mut total_warnings = 0;
    let mut all_entries: Vec<String> = Vec::new();
    let mut all_capabilities: std::collections::BTreeSet<&'static str> =
        std::collections::BTreeSet::new();
    for path in &paths {
        let source = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("{path}: failed to read: {err}");
                overall_status = 1;
                continue;
            }
        };
        for cap in collect_required_capabilities(&source) {
            all_capabilities.insert(cap);
        }
        let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
        match runtime.lint(&source) {
            Ok(warnings) => {
                if json {
                    for warning in &warnings {
                        let mut entry = String::new();
                        write!(
                            &mut entry,
                            "{{\"file\":\"{}\",\"code\":\"{}\",\"message\":\"{}\",\"span\":{{\"start\":{},\"end\":{}}}}}",
                            json_escape(path),
                            warning.code,
                            json_escape(&warning.message),
                            warning.span.start,
                            warning.span.end
                        )
                        .expect("writing to String cannot fail");
                        all_entries.push(entry);
                    }
                } else {
                    for warning in &warnings {
                        println!(
                            "{}:{}:{} [{}] {}",
                            path,
                            warning.span.start,
                            warning.span.end,
                            warning.code,
                            warning.message
                        );
                    }
                }
                total_warnings += warnings.len();
            }
            Err(diagnostic) => {
                overall_status = 1;
                print_diagnostic(path, &diagnostic);
            }
        }
    }

    if json {
        let mut out = String::from("{\"schema\":\"nox.lint.v1\",\"warnings\":[");
        for (index, entry) in all_entries.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(entry);
        }
        let caps_json = all_capabilities
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(",");
        write!(
            &mut out,
            "],\"summary\":{{\"file_count\":{},\"warning_count\":{},\"capabilities\":[{caps_json}]}}}}",
            paths.len(),
            total_warnings
        )
        .expect("writing to String cannot fail");
        println!("{out}");
    } else {
        println!("summary: {} files, {total_warnings} warnings", paths.len());
        if !all_capabilities.is_empty() {
            let listing = all_capabilities
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .join(", ");
            println!("capabilities: {listing}");
        }
    }

    overall_status
}

fn run_watch(raw_args: Vec<String>) -> i32 {
    let mut interval_ms: u64 = 500;
    let mut remaining: Vec<String> = Vec::new();
    let mut iter = raw_args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--interval-ms" {
            let Some(value) = iter.next() else {
                eprintln!("watch: --interval-ms requires a millisecond value");
                return 2;
            };
            match value.parse::<u64>() {
                Ok(parsed) if parsed >= 1 => interval_ms = parsed,
                _ => {
                    eprintln!("watch: --interval-ms expects a positive integer");
                    return 2;
                }
            }
        } else {
            remaining.push(arg);
            remaining.extend(iter);
            break;
        }
    }
    let Some(subcommand) = remaining.first().cloned() else {
        eprintln!("watch: expected one of check / test / run after options");
        return 2;
    };
    if !matches!(subcommand.as_str(), "check" | "test" | "run") {
        eprintln!("watch: unsupported subcommand '{subcommand}'; expected check, test, or run");
        return 2;
    }
    let forwarded_args: Vec<String> = remaining.into_iter().skip(1).collect();

    let watch_paths = match collect_watch_paths() {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("watch: [{}] {}", err.code, err.message);
            return 2;
        }
    };

    let nox_bin = match env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("watch: cannot determine current nox executable: {err}");
            return 2;
        }
    };

    eprintln!(
        "watch: monitoring {} path(s); interval {}ms; subcommand: {}",
        watch_paths.len(),
        interval_ms,
        subcommand
    );

    let mut last_signature = scan_signature(&watch_paths);
    run_watch_subcommand(&nox_bin, &subcommand, &forwarded_args);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(interval_ms));
        let signature = scan_signature(&watch_paths);
        if signature != last_signature {
            last_signature = signature;
            eprintln!("watch: change detected, re-running '{subcommand}'");
            run_watch_subcommand(&nox_bin, &subcommand, &forwarded_args);
        }
    }
}

fn run_watch_subcommand(nox_bin: &Path, subcommand: &str, args: &[String]) {
    let mut command = Command::new(nox_bin);
    command.arg(subcommand);
    for arg in args {
        command.arg(arg);
    }
    match command.status() {
        Ok(status) => {
            eprintln!(
                "watch: subcommand finished with exit code {}",
                status.code().unwrap_or(-1)
            );
        }
        Err(err) => {
            eprintln!("watch: failed to launch subcommand: {err}");
        }
    }
}

fn collect_watch_paths() -> Result<Vec<PathBuf>, WatchError> {
    let cwd = env::current_dir().map_err(|err| WatchError {
        code: "watch.error",
        message: format!("cannot read current directory: {err}"),
    })?;
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(Some(manifest)) = Manifest::discover(&cwd) {
        for dir in &manifest.modules.source_dirs {
            roots.push(manifest.root.join(dir));
        }
        for dir in &manifest.modules.test_dirs {
            roots.push(manifest.root.join(dir));
        }
    }
    if roots.is_empty() {
        roots.push(cwd.clone());
    }
    for root in &roots {
        if !root.exists() {
            return Err(WatchError {
                code: "watch.path-not-found",
                message: format!("watch path not found: {}", root.display()),
            });
        }
    }
    Ok(roots)
}

struct WatchError {
    code: &'static str,
    message: String,
}

fn scan_signature(roots: &[PathBuf]) -> Vec<(PathBuf, std::time::SystemTime, u64)> {
    let mut entries: Vec<(PathBuf, std::time::SystemTime, u64)> = Vec::new();
    for root in roots {
        collect_watch_files(root, &mut entries);
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

fn collect_watch_files(root: &Path, entries: &mut Vec<(PathBuf, std::time::SystemTime, u64)>) {
    let Ok(metadata) = root.metadata() else {
        return;
    };
    if metadata.is_file() {
        if root.extension().and_then(|s| s.to_str()) == Some("nox") {
            let modified = metadata.modified().unwrap_or(std::time::UNIX_EPOCH);
            entries.push((root.to_path_buf(), modified, metadata.len()));
        }
        return;
    }
    if metadata.is_dir() {
        let Ok(read_dir) = fs::read_dir(root) else {
            return;
        };
        for entry in read_dir.flatten() {
            collect_watch_files(&entry.path(), entries);
        }
    }
}

fn run_dap() -> i32 {
    match nox::dap::run_stdio() {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("dap error: {err}");
            1
        }
    }
}

fn run_profile(raw_args: Vec<String>, coverage: bool) -> i32 {
    let mut json = false;
    let mut ndjson = false;
    let mut path: Option<String> = None;
    for arg in raw_args {
        match arg.as_str() {
            "--json" => json = true,
            "--ndjson" => ndjson = true,
            other if other.starts_with("--") => {
                eprintln!("profile: unknown flag '{other}'");
                return 2;
            }
            _ => path = Some(arg),
        }
    }
    if json && ndjson {
        eprintln!("profile: --json and --ndjson are mutually exclusive");
        return 2;
    }
    let Some(path) = path else {
        eprintln!("missing script path");
        return 2;
    };
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("{path}: failed to read: {err}");
            return 1;
        }
    };
    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    let result = runtime.profile(&source);
    match result {
        Ok((value, profile)) => {
            if json {
                use std::fmt::Write as _;
                let mut json_out = String::new();
                let schema = if coverage {
                    "nox.coverage.v1"
                } else {
                    "nox.profile.v1"
                };
                write!(&mut json_out, "{{\"schema\":\"{schema}\",\"functions\":[").unwrap();
                for (index, (function, row)) in profile.functions.iter().enumerate() {
                    if index > 0 {
                        json_out.push(',');
                    }
                    write!(
                        &mut json_out,
                        "{{\"name\":\"{}\",\"call_count\":{},\"total_us\":{}}}",
                        json_escape(function),
                        row.call_count,
                        row.total_time.as_micros()
                    )
                    .unwrap();
                }
                json_out.push_str("],\"operations\":[");
                for (index, (operation, row)) in profile.operations.iter().enumerate() {
                    if index > 0 {
                        json_out.push(',');
                    }
                    write!(
                        &mut json_out,
                        "{{\"name\":\"{}\",\"count\":{},\"total_us\":{}}}",
                        json_escape(operation),
                        row.count,
                        row.total_time.as_micros()
                    )
                    .unwrap();
                }
                if coverage {
                    json_out.push_str("],\"statements\":[");
                    for (index, (span, row)) in profile.statements.iter().enumerate() {
                        if index > 0 {
                            json_out.push(',');
                        }
                        write_coverage_statement_json(
                            &mut json_out,
                            &source,
                            *span,
                            row.execution_count,
                        );
                    }
                    json_out.push_str("],\"branches\":[");
                    for (index, (span, row)) in profile.branches.iter().enumerate() {
                        if index > 0 {
                            json_out.push(',');
                        }
                        write_coverage_branch_json(
                            &mut json_out,
                            &source,
                            *span,
                            row.true_count,
                            row.false_count,
                        );
                    }
                    json_out.push(']');
                }
                json_out.push_str("]}");
                println!("{json_out}");
            } else if ndjson {
                use std::fmt::Write as _;
                let schema = if coverage {
                    "nox.coverage.event.v1"
                } else {
                    "nox.profile.event.v1"
                };
                for (function, row) in &profile.functions {
                    let mut line = String::new();
                    write!(
                        &mut line,
                        "{{\"schema\":\"{schema}\",\"name\":\"{}\",\"call_count\":{},\"total_us\":{}}}",
                        json_escape(function),
                        row.call_count,
                        row.total_time.as_micros()
                    )
                    .unwrap();
                    println!("{line}");
                }
                for (operation, row) in &profile.operations {
                    let mut line = String::new();
                    write!(
                        &mut line,
                        "{{\"schema\":\"{schema}\",\"kind\":\"operation\",\"name\":\"{}\",\"count\":{},\"total_us\":{}}}",
                        json_escape(operation),
                        row.count,
                        row.total_time.as_micros()
                    )
                    .unwrap();
                    println!("{line}");
                }
                if coverage {
                    for (span, row) in &profile.statements {
                        let mut line = String::new();
                        write!(
                            &mut line,
                            "{{\"schema\":\"{schema}\",\"kind\":\"statement\","
                        )
                        .unwrap();
                        write_coverage_statement_fields(
                            &mut line,
                            &source,
                            *span,
                            row.execution_count,
                        );
                        line.push('}');
                        println!("{line}");
                    }
                    for (span, row) in &profile.branches {
                        let mut line = String::new();
                        write!(&mut line, "{{\"schema\":\"{schema}\",\"kind\":\"branch\",")
                            .unwrap();
                        write_coverage_branch_fields(
                            &mut line,
                            &source,
                            *span,
                            row.true_count,
                            row.false_count,
                        );
                        line.push('}');
                        println!("{line}");
                    }
                }
            } else if coverage {
                println!("coverage\tfunction\tcovered");
                for (function, row) in &profile.functions {
                    println!("coverage\t{function}\t{}", row.call_count > 0);
                }
                println!("coverage\tstatement\tstart\tend\texecution_count");
                for (span, row) in &profile.statements {
                    println!(
                        "coverage\tstatement\t{}\t{}\t{}",
                        span.start, span.end, row.execution_count
                    );
                }
                println!("coverage\tbranch\tstart\tend\ttrue_count\tfalse_count");
                for (span, row) in &profile.branches {
                    println!(
                        "coverage\tbranch\t{}\t{}\t{}\t{}",
                        span.start, span.end, row.true_count, row.false_count
                    );
                }
            } else {
                println!("function\tcall_count\ttotal_us");
                for (function, row) in &profile.functions {
                    println!(
                        "{function}\t{}\t{}",
                        row.call_count,
                        row.total_time.as_micros()
                    );
                }
                if !profile.operations.is_empty() {
                    println!("operation\tcount\ttotal_us");
                    for (operation, row) in &profile.operations {
                        println!(
                            "operation\t{operation}\t{}\t{}",
                            row.count,
                            row.total_time.as_micros()
                        );
                    }
                }
            }
            print_run_value(&value);
            0
        }
        Err(err) => {
            print_diagnostic(&path, &err.with_source(&path, &source));
            1
        }
    }
}

fn write_coverage_statement_json(
    out: &mut String,
    source: &str,
    span: nox_core::Span,
    execution_count: u64,
) {
    out.push('{');
    write_coverage_statement_fields(out, source, span, execution_count);
    out.push('}');
}

fn write_coverage_statement_fields(
    out: &mut String,
    source: &str,
    span: nox_core::Span,
    execution_count: u64,
) {
    use std::fmt::Write as _;

    write_span_source_fields(out, source, span);
    write!(out, ",\"execution_count\":{execution_count}").unwrap();
}

fn write_coverage_branch_json(
    out: &mut String,
    source: &str,
    span: nox_core::Span,
    true_count: u64,
    false_count: u64,
) {
    out.push('{');
    write_coverage_branch_fields(out, source, span, true_count, false_count);
    out.push('}');
}

fn write_coverage_branch_fields(
    out: &mut String,
    source: &str,
    span: nox_core::Span,
    true_count: u64,
    false_count: u64,
) {
    use std::fmt::Write as _;

    write_span_source_fields(out, source, span);
    write!(
        out,
        ",\"true_count\":{true_count},\"false_count\":{false_count},\"covered\":{}",
        true_count > 0 && false_count > 0
    )
    .unwrap();
}

fn write_span_source_fields(out: &mut String, source: &str, span: nox_core::Span) {
    use std::fmt::Write as _;

    let (line, column) = source_line_column(source, span.start);
    let (end_line, end_column) = source_line_column(source, span.end);
    write!(
        out,
        "\"span\":{{\"start\":{},\"end\":{}}},\"source\":{{\"line\":{},\"column\":{},\"end_line\":{},\"end_column\":{}}}",
        span.start, span.end, line, column, end_line, end_column
    )
    .unwrap();
}

fn source_line_column(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut column = 1usize;
    for (index, byte) in source.bytes().enumerate() {
        if index >= offset {
            break;
        }
        if byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn run_trace(raw_args: Vec<String>) -> i32 {
    use std::fmt::Write as _;

    let mut path: Option<String> = None;
    for arg in raw_args {
        match arg.as_str() {
            "--ndjson" => {}
            other if other.starts_with("--") => {
                eprintln!("trace: unknown flag '{other}'");
                return 2;
            }
            _ => path = Some(arg),
        }
    }
    let Some(path) = path else {
        eprintln!("trace: missing script path");
        return 2;
    };
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("{path}: failed to read: {err}");
            return 1;
        }
    };

    let trace_id = format!("trace-{}", stable_trace_id(&path, &source));
    let mut sequence = 0u64;
    emit_trace_event(
        &trace_id,
        &mut sequence,
        "run_start",
        &format!("\"file\":\"{}\"", json_escape(&path)),
    );
    let capabilities = collect_required_capabilities(&source);
    let caps_json = capabilities
        .iter()
        .map(|cap| format!("\"{}\"", json_escape(cap)))
        .collect::<Vec<_>>()
        .join(",");
    emit_trace_event(
        &trace_id,
        &mut sequence,
        "permission_summary",
        &format!("\"capabilities\":[{caps_json}]"),
    );
    for capability in &capabilities {
        emit_trace_event(
            &trace_id,
            &mut sequence,
            "permission_check",
            &format!(
                "\"capability\":\"{}\",\"result\":\"required\"",
                json_escape(capability)
            ),
        );
    }

    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    runtime.set_mock_stdout(true);
    runtime.set_runtime_trace_enabled(true);
    let result = runtime.profile_file(&path);
    let runtime_events = runtime.take_runtime_trace_events();
    let stdout = runtime.take_stdout();
    let stderr = runtime.take_stderr();
    for event in &runtime_events {
        emit_runtime_trace_event(&trace_id, &mut sequence, event);
    }
    if !stdout.is_empty() {
        emit_trace_event(
            &trace_id,
            &mut sequence,
            "stdout",
            &format!("\"text\":\"{}\"", json_escape(&stdout)),
        );
    }
    if !stderr.is_empty() {
        emit_trace_event(
            &trace_id,
            &mut sequence,
            "stderr",
            &format!("\"text\":\"{}\"", json_escape(&stderr)),
        );
    }

    match result {
        Ok((_value, profile)) => {
            for (function, row) in &profile.functions {
                let mut fields = String::new();
                write!(
                    &mut fields,
                    "\"name\":\"{}\",\"call_count\":{},\"total_us\":{}",
                    json_escape(function),
                    row.call_count,
                    row.total_time.as_micros()
                )
                .unwrap();
                emit_trace_event(&trace_id, &mut sequence, "function_profile", &fields);
            }
            for (operation, row) in &profile.operations {
                let mut fields = String::new();
                write!(
                    &mut fields,
                    "\"name\":\"{}\",\"count\":{},\"total_us\":{}",
                    json_escape(operation),
                    row.count,
                    row.total_time.as_micros()
                )
                .unwrap();
                emit_trace_event(&trace_id, &mut sequence, "operation_profile", &fields);
                if operation == "host_callback" {
                    emit_trace_event(&trace_id, &mut sequence, "host_callback", &fields);
                }
            }
            for event in &profile.host_callbacks {
                let mut fields = String::new();
                write!(
                    &mut fields,
                    "\"name\":\"{}\",\"phase\":\"{}\",\"span\":{{\"start\":{},\"end\":{}}},\"elapsed_us\":{}",
                    json_escape(&event.name),
                    event.phase.as_str(),
                    event.span.start,
                    event.span.end,
                    event.elapsed.as_micros()
                )
                .unwrap();
                if let Some(status) = &event.status {
                    write!(&mut fields, ",\"status\":\"{}\"", json_escape(status)).unwrap();
                }
                emit_trace_event(&trace_id, &mut sequence, "host_callback_call", &fields);
            }
            emit_trace_event(&trace_id, &mut sequence, "run_finish", "\"status\":\"ok\"");
            0
        }
        Err(err) => {
            let diagnostic = err.with_source(&path, &source);
            emit_trace_event(
                &trace_id,
                &mut sequence,
                "diagnostic",
                &trace_diagnostic_fields(&diagnostic),
            );
            emit_trace_event(
                &trace_id,
                &mut sequence,
                "run_finish",
                "\"status\":\"error\"",
            );
            1
        }
    }
}

fn stable_trace_id(path: &str, source: &str) -> u64 {
    let mut hash = 1469598103934665603u64;
    for byte in path.bytes().chain(source.bytes()) {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

fn emit_trace_event(trace_id: &str, sequence: &mut u64, event: &str, fields: &str) {
    println!(
        "{{\"schema\":\"nox.trace.event.v1\",\"trace_id\":\"{}\",\"seq\":{},\"event\":\"{}\",{fields}}}",
        json_escape(trace_id),
        *sequence,
        json_escape(event)
    );
    *sequence += 1;
}

fn trace_diagnostic_fields(diagnostic: &Diagnostic) -> String {
    let mut fields = String::new();
    write!(
        &mut fields,
        "\"code\":\"{}\",\"message\":\"{}\",\"span\":{{\"start\":{},\"end\":{}}}",
        json_escape(diagnostic.code),
        json_escape(&diagnostic.message),
        diagnostic.span.start,
        diagnostic.span.end
    )
    .expect("writing to String cannot fail");
    if let Some(source) = &diagnostic.source {
        write!(
            &mut fields,
            ",\"source\":{{\"name\":\"{}\",\"line\":{},\"column\":{}}}",
            json_escape(&source.name),
            source.line,
            source.column
        )
        .expect("writing to String cannot fail");
    } else {
        fields.push_str(",\"source\":null");
    }
    write_stack_frames_json(&mut fields, diagnostic);
    fields
}

fn emit_runtime_trace_event(trace_id: &str, sequence: &mut u64, event: &RuntimeTraceEvent) {
    let mut fields = String::new();
    for (index, (key, value)) in event.fields.iter().enumerate() {
        if index > 0 {
            fields.push(',');
        }
        write!(
            &mut fields,
            "\"{}\":{}",
            json_escape(key),
            runtime_trace_value_json(value)
        )
        .expect("writing to String cannot fail");
    }
    emit_trace_event(trace_id, sequence, &event.event, &fields);
}

fn runtime_trace_value_json(value: &RuntimeTraceValue) -> String {
    match value {
        RuntimeTraceValue::String(value) => format!("\"{}\"", json_escape(value)),
        RuntimeTraceValue::Int(value) => value.to_string(),
        RuntimeTraceValue::UInt(value) => value.to_string(),
        RuntimeTraceValue::Bool(value) => value.to_string(),
    }
}

fn run_repl() -> i32 {
    let stdin = io::stdin();
    let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
    let mut history = String::new();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                eprintln!("repl: failed to read input: {err}");
                return 1;
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if matches!(trimmed, ":quit" | ":exit") {
            return 0;
        }
        let source = repl_source(trimmed);
        let mut eval_source = history.clone();
        eval_source.push_str(&source);
        eval_source.push('\n');
        match runtime.eval(&eval_source) {
            Ok(value) => {
                print_run_value(&value);
                history = eval_source;
            }
            Err(err) => print_diagnostic("<repl>", &err.with_source("<repl>", &eval_source)),
        }
    }
    0
}

fn repl_source(input: &str) -> String {
    if input.ends_with(';') || input.ends_with('}') {
        input.to_string()
    } else {
        format!("{input};")
    }
}

fn print_run_value(value: &Value) {
    if !matches!(value, Value::Null) {
        println!("{value}");
    }
}

fn prepare_process_stdin(runtime: &mut Runtime) {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_ok() {
        runtime.set_stdin(input);
    }
}

fn print_process_stderr(runtime: &mut Runtime) {
    let stderr = runtime.take_stderr();
    if !stderr.is_empty() {
        eprint!("{stderr}");
    }
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
            if json {
                let diagnostic = project_discovery_diagnostic(err);
                println!(
                    "{}",
                    diagnostics_json(
                        false,
                        &[("<project>".to_string(), diagnostic)],
                        &[CheckFileReport {
                            path: "<project>".to_string(),
                            ok: false,
                            diagnostic_count: 1,
                        }],
                    )
                );
                return 2;
            }
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

    let lockfile = project_lockfile_report(&manifest);
    if lockfile.status != "not_required" {
        println!("project check: lockfile");
    }
    if !lockfile.ok {
        for diagnostic in &lockfile.diagnostics {
            println!("project check: {diagnostic}");
        }
        return 1;
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
    let lockfile = project_lockfile_report(manifest);
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

    let ok = lockfile.ok && steps.iter().all(|step| step.status == 0);
    println!("{}", project_check_json(manifest, &lockfile, &steps));
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
    let mut filter: Option<String> = None;
    let mut retry: usize = 0;
    let mut export_failures: Option<ExportFailures> = None;
    let mut iter = raw_args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--filter" => {
                let Some(value) = iter.next() else {
                    eprintln!("test: --filter requires a pattern");
                    return 2;
                };
                filter = Some(value);
            }
            other if other.starts_with("--filter=") => {
                filter = Some(other.trim_start_matches("--filter=").to_string());
            }
            "--retry" => {
                let Some(value) = iter.next() else {
                    eprintln!("test: --retry requires an integer");
                    return 2;
                };
                match value.parse::<usize>() {
                    Ok(parsed) if parsed <= 10 => retry = parsed,
                    _ => {
                        eprintln!("test: --retry expects an integer between 0 and 10");
                        return 2;
                    }
                }
            }
            other if other.starts_with("--retry=") => {
                let value = other.trim_start_matches("--retry=");
                match value.parse::<usize>() {
                    Ok(parsed) if parsed <= 10 => retry = parsed,
                    _ => {
                        eprintln!("test: --retry expects an integer between 0 and 10");
                        return 2;
                    }
                }
            }
            "--export-failures" => {
                let Some(value) = iter.next() else {
                    eprintln!("test: --export-failures requires a directory");
                    return 2;
                };
                export_failures = Some(ExportFailures::Flat(PathBuf::from(value)));
            }
            other if other.starts_with("--export-failures=") => {
                export_failures = Some(ExportFailures::Flat(PathBuf::from(
                    other.trim_start_matches("--export-failures="),
                )));
            }
            "--export-failures-classified" => {
                let Some(value) = iter.next() else {
                    eprintln!("test: --export-failures-classified requires a directory");
                    return 2;
                };
                export_failures = Some(ExportFailures::Classified(PathBuf::from(value)));
            }
            other if other.starts_with("--export-failures-classified=") => {
                export_failures = Some(ExportFailures::Classified(PathBuf::from(
                    other.trim_start_matches("--export-failures-classified="),
                )));
            }
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
        let test_kind = classify_test_kind(&path);
        let max_attempts = retry + 1;
        let mut final_results: Vec<TestAttemptResult> = Vec::new();
        let mut module_failure: Option<Diagnostic> = None;
        for attempt in 1..=max_attempts {
            let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
            match runtime.run_test_file(&path) {
                Ok(result) => {
                    let mut all_pass = true;
                    let mut attempt_results: Vec<TestAttemptResult> = Vec::new();
                    for test in result.tests {
                        if let Some(pattern) = &filter {
                            if !test.name.contains(pattern.as_str()) {
                                continue;
                            }
                        }
                        if !test.passed {
                            all_pass = false;
                        }
                        attempt_results.push(TestAttemptResult {
                            name: test.name,
                            ok: test.passed,
                            diagnostic: test.diagnostic,
                            attempts: attempt,
                            duration_us: test.duration_us,
                            stdout: test.stdout,
                            stderr: test.stderr,
                            mock_events: test.mock_events,
                        });
                    }
                    if all_pass || attempt == max_attempts {
                        final_results.extend(attempt_results);
                        break;
                    }
                }
                Err(diagnostic) => {
                    if attempt == max_attempts {
                        module_failure = Some(diagnostic);
                    }
                }
            }
        }

        if let Some(diagnostic) = module_failure {
            failed = true;
            if !json {
                println!("{}::<module> FAIL", path.display());
                print_diagnostic(&path_string, &diagnostic);
            }
            reports.push(TestReport {
                path: path_string.clone(),
                name: "<module>".to_string(),
                kind: test_kind,
                ok: false,
                diagnostic: Some(diagnostic),
                attempts: max_attempts,
                retried: max_attempts > 1,
                duration_us: 0,
                stdout: String::new(),
                stderr: String::new(),
                mock_events: Vec::new(),
            });
            continue;
        }

        for result in final_results {
            if !result.ok {
                failed = true;
            }
            if !json {
                let retry_tag = if result.attempts > 1 {
                    format!(" (retried {} times)", result.attempts - 1)
                } else {
                    String::new()
                };
                println!(
                    "{}::{} {}{}",
                    path.display(),
                    result.name,
                    if result.ok { "PASS" } else { "FAIL" },
                    retry_tag
                );
                if let Some(diagnostic) = &result.diagnostic {
                    print_diagnostic(&path_string, diagnostic);
                }
            }
            reports.push(TestReport {
                path: path_string.clone(),
                name: result.name,
                kind: test_kind,
                ok: result.ok,
                diagnostic: result.diagnostic,
                attempts: result.attempts,
                retried: result.attempts > 1,
                duration_us: result.duration_us,
                stdout: result.stdout,
                stderr: result.stderr,
                mock_events: result.mock_events,
            });
        }
    }

    if let Some(export) = &export_failures {
        if let Err(err) = export_test_failures(&reports, export) {
            eprintln!("test: failed to export failures: {err}");
            return 2;
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

enum ExportFailures {
    Flat(PathBuf),
    Classified(PathBuf),
}

fn export_test_failures(reports: &[TestReport], export: &ExportFailures) -> io::Result<usize> {
    match export {
        ExportFailures::Flat(dir) => export_property_failures(reports, dir),
        ExportFailures::Classified(dir) => export_classified_failures(reports, dir),
    }
}

fn export_property_failures(reports: &[TestReport], dir: &Path) -> io::Result<usize> {
    let mut exported = 0usize;
    for report in reports {
        let Some(diagnostic) = exportable_failure(report) else {
            continue;
        };
        if !is_property_failure(diagnostic) {
            continue;
        }
        exported += 1;
        write_failure_corpus(report, diagnostic, dir, "property", exported)?;
    }
    Ok(exported)
}

fn export_classified_failures(reports: &[TestReport], dir: &Path) -> io::Result<usize> {
    let mut exported = 0usize;
    for report in reports {
        let Some(diagnostic) = exportable_failure(report) else {
            continue;
        };
        let Some(classification) = classify_export_failure(report, diagnostic) else {
            continue;
        };
        exported += 1;
        let target_dir = dir.join(classification);
        write_failure_corpus(report, diagnostic, &target_dir, classification, exported)?;
    }
    Ok(exported)
}

fn exportable_failure(report: &TestReport) -> Option<&Diagnostic> {
    if report.ok {
        return None;
    }
    report.diagnostic.as_ref()
}

fn write_failure_corpus(
    report: &TestReport,
    diagnostic: &Diagnostic,
    dir: &Path,
    classification: &str,
    index: usize,
) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let source = fs::read_to_string(&report.path)?;
    let stem = Path::new(&report.path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("property");
    let file_name = format!(
        "{}-{}-{}.nox",
        sanitize_corpus_name(stem),
        sanitize_corpus_name(&report.name),
        index
    );
    let target = dir.join(file_name);
    let mut contents = String::new();
    writeln!(contents, "// Exported by nox test --export-failures.")
        .expect("writing to String cannot fail");
    writeln!(contents, "// classification: {classification}")
        .expect("writing to String cannot fail");
    writeln!(contents, "// source: {}", report.path).expect("writing to String cannot fail");
    writeln!(contents, "// test: {}", report.name).expect("writing to String cannot fail");
    writeln!(
        contents,
        "// diagnostic: {}",
        diagnostic.message.replace('\n', "\\n")
    )
    .expect("writing to String cannot fail");
    contents.push_str(&source);
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    fs::write(target, contents)
}

fn is_property_failure(diagnostic: &Diagnostic) -> bool {
    diagnostic.code == "test.assertion-failed"
        && diagnostic.message.contains("property failed seed=")
        && diagnostic.message.contains(" replay=\"")
}

fn classify_export_failure(report: &TestReport, diagnostic: &Diagnostic) -> Option<&'static str> {
    if is_property_failure(diagnostic) {
        return Some("property");
    }
    if report.name == "<module>" && diagnostic.code == "error" {
        return Some("parser");
    }
    if diagnostic.code.starts_with("parse.") || diagnostic.code.starts_with("parser.") {
        return Some("parser");
    }
    if diagnostic.code.starts_with("type.")
        || diagnostic.code.starts_with("typecheck.")
        || diagnostic.code == "test.signature"
    {
        return Some("typecheck");
    }
    if diagnostic.code.starts_with("verify.") || diagnostic.code.starts_with("verifier.") {
        return Some("verifier");
    }
    if diagnostic.code.starts_with("runtime.") {
        return Some("runtime");
    }
    None
}

fn sanitize_corpus_name(value: &str) -> String {
    let mut sanitized = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            sanitized.push(ch);
        } else {
            sanitized.push('-');
        }
    }
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "case".to_string()
    } else {
        trimmed.to_string()
    }
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

fn classify_test_kind(path: &Path) -> &'static str {
    if path
        .components()
        .any(|component| component.as_os_str() == std::ffi::OsStr::new("fixtures"))
    {
        "fixture"
    } else if path
        .components()
        .any(|component| component.as_os_str() == std::ffi::OsStr::new("tests"))
    {
        "integration"
    } else {
        "unit"
    }
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

fn project_discovery_diagnostic(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(message, nox_core::Span { start: 0, end: 0 }).with_code("project.discovery")
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
    for frame in &diagnostic.stack_frames {
        let kind = frame.kind.as_str();
        if let Some(source) = &frame.source {
            eprintln!(
                "  at {} [{}] ({}:{}:{})",
                frame.name, kind, source.name, source.line, source.column
            );
        } else {
            eprintln!(
                "  at {} [{}] ({}..{})",
                frame.name, kind, frame.span.start, frame.span.end
            );
        }
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
        write_stack_frames_json(&mut json, diagnostic);
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
            "{{\"file\":\"{}\",\"name\":\"{}\",\"ok\":{},\"attempts\":{},\"retried\":{},\"duration_us\":{}",
            json_escape(&report.path),
            json_escape(&report.name),
            report.ok,
            report.attempts,
            report.retried,
            report.duration_us
        )
        .expect("writing to String cannot fail");
        write!(
            &mut json,
            ",\"stdout\":\"{}\",\"stderr\":\"{}\",\"kind\":\"{}\",\"mock_events\":[",
            json_escape(&report.stdout),
            json_escape(&report.stderr),
            json_escape(report.kind)
        )
        .expect("writing to String cannot fail");
        for (event_index, event) in report.mock_events.iter().enumerate() {
            if event_index > 0 {
                json.push(',');
            }
            write!(json, "\"{}\"", json_escape(event)).expect("writing to String cannot fail");
        }
        json.push(']');
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
                write_stack_frames_json(&mut json, diagnostic);
                json.push('}');
                write_snapshot_diff_json(&mut json, diagnostic);
            }
            None => json.push_str(",\"diagnostic\":null,\"snapshot_diff\":null"),
        }
        json.push('}');
    }
    let total = reports.len();
    let failed = reports.iter().filter(|report| !report.ok).count();
    let passed = total - failed;
    json.push_str("],\"suites\":[");
    write_test_suites_json(&mut json, reports);
    write!(
        &mut json,
        "],\"summary\":{{\"tests\":{total},\"passed\":{passed},\"failed\":{failed}}}}}"
    )
    .expect("writing to String cannot fail");
    json
}

fn write_test_suites_json(json: &mut String, reports: &[TestReport]) {
    let mut suites: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for report in reports {
        suites
            .entry(report.path.as_str())
            .or_default()
            .push(report.name.as_str());
    }
    for (suite_index, (file, cases)) in suites.iter().enumerate() {
        if suite_index > 0 {
            json.push(',');
        }
        write!(json, "{{\"file\":\"{}\",\"cases\":[", json_escape(file))
            .expect("writing to String cannot fail");
        for (case_index, case) in cases.iter().enumerate() {
            if case_index > 0 {
                json.push(',');
            }
            write!(json, "\"{}\"", json_escape(case)).expect("writing to String cannot fail");
        }
        json.push_str("]}");
    }
}

fn write_stack_frames_json(json: &mut String, diagnostic: &Diagnostic) {
    if diagnostic.stack_frames.is_empty() {
        return;
    }
    json.push_str(",\"stack_frames\":[");
    for (index, frame) in diagnostic.stack_frames.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        write!(
            json,
            "{{\"name\":\"{}\",\"kind\":\"{}\",\"span\":{{\"start\":{},\"end\":{}}}",
            json_escape(&frame.name),
            frame.kind.as_str(),
            frame.span.start,
            frame.span.end
        )
        .expect("writing to String cannot fail");
        if let Some(source) = &frame.source {
            write!(
                json,
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
    json.push(']');
}

fn project_lockfile_report(manifest: &Manifest) -> ProjectLockfileReport {
    let validation: LockfileValidation = validate_lockfile_for_manifest(manifest);
    ProjectLockfileReport {
        path: validation.path.display().to_string(),
        ok: validation.ok,
        status: validation.status,
        diagnostics: validation.diagnostics,
    }
}

fn project_check_json(
    manifest: &Manifest,
    lockfile: &ProjectLockfileReport,
    steps: &[ProjectStepReport],
) -> String {
    let mut json = String::new();
    let ok = lockfile.ok && steps.iter().all(|step| step.status == 0);
    let graph = project_module_graph(manifest);
    write!(
        &mut json,
        "{{\"schema\":\"nox.project-check.v1\",\"ok\":{ok},\"manifest\":{{\"root\":\"{}\",\"package\":{{\"name\":\"{}\",\"version\":\"{}\"}}}},",
        json_escape(&manifest.root.display().to_string()),
        json_escape(&manifest.package.name),
        json_escape(&manifest.package.version)
    )
    .expect("writing to String cannot fail");
    write_project_schema_validation_json(&mut json);
    json.push(',');
    write_project_entrypoints_json(&mut json, manifest);
    json.push(',');
    write_project_capabilities_json(&mut json, manifest);
    json.push(',');
    write_project_dependencies_json(&mut json, manifest, lockfile);
    json.push(',');
    write_project_module_graph_json(&mut json, &graph);
    json.push_str(",\"steps\":[");
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

fn write_snapshot_diff_json(json: &mut String, diagnostic: &Diagnostic) {
    if let Some((label, actual, expected)) = snapshot_diff_from_diagnostic(diagnostic) {
        write!(
            json,
            ",\"snapshot_diff\":{{\"label\":\"{}\",\"actual\":\"{}\",\"expected\":\"{}\"}}",
            json_escape(&label),
            json_escape(&actual),
            json_escape(&expected)
        )
        .expect("writing to String cannot fail");
    } else {
        json.push_str(",\"snapshot_diff\":null");
    }
}

fn snapshot_diff_from_diagnostic(diagnostic: &Diagnostic) -> Option<(String, String, String)> {
    if diagnostic.code != "test.assertion-failed" {
        return None;
    }
    let (prefix, rest) = diagnostic.message.split_once("] snapshot mismatch:\n")?;
    let label = prefix.rsplit_once('[')?.1.to_string();
    let mut actual = None;
    let mut expected = None;
    for line in rest.lines() {
        if let Some(value) = line.strip_prefix("  actual:   ") {
            actual = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("  expected: ") {
            expected = Some(value.to_string());
        }
    }
    Some((label, actual?, expected?))
}

fn write_project_schema_validation_json(json: &mut String) {
    json.push_str("\"schema_validation\":{\"ok\":true,\"manifest_sections\":[");
    for (index, section) in [
        "package",
        "entrypoints",
        "modules",
        "dependencies",
        "runtime",
    ]
    .iter()
    .enumerate()
    {
        if index > 0 {
            json.push(',');
        }
        write!(json, "\"{section}\"").expect("writing to String cannot fail");
    }
    json.push_str("],\"unknown_sections\":\"rejected\",\"unknown_keys\":\"rejected\"}");
}

fn write_project_dependencies_json(
    json: &mut String,
    manifest: &Manifest,
    lockfile: &ProjectLockfileReport,
) {
    json.push_str("\"dependencies\":{\"declared\":[");
    for (index, dependency) in manifest.dependencies.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        write!(
            json,
            "{{\"name\":\"{}\",\"source\":{{\"kind\":\"{}\",\"value\":\"{}\"}},\"pin\":{{\"kind\":\"{}\",\"value\":\"{}\"}}}}",
            json_escape(&dependency.name),
            dependency.source.kind(),
            json_escape(dependency.source.value()),
            dependency.pin.kind(),
            json_escape(dependency.pin.value())
        )
        .expect("writing to String cannot fail");
    }
    write!(
        json,
        "],\"lockfile\":{{\"path\":\"{}\",\"ok\":{},\"status\":\"{}\",\"diagnostics\":[",
        json_escape(&lockfile.path),
        lockfile.ok,
        lockfile.status
    )
    .expect("writing to String cannot fail");
    for (index, diagnostic) in lockfile.diagnostics.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        write!(
            json,
            "{{\"code\":\"{}\",\"message\":\"{}\"}}",
            json_escape(diagnostic.code),
            json_escape(&diagnostic.message)
        )
        .expect("writing to String cannot fail");
    }
    json.push_str("]}}");
}

fn project_module_graph(manifest: &Manifest) -> ProjectModuleGraph {
    let mut roots = Vec::new();
    let mut files = Vec::new();
    for root in manifest.source_dirs() {
        roots.push(root.display().to_string());
        let mut root_files = Vec::new();
        let _ = collect_nox_files(&root, &mut root_files);
        files.extend(
            root_files
                .into_iter()
                .map(|path| path.display().to_string()),
        );
    }
    files.sort();
    files.dedup();
    ProjectModuleGraph { roots, files }
}

fn write_project_entrypoints_json(json: &mut String, manifest: &Manifest) {
    json.push_str("\"entrypoints\":{");
    match &manifest.entrypoints.main {
        Some(main) => write!(
            json,
            "\"main\":\"{}\"",
            json_escape(&manifest.root.join(main).display().to_string())
        )
        .expect("writing to String cannot fail"),
        None => json.push_str("\"main\":null"),
    }
    json.push_str(",\"named\":[");
    for (index, (name, path)) in manifest.entrypoints.named.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        write!(
            json,
            "{{\"name\":\"{}\",\"path\":\"{}\"}}",
            json_escape(name),
            json_escape(&manifest.root.join(path).display().to_string())
        )
        .expect("writing to String cannot fail");
    }
    json.push_str("]}");
}

fn write_project_capabilities_json(json: &mut String, manifest: &Manifest) {
    json.push_str("\"capabilities\":{");
    json.push_str("\"declared\":[");
    for (index, permission) in manifest.runtime.permissions.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        write!(json, "\"{}\"", manifest_permission_name(*permission))
            .expect("writing to String cannot fail");
    }
    json.push_str("]}");
}

fn manifest_permission_name(permission: nox::manifest::RuntimePermissionDecl) -> &'static str {
    match permission {
        nox::manifest::RuntimePermissionDecl::FilesystemRead => "filesystem.read",
        nox::manifest::RuntimePermissionDecl::FilesystemWrite => "filesystem.write",
        nox::manifest::RuntimePermissionDecl::Network => "network",
        nox::manifest::RuntimePermissionDecl::Timers => "timers",
        nox::manifest::RuntimePermissionDecl::Environment => "environment",
        nox::manifest::RuntimePermissionDecl::AsyncTasks => "async_tasks",
        nox::manifest::RuntimePermissionDecl::ProcessRun => "process_run",
    }
}

fn write_project_module_graph_json(json: &mut String, graph: &ProjectModuleGraph) {
    json.push_str("\"module_graph\":{\"roots\":[");
    for (index, root) in graph.roots.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        write!(json, "\"{}\"", json_escape(root)).expect("writing to String cannot fail");
    }
    json.push_str("],\"files\":[");
    for (index, file) in graph.files.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        write!(json, "\"{}\"", json_escape(file)).expect("writing to String cannot fail");
    }
    json.push_str("]}");
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
