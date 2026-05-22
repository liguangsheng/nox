use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    env, fs,
    net::TcpStream,
    path::{Path, PathBuf},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use nox_core::{Diagnostic, Engine, HostFunctionBuilder, Span, TestModuleResult, Type, Value};

pub mod lsp;
pub mod manifest;

use manifest::Manifest;

#[derive(Default)]
pub struct Runtime {
    engine: Engine,
    permissions: RuntimePermissions,
    args: Rc<RefCell<Vec<String>>>,
    task_runtime: Rc<RefCell<TaskRuntime>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimePermissions {
    pub filesystem: bool,
    pub filesystem_write: bool,
    pub filesystem_read_roots: Vec<PathBuf>,
    pub filesystem_write_roots: Vec<PathBuf>,
    pub network: bool,
    pub timers: bool,
    pub environment: bool,
    pub async_tasks: bool,
}

impl RuntimePermissions {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn cli() -> Self {
        Self {
            filesystem: true,
            filesystem_write: false,
            filesystem_read_roots: Vec::new(),
            filesystem_write_roots: Vec::new(),
            network: false,
            timers: false,
            environment: false,
            async_tasks: false,
        }
    }

    pub fn allow_filesystem_read_under(mut self, root: impl Into<PathBuf>) -> Self {
        self.filesystem = true;
        self.filesystem_read_roots.push(root.into());
        self
    }

    pub fn allow_filesystem_write_under(mut self, root: impl Into<PathBuf>) -> Self {
        self.filesystem_write = true;
        self.filesystem_write_roots.push(root.into());
        self
    }
}

pub(crate) fn std_module_source(specifier: &str) -> Result<Option<&'static str>, Diagnostic> {
    let source = match specifier {
        "std/fs.nox" => {
            r#"export fn read_text(path: str) -> str {
    return __nox_std_fs_read_text(path);
}

export fn try_read_text(path: str) -> result[str, str] {
    return __nox_std_fs_try_read_text(path);
}

export fn exists(path: str) -> bool {
    return __nox_std_fs_exists(path);
}

export fn write_text(path: str, contents: str) -> null {
    return __nox_std_fs_write_text(path, contents);
}
"#
        }
        "std/env.nox" => {
            r#"export fn get(name: str) -> str {
    return __nox_std_env_get(name);
}

export fn try_get(name: str) -> option[str] {
    return __nox_std_env_try_get(name);
}

export fn list() -> map[str, str] {
    return __nox_std_env_list();
}
"#
        }
        "std/time.nox" => {
            r#"export fn sleep_ms(ms: int) -> null {
    return __nox_std_time_sleep_ms(ms);
}
"#
        }
        _ if specifier.starts_with("std/") => {
            return Err(Diagnostic::new(
                format!("standard module '{specifier}' is not provided by this runtime"),
                Span { start: 0, end: 0 },
            )
            .with_code("module.not-found"));
        }
        _ => return Ok(None),
    };
    Ok(Some(source))
}

pub(crate) fn install_lsp_stdlib(session: &mut nox_core::Session) {
    let engine = session.engine_mut();
    engine
        .register_host_function(
            HostFunctionBuilder::new("args", Type::Array(Box::new(Type::Str))),
            |_| Ok(Value::array(Type::Str, Vec::new())),
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("sqrt", Type::Float).param("value", Type::Float),
            |args| match args {
                [Value::Float(value)] => Ok(Value::Float(value.sqrt())),
                _ => unreachable!("static checker guarantees sqrt argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("read_text", Type::Str).param("path", Type::Str),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_fs_read_text", Type::Str).param("path", Type::Str),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_fs_try_read_text",
                Type::Result {
                    ok: Box::new(Type::Str),
                    err: Box::new(Type::Str),
                },
            )
            .param("path", Type::Str),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("exists", Type::Bool).param("path", Type::Str),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_fs_exists", Type::Bool).param("path", Type::Str),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("write_text", Type::Null)
                .param("path", Type::Str)
                .param("text", Type::Str),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_fs_write_text", Type::Null)
                .param("path", Type::Str)
                .param("contents", Type::Str),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("env_get", Type::Str).param("name", Type::Str),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_env_get", Type::Str).param("name", Type::Str),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_env_try_get", Type::Option(Box::new(Type::Str)))
                .param("name", Type::Str),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("env_list", Type::Map(Box::new(Type::Str))),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_env_list", Type::Map(Box::new(Type::Str))),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("sleep_ms", Type::Null).param("ms", Type::Int),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_sleep_ms", Type::Null).param("ms", Type::Int),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("tcp_connect", Type::Bool)
                .param("host", Type::Str)
                .param("port", Type::Int),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("task_sleep_ms", Type::Int).param("ms", Type::Int),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("task_ready", Type::Bool).param("id", Type::Int),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("task_cancel", Type::Null).param("id", Type::Int),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
}

#[derive(Default)]
struct TaskRuntime {
    next_id: u64,
    sleep_tasks: HashMap<u64, Instant>,
}

impl TaskRuntime {
    fn spawn_sleep(&mut self, duration: Duration) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.sleep_tasks.insert(id, Instant::now() + duration);
        id
    }

    fn poll(&mut self, id: u64) -> Result<bool, &'static str> {
        let Some(deadline) = self.sleep_tasks.get(&id).copied() else {
            return Err("unknown async task id");
        };
        if Instant::now() < deadline {
            return Ok(false);
        }
        self.sleep_tasks.remove(&id);
        Ok(true)
    }

    fn cancel(&mut self, id: u64) -> Result<(), &'static str> {
        if self.sleep_tasks.remove(&id).is_none() {
            return Err("unknown async task id");
        }
        Ok(())
    }

    fn pending_count(&self) -> usize {
        self.sleep_tasks.len()
    }

    fn next_id(&self) -> u64 {
        self.next_id
    }

    fn remove_tasks_created_since(&mut self, first_id: u64) {
        self.sleep_tasks.retain(|id, _| *id < first_id);
    }
}

impl Runtime {
    pub fn new() -> Self {
        let mut runtime = Self {
            engine: Engine::new(),
            permissions: RuntimePermissions::none(),
            args: Rc::new(RefCell::new(Vec::new())),
            task_runtime: Rc::new(RefCell::new(TaskRuntime::default())),
        };
        runtime.install_stdlib();
        runtime
    }

    pub fn with_permissions(permissions: RuntimePermissions) -> Self {
        let mut runtime = Self {
            engine: Engine::new(),
            permissions,
            args: Rc::new(RefCell::new(Vec::new())),
            task_runtime: Rc::new(RefCell::new(TaskRuntime::default())),
        };
        runtime.install_stdlib();
        runtime
    }

    pub fn set_args(&mut self, args: Vec<String>) {
        *self.args.borrow_mut() = args;
    }

    pub fn pending_async_task_count(&self) -> usize {
        self.task_runtime.borrow().pending_count()
    }

    pub fn set_instruction_budget(&mut self, budget: Option<usize>) {
        self.engine.set_instruction_budget(budget);
    }

    pub fn eval(&mut self, source: &str) -> Result<Value, Diagnostic> {
        let first_task_id = self.task_runtime.borrow().next_id();
        let result = self.engine.eval(source);
        if result.is_err() {
            self.task_runtime
                .borrow_mut()
                .remove_tasks_created_since(first_task_id);
        }
        result
    }

    pub fn eval_file(&mut self, path: impl AsRef<Path>) -> Result<Value, Diagnostic> {
        let path = path.as_ref();
        let source = self.prepare_file_source(path)?;
        self.eval(&source)
            .map_err(|err| err.with_source(path.display().to_string(), &source))
    }

    pub fn check_file(&mut self, path: impl AsRef<Path>) -> Result<(), Diagnostic> {
        let path = path.as_ref();
        let source = self.prepare_file_source(path)?;
        self.engine
            .check(&source)
            .map_err(|err| err.with_source(path.display().to_string(), &source))
    }

    pub fn check_file_diagnostics(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<(), Vec<Diagnostic>> {
        let path = path.as_ref();
        let source = self.prepare_file_source(path).map_err(|err| vec![err])?;
        self.engine
            .check_diagnostics(&source)
            .map_err(|diagnostics| {
                diagnostics
                    .into_iter()
                    .map(|err| err.with_source(path.display().to_string(), &source))
                    .collect()
            })
    }

    pub fn run_test_file(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<TestModuleResult, Diagnostic> {
        let path = path.as_ref();
        let source = self.prepare_file_source(path)?;
        let first_task_id = self.task_runtime.borrow().next_id();
        let result = self.engine.run_tests(&source).map(|mut result| {
            for test in &mut result.tests {
                if let Some(diagnostic) = test.diagnostic.take() {
                    test.diagnostic =
                        Some(diagnostic.with_source(path.display().to_string(), &source));
                }
            }
            result
        });
        if result.is_err() {
            self.task_runtime
                .borrow_mut()
                .remove_tasks_created_since(first_task_id);
        }
        result.map_err(|err| err.with_source(path.display().to_string(), &source))
    }

    pub fn check_source_diagnostics(&mut self, source: &str) -> Result<(), Vec<Diagnostic>> {
        self.engine.check_diagnostics(source)
    }

    pub fn discover_manifest(path: impl AsRef<Path>) -> Result<Option<Manifest>, Diagnostic> {
        Manifest::discover(path.as_ref())
    }

    pub fn check_source_diagnostics_with_base(
        &mut self,
        source: &str,
        base: impl AsRef<Path>,
    ) -> Result<(), Vec<Diagnostic>> {
        let base = base.as_ref().to_path_buf();
        let search_paths = manifest_search_paths(&base);
        self.set_import_base(base, search_paths);
        self.engine.check_diagnostics(source)
    }

    pub fn check_source_diagnostics_with_overlay(
        &mut self,
        source: &str,
        base: impl AsRef<Path>,
        overlay: HashMap<PathBuf, String>,
    ) -> Result<(), Vec<Diagnostic>> {
        let base = base.as_ref().to_path_buf();
        let search_paths = manifest_search_paths(&base);
        self.set_import_base_with_overlay(base, search_paths, overlay);
        self.engine.check_diagnostics(source)
    }

    pub fn inspect_bytecode_file(&mut self, path: impl AsRef<Path>) -> Result<String, Diagnostic> {
        let path = path.as_ref();
        let source = self.prepare_file_source(path)?;
        self.engine
            .inspect_bytecode(&source)
            .map_err(|err| err.with_source(path.display().to_string(), &source))
    }

    pub fn inspect_bytecode_file_compact(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<String, Diagnostic> {
        let path = path.as_ref();
        let source = self.prepare_file_source(path)?;
        self.engine
            .inspect_bytecode_compact(&source)
            .map_err(|err| err.with_source(path.display().to_string(), &source))
    }

    pub fn format_file(&mut self, path: impl AsRef<Path>) -> Result<String, Diagnostic> {
        let path = path.as_ref();
        let source = self.prepare_file_source(path)?;
        self.engine
            .format_source(&source)
            .map_err(|err| err.with_source(path.display().to_string(), &source))
    }

    pub fn format_source(&self, source: &str) -> Result<String, Diagnostic> {
        self.engine.format_source(source)
    }

    pub fn hover_type_source(
        &mut self,
        source: &str,
        byte_offset: usize,
    ) -> Result<Option<Type>, Diagnostic> {
        self.engine.hover_type(source, byte_offset)
    }

    pub fn hover_type_source_with_base(
        &mut self,
        source: &str,
        byte_offset: usize,
        base: impl AsRef<Path>,
    ) -> Result<Option<Type>, Diagnostic> {
        let base = base.as_ref().to_path_buf();
        let search_paths = manifest_search_paths(&base);
        self.set_import_base(base, search_paths);
        self.engine.hover_type(source, byte_offset)
    }

    pub fn hover_type_source_with_overlay(
        &mut self,
        source: &str,
        byte_offset: usize,
        base: impl AsRef<Path>,
        overlay: HashMap<PathBuf, String>,
    ) -> Result<Option<Type>, Diagnostic> {
        let base = base.as_ref().to_path_buf();
        let search_paths = manifest_search_paths(&base);
        self.set_import_base_with_overlay(base, search_paths, overlay);
        self.engine.hover_type(source, byte_offset)
    }

    fn prepare_file_source(&mut self, path: &Path) -> Result<String, Diagnostic> {
        if !self.permissions.filesystem {
            return Err(capability_required("filesystem", "evaluate files"));
        }

        let base = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let manifest = Manifest::discover(path)?;
        let search_paths = manifest
            .as_ref()
            .map(Manifest::source_dirs)
            .unwrap_or_default();
        self.set_import_base(base, search_paths);

        let source = fs::read_to_string(path).map_err(|err| {
            Diagnostic::new(
                format!("failed to read '{}': {err}", path.display()),
                Span { start: 0, end: 0 },
            )
        })?;
        Ok(source)
    }

    fn set_import_base(&mut self, base: PathBuf, search_paths: Vec<PathBuf>) {
        self.engine.set_module_loader(move |specifier| {
            if let Some(source) = std_module_source(specifier)? {
                return Ok(source.to_string());
            }
            let primary = base.join(specifier);
            if primary.is_file() {
                return read_module(&primary);
            }
            for search in &search_paths {
                let candidate = search.join(specifier);
                if candidate.is_file() {
                    return read_module(&candidate);
                }
            }
            read_module(&primary)
        });
    }

    fn set_import_base_with_overlay(
        &mut self,
        base: PathBuf,
        search_paths: Vec<PathBuf>,
        overlay: HashMap<PathBuf, String>,
    ) {
        self.engine.set_module_loader(move |specifier| {
            if let Some(source) = std_module_source(specifier)? {
                return Ok(source.to_string());
            }
            let primary = base.join(specifier);
            if let Some(source) = overlay.get(&primary) {
                return Ok(source.clone());
            }
            if primary.is_file() {
                return read_module(&primary);
            }
            for search in &search_paths {
                let candidate = search.join(specifier);
                if let Some(source) = overlay.get(&candidate) {
                    return Ok(source.clone());
                }
                if candidate.is_file() {
                    return read_module(&candidate);
                }
            }
            read_module(&primary)
        });
    }
}

fn manifest_search_paths(base: &Path) -> Vec<PathBuf> {
    let probe = base.join("probe.nox");
    match Manifest::discover(&probe) {
        Ok(Some(manifest)) => manifest.source_dirs(),
        _ => Vec::new(),
    }
}

fn read_module(path: &Path) -> Result<String, Diagnostic> {
    fs::read_to_string(path).map_err(|err| {
        Diagnostic::new(
            format!("failed to load module '{}': {err}", path.display()),
            Span { start: 0, end: 0 },
        )
        .with_code("module.not-found")
    })
}

fn permission_denied(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(message, Span { start: 0, end: 0 }).with_code("permission.denied")
}

fn capability_required(capability: &str, operation: &str) -> Diagnostic {
    permission_denied(format!(
        "{capability} capability is required to {operation}"
    ))
}

fn call_capability_required(capability: &str, function: &str) -> Diagnostic {
    capability_required(capability, &format!("call {function}"))
}

fn check_filesystem_read(path: &Path, roots: &[PathBuf]) -> Result<(), Diagnostic> {
    check_filesystem_access(path, roots, "read")
}

fn check_filesystem_write(path: &Path, roots: &[PathBuf]) -> Result<(), Diagnostic> {
    check_filesystem_access(path, roots, "write")
}

fn check_filesystem_access(
    path: &Path,
    roots: &[PathBuf],
    operation: &str,
) -> Result<(), Diagnostic> {
    if roots.is_empty() {
        return validate_runtime_path(path).map(|_| ());
    }

    let target = normalize_runtime_path(path)?;
    for root in roots {
        let root = normalize_existing_root(root)?;
        if target.starts_with(&root) {
            return Ok(());
        }
    }

    Err(permission_denied(format!(
        "filesystem {operation} permission denied for '{}'",
        path.display()
    )))
}

fn validate_runtime_path(path: &Path) -> Result<(), Diagnostic> {
    if path.as_os_str().is_empty() {
        return Err(invalid_path(path));
    }
    Ok(())
}

fn normalize_existing_root(path: &Path) -> Result<PathBuf, Diagnostic> {
    validate_runtime_path(path)?;
    fs::canonicalize(path).map_err(|err| {
        Diagnostic::new(
            format!(
                "invalid filesystem allowlist root '{}': {err}",
                path.display()
            ),
            Span { start: 0, end: 0 },
        )
    })
}

fn normalize_runtime_path(path: &Path) -> Result<PathBuf, Diagnostic> {
    validate_runtime_path(path)?;
    if let Ok(path) = fs::canonicalize(path) {
        return Ok(path);
    }

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|err| {
                Diagnostic::new(
                    format!("invalid current directory: {err}"),
                    Span { start: 0, end: 0 },
                )
            })?
            .join(path)
    };
    Ok(normalize_missing_path(&absolute))
}

fn normalize_missing_path(path: &Path) -> PathBuf {
    let lexical = normalize_lexical(path);
    let mut probe = lexical.as_path();
    let mut suffix = Vec::new();

    loop {
        if let Ok(mut canonical) = fs::canonicalize(probe) {
            for component in suffix.iter().rev() {
                canonical.push(component);
            }
            return normalize_lexical(&canonical);
        }
        let Some(name) = probe.file_name() else {
            return lexical;
        };
        suffix.push(name.to_os_string());
        let Some(parent) = probe.parent() else {
            return lexical;
        };
        probe = parent;
    }
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn invalid_path(path: &Path) -> Diagnostic {
    Diagnostic::new(
        format!("invalid filesystem path '{}'", path.display()),
        Span { start: 0, end: 0 },
    )
}

fn read_optional_env(name: &str) -> Result<Value, Diagnostic> {
    match env::var(name) {
        Ok(value) => Ok(Value::some(Type::Str, Value::string(value))),
        Err(env::VarError::NotPresent) => Ok(Value::none(Type::Str)),
        Err(err) => Err(Diagnostic::new(
            format!("failed to read environment variable '{name}': {err}"),
            Span { start: 0, end: 0 },
        )),
    }
}

fn read_environment_map() -> Result<BTreeMap<String, Value>, Diagnostic> {
    let mut entries = BTreeMap::new();
    for (key, value) in env::vars_os() {
        let key = key.into_string().map_err(|_| {
            Diagnostic::new(
                "failed to read environment variable name: not valid unicode",
                Span { start: 0, end: 0 },
            )
        })?;
        let value = value.into_string().map_err(|_| {
            Diagnostic::new(
                format!("failed to read environment variable '{key}': not valid unicode"),
                Span { start: 0, end: 0 },
            )
        })?;
        entries.insert(key, Value::string(value));
    }
    Ok(entries)
}

fn read_text_result(path: &str, roots: &[PathBuf]) -> Result<Value, Diagnostic> {
    let path_ref = Path::new(path);
    check_filesystem_read(path_ref, roots)?;
    Ok(match fs::read_to_string(path_ref) {
        Ok(contents) => Value::ok(Type::Str, Type::Str, Value::string(contents)),
        Err(err) => Value::err(
            Type::Str,
            Type::Str,
            Value::string(format!("failed to read '{path}': {err}")),
        ),
    })
}

fn install_std_module_aliases(engine: &mut Engine, permissions: &RuntimePermissions) {
    let filesystem_read_allowed = permissions.filesystem;
    let filesystem_read_roots = permissions.filesystem_read_roots.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_fs_read_text", Type::Str).param("path", Type::Str),
            move |args| {
                if !filesystem_read_allowed {
                    return Err(call_capability_required("filesystem", "read_text"));
                }
                match args {
                    [Value::String(path)] => {
                        let path_ref = Path::new(path.as_ref());
                        check_filesystem_read(path_ref, &filesystem_read_roots)?;
                        fs::read_to_string(path_ref)
                            .map(Value::string)
                            .map_err(|err| {
                                Diagnostic::new(
                                    format!("failed to read '{path}': {err}"),
                                    Span { start: 0, end: 0 },
                                )
                            })
                    }
                    _ => unreachable!("static checker guarantees read_text argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let filesystem_read_allowed = permissions.filesystem;
    let filesystem_read_roots = permissions.filesystem_read_roots.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_fs_try_read_text",
                Type::Result {
                    ok: Box::new(Type::Str),
                    err: Box::new(Type::Str),
                },
            )
            .param("path", Type::Str),
            move |args| {
                if !filesystem_read_allowed {
                    return Err(call_capability_required("filesystem", "try_read_text"));
                }
                match args {
                    [Value::String(path)] => {
                        read_text_result(path.as_ref(), &filesystem_read_roots)
                    }
                    _ => unreachable!("static checker guarantees try_read_text argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let filesystem_read_allowed = permissions.filesystem;
    let filesystem_read_roots = permissions.filesystem_read_roots.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_fs_exists", Type::Bool).param("path", Type::Str),
            move |args| {
                if !filesystem_read_allowed {
                    return Err(call_capability_required("filesystem", "exists"));
                }
                match args {
                    [Value::String(path)] => {
                        let path_ref = Path::new(path.as_ref());
                        check_filesystem_read(path_ref, &filesystem_read_roots)?;
                        Ok(Value::Bool(path_ref.exists()))
                    }
                    _ => unreachable!("static checker guarantees exists argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let filesystem_write_allowed = permissions.filesystem_write;
    let filesystem_write_roots = permissions.filesystem_write_roots.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_fs_write_text", Type::Null)
                .param("path", Type::Str)
                .param("contents", Type::Str),
            move |args| {
                if !filesystem_write_allowed {
                    return Err(call_capability_required("filesystem write", "write_text"));
                }
                match args {
                    [Value::String(path), Value::String(contents)] => {
                        let path_ref = Path::new(path.as_ref());
                        check_filesystem_write(path_ref, &filesystem_write_roots)?;
                        fs::write(path_ref, contents.as_ref())
                            .map(|_| Value::Null)
                            .map_err(|err| {
                                Diagnostic::new(
                                    format!("failed to write '{path}': {err}"),
                                    Span { start: 0, end: 0 },
                                )
                            })
                    }
                    _ => unreachable!("static checker guarantees write_text argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let environment_allowed = permissions.environment;
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_env_get", Type::Str).param("name", Type::Str),
            move |args| {
                if !environment_allowed {
                    return Err(call_capability_required("environment", "env_get"));
                }

                match args {
                    [Value::String(name)] => {
                        env::var(name.as_ref()).map(Value::string).map_err(|err| {
                            Diagnostic::new(
                                format!("failed to read environment variable '{name}': {err}"),
                                Span { start: 0, end: 0 },
                            )
                        })
                    }
                    _ => unreachable!("static checker guarantees env_get argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let environment_allowed = permissions.environment;
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_env_try_get", Type::Option(Box::new(Type::Str)))
                .param("name", Type::Str),
            move |args| {
                if !environment_allowed {
                    return Err(call_capability_required("environment", "env_try_get"));
                }

                match args {
                    [Value::String(name)] => read_optional_env(name.as_ref()),
                    _ => unreachable!("static checker guarantees env_try_get argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let environment_allowed = permissions.environment;
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_env_list", Type::Map(Box::new(Type::Str))),
            move |_| {
                if !environment_allowed {
                    return Err(call_capability_required("environment", "env_list"));
                }

                read_environment_map().map(|entries| Value::map(Type::Str, entries))
            },
        )
        .expect("stdlib function registration is static");

    let timers_allowed = permissions.timers;
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_sleep_ms", Type::Null).param("ms", Type::Int),
            move |args| {
                if !timers_allowed {
                    return Err(call_capability_required("timer", "sleep_ms"));
                }

                match args {
                    [Value::Int(ms)] if *ms >= 0 => {
                        thread::sleep(Duration::from_millis(*ms as u64));
                        Ok(Value::Null)
                    }
                    [Value::Int(_)] => Err(Diagnostic::new(
                        "sleep_ms expects a non-negative duration",
                        Span { start: 0, end: 0 },
                    )),
                    _ => unreachable!("static checker guarantees sleep_ms argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");
}

impl Runtime {
    fn install_stdlib(&mut self) {
        let task_runtime = self.task_runtime.clone();

        let args = self.args.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("args", Type::Array(Box::new(Type::Str))),
                move |_| {
                    let values = args.borrow().iter().cloned().map(Value::string).collect();
                    Ok(Value::array(Type::Str, values))
                },
            )
            .expect("stdlib function registration is static");

        self.engine
            .register_host_function(
                HostFunctionBuilder::new("sqrt", Type::Float).param("value", Type::Float),
                |args| match args {
                    [Value::Float(value)] => Ok(Value::Float(value.sqrt())),
                    _ => unreachable!("static checker guarantees sqrt argument type"),
                },
            )
            .expect("stdlib function registration is static");

        let filesystem_read_allowed = self.permissions.filesystem;
        let filesystem_read_roots = self.permissions.filesystem_read_roots.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("read_text", Type::Str).param("path", Type::Str),
                move |args| {
                    if !filesystem_read_allowed {
                        return Err(call_capability_required("filesystem", "read_text"));
                    }
                    match args {
                        [Value::String(path)] => {
                            let path_ref = Path::new(path.as_ref());
                            check_filesystem_read(path_ref, &filesystem_read_roots)?;
                            fs::read_to_string(path_ref)
                                .map(Value::string)
                                .map_err(|err| {
                                    Diagnostic::new(
                                        format!("failed to read '{path}': {err}"),
                                        Span { start: 0, end: 0 },
                                    )
                                })
                        }
                        _ => unreachable!("static checker guarantees read_text argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let filesystem_read_allowed = self.permissions.filesystem;
        let filesystem_read_roots = self.permissions.filesystem_read_roots.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("exists", Type::Bool).param("path", Type::Str),
                move |args| {
                    if !filesystem_read_allowed {
                        return Err(call_capability_required("filesystem", "exists"));
                    }
                    match args {
                        [Value::String(path)] => {
                            let path_ref = Path::new(path.as_ref());
                            check_filesystem_read(path_ref, &filesystem_read_roots)?;
                            Ok(Value::Bool(path_ref.exists()))
                        }
                        _ => unreachable!("static checker guarantees exists argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let filesystem_write_allowed = self.permissions.filesystem_write;
        let filesystem_write_roots = self.permissions.filesystem_write_roots.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("write_text", Type::Null)
                    .param("path", Type::Str)
                    .param("contents", Type::Str),
                move |args| {
                    if !filesystem_write_allowed {
                        return Err(call_capability_required("filesystem write", "write_text"));
                    }
                    match args {
                        [Value::String(path), Value::String(contents)] => {
                            let path_ref = Path::new(path.as_ref());
                            check_filesystem_write(path_ref, &filesystem_write_roots)?;
                            fs::write(path_ref, contents.as_ref())
                                .map(|_| Value::Null)
                                .map_err(|err| {
                                    Diagnostic::new(
                                        format!("failed to write '{path}': {err}"),
                                        Span { start: 0, end: 0 },
                                    )
                                })
                        }
                        _ => unreachable!("static checker guarantees write_text argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let environment_allowed = self.permissions.environment;
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("env_get", Type::Str).param("name", Type::Str),
                move |args| {
                    if !environment_allowed {
                        return Err(call_capability_required("environment", "env_get"));
                    }

                    match args {
                        [Value::String(name)] => {
                            env::var(name.as_ref()).map(Value::string).map_err(|err| {
                                Diagnostic::new(
                                    format!("failed to read environment variable '{name}': {err}"),
                                    Span { start: 0, end: 0 },
                                )
                            })
                        }
                        _ => unreachable!("static checker guarantees env_get argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let environment_allowed = self.permissions.environment;
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("env_list", Type::Map(Box::new(Type::Str))),
                move |_| {
                    if !environment_allowed {
                        return Err(call_capability_required("environment", "env_list"));
                    }

                    read_environment_map().map(|entries| Value::map(Type::Str, entries))
                },
            )
            .expect("stdlib function registration is static");

        let timers_allowed = self.permissions.timers;
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("sleep_ms", Type::Null).param("ms", Type::Int),
                move |args| {
                    if !timers_allowed {
                        return Err(call_capability_required("timer", "sleep_ms"));
                    }

                    match args {
                        [Value::Int(ms)] if *ms >= 0 => {
                            thread::sleep(Duration::from_millis(*ms as u64));
                            Ok(Value::Null)
                        }
                        [Value::Int(_)] => Err(Diagnostic::new(
                            "sleep_ms expects a non-negative duration",
                            Span { start: 0, end: 0 },
                        )),
                        _ => unreachable!("static checker guarantees sleep_ms argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let network_allowed = self.permissions.network;
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("tcp_connect", Type::Bool)
                    .param("host", Type::Str)
                    .param("port", Type::Int),
                move |args| {
                    if !network_allowed {
                        return Err(call_capability_required("network", "tcp_connect"));
                    }

                    match args {
                        [Value::String(host), Value::Int(port)] if (0..=65535).contains(port) => {
                            let port = *port as u16;
                            Ok(Value::Bool(
                                TcpStream::connect((host.as_ref(), port)).is_ok(),
                            ))
                        }
                        [Value::String(_), Value::Int(_)] => Err(Diagnostic::new(
                            "tcp_connect expects an integer port between 0 and 65535",
                            Span { start: 0, end: 0 },
                        )),
                        _ => unreachable!("static checker guarantees tcp_connect argument types"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let async_allowed = self.permissions.async_tasks;
        let task_runtime_for_spawn = task_runtime.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("task_sleep_ms", Type::Int).param("ms", Type::Int),
                move |args| {
                    if !async_allowed {
                        return Err(call_capability_required("async task", "task_sleep_ms"));
                    }

                    match args {
                        [Value::Int(ms)] if *ms >= 0 => {
                            let id = task_runtime_for_spawn
                                .borrow_mut()
                                .spawn_sleep(Duration::from_millis(*ms as u64));
                            Ok(Value::Int(id as i64))
                        }
                        [Value::Int(_)] => Err(Diagnostic::new(
                            "task_sleep_ms expects a non-negative duration",
                            Span { start: 0, end: 0 },
                        )),
                        _ => unreachable!("static checker guarantees task_sleep_ms argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let async_allowed = self.permissions.async_tasks;
        let task_runtime_for_ready = task_runtime.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("task_ready", Type::Bool).param("id", Type::Int),
                move |args| {
                    if !async_allowed {
                        return Err(call_capability_required("async task", "task_ready"));
                    }

                    match args {
                        [Value::Int(id)] if *id >= 0 => {
                            let ready = task_runtime_for_ready
                                .borrow_mut()
                                .poll(*id as u64)
                                .map_err(|msg| Diagnostic::new(msg, Span { start: 0, end: 0 }))?;
                            Ok(Value::Bool(ready))
                        }
                        [Value::Int(_)] => Err(Diagnostic::new(
                            "task_ready expects a non-negative integer task id",
                            Span { start: 0, end: 0 },
                        )),
                        _ => unreachable!("static checker guarantees task_ready argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let async_allowed = self.permissions.async_tasks;
        let task_runtime_for_cancel = task_runtime;
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("task_cancel", Type::Null).param("id", Type::Int),
                move |args| {
                    if !async_allowed {
                        return Err(call_capability_required("async task", "task_cancel"));
                    }

                    match args {
                        [Value::Int(id)] if *id >= 0 => {
                            task_runtime_for_cancel
                                .borrow_mut()
                                .cancel(*id as u64)
                                .map_err(|msg| Diagnostic::new(msg, Span { start: 0, end: 0 }))?;
                            Ok(Value::Null)
                        }
                        [Value::Int(_)] => Err(Diagnostic::new(
                            "task_cancel expects a non-negative integer task id",
                            Span { start: 0, end: 0 },
                        )),
                        _ => unreachable!("static checker guarantees task_cancel argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        install_std_module_aliases(&mut self.engine, &self.permissions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nox_core::Session;
    use std::sync::{Mutex, MutexGuard};

    static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

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
    fn runtime_resolves_std_fs_module() {
        let dir =
            std::env::temp_dir().join(format!("nox-std-fs-{}-{}", std::process::id(), line!()));
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
    fn read_text_reads_existing_file() {
        let dir =
            std::env::temp_dir().join(format!("nox-rt-read-{}-{}", std::process::id(), line!()));
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

        let mut runtime = Runtime::with_permissions(
            RuntimePermissions::none().allow_filesystem_read_under(&allowed),
        );
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

        let mut runtime = Runtime::with_permissions(
            RuntimePermissions::none().allow_filesystem_read_under(&allowed),
        );
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

        let mut runtime = Runtime::with_permissions(
            RuntimePermissions::none().allow_filesystem_read_under(&allowed),
        );
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
}
