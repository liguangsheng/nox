use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap},
    env, fs, io,
    io::Read,
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nox_core::{
    Array, Diagnostic, Engine, HostFunctionBuilder, JsonValue, LintWarning, Map, ProfileReport,
    Span, TestModuleResult, Type, Value,
};

pub mod dap;
pub mod lsp;
pub mod manifest;

#[path = "lib/crypto.rs"]
mod crypto;
#[path = "lib/std_sources.rs"]
mod std_sources;

pub use crypto::sha256_hex_bytes;
pub(crate) use crypto::{
    base64_decode_bytes, base64_encode_bytes, hex_decode_bytes, hex_encode_bytes,
    hmac_sha256_hex_bytes,
};

use manifest::{validate_lockfile_for_manifest, Lockfile, Manifest};

#[derive(Debug, Clone)]
pub(crate) struct ExternalModuleDependency {
    name: String,
    cache_path: PathBuf,
    resolved: String,
    content_hash: String,
}

#[derive(Default)]
pub struct Runtime {
    engine: Engine,
    permissions: RuntimePermissions,
    args: Rc<RefCell<Vec<String>>>,
    stdin: Rc<RefCell<Option<String>>>,
    stdout: Rc<RefCell<String>>,
    stderr: Rc<RefCell<String>>,
    capture_stdout: Rc<RefCell<bool>>,
    exit_code: Rc<RefCell<Option<i64>>>,
    task_runtime: Rc<RefCell<TaskRuntime>>,
    mock_clock: Rc<RefCell<Option<i64>>>,
    mock_env: Rc<RefCell<Option<BTreeMap<String, String>>>>,
    mock_filesystem: Rc<RefCell<Option<MockFilesystem>>>,
    mock_network: Rc<RefCell<Option<MockNetwork>>>,
    process_run_active: Rc<RefCell<usize>>,
    runtime_trace_events: Rc<RefCell<Option<Vec<RuntimeTraceEvent>>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTraceValue {
    String(String),
    Int(i64),
    UInt(u64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTraceEvent {
    pub event: String,
    pub fields: BTreeMap<String, RuntimeTraceValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncTaskPoll {
    Pending,
    Ready,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MockFilesystem {
    files: BTreeMap<PathBuf, Vec<u8>>,
    dirs: BTreeMap<PathBuf, Vec<String>>,
}

impl MockFilesystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text_file(mut self, path: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        self.files.insert(
            normalize_mock_filesystem_path(path.into().as_path()),
            contents.into().into_bytes(),
        );
        self
    }

    pub fn with_binary_file(mut self, path: impl Into<PathBuf>, contents: Vec<u8>) -> Self {
        self.files.insert(
            normalize_mock_filesystem_path(path.into().as_path()),
            contents,
        );
        self
    }

    pub fn with_dir(mut self, path: impl Into<PathBuf>, entries: Vec<String>) -> Self {
        self.dirs.insert(
            normalize_mock_filesystem_path(path.into().as_path()),
            entries,
        );
        self
    }

    fn file(&self, path: &Path) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    fn write_file(&mut self, path: PathBuf, contents: Vec<u8>) {
        self.files.insert(path, contents);
    }

    fn is_file(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.dirs.contains_key(path)
            || self.files.keys().any(|file| {
                file.parent()
                    .is_some_and(|parent| parent == path || parent.starts_with(path))
            })
            || self
                .dirs
                .keys()
                .any(|dir| dir != path && dir.starts_with(path))
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<String>, String> {
        if !self.is_dir(path) {
            return Err("not found".to_string());
        }

        let mut entries = BTreeSet::new();
        if let Some(explicit) = self.dirs.get(path) {
            entries.extend(explicit.iter().cloned());
        }
        for file in self.files.keys() {
            if let Some(name) = mock_immediate_child_name(path, file) {
                entries.insert(name);
            }
        }
        for dir in self.dirs.keys() {
            if let Some(name) = mock_immediate_child_name(path, dir) {
                entries.insert(name);
            }
        }

        Ok(entries.into_iter().collect())
    }
}

fn mock_immediate_child_name(parent: &Path, child: &Path) -> Option<String> {
    let relative = child.strip_prefix(parent).ok()?;
    let mut components = relative.components();
    let first = components.next()?;
    match first {
        std::path::Component::Normal(name) => name.to_str().map(ToOwned::to_owned),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MockNetwork {
    tcp_connect: BTreeMap<(String, u16), bool>,
    http_responses: BTreeMap<(String, String), MockHttpResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockHttpResponse {
    pub status: i64,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl MockNetwork {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tcp_connect(mut self, host: impl Into<String>, port: u16, reachable: bool) -> Self {
        self.tcp_connect.insert((host.into(), port), reachable);
        self
    }

    pub fn with_http_text_response(
        self,
        method: impl Into<String>,
        url: impl Into<String>,
        status: i64,
        body: impl Into<String>,
    ) -> Self {
        self.with_http_binary_response(method, url, status, body.into().into_bytes())
    }

    pub fn with_http_text_response_headers(
        self,
        method: impl Into<String>,
        url: impl Into<String>,
        status: i64,
        headers: BTreeMap<String, String>,
        body: impl Into<String>,
    ) -> Self {
        self.with_http_binary_response_headers(
            method,
            url,
            status,
            headers,
            body.into().into_bytes(),
        )
    }

    pub fn with_http_binary_response(
        mut self,
        method: impl Into<String>,
        url: impl Into<String>,
        status: i64,
        body: Vec<u8>,
    ) -> Self {
        self = self.with_http_binary_response_headers(method, url, status, BTreeMap::new(), body);
        self
    }

    pub fn with_http_binary_response_headers(
        mut self,
        method: impl Into<String>,
        url: impl Into<String>,
        status: i64,
        headers: BTreeMap<String, String>,
        body: Vec<u8>,
    ) -> Self {
        self.http_responses.insert(
            (method.into().to_ascii_uppercase(), url.into()),
            MockHttpResponse {
                status,
                headers: normalize_http_headers(headers),
                body,
            },
        );
        self
    }

    fn tcp_connect(&self, host: &str, port: u16) -> bool {
        self.tcp_connect
            .get(&(host.to_string(), port))
            .copied()
            .unwrap_or(false)
    }

    fn http_response(&self, method: &str, url: &str) -> Option<MockHttpResponse> {
        self.http_responses
            .get(&(method.to_ascii_uppercase(), url.to_string()))
            .cloned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePermissions {
    pub filesystem: bool,
    pub filesystem_write: bool,
    pub filesystem_read_roots: Vec<PathBuf>,
    pub filesystem_write_roots: Vec<PathBuf>,
    pub network: bool,
    pub timers: bool,
    pub environment: bool,
    pub async_tasks: bool,
    pub async_task_max_pending: Option<usize>,
    pub process_run: bool,
    pub process_run_allowlist: Vec<String>,
    pub process_run_max_concurrent: Option<usize>,
}

impl Default for RuntimePermissions {
    fn default() -> Self {
        Self {
            filesystem: false,
            filesystem_write: false,
            filesystem_read_roots: Vec::new(),
            filesystem_write_roots: Vec::new(),
            network: false,
            timers: false,
            environment: false,
            async_tasks: false,
            async_task_max_pending: Some(1024),
            process_run: false,
            process_run_allowlist: Vec::new(),
            process_run_max_concurrent: Some(8),
        }
    }
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
            async_task_max_pending: Some(1024),
            process_run: false,
            process_run_allowlist: Vec::new(),
            process_run_max_concurrent: Some(8),
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

pub(crate) use std_sources::std_module_source;

pub(crate) fn install_lsp_stdlib(session: &mut nox_core::Session) {
    let engine = session.engine_mut();
    register_print_intrinsic(engine, |_| {});
    register_to_str_intrinsics(engine);
    register_math_intrinsics(engine);
    register_string_stdlib(engine);
    register_json_stdlib(engine);
    register_delimited_text_stdlib(engine);
    register_collection_stdlib(engine);
    register_path_stdlib(engine);
    register_url_stdlib(engine);
    register_random_stdlib(engine);
    register_http_lsp_stubs(engine);
    register_test_stdlib(engine);
    register_encoding_stdlib(engine);
    register_hash_stdlib(engine);
    register_dotenv_stdlib(engine);
    register_ini_stdlib(engine);
    register_toml_stdlib(engine);
    register_yaml_stdlib(engine);
    register_term_stdlib(engine);
    register_bytes_stdlib(engine);
    register_lsp_process_stdlib(engine);
    engine
        .register_host_function(
            HostFunctionBuilder::new("args", Type::Array(Box::new(Type::Str))),
            |_| Ok(Value::array(Type::Str, Vec::new())),
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("read_text", Type::Str)
                .param("path", Type::Str)
                .docstring("Read a UTF-8 text file through the host filesystem boundary.")
                .capability("filesystem"),
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
            HostFunctionBuilder::new("__nox_std_fs_is_file", Type::Bool).param("path", Type::Str),
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
            HostFunctionBuilder::new("__nox_std_fs_is_dir", Type::Bool).param("path", Type::Str),
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
                "__nox_std_fs_list_dir",
                Type::Result {
                    ok: Box::new(Type::Array(Box::new(Type::Str))),
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
            HostFunctionBuilder::new(
                "__nox_std_fs_read_binary",
                Type::Result {
                    ok: Box::new(Type::Array(Box::new(Type::Int))),
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
            HostFunctionBuilder::new(
                "__nox_std_fs_write_binary",
                Type::Result {
                    ok: Box::new(Type::Null),
                    err: Box::new(Type::Str),
                },
            )
            .param("path", Type::Str)
            .param("bytes", Type::Array(Box::new(Type::Int))),
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
                "__nox_std_fs_canonicalize",
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
    register_time_analysis_stubs(engine);
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
            HostFunctionBuilder::new("task_sleep", Type::Task(Box::new(Type::Null)))
                .param("ms", Type::Int),
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
    engine
        .register_host_function(
            HostFunctionBuilder::new("task_join", Type::Bool)
                .param("id", Type::Int)
                .param("timeout_ms", Type::Int),
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
            HostFunctionBuilder::new("task_pending_count", Type::Int),
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

fn async_task_capability_required(operation: &str) -> Diagnostic {
    call_capability_required("async task", operation)
}

fn async_task_cap_exceeded(max: usize) -> Diagnostic {
    Diagnostic::new(
        format!("async task pending count would exceed configured cap of {max}"),
        Span { start: 0, end: 0 },
    )
    .with_code("runtime.task-pending-cap")
}

fn unknown_async_task_diagnostic(message: &'static str) -> Diagnostic {
    Diagnostic::new(message, Span { start: 0, end: 0 })
}

impl Runtime {
    pub fn new() -> Self {
        let mut runtime = Self {
            engine: Engine::new(),
            permissions: RuntimePermissions::none(),
            args: Rc::new(RefCell::new(Vec::new())),
            stdin: Rc::new(RefCell::new(None)),
            stdout: Rc::new(RefCell::new(String::new())),
            stderr: Rc::new(RefCell::new(String::new())),
            capture_stdout: Rc::new(RefCell::new(false)),
            exit_code: Rc::new(RefCell::new(None)),
            task_runtime: Rc::new(RefCell::new(TaskRuntime::default())),
            mock_clock: Rc::new(RefCell::new(None)),
            mock_env: Rc::new(RefCell::new(None)),
            mock_filesystem: Rc::new(RefCell::new(None)),
            mock_network: Rc::new(RefCell::new(None)),
            process_run_active: Rc::new(RefCell::new(0)),
            runtime_trace_events: Rc::new(RefCell::new(None)),
        };
        runtime.install_stdlib();
        runtime
    }

    pub fn with_permissions(permissions: RuntimePermissions) -> Self {
        let mut runtime = Self {
            engine: Engine::new(),
            permissions,
            args: Rc::new(RefCell::new(Vec::new())),
            stdin: Rc::new(RefCell::new(None)),
            stdout: Rc::new(RefCell::new(String::new())),
            stderr: Rc::new(RefCell::new(String::new())),
            capture_stdout: Rc::new(RefCell::new(false)),
            exit_code: Rc::new(RefCell::new(None)),
            task_runtime: Rc::new(RefCell::new(TaskRuntime::default())),
            mock_clock: Rc::new(RefCell::new(None)),
            mock_env: Rc::new(RefCell::new(None)),
            mock_filesystem: Rc::new(RefCell::new(None)),
            mock_network: Rc::new(RefCell::new(None)),
            process_run_active: Rc::new(RefCell::new(0)),
            runtime_trace_events: Rc::new(RefCell::new(None)),
        };
        runtime.install_stdlib();
        runtime
    }

    pub fn set_mock_clock_unix(&mut self, value: Option<i64>) {
        *self.mock_clock.borrow_mut() = value;
    }

    pub fn set_mock_env(&mut self, value: Option<BTreeMap<String, String>>) {
        *self.mock_env.borrow_mut() = value;
    }

    pub fn set_mock_filesystem(&mut self, value: Option<MockFilesystem>) {
        *self.mock_filesystem.borrow_mut() = value;
    }

    pub fn set_mock_network(&mut self, value: Option<MockNetwork>) {
        *self.mock_network.borrow_mut() = value;
    }

    pub fn set_args(&mut self, args: Vec<String>) {
        *self.args.borrow_mut() = args;
    }

    pub fn set_stdin(&mut self, input: impl Into<String>) {
        *self.stdin.borrow_mut() = Some(input.into());
    }

    pub fn set_mock_stdin(&mut self, input: Option<String>) {
        *self.stdin.borrow_mut() = input;
    }

    pub fn set_mock_stdout(&mut self, enabled: bool) {
        *self.capture_stdout.borrow_mut() = enabled;
    }

    pub fn take_stdout(&mut self) -> String {
        std::mem::take(&mut *self.stdout.borrow_mut())
    }

    pub fn take_stderr(&mut self) -> String {
        std::mem::take(&mut *self.stderr.borrow_mut())
    }

    pub fn exit_code(&self) -> Option<i64> {
        *self.exit_code.borrow()
    }

    pub fn pending_async_task_count(&self) -> usize {
        self.task_runtime.borrow().pending_count()
    }

    pub fn spawn_sleep_task(&mut self, duration: Duration) -> Result<u64, Diagnostic> {
        if !self.permissions.async_tasks {
            return Err(async_task_capability_required("spawn_sleep_task"));
        }
        let mut task_runtime = self.task_runtime.borrow_mut();
        if let Some(max) = self.permissions.async_task_max_pending {
            if task_runtime.pending_count() >= max {
                return Err(async_task_cap_exceeded(max));
            }
        }
        Ok(task_runtime.spawn_sleep(duration))
    }

    pub fn poll_async_task(&mut self, id: u64) -> Result<AsyncTaskPoll, Diagnostic> {
        if !self.permissions.async_tasks {
            return Err(async_task_capability_required("poll_async_task"));
        }
        let ready = self
            .task_runtime
            .borrow_mut()
            .poll(id)
            .map_err(unknown_async_task_diagnostic)?;
        if ready {
            Ok(AsyncTaskPoll::Ready)
        } else {
            Ok(AsyncTaskPoll::Pending)
        }
    }

    pub fn cancel_async_task(&mut self, id: u64) -> Result<(), Diagnostic> {
        if !self.permissions.async_tasks {
            return Err(async_task_capability_required("cancel_async_task"));
        }
        self.task_runtime
            .borrow_mut()
            .cancel(id)
            .map_err(unknown_async_task_diagnostic)
    }

    pub fn set_runtime_trace_enabled(&mut self, enabled: bool) {
        *self.runtime_trace_events.borrow_mut() = if enabled { Some(Vec::new()) } else { None };
    }

    pub fn take_runtime_trace_events(&mut self) -> Vec<RuntimeTraceEvent> {
        self.runtime_trace_events
            .borrow_mut()
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    pub fn set_instruction_budget(&mut self, budget: Option<usize>) {
        self.engine.set_instruction_budget(budget);
    }

    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
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

    pub fn lint(&mut self, source: &str) -> Result<Vec<LintWarning>, Diagnostic> {
        self.engine.lint(source)
    }

    pub fn profile(&mut self, source: &str) -> Result<(Value, ProfileReport), Diagnostic> {
        let first_task_id = self.task_runtime.borrow().next_id();
        let result = self.engine.profile(source);
        if result.is_err() {
            self.task_runtime
                .borrow_mut()
                .remove_tasks_created_since(first_task_id);
        }
        result
    }

    pub fn profile_file(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<(Value, ProfileReport), Diagnostic> {
        let path = path.as_ref();
        let source = self.prepare_file_source(path)?;
        self.profile(&source)
            .map_err(|err| err.with_source(path.display().to_string(), &source))
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
        self.stdout.borrow_mut().clear();
        self.stderr.borrow_mut().clear();
        let previous_capture_stdout = *self.capture_stdout.borrow();
        *self.capture_stdout.borrow_mut() = true;
        let result = self.engine.run_tests(&source).map(|mut result| {
            for test in &mut result.tests {
                if let Some(diagnostic) = test.diagnostic.take() {
                    test.diagnostic =
                        Some(diagnostic.with_source(path.display().to_string(), &source));
                }
            }
            result
        });
        *self.capture_stdout.borrow_mut() = previous_capture_stdout;
        let should_cleanup_tasks = match &result {
            Ok(result) => result.tests.iter().any(|test| !test.passed),
            Err(_) => true,
        };
        if should_cleanup_tasks {
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
        let (search_paths, external_modules) = match import_context_for_base(&base) {
            Ok(context) => context,
            Err(err) => return Err(vec![err]),
        };
        self.set_import_base_with_external(base, search_paths, external_modules);
        self.engine.check_diagnostics(source)
    }

    pub fn check_source_diagnostics_with_overlay(
        &mut self,
        source: &str,
        base: impl AsRef<Path>,
        overlay: HashMap<PathBuf, String>,
    ) -> Result<(), Vec<Diagnostic>> {
        let base = base.as_ref().to_path_buf();
        let (search_paths, external_modules) = match import_context_for_base(&base) {
            Ok(context) => context,
            Err(err) => return Err(vec![err]),
        };
        self.set_import_base_with_overlay_and_external(
            base,
            search_paths,
            overlay,
            external_modules,
        );
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
        let (search_paths, external_modules) = import_context_for_base(&base)?;
        self.set_import_base_with_external(base, search_paths, external_modules);
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
        let (search_paths, external_modules) = import_context_for_base(&base)?;
        self.set_import_base_with_overlay_and_external(
            base,
            search_paths,
            overlay,
            external_modules,
        );
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
        let external_modules = manifest
            .as_ref()
            .map(external_modules_for_manifest)
            .transpose()?
            .unwrap_or_default();
        self.set_import_base_with_external(base, search_paths, external_modules);

        let source = fs::read_to_string(path).map_err(|err| {
            Diagnostic::new(
                format!("failed to read '{}': {err}", path.display()),
                Span { start: 0, end: 0 },
            )
        })?;
        Ok(source)
    }

    #[cfg(test)]
    fn set_import_base(&mut self, base: PathBuf, search_paths: Vec<PathBuf>) {
        self.set_import_base_with_external(base, search_paths, Vec::new());
    }

    fn set_import_base_with_external(
        &mut self,
        base: PathBuf,
        search_paths: Vec<PathBuf>,
        external_modules: Vec<ExternalModuleDependency>,
    ) {
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
            if let Some(source) = load_external_module(specifier, &external_modules)? {
                return Ok(source);
            }
            read_module(&primary)
        });
    }

    fn set_import_base_with_overlay_and_external(
        &mut self,
        base: PathBuf,
        search_paths: Vec<PathBuf>,
        overlay: HashMap<PathBuf, String>,
        external_modules: Vec<ExternalModuleDependency>,
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
            if let Some(source) = load_external_module(specifier, &external_modules)? {
                return Ok(source);
            }
            read_module(&primary)
        });
    }
}

fn import_context_for_base(
    base: &Path,
) -> Result<(Vec<PathBuf>, Vec<ExternalModuleDependency>), Diagnostic> {
    let probe = base.join("probe.nox");
    match Manifest::discover(&probe)? {
        Some(manifest) => Ok((
            manifest.source_dirs(),
            external_modules_for_manifest(&manifest)?,
        )),
        None => Ok((Vec::new(), Vec::new())),
    }
}

pub(crate) fn external_modules_for_manifest(
    manifest: &Manifest,
) -> Result<Vec<ExternalModuleDependency>, Diagnostic> {
    if manifest.dependencies.is_empty() {
        return Ok(Vec::new());
    }

    let validation = validate_lockfile_for_manifest(manifest);
    if !validation.ok {
        return Err(validation
            .diagnostics
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                Diagnostic::new("dependency lockfile is invalid", Span { start: 0, end: 0 })
                    .with_code("lockfile.invalid")
            }));
    }

    let lockfile = Lockfile::load(&validation.path)?;
    let cache_dir = default_module_cache_dir();
    Ok(lockfile
        .dependencies
        .into_iter()
        .map(|dependency| ExternalModuleDependency {
            name: dependency.name,
            cache_path: cache_dir.join(dependency.cache_key),
            resolved: dependency.resolved,
            content_hash: dependency.content_hash,
        })
        .collect())
}

pub(crate) fn default_module_cache_dir() -> PathBuf {
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

pub(crate) fn load_external_module(
    specifier: &str,
    dependencies: &[ExternalModuleDependency],
) -> Result<Option<String>, Diagnostic> {
    let Some((dependency_name, module_path)) = specifier.split_once('/') else {
        return Ok(None);
    };
    let Some(dependency) = dependencies
        .iter()
        .find(|dependency| dependency.name == dependency_name)
    else {
        return Ok(None);
    };
    validate_external_module_path(specifier, module_path)?;
    if !dependency.cache_path.is_dir() {
        return Err(Diagnostic::new(
            format!(
                "external module dependency '{}' cache is missing at '{}'; run nox fetch",
                dependency.name,
                dependency.cache_path.display()
            ),
            Span { start: 0, end: 0 },
        )
        .with_code("module.cache-missing"));
    }
    verify_external_module_hash(dependency)?;
    let object = format!("{}:{module_path}", dependency.resolved);
    let source = run_git_capture(
        &[
            "--git-dir",
            &dependency.cache_path.display().to_string(),
            "show",
            &object,
        ],
        None,
    )
    .map_err(|err| {
        Diagnostic::new(
            format!("failed to load external module '{specifier}': {err}"),
            Span { start: 0, end: 0 },
        )
        .with_code("module.not-found")
    })?;
    String::from_utf8(source).map(Some).map_err(|err| {
        Diagnostic::new(
            format!("external module '{specifier}' is not valid UTF-8: {err}"),
            Span { start: 0, end: 0 },
        )
        .with_code("module.invalid-source")
    })
}

fn validate_external_module_path(specifier: &str, module_path: &str) -> Result<(), Diagnostic> {
    let path = Path::new(module_path);
    if module_path.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(Diagnostic::new(
            format!("invalid external module import '{specifier}'"),
            Span { start: 0, end: 0 },
        )
        .with_code("module.invalid-specifier"));
    }
    Ok(())
}

fn verify_external_module_hash(dependency: &ExternalModuleDependency) -> Result<(), Diagnostic> {
    let archive = run_git_capture(
        &[
            "--git-dir",
            &dependency.cache_path.display().to_string(),
            "archive",
            "--format=tar",
            &dependency.resolved,
        ],
        None,
    )
    .map_err(|err| {
        Diagnostic::new(
            format!(
                "external module dependency '{}' cache is corrupt: {err}",
                dependency.name
            ),
            Span { start: 0, end: 0 },
        )
        .with_code("module.cache-corrupt")
    })?;
    let actual = format!("sha256:{}", sha256_hex_bytes(&archive));
    if actual != dependency.content_hash {
        return Err(Diagnostic::new(
            format!(
                "external module dependency '{}' content hash mismatch: lockfile has {}, cache has {}",
                dependency.name, dependency.content_hash, actual
            ),
            Span { start: 0, end: 0 },
        )
        .with_code("module.hash-mismatch"));
    }
    Ok(())
}

pub fn run_git_capture(args: &[&str], cwd: Option<&Path>) -> Result<Vec<u8>, String> {
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .args(args)
        .output()
        .map_err(|err| format!("failed to run git: {err}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
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

fn normalize_mock_filesystem_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    normalize_lexical(&absolute)
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

type MockFilesystemHandle = Rc<RefCell<Option<MockFilesystem>>>;

fn write_mock_file(
    path: &Path,
    contents: &[u8],
    mock_filesystem: Option<&MockFilesystemHandle>,
) -> Result<bool, Diagnostic> {
    let Some(mock_filesystem) = mock_filesystem else {
        return Ok(false);
    };
    let normalized = normalize_runtime_path(path)?;
    if let Some(mock) = mock_filesystem.borrow_mut().as_mut() {
        mock.write_file(normalized, contents.to_vec());
        return Ok(true);
    }
    Ok(false)
}

fn read_text_result_with_mock(
    path: &str,
    roots: &[PathBuf],
    mock_filesystem: Option<&MockFilesystemHandle>,
) -> Result<Value, Diagnostic> {
    let path_ref = Path::new(path);
    check_filesystem_read(path_ref, roots)?;
    if let Some(mock_filesystem) = mock_filesystem {
        let normalized = normalize_runtime_path(path_ref)?;
        if let Some(mock) = mock_filesystem.borrow().as_ref() {
            return Ok(match mock.file(&normalized).map(|bytes| bytes.to_vec()) {
                Some(bytes) => match String::from_utf8(bytes) {
                    Ok(contents) => Value::ok(Type::Str, Type::Str, Value::string(contents)),
                    Err(err) => Value::err(
                        Type::Str,
                        Type::Str,
                        Value::string(format!("failed to read '{path}': {err}")),
                    ),
                },
                None => Value::err(
                    Type::Str,
                    Type::Str,
                    Value::string(format!(
                        "failed to read '{path}': not found in mock filesystem"
                    )),
                ),
            });
        }
    }
    read_text_result(path, roots)
}

fn read_text_with_mock(
    path: &str,
    roots: &[PathBuf],
    mock_filesystem: Option<&MockFilesystemHandle>,
) -> Result<Value, Diagnostic> {
    let path_ref = Path::new(path);
    check_filesystem_read(path_ref, roots)?;
    if let Some(mock_filesystem) = mock_filesystem {
        let normalized = normalize_runtime_path(path_ref)?;
        if let Some(mock) = mock_filesystem.borrow().as_ref() {
            let Some(bytes) = mock.file(&normalized).map(|bytes| bytes.to_vec()) else {
                return Err(Diagnostic::new(
                    format!("failed to read '{path}': not found in mock filesystem"),
                    Span { start: 0, end: 0 },
                ));
            };
            return String::from_utf8(bytes).map(Value::string).map_err(|err| {
                Diagnostic::new(
                    format!("failed to read '{path}': {err}"),
                    Span { start: 0, end: 0 },
                )
            });
        }
    }
    fs::read_to_string(path_ref)
        .map(Value::string)
        .map_err(|err| {
            Diagnostic::new(
                format!("failed to read '{path}': {err}"),
                Span { start: 0, end: 0 },
            )
        })
}

fn fs_exists(
    path: &str,
    roots: &[PathBuf],
    mock_filesystem: Option<&MockFilesystemHandle>,
) -> Result<Value, Diagnostic> {
    let path_ref = Path::new(path);
    check_filesystem_read(path_ref, roots)?;
    if let Some(mock_filesystem) = mock_filesystem {
        let normalized = normalize_runtime_path(path_ref)?;
        if let Some(mock) = mock_filesystem.borrow().as_ref() {
            return Ok(Value::Bool(
                mock.is_file(&normalized) || mock.is_dir(&normalized),
            ));
        }
    }
    Ok(Value::Bool(path_ref.exists()))
}

fn fs_is_file(
    path: &str,
    roots: &[PathBuf],
    mock_filesystem: Option<&MockFilesystemHandle>,
) -> Result<Value, Diagnostic> {
    let path_ref = Path::new(path);
    check_filesystem_read(path_ref, roots)?;
    if let Some(mock_filesystem) = mock_filesystem {
        let normalized = normalize_runtime_path(path_ref)?;
        if let Some(mock) = mock_filesystem.borrow().as_ref() {
            return Ok(Value::Bool(mock.is_file(&normalized)));
        }
    }
    Ok(Value::Bool(path_ref.is_file()))
}

fn fs_is_dir(
    path: &str,
    roots: &[PathBuf],
    mock_filesystem: Option<&MockFilesystemHandle>,
) -> Result<Value, Diagnostic> {
    let path_ref = Path::new(path);
    check_filesystem_read(path_ref, roots)?;
    if let Some(mock_filesystem) = mock_filesystem {
        let normalized = normalize_runtime_path(path_ref)?;
        if let Some(mock) = mock_filesystem.borrow().as_ref() {
            return Ok(Value::Bool(mock.is_dir(&normalized)));
        }
    }
    Ok(Value::Bool(path_ref.is_dir()))
}

fn fs_list_dir(
    path: &str,
    roots: &[PathBuf],
    mock_filesystem: Option<&MockFilesystemHandle>,
) -> Result<Value, Diagnostic> {
    let path_ref = Path::new(path);
    check_filesystem_read(path_ref, roots)?;
    if let Some(mock_filesystem) = mock_filesystem {
        let normalized = normalize_runtime_path(path_ref)?;
        if let Some(mock) = mock_filesystem.borrow().as_ref() {
            let entries = mock.list_dir(&normalized).map_err(|err| {
                Diagnostic::new(
                    format!("failed to list directory '{path}': {err}"),
                    Span { start: 0, end: 0 },
                )
            })?;
            return Ok(Value::ok(
                Type::Array(Box::new(Type::Str)),
                Type::Str,
                Value::array(Type::Str, entries.into_iter().map(Value::string).collect()),
            ));
        }
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(path_ref).map_err(|err| {
        Diagnostic::new(
            format!("failed to list directory '{path}': {err}"),
            Span { start: 0, end: 0 },
        )
    })? {
        let entry = entry.map_err(|err| {
            Diagnostic::new(
                format!("failed to list directory '{path}': {err}"),
                Span { start: 0, end: 0 },
            )
        })?;
        let name = entry.file_name().into_string().map_err(|_| {
            Diagnostic::new(
                format!("failed to list directory '{path}': entry name is not valid unicode"),
                Span { start: 0, end: 0 },
            )
        })?;
        entries.push(name);
    }
    entries.sort();
    Ok(Value::ok(
        Type::Array(Box::new(Type::Str)),
        Type::Str,
        Value::array(Type::Str, entries.into_iter().map(Value::string).collect()),
    ))
}

type MockEnvHandle = Rc<RefCell<Option<BTreeMap<String, String>>>>;
type MockNetworkHandle = Rc<RefCell<Option<MockNetwork>>>;

struct StdModuleAliasContext {
    mock_clock: Option<Rc<RefCell<Option<i64>>>>,
    mock_env: Option<MockEnvHandle>,
    mock_filesystem: Option<MockFilesystemHandle>,
    mock_network: Option<MockNetworkHandle>,
    process_run_active: Option<Rc<RefCell<usize>>>,
    runtime_trace_events: Option<Rc<RefCell<Option<Vec<RuntimeTraceEvent>>>>>,
}

fn install_std_module_aliases(
    engine: &mut Engine,
    permissions: &RuntimePermissions,
    context: StdModuleAliasContext,
) {
    let StdModuleAliasContext {
        mock_clock,
        mock_env,
        mock_filesystem,
        mock_network,
        process_run_active,
        runtime_trace_events,
    } = context;
    register_string_stdlib(engine);
    register_json_stdlib(engine);
    register_delimited_text_stdlib(engine);
    register_collection_stdlib(engine);
    register_path_stdlib(engine);

    let filesystem_read_allowed = permissions.filesystem;
    let filesystem_read_roots = permissions.filesystem_read_roots.clone();
    let mock_filesystem_for_read_text = mock_filesystem.clone();
    let trace_read_text_events = runtime_trace_events.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_fs_read_text", Type::Str).param("path", Type::Str),
            move |args| {
                if !filesystem_read_allowed {
                    if let Some(events) = &trace_read_text_events {
                        push_filesystem_trace_event(
                            events,
                            "read_text",
                            None,
                            false,
                            None,
                            None,
                            None,
                        );
                    }
                    return Err(call_capability_required("filesystem", "read_text"));
                }
                match args {
                    [Value::String(path)] => {
                        let result = read_text_with_mock(
                            path.as_ref(),
                            &filesystem_read_roots,
                            mock_filesystem_for_read_text.as_ref(),
                        );
                        if let Some(events) = &trace_read_text_events {
                            let (status, bytes) = match &result {
                                Ok(Value::String(contents)) => {
                                    ("ok", Some(contents.as_ref().len() as u64))
                                }
                                Ok(_) => ("ok", None),
                                Err(_) => ("error", None),
                            };
                            push_filesystem_trace_event(
                                events,
                                "read_text",
                                Some(path.as_ref()),
                                true,
                                Some(status),
                                bytes,
                                None,
                            );
                        }
                        result
                    }
                    _ => unreachable!("static checker guarantees read_text argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let filesystem_read_allowed = permissions.filesystem;
    let filesystem_read_roots = permissions.filesystem_read_roots.clone();
    let mock_filesystem_for_try_read_text = mock_filesystem.clone();
    let trace_try_read_text_events = runtime_trace_events.clone();
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
                    if let Some(events) = &trace_try_read_text_events {
                        push_filesystem_trace_event(
                            events,
                            "try_read_text",
                            None,
                            false,
                            None,
                            None,
                            None,
                        );
                    }
                    return Err(call_capability_required("filesystem", "try_read_text"));
                }
                match args {
                    [Value::String(path)] => {
                        let result = read_text_result_with_mock(
                            path.as_ref(),
                            &filesystem_read_roots,
                            mock_filesystem_for_try_read_text.as_ref(),
                        );
                        if let Some(events) = &trace_try_read_text_events {
                            let status = match &result {
                                Ok(Value::Result(value)) if value.is_ok() => "ok",
                                Ok(_) => "error",
                                Err(_) => "error",
                            };
                            push_filesystem_trace_event(
                                events,
                                "try_read_text",
                                Some(path.as_ref()),
                                true,
                                Some(status),
                                None,
                                None,
                            );
                        }
                        result
                    }
                    _ => unreachable!("static checker guarantees try_read_text argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let filesystem_read_allowed = permissions.filesystem;
    let filesystem_read_roots = permissions.filesystem_read_roots.clone();
    let mock_filesystem_for_exists = mock_filesystem.clone();
    let trace_exists_events = runtime_trace_events.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_fs_exists", Type::Bool).param("path", Type::Str),
            move |args| {
                if !filesystem_read_allowed {
                    if let Some(events) = &trace_exists_events {
                        push_filesystem_trace_event(
                            events, "exists", None, false, None, None, None,
                        );
                    }
                    return Err(call_capability_required("filesystem", "exists"));
                }
                match args {
                    [Value::String(path)] => {
                        let result = fs_exists(
                            path.as_ref(),
                            &filesystem_read_roots,
                            mock_filesystem_for_exists.as_ref(),
                        );
                        if let Some(events) = &trace_exists_events {
                            let status = if result.is_ok() { "ok" } else { "error" };
                            push_filesystem_trace_event(
                                events,
                                "exists",
                                Some(path.as_ref()),
                                true,
                                Some(status),
                                None,
                                None,
                            );
                        }
                        result
                    }
                    _ => unreachable!("static checker guarantees exists argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let filesystem_read_allowed = permissions.filesystem;
    let filesystem_read_roots = permissions.filesystem_read_roots.clone();
    let mock_filesystem_for_is_file = mock_filesystem.clone();
    let trace_is_file_events = runtime_trace_events.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_fs_is_file", Type::Bool).param("path", Type::Str),
            move |args| {
                if !filesystem_read_allowed {
                    if let Some(events) = &trace_is_file_events {
                        push_filesystem_trace_event(
                            events, "is_file", None, false, None, None, None,
                        );
                    }
                    return Err(call_capability_required("filesystem", "is_file"));
                }
                match args {
                    [Value::String(path)] => {
                        let result = fs_is_file(
                            path.as_ref(),
                            &filesystem_read_roots,
                            mock_filesystem_for_is_file.as_ref(),
                        );
                        if let Some(events) = &trace_is_file_events {
                            push_filesystem_trace_event(
                                events,
                                "is_file",
                                Some(path.as_ref()),
                                true,
                                Some(if result.is_ok() { "ok" } else { "error" }),
                                None,
                                None,
                            );
                        }
                        result
                    }
                    _ => unreachable!("static checker guarantees is_file argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let filesystem_read_allowed = permissions.filesystem;
    let filesystem_read_roots = permissions.filesystem_read_roots.clone();
    let mock_filesystem_for_is_dir = mock_filesystem.clone();
    let trace_is_dir_events = runtime_trace_events.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_fs_is_dir", Type::Bool).param("path", Type::Str),
            move |args| {
                if !filesystem_read_allowed {
                    if let Some(events) = &trace_is_dir_events {
                        push_filesystem_trace_event(
                            events, "is_dir", None, false, None, None, None,
                        );
                    }
                    return Err(call_capability_required("filesystem", "is_dir"));
                }
                match args {
                    [Value::String(path)] => {
                        let result = fs_is_dir(
                            path.as_ref(),
                            &filesystem_read_roots,
                            mock_filesystem_for_is_dir.as_ref(),
                        );
                        if let Some(events) = &trace_is_dir_events {
                            push_filesystem_trace_event(
                                events,
                                "is_dir",
                                Some(path.as_ref()),
                                true,
                                Some(if result.is_ok() { "ok" } else { "error" }),
                                None,
                                None,
                            );
                        }
                        result
                    }
                    _ => unreachable!("static checker guarantees is_dir argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let filesystem_read_allowed = permissions.filesystem;
    let filesystem_read_roots = permissions.filesystem_read_roots.clone();
    let mock_filesystem_for_list_dir = mock_filesystem.clone();
    let trace_list_dir_events = runtime_trace_events.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_fs_list_dir",
                Type::Result {
                    ok: Box::new(Type::Array(Box::new(Type::Str))),
                    err: Box::new(Type::Str),
                },
            )
            .param("path", Type::Str),
            move |args| {
                if !filesystem_read_allowed {
                    if let Some(events) = &trace_list_dir_events {
                        push_filesystem_trace_event(
                            events, "list_dir", None, false, None, None, None,
                        );
                    }
                    return Err(call_capability_required("filesystem", "list_dir"));
                }
                match args {
                    [Value::String(path)] => {
                        let result = fs_list_dir(
                            path.as_ref(),
                            &filesystem_read_roots,
                            mock_filesystem_for_list_dir.as_ref(),
                        );
                        if let Some(events) = &trace_list_dir_events {
                            let status = match &result {
                                Ok(Value::Result(value)) if value.is_ok() => "ok",
                                Ok(_) => "error",
                                Err(_) => "error",
                            };
                            push_filesystem_trace_event(
                                events,
                                "list_dir",
                                Some(path.as_ref()),
                                true,
                                Some(status),
                                None,
                                None,
                            );
                        }
                        result
                    }
                    _ => unreachable!("static checker guarantees list_dir argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let filesystem_write_allowed = permissions.filesystem_write;
    let filesystem_write_roots = permissions.filesystem_write_roots.clone();
    let mock_filesystem_for_write_text = mock_filesystem.clone();
    let trace_write_text_events = runtime_trace_events.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_fs_write_text", Type::Null)
                .param("path", Type::Str)
                .param("contents", Type::Str),
            move |args| {
                if !filesystem_write_allowed {
                    if let Some(events) = &trace_write_text_events {
                        push_filesystem_trace_event(
                            events,
                            "write_text",
                            None,
                            false,
                            None,
                            None,
                            None,
                        );
                    }
                    return Err(call_capability_required("filesystem write", "write_text"));
                }
                match args {
                    [Value::String(path), Value::String(contents)] => {
                        let path_ref = Path::new(path.as_ref());
                        let result = (|| {
                            check_filesystem_write(path_ref, &filesystem_write_roots)?;
                            if write_mock_file(
                                path_ref,
                                contents.as_ref().as_bytes(),
                                mock_filesystem_for_write_text.as_ref(),
                            )? {
                                return Ok(Value::Null);
                            }
                            fs::write(path_ref, contents.as_ref())
                                .map(|_| Value::Null)
                                .map_err(|err| {
                                    Diagnostic::new(
                                        format!("failed to write '{path}': {err}"),
                                        Span { start: 0, end: 0 },
                                    )
                                })
                        })();
                        if let Some(events) = &trace_write_text_events {
                            push_filesystem_trace_event(
                                events,
                                "write_text",
                                Some(path.as_ref()),
                                true,
                                Some(if result.is_ok() { "ok" } else { "error" }),
                                Some(contents.as_ref().len() as u64),
                                None,
                            );
                        }
                        result
                    }
                    _ => unreachable!("static checker guarantees write_text argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let filesystem_read_allowed = permissions.filesystem;
    let filesystem_read_roots = permissions.filesystem_read_roots.clone();
    let mock_filesystem_for_read_binary = mock_filesystem.clone();
    let trace_read_binary_events = runtime_trace_events.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_fs_read_binary",
                Type::Result {
                    ok: Box::new(Type::Array(Box::new(Type::Int))),
                    err: Box::new(Type::Str),
                },
            )
            .param("path", Type::Str),
            move |args| {
                if !filesystem_read_allowed {
                    if let Some(events) = &trace_read_binary_events {
                        push_filesystem_trace_event(
                            events, "read_binary", None, false, None, None, None,
                        );
                    }
                    return Err(call_capability_required("filesystem", "read_binary"));
                }
                match args {
                    [Value::String(path)] => {
                        let result = (|| {
                            let ok_type = Type::Array(Box::new(Type::Int));
                            let path_ref = Path::new(path.as_ref());
                            check_filesystem_read(path_ref, &filesystem_read_roots)?;
                            if let Some(mock_filesystem) = &mock_filesystem_for_read_binary {
                                let normalized = normalize_runtime_path(path_ref)?;
                                if let Some(mock) = mock_filesystem.borrow().as_ref() {
                                    return Ok(match mock.file(&normalized) {
                                        Some(bytes) => Value::ok(
                                            ok_type,
                                            Type::Str,
                                            bytes_vec_to_array(bytes.to_vec()),
                                        ),
                                        None => Value::err(
                                            ok_type,
                                            Type::Str,
                                            Value::string(format!(
                                                "failed to read '{path}': not found in mock filesystem"
                                            )),
                                        ),
                                    });
                                }
                            }
                            match fs::read(path_ref) {
                                Ok(bytes) => {
                                    Ok(Value::ok(ok_type, Type::Str, bytes_vec_to_array(bytes)))
                                }
                                Err(err) => Ok(Value::err(
                                    ok_type,
                                    Type::Str,
                                    Value::string(format!("failed to read '{path}': {err}")),
                                )),
                            }
                        })();
                        if let Some(events) = &trace_read_binary_events {
                            let (status, bytes) = match &result {
                                Ok(Value::Result(value)) if value.is_ok() => {
                                    let bytes = match value.payload() {
                                        Value::Array(array) => Some(array.snapshot().len() as u64),
                                        _ => None,
                                    };
                                    ("ok", bytes)
                                }
                                Ok(_) => ("error", None),
                                Err(_) => ("error", None),
                            };
                            push_filesystem_trace_event(
                                events,
                                "read_binary",
                                Some(path.as_ref()),
                                true,
                                Some(status),
                                bytes,
                                None,
                            );
                        }
                        result
                    }
                    _ => unreachable!("static checker guarantees read_binary argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let filesystem_write_allowed = permissions.filesystem_write;
    let filesystem_write_roots = permissions.filesystem_write_roots.clone();
    let mock_filesystem_for_write_binary = mock_filesystem.clone();
    let trace_write_binary_events = runtime_trace_events.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_fs_write_binary",
                Type::Result {
                    ok: Box::new(Type::Null),
                    err: Box::new(Type::Str),
                },
            )
            .param("path", Type::Str)
            .param("bytes", Type::Array(Box::new(Type::Int))),
            move |args| {
                if !filesystem_write_allowed {
                    if let Some(events) = &trace_write_binary_events {
                        push_filesystem_trace_event(
                            events,
                            "write_binary",
                            None,
                            false,
                            None,
                            None,
                            None,
                        );
                    }
                    return Err(call_capability_required("filesystem write", "write_binary"));
                }
                match args {
                    [Value::String(path), Value::Array(byte_array)] => {
                        let ok_type = Type::Null;
                        let result = match bytes_array_to_vec(byte_array) {
                            Ok(bytes) => {
                                let path_ref = Path::new(path.as_ref());
                                let bytes_len = bytes.len() as u64;
                                let result = (|| {
                                    check_filesystem_write(path_ref, &filesystem_write_roots)?;
                                    if write_mock_file(
                                        path_ref,
                                        &bytes,
                                        mock_filesystem_for_write_binary.as_ref(),
                                    )? {
                                        return Ok(Value::ok(ok_type, Type::Str, Value::Null));
                                    }
                                    match fs::write(path_ref, &bytes) {
                                        Ok(_) => Ok(Value::ok(ok_type, Type::Str, Value::Null)),
                                        Err(err) => Ok(Value::err(
                                            ok_type,
                                            Type::Str,
                                            Value::string(format!(
                                                "failed to write '{path}': {err}"
                                            )),
                                        )),
                                    }
                                })();
                                if let Some(events) = &trace_write_binary_events {
                                    let status = match &result {
                                        Ok(Value::Result(value)) if value.is_ok() => "ok",
                                        Ok(_) => "error",
                                        Err(_) => "error",
                                    };
                                    push_filesystem_trace_event(
                                        events,
                                        "write_binary",
                                        Some(path.as_ref()),
                                        true,
                                        Some(status),
                                        Some(bytes_len),
                                        None,
                                    );
                                }
                                result
                            }
                            Err(message) => {
                                if let Some(events) = &trace_write_binary_events {
                                    push_filesystem_trace_event(
                                        events,
                                        "write_binary",
                                        Some(path.as_ref()),
                                        true,
                                        Some("error"),
                                        None,
                                        None,
                                    );
                                }
                                Ok(Value::err(ok_type, Type::Str, Value::string(message)))
                            }
                        };
                        result
                    }
                    _ => unreachable!("static checker guarantees write_binary argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let filesystem_read_allowed = permissions.filesystem;
    let filesystem_read_roots = permissions.filesystem_read_roots.clone();
    let mock_filesystem_for_canonicalize = mock_filesystem.clone();
    let trace_canonicalize_events = runtime_trace_events.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_fs_canonicalize",
                Type::Result {
                    ok: Box::new(Type::Str),
                    err: Box::new(Type::Str),
                },
            )
            .param("path", Type::Str),
            move |args| {
                if !filesystem_read_allowed {
                    if let Some(events) = &trace_canonicalize_events {
                        push_filesystem_trace_event(
                            events, "canonicalize", None, false, None, None, None,
                        );
                    }
                    return Err(call_capability_required("filesystem", "canonicalize"));
                }
                match args {
                    [Value::String(path)] => {
                        let result = (|| {
                            let path_ref = Path::new(path.as_ref());
                            check_filesystem_read(path_ref, &filesystem_read_roots)?;
                            if let Some(mock_filesystem) = &mock_filesystem_for_canonicalize {
                                let normalized = normalize_runtime_path(path_ref)?;
                                if let Some(mock) = mock_filesystem.borrow().as_ref() {
                                    return Ok(if mock.is_file(&normalized)
                                        || mock.is_dir(&normalized)
                                    {
                                        Value::ok(
                                            Type::Str,
                                            Type::Str,
                                            Value::string(normalized.to_string_lossy().to_string()),
                                        )
                                    } else {
                                        Value::err(
                                            Type::Str,
                                            Type::Str,
                                            Value::string(format!(
                                                "failed to canonicalize '{path}': not found in mock filesystem"
                                            )),
                                        )
                                    });
                                }
                            }
                            match fs::canonicalize(path_ref) {
                                Ok(resolved) => Ok(Value::ok(
                                    Type::Str,
                                    Type::Str,
                                    Value::string(resolved.to_string_lossy().to_string()),
                                )),
                                Err(err) => Ok(Value::err(
                                    Type::Str,
                                    Type::Str,
                                    Value::string(format!("failed to canonicalize '{path}': {err}")),
                                )),
                            }
                        })();
                        if let Some(events) = &trace_canonicalize_events {
                            let status = match &result {
                                Ok(Value::Result(value)) if value.is_ok() => "ok",
                                Ok(_) => "error",
                                Err(_) => "error",
                            };
                            push_filesystem_trace_event(
                                events,
                                "canonicalize",
                                Some(path.as_ref()),
                                true,
                                Some(status),
                                None,
                                None,
                            );
                        }
                        result
                    }
                    _ => unreachable!("static checker guarantees canonicalize argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let environment_allowed = permissions.environment;
    let mock_env_get = mock_env.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_env_get", Type::Str).param("name", Type::Str),
            move |args| {
                if !environment_allowed {
                    return Err(call_capability_required("environment", "env_get"));
                }

                match args {
                    [Value::String(name)] => {
                        if let Some(handle) = &mock_env_get {
                            if let Some(mocks) = handle.borrow().as_ref() {
                                return match mocks.get(name.as_ref()) {
                                    Some(value) => Ok(Value::string(value.clone())),
                                    None => Err(Diagnostic::new(
                                        format!(
                                            "failed to read environment variable '{name}': not present in mock env"
                                        ),
                                        Span { start: 0, end: 0 },
                                    )),
                                };
                            }
                        }
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
    let mock_env_try_get = mock_env.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_env_try_get", Type::Option(Box::new(Type::Str)))
                .param("name", Type::Str),
            move |args| {
                if !environment_allowed {
                    return Err(call_capability_required("environment", "env_try_get"));
                }

                match args {
                    [Value::String(name)] => {
                        if let Some(handle) = &mock_env_try_get {
                            if let Some(mocks) = handle.borrow().as_ref() {
                                return Ok(match mocks.get(name.as_ref()) {
                                    Some(value) => {
                                        Value::some(Type::Str, Value::string(value.clone()))
                                    }
                                    None => Value::none(Type::Str),
                                });
                            }
                        }
                        read_optional_env(name.as_ref())
                    }
                    _ => unreachable!("static checker guarantees env_try_get argument type"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let environment_allowed = permissions.environment;
    let mock_env_list = mock_env;
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_env_list", Type::Map(Box::new(Type::Str))),
            move |_| {
                if !environment_allowed {
                    return Err(call_capability_required("environment", "env_list"));
                }

                if let Some(handle) = &mock_env_list {
                    if let Some(mocks) = handle.borrow().as_ref() {
                        let entries: BTreeMap<String, Value> = mocks
                            .iter()
                            .map(|(k, v)| (k.clone(), Value::string(v.clone())))
                            .collect();
                        return Ok(Value::map(Type::Str, entries));
                    }
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

    register_time_intrinsics_with_clock(engine, mock_clock);

    register_url_stdlib(engine);
    register_random_stdlib(engine);
    register_http_stdlib(engine, permissions, mock_network);
    register_test_stdlib(engine);
    register_encoding_stdlib(engine);
    register_hash_stdlib(engine);
    register_dotenv_stdlib(engine);
    register_ini_stdlib(engine);
    register_toml_stdlib(engine);
    register_yaml_stdlib(engine);
    register_process_run_stdlib(
        engine,
        permissions,
        process_run_active.unwrap_or_else(|| Rc::new(RefCell::new(0))),
    );
    register_term_stdlib(engine);
    register_bytes_stdlib(engine);
}

fn bytes_array_to_vec(values: &Array) -> Result<Vec<u8>, String> {
    let snapshot = values.snapshot();
    let mut out = Vec::with_capacity(snapshot.len());
    for value in snapshot {
        let Value::Int(byte) = value else {
            return Err("bytes array must contain int values".to_string());
        };
        if !(0..=255).contains(&byte) {
            return Err(format!("byte value {byte} is out of range 0..255"));
        }
        out.push(byte as u8);
    }
    Ok(out)
}

fn bytes_vec_to_array(bytes: Vec<u8>) -> Value {
    Value::array(
        Type::Int,
        bytes.into_iter().map(|b| Value::Int(b as i64)).collect(),
    )
}

fn register_bytes_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_bytes_encode_utf8",
                Type::Array(Box::new(Type::Int)),
            )
            .param("text", Type::Str),
            |args| match args {
                [Value::String(text)] => Ok(bytes_vec_to_array(text.as_bytes().to_vec())),
                _ => unreachable!("static checker guarantees bytes.encode_utf8 argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_bytes_decode_utf8",
                Type::Result {
                    ok: Box::new(Type::Str),
                    err: Box::new(Type::Str),
                },
            )
            .param("values", Type::Array(Box::new(Type::Int))),
            |args| match args {
                [Value::Array(array)] => match bytes_array_to_vec(array) {
                    Ok(bytes) => match String::from_utf8(bytes) {
                        Ok(text) => Ok(Value::ok(Type::Str, Type::Str, Value::string(text))),
                        Err(_) => Ok(Value::err(
                            Type::Str,
                            Type::Str,
                            Value::string("bytes are not valid UTF-8"),
                        )),
                    },
                    Err(message) => Ok(Value::err(Type::Str, Type::Str, Value::string(message))),
                },
                _ => unreachable!("static checker guarantees bytes.decode_utf8 argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_bytes_len", Type::Int)
                .param("values", Type::Array(Box::new(Type::Int))),
            |args| match args {
                [Value::Array(array)] => match bytes_array_to_vec(array) {
                    Ok(bytes) => Ok(Value::Int(bytes.len() as i64)),
                    Err(message) => Err(Diagnostic::new(message, Span { start: 0, end: 0 })),
                },
                _ => unreachable!("static checker guarantees bytes.len argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_bytes_get",
                Type::Result {
                    ok: Box::new(Type::Int),
                    err: Box::new(Type::Str),
                },
            )
            .param("values", Type::Array(Box::new(Type::Int)))
            .param("index", Type::Int),
            |args| match args {
                [Value::Array(array), Value::Int(index)] => match bytes_array_to_vec(array) {
                    Ok(bytes) => {
                        let ok_type = Type::Int;
                        let Some(index) = usize::try_from(*index).ok() else {
                            return Ok(Value::err(
                                ok_type,
                                Type::Str,
                                Value::string("byte index must be non-negative"),
                            ));
                        };
                        match bytes.get(index) {
                            Some(byte) => {
                                Ok(Value::ok(ok_type, Type::Str, Value::Int(*byte as i64)))
                            }
                            None => Ok(Value::err(
                                ok_type,
                                Type::Str,
                                Value::string(format!("byte index {index} is out of range")),
                            )),
                        }
                    }
                    Err(message) => Ok(Value::err(Type::Int, Type::Str, Value::string(message))),
                },
                _ => unreachable!("static checker guarantees bytes.get argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_bytes_slice_copy",
                Type::Result {
                    ok: Box::new(Type::Array(Box::new(Type::Int))),
                    err: Box::new(Type::Str),
                },
            )
            .param("values", Type::Array(Box::new(Type::Int)))
            .param("start", Type::Int)
            .param("length", Type::Int),
            |args| match args {
                [Value::Array(array), Value::Int(start), Value::Int(length)] => {
                    let ok_type = Type::Array(Box::new(Type::Int));
                    match bytes_array_to_vec(array) {
                        Ok(bytes) => {
                            let Some(start) = usize::try_from(*start).ok() else {
                                return Ok(Value::err(
                                    ok_type,
                                    Type::Str,
                                    Value::string("byte slice start must be non-negative"),
                                ));
                            };
                            let Some(length) = usize::try_from(*length).ok() else {
                                return Ok(Value::err(
                                    ok_type,
                                    Type::Str,
                                    Value::string("byte slice length must be non-negative"),
                                ));
                            };
                            let Some(end) = start.checked_add(length) else {
                                return Ok(Value::err(
                                    ok_type,
                                    Type::Str,
                                    Value::string("byte slice range overflows"),
                                ));
                            };
                            if end > bytes.len() {
                                return Ok(Value::err(
                                    ok_type,
                                    Type::Str,
                                    Value::string(format!(
                                        "byte slice {start}..{end} is out of range"
                                    )),
                                ));
                            }
                            Ok(Value::ok(
                                ok_type,
                                Type::Str,
                                bytes_vec_to_array(bytes[start..end].to_vec()),
                            ))
                        }
                        Err(message) => Ok(Value::err(ok_type, Type::Str, Value::string(message))),
                    }
                }
                _ => unreachable!("static checker guarantees bytes.slice_copy argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_bytes_equal", Type::Bool)
                .param("left", Type::Array(Box::new(Type::Int)))
                .param("right", Type::Array(Box::new(Type::Int))),
            |args| match args {
                [Value::Array(left), Value::Array(right)] => {
                    let left = bytes_array_to_vec(left)
                        .map_err(|message| Diagnostic::new(message, Span { start: 0, end: 0 }))?;
                    let right = bytes_array_to_vec(right)
                        .map_err(|message| Diagnostic::new(message, Span { start: 0, end: 0 }))?;
                    Ok(Value::Bool(left == right))
                }
                _ => unreachable!("static checker guarantees bytes.equal argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_bytes_base64_encode", Type::Str)
                .param("values", Type::Array(Box::new(Type::Int))),
            |args| match args {
                [Value::Array(array)] => match bytes_array_to_vec(array) {
                    Ok(bytes) => Ok(Value::string(base64_encode_bytes(&bytes))),
                    Err(message) => Err(Diagnostic::new(message, Span { start: 0, end: 0 })),
                },
                _ => unreachable!("static checker guarantees bytes.base64_encode argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_bytes_base64_decode",
                Type::Result {
                    ok: Box::new(Type::Array(Box::new(Type::Int))),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => {
                    let ok_type = Type::Array(Box::new(Type::Int));
                    match base64_decode_bytes(value.as_ref()) {
                        Ok(bytes) => Ok(Value::ok(ok_type, Type::Str, bytes_vec_to_array(bytes))),
                        Err(message) => Ok(Value::err(ok_type, Type::Str, Value::string(message))),
                    }
                }
                _ => unreachable!("static checker guarantees bytes.base64_decode argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_bytes_hex_encode", Type::Str)
                .param("values", Type::Array(Box::new(Type::Int))),
            |args| match args {
                [Value::Array(array)] => match bytes_array_to_vec(array) {
                    Ok(bytes) => Ok(Value::string(hex_encode_bytes(&bytes))),
                    Err(message) => Err(Diagnostic::new(message, Span { start: 0, end: 0 })),
                },
                _ => unreachable!("static checker guarantees bytes.hex_encode argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_bytes_hex_decode",
                Type::Result {
                    ok: Box::new(Type::Array(Box::new(Type::Int))),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => {
                    let ok_type = Type::Array(Box::new(Type::Int));
                    match hex_decode_bytes(value.as_ref()) {
                        Ok(bytes) => Ok(Value::ok(ok_type, Type::Str, bytes_vec_to_array(bytes))),
                        Err(message) => Ok(Value::err(ok_type, Type::Str, Value::string(message))),
                    }
                }
                _ => unreachable!("static checker guarantees bytes.hex_decode argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn term_is_tty_stdout() -> bool {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let stdout = std::io::stdout();
        unsafe { libc_isatty(stdout.as_raw_fd()) != 0 }
    }
    #[cfg(windows)]
    {
        windows_console_is_tty(STD_OUTPUT_HANDLE)
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

fn term_is_tty_stderr() -> bool {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let stderr = std::io::stderr();
        unsafe { libc_isatty(stderr.as_raw_fd()) != 0 }
    }
    #[cfg(windows)]
    {
        windows_console_is_tty(STD_ERROR_HANDLE)
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

#[cfg(unix)]
extern "C" {
    fn isatty(fd: i32) -> i32;
}

#[cfg(unix)]
unsafe fn libc_isatty(fd: i32) -> i32 {
    isatty(fd)
}

#[cfg(windows)]
const STD_OUTPUT_HANDLE: u32 = (-11i32) as u32;
#[cfg(windows)]
const STD_ERROR_HANDLE: u32 = (-12i32) as u32;

#[cfg(windows)]
unsafe extern "system" {
    fn GetStdHandle(n_std_handle: u32) -> *mut std::ffi::c_void;
    fn GetConsoleMode(h_console_handle: *mut std::ffi::c_void, lp_mode: *mut u32) -> i32;
}

#[cfg(windows)]
fn windows_console_is_tty(std_handle: u32) -> bool {
    let handle = unsafe { GetStdHandle(std_handle) };
    if handle.is_null() || handle as isize == -1 {
        return false;
    }
    let mut mode = 0u32;
    unsafe { GetConsoleMode(handle, &mut mode) != 0 }
}

#[cfg(all(unix, target_os = "linux"))]
#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxTermios {
    c_iflag: u32,
    c_oflag: u32,
    c_cflag: u32,
    c_lflag: u32,
    c_line: u8,
    c_cc: [u8; 32],
    c_ispeed: u32,
    c_ospeed: u32,
}

#[cfg(all(unix, target_os = "linux"))]
unsafe extern "C" {
    fn tcgetattr(fd: i32, termios_p: *mut LinuxTermios) -> i32;
    fn tcsetattr(fd: i32, optional_actions: i32, termios_p: *const LinuxTermios) -> i32;
}

#[cfg(all(unix, target_os = "linux"))]
struct TerminalEchoGuard {
    fd: i32,
    original: LinuxTermios,
}

#[cfg(all(unix, target_os = "linux"))]
impl Drop for TerminalEchoGuard {
    fn drop(&mut self) {
        const TCSANOW: i32 = 0;
        unsafe {
            let _ = tcsetattr(self.fd, TCSANOW, &self.original);
        }
    }
}

#[cfg(all(unix, target_os = "linux"))]
fn disable_terminal_echo(fd: i32) -> Result<TerminalEchoGuard, String> {
    const ECHO: u32 = 0o0000010;
    const TCSANOW: i32 = 0;
    let mut original = LinuxTermios {
        c_iflag: 0,
        c_oflag: 0,
        c_cflag: 0,
        c_lflag: 0,
        c_line: 0,
        c_cc: [0; 32],
        c_ispeed: 0,
        c_ospeed: 0,
    };
    let get_result = unsafe { tcgetattr(fd, &mut original) };
    if get_result != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut without_echo = original;
    without_echo.c_lflag &= !ECHO;
    let set_result = unsafe { tcsetattr(fd, TCSANOW, &without_echo) };
    if set_result != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(TerminalEchoGuard { fd, original })
}

fn term_color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    term_is_tty_stdout()
}

fn term_style_color(value: &str, color: &str) -> String {
    if !term_color_enabled() {
        return value.to_string();
    }
    let code = match color {
        "red" => "31",
        "green" => "32",
        "yellow" => "33",
        "blue" => "34",
        "magenta" => "35",
        "cyan" => "36",
        "bold" => "1",
        _ => return value.to_string(),
    };
    format!("\x1b[{code}m{value}\x1b[0m")
}

fn term_pad_column(value: &str, width: i64) -> String {
    let width = if width <= 0 { 0 } else { width as usize };
    let count = value.chars().count();
    if count >= width {
        value.to_string()
    } else {
        let mut out = String::with_capacity(value.len() + (width - count));
        out.push_str(value);
        for _ in count..width {
            out.push(' ');
        }
        out
    }
}

fn register_term_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_term_is_tty_stdout", Type::Bool),
            |_| Ok(Value::Bool(term_is_tty_stdout())),
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_term_is_tty_stderr", Type::Bool),
            |_| Ok(Value::Bool(term_is_tty_stderr())),
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_term_color_enabled", Type::Bool),
            |_| Ok(Value::Bool(term_color_enabled())),
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_term_style_color", Type::Str)
                .param("value", Type::Str)
                .param("color", Type::Str),
            |args| match args {
                [Value::String(value), Value::String(color)] => Ok(Value::string(
                    term_style_color(value.as_ref(), color.as_ref()),
                )),
                _ => unreachable!("static checker guarantees term.style_color argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_term_pad_column", Type::Str)
                .param("value", Type::Str)
                .param("width", Type::Int),
            |args| match args {
                [Value::String(value), Value::Int(width)] => {
                    Ok(Value::string(term_pad_column(value.as_ref(), *width)))
                }
                _ => unreachable!("static checker guarantees term.pad_column argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_term_prompt",
                Type::Result {
                    ok: Box::new(Type::Str),
                    err: Box::new(Type::Str),
                },
            )
            .param("message", Type::Str),
            |args| match args {
                [Value::String(message)] => {
                    use std::io::Write as _;
                    eprint!("{message}");
                    let _ = std::io::stderr().flush();
                    let mut line = String::new();
                    match std::io::stdin().read_line(&mut line) {
                        Ok(0) => Ok(Value::err(
                            Type::Str,
                            Type::Str,
                            Value::string("stdin reached EOF"),
                        )),
                        Ok(_) => {
                            let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
                            Ok(Value::ok(Type::Str, Type::Str, Value::string(trimmed)))
                        }
                        Err(err) => Ok(Value::err(
                            Type::Str,
                            Type::Str,
                            Value::string(format!("stdin read failed: {err}")),
                        )),
                    }
                }
                _ => unreachable!("static checker guarantees term.prompt argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_term_confirm",
                Type::Result {
                    ok: Box::new(Type::Bool),
                    err: Box::new(Type::Str),
                },
            )
            .param("message", Type::Str)
            .param("default_yes", Type::Bool),
            |args| match args {
                [Value::String(message), Value::Bool(default_yes)] => {
                    use std::io::Write as _;
                    let suffix = if *default_yes { " [Y/n] " } else { " [y/N] " };
                    eprint!("{message}{suffix}");
                    let _ = std::io::stderr().flush();
                    let mut line = String::new();
                    match std::io::stdin().read_line(&mut line) {
                        Ok(0) => Ok(Value::ok(Type::Bool, Type::Str, Value::Bool(*default_yes))),
                        Ok(_) => {
                            let trimmed = line.trim().to_ascii_lowercase();
                            let answer = if trimmed.is_empty() {
                                *default_yes
                            } else {
                                matches!(trimmed.as_str(), "y" | "yes" | "true" | "1")
                            };
                            Ok(Value::ok(Type::Bool, Type::Str, Value::Bool(answer)))
                        }
                        Err(err) => Ok(Value::err(
                            Type::Bool,
                            Type::Str,
                            Value::string(format!("stdin read failed: {err}")),
                        )),
                    }
                }
                _ => unreachable!("static checker guarantees term.confirm argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_term_select",
                Type::Result {
                    ok: Box::new(Type::Int),
                    err: Box::new(Type::Str),
                },
            )
            .param("message", Type::Str)
            .param("items", Type::Array(Box::new(Type::Str)))
            .param("default_index", Type::Int),
            |args| match args {
                [Value::String(message), Value::Array(items), Value::Int(default_index)] => {
                    use std::io::Write as _;
                    let snapshot = items.snapshot();
                    if snapshot.is_empty() {
                        return Ok(Value::err(
                            Type::Int,
                            Type::Str,
                            Value::string("term.select requires at least one item"),
                        ));
                    }
                    let default_idx = *default_index;
                    let in_range = default_idx >= 0 && (default_idx as usize) < snapshot.len();
                    eprintln!("{message}");
                    for (idx, item) in snapshot.iter().enumerate() {
                        let Value::String(text) = item else {
                            return Err(Diagnostic::new(
                                "term.select items must be strings",
                                Span { start: 0, end: 0 },
                            ));
                        };
                        let marker = if in_range && idx == default_idx as usize {
                            "*"
                        } else {
                            " "
                        };
                        eprintln!("  {marker} {}) {}", idx + 1, text.as_ref());
                    }
                    let prompt = if in_range {
                        format!("Enter 1..{} [{}]: ", snapshot.len(), default_idx + 1)
                    } else {
                        format!("Enter 1..{}: ", snapshot.len())
                    };
                    eprint!("{prompt}");
                    let _ = std::io::stderr().flush();
                    let mut line = String::new();
                    match std::io::stdin().read_line(&mut line) {
                        Ok(0) => {
                            if in_range {
                                Ok(Value::ok(Type::Int, Type::Str, Value::Int(default_idx)))
                            } else {
                                Ok(Value::err(
                                    Type::Int,
                                    Type::Str,
                                    Value::string("stdin reached EOF without default"),
                                ))
                            }
                        }
                        Ok(_) => {
                            let trimmed = line.trim();
                            if trimmed.is_empty() {
                                return if in_range {
                                    Ok(Value::ok(Type::Int, Type::Str, Value::Int(default_idx)))
                                } else {
                                    Ok(Value::err(
                                        Type::Int,
                                        Type::Str,
                                        Value::string("empty input without default"),
                                    ))
                                };
                            }
                            match trimmed.parse::<i64>() {
                                Ok(num) if num >= 1 && (num as usize) <= snapshot.len() => {
                                    Ok(Value::ok(Type::Int, Type::Str, Value::Int(num - 1)))
                                }
                                Ok(num) => Ok(Value::err(
                                    Type::Int,
                                    Type::Str,
                                    Value::string(format!(
                                        "selection {num} out of range 1..{}",
                                        snapshot.len()
                                    )),
                                )),
                                Err(err) => Ok(Value::err(
                                    Type::Int,
                                    Type::Str,
                                    Value::string(format!("invalid selection '{trimmed}': {err}")),
                                )),
                            }
                        }
                        Err(err) => Ok(Value::err(
                            Type::Int,
                            Type::Str,
                            Value::string(format!("stdin read failed: {err}")),
                        )),
                    }
                }
                _ => unreachable!("static checker guarantees term.select argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_term_progress", Type::Str)
                .param("current", Type::Int)
                .param("total", Type::Int)
                .param("width", Type::Int),
            |args| match args {
                [Value::Int(current), Value::Int(total), Value::Int(width)] => {
                    if *width < 0 {
                        return Err(Diagnostic::new(
                            "term.progress width must be non-negative",
                            Span { start: 0, end: 0 },
                        ));
                    }
                    let width = *width as usize;
                    if width == 0 {
                        return Ok(Value::string(format!(
                            "[] {}/{}",
                            current.max(&0),
                            total.max(&0)
                        )));
                    }
                    let total_clamped = (*total).max(0) as u64;
                    let current_clamped = (*current).max(0).min(*total) as u64;
                    let filled = if total_clamped == 0 {
                        0
                    } else {
                        ((current_clamped as u128 * width as u128) / total_clamped as u128) as usize
                    };
                    let filled = filled.min(width);
                    let mut bar = String::with_capacity(width + 8);
                    bar.push('[');
                    for _ in 0..filled {
                        bar.push('#');
                    }
                    for _ in filled..width {
                        bar.push('-');
                    }
                    bar.push(']');
                    let percent = if total_clamped == 0 {
                        0
                    } else {
                        ((current_clamped as u128 * 100) / total_clamped as u128) as u64
                    };
                    bar.push_str(&format!(
                        " {}/{} ({}%)",
                        current_clamped, total_clamped, percent
                    ));
                    Ok(Value::string(bar))
                }
                _ => unreachable!("static checker guarantees term.progress argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_term_prompt_password",
                Type::Result {
                    ok: Box::new(Type::Str),
                    err: Box::new(Type::Str),
                },
            )
            .param("message", Type::Str),
            |args| match args {
                [Value::String(message)] => term_prompt_password(message.as_ref()),
                _ => unreachable!("static checker guarantees term.prompt_password argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn term_prompt_password(message: &str) -> Result<Value, Diagnostic> {
    use std::io::Write as _;

    eprint!("{message}");
    let _ = std::io::stderr().flush();

    #[cfg(all(unix, target_os = "linux"))]
    let echo_guard = match disable_terminal_echo(0) {
        Ok(guard) => guard,
        Err(err) => {
            eprintln!();
            return Ok(Value::err(
                Type::Str,
                Type::Str,
                Value::string(format!(
                    "term.prompt-password.echo-disable-failed: could not disable terminal echo via termios: {err}"
                )),
            ));
        }
    };
    #[cfg(not(all(unix, target_os = "linux")))]
    {
        eprintln!();
        return Ok(Value::err(
            Type::Str,
            Type::Str,
            Value::string(
                "term.prompt-password.echo-disable-failed: terminal echo control is not supported on this platform",
            ),
        ));
    }

    #[cfg(all(unix, target_os = "linux"))]
    {
        let mut line = String::new();
        let read_outcome = std::io::stdin().read_line(&mut line);

        drop(echo_guard);
        eprintln!();

        match read_outcome {
            Ok(0) => Ok(Value::err(
                Type::Str,
                Type::Str,
                Value::string("term.prompt-password.eof: stdin reached EOF"),
            )),
            Ok(_) => {
                let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
                Ok(Value::ok(Type::Str, Type::Str, Value::string(trimmed)))
            }
            Err(err) => Ok(Value::err(
                Type::Str,
                Type::Str,
                Value::string(format!(
                    "term.prompt-password.read-failed: stdin read failed: {err}"
                )),
            )),
        }
    }
}

const PROCESS_RUN_MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

struct ProcessRunSlot {
    active: Rc<RefCell<usize>>,
}

impl Drop for ProcessRunSlot {
    fn drop(&mut self) {
        let mut active = self.active.borrow_mut();
        *active = active.saturating_sub(1);
    }
}

fn acquire_process_run_slot(
    active: Rc<RefCell<usize>>,
    max_concurrent: Option<usize>,
) -> Result<ProcessRunSlot, String> {
    if let Some(limit) = max_concurrent {
        let mut active_count = active.borrow_mut();
        if *active_count >= limit {
            return Err(format!(
                "process_run.concurrent-limit: process_run concurrent process limit {limit} reached"
            ));
        }
        *active_count += 1;
        drop(active_count);
        Ok(ProcessRunSlot { active })
    } else {
        *active.borrow_mut() += 1;
        Ok(ProcessRunSlot { active })
    }
}

fn register_process_run_stdlib(
    engine: &mut Engine,
    permissions: &RuntimePermissions,
    process_run_active: Rc<RefCell<usize>>,
) {
    let process_run_allowed = permissions.process_run;
    let process_run_allowlist = permissions.process_run_allowlist.clone();
    let process_run_max_concurrent = permissions.process_run_max_concurrent;
    let process_run_active_run = process_run_active.clone();
    let response_type = Type::Tuple(vec![Type::Int, Type::Str, Type::Str]);
    let response_type_run = response_type.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_process_run",
                Type::Result {
                    ok: Box::new(response_type),
                    err: Box::new(Type::Str),
                },
            )
            .param("program", Type::Str)
            .param("args", Type::Array(Box::new(Type::Str)))
            .param("stdin", Type::Str)
            .param("timeout_ms", Type::Int),
            move |args| {
                if !process_run_allowed {
                    return Err(call_capability_required("process run", "process.run"));
                }
                match args {
                    [
                        Value::String(program),
                        Value::Array(arg_array),
                        Value::String(stdin_text),
                        Value::Int(timeout_ms),
                    ] => {
                        if !process_run_allowlist.is_empty()
                            && !process_run_allowlist.iter().any(|p| p == program.as_ref())
                        {
                            return Ok(Value::err(
                                response_type_run.clone(),
                                Type::Str,
                                Value::string(format!(
                                    "process_run.allowlist-denied: program '{program}' is not in the process_run allowlist"
                                )),
                            ));
                        }
                        let arg_snapshot = arg_array.snapshot();
                        let mut arg_strings: Vec<String> = Vec::with_capacity(arg_snapshot.len());
                        for arg in &arg_snapshot {
                            match arg {
                                Value::String(s) => arg_strings.push(s.as_ref().to_string()),
                                _ => unreachable!(
                                    "static checker guarantees process.run args are [str]"
                                ),
                            }
                        }
                        let _slot = match acquire_process_run_slot(
                            process_run_active_run.clone(),
                            process_run_max_concurrent,
                        ) {
                            Ok(slot) => slot,
                            Err(message) => {
                                return Ok(Value::err(
                                    response_type_run.clone(),
                                    Type::Str,
                                    Value::string(message),
                                ));
                            }
                        };
                        match exec_process_run(
                            program.as_ref(),
                            &arg_strings,
                            stdin_text.as_ref(),
                            *timeout_ms,
                            None,
                            &[],
                        ) {
                            Ok((code, out, err_out)) => {
                                let tuple = Value::tuple(
                                    vec![Type::Int, Type::Str, Type::Str],
                                    vec![
                                        Value::Int(code),
                                        Value::string(out),
                                        Value::string(err_out),
                                    ],
                                );
                                Ok(Value::ok(response_type_run.clone(), Type::Str, tuple))
                            }
                            Err(message) => Ok(Value::err(
                                response_type_run.clone(),
                                Type::Str,
                                Value::string(message),
                            )),
                        }
                    }
                    _ => unreachable!("static checker guarantees process.run argument types"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let process_run_allowlist_with = permissions.process_run_allowlist.clone();
    let process_run_allowed_with = permissions.process_run;
    let process_run_max_concurrent_with = permissions.process_run_max_concurrent;
    let process_run_active_with = process_run_active;
    let response_type_run_with = Type::Tuple(vec![Type::Int, Type::Str, Type::Str]);
    let env_pair_type = Type::Tuple(vec![Type::Str, Type::Str]);
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_process_run_with",
                Type::Result {
                    ok: Box::new(response_type_run_with.clone()),
                    err: Box::new(Type::Str),
                },
            )
            .param("program", Type::Str)
            .param("args", Type::Array(Box::new(Type::Str)))
            .param("stdin", Type::Str)
            .param("timeout_ms", Type::Int)
            .param("cwd", Type::Str)
            .param("env_pairs", Type::Array(Box::new(env_pair_type))),
            move |args| {
                if !process_run_allowed_with {
                    return Err(call_capability_required("process run", "process.run"));
                }
                match args {
                    [
                        Value::String(program),
                        Value::Array(arg_array),
                        Value::String(stdin_text),
                        Value::Int(timeout_ms),
                        Value::String(cwd),
                        Value::Array(env_array),
                    ] => {
                        if !process_run_allowlist_with.is_empty()
                            && !process_run_allowlist_with
                                .iter()
                                .any(|p| p == program.as_ref())
                        {
                            return Ok(Value::err(
                                response_type_run_with.clone(),
                                Type::Str,
                                Value::string(format!(
                                    "process_run.allowlist-denied: program '{program}' is not in the process_run allowlist"
                                )),
                            ));
                        }
                        let arg_snapshot = arg_array.snapshot();
                        let mut arg_strings: Vec<String> = Vec::with_capacity(arg_snapshot.len());
                        for arg in &arg_snapshot {
                            match arg {
                                Value::String(s) => arg_strings.push(s.as_ref().to_string()),
                                _ => unreachable!(
                                    "static checker guarantees process.run args are [str]"
                                ),
                            }
                        }
                        let env_snapshot = env_array.snapshot();
                        let mut env_pairs: Vec<(String, String)> =
                            Vec::with_capacity(env_snapshot.len());
                        for entry in &env_snapshot {
                            match entry {
                                Value::Tuple(tuple) => {
                                    let elements = tuple.elements();
                                    match (elements.first(), elements.get(1)) {
                                        (Some(Value::String(key)), Some(Value::String(value))) => {
                                            env_pairs.push((
                                                key.as_ref().to_string(),
                                                value.as_ref().to_string(),
                                            ));
                                        }
                                        _ => unreachable!(
                                            "static checker guarantees env_pairs entries are (str, str)"
                                        ),
                                    }
                                }
                                _ => unreachable!(
                                    "static checker guarantees env_pairs is [(str, str)]"
                                ),
                            }
                        }
                        let cwd_opt: Option<&str> = if cwd.as_ref().is_empty() {
                            None
                        } else {
                            Some(cwd.as_ref())
                        };
                        let _slot = match acquire_process_run_slot(
                            process_run_active_with.clone(),
                            process_run_max_concurrent_with,
                        ) {
                            Ok(slot) => slot,
                            Err(message) => {
                                return Ok(Value::err(
                                    response_type_run_with.clone(),
                                    Type::Str,
                                    Value::string(message),
                                ));
                            }
                        };
                        match exec_process_run(
                            program.as_ref(),
                            &arg_strings,
                            stdin_text.as_ref(),
                            *timeout_ms,
                            cwd_opt,
                            &env_pairs,
                        ) {
                            Ok((code, out, err_out)) => {
                                let tuple = Value::tuple(
                                    vec![Type::Int, Type::Str, Type::Str],
                                    vec![
                                        Value::Int(code),
                                        Value::string(out),
                                        Value::string(err_out),
                                    ],
                                );
                                Ok(Value::ok(response_type_run_with.clone(), Type::Str, tuple))
                            }
                            Err(message) => Ok(Value::err(
                                response_type_run_with.clone(),
                                Type::Str,
                                Value::string(message),
                            )),
                        }
                    }
                    _ => unreachable!(
                        "static checker guarantees process.run_with argument types"
                    ),
                }
            },
        )
        .expect("stdlib function registration is static");
}

fn exec_process_run(
    program: &str,
    args: &[String],
    stdin_text: &str,
    timeout_ms: i64,
    cwd: Option<&str>,
    env_pairs: &[(String, String)],
) -> Result<(i64, String, String), String> {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};

    let mut command = Command::new(program);
    command.args(args);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    for (key, value) in env_pairs {
        if value == "<unset>" {
            command.env_remove(key);
        } else {
            command.env(key, value);
        }
    }

    let mut child = command
        .spawn()
        .map_err(|err| format!("process_run.spawn-failed: failed to spawn '{program}': {err}"))?;

    if !stdin_text.is_empty() {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(stdin_text.as_bytes()).map_err(|err| {
                format!("process_run.stdin-write-failed: write stdin failed: {err}")
            })?;
        }
    }
    drop(child.stdin.take());

    let deadline = if timeout_ms > 0 {
        Some(Instant::now() + Duration::from_millis(timeout_ms as u64))
    } else {
        None
    };

    let mut stdout_buf: Vec<u8> = Vec::new();
    let mut stderr_buf: Vec<u8> = Vec::new();
    let mut child_stdout = child.stdout.take();
    let mut child_stderr = child.stderr.take();
    let mut tmp = [0u8; 4096];

    loop {
        if let Some(stdout) = child_stdout.as_mut() {
            match stdout.read(&mut tmp) {
                Ok(0) => {
                    child_stdout = None;
                }
                Ok(n) => {
                    if stdout_buf.len() + n > PROCESS_RUN_MAX_OUTPUT_BYTES {
                        let _ = child.kill();
                        return Err(format!(
                            "process_run.output-cap-stdout: stdout exceeded {} byte cap",
                            PROCESS_RUN_MAX_OUTPUT_BYTES
                        ));
                    }
                    stdout_buf.extend_from_slice(&tmp[..n]);
                }
                Err(_) => {
                    child_stdout = None;
                }
            }
        }
        if let Some(stderr) = child_stderr.as_mut() {
            match stderr.read(&mut tmp) {
                Ok(0) => {
                    child_stderr = None;
                }
                Ok(n) => {
                    if stderr_buf.len() + n > PROCESS_RUN_MAX_OUTPUT_BYTES {
                        let _ = child.kill();
                        return Err(format!(
                            "process_run.output-cap-stderr: stderr exceeded {} byte cap",
                            PROCESS_RUN_MAX_OUTPUT_BYTES
                        ));
                    }
                    stderr_buf.extend_from_slice(&tmp[..n]);
                }
                Err(_) => {
                    child_stderr = None;
                }
            }
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some(mut stdout) = child_stdout.take() {
                    let _ = stdout.read_to_end(&mut stdout_buf);
                }
                if let Some(mut stderr) = child_stderr.take() {
                    let _ = stderr.read_to_end(&mut stderr_buf);
                }
                if stdout_buf.len() > PROCESS_RUN_MAX_OUTPUT_BYTES {
                    return Err(format!(
                        "process_run.output-cap-stdout: stdout exceeded {} byte cap",
                        PROCESS_RUN_MAX_OUTPUT_BYTES
                    ));
                }
                if stderr_buf.len() > PROCESS_RUN_MAX_OUTPUT_BYTES {
                    return Err(format!(
                        "process_run.output-cap-stderr: stderr exceeded {} byte cap",
                        PROCESS_RUN_MAX_OUTPUT_BYTES
                    ));
                }
                return Ok((
                    status.code().unwrap_or(-1) as i64,
                    String::from_utf8_lossy(&stdout_buf).to_string(),
                    String::from_utf8_lossy(&stderr_buf).to_string(),
                ));
            }
            Ok(None) => {
                if let Some(deadline) = deadline {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        return Err(format!(
                            "process_run.timeout: process timed out after {timeout_ms}ms"
                        ));
                    }
                }
                if child_stdout.is_none() && child_stderr.is_none() {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            Err(err) => {
                let _ = child.kill();
                return Err(format!("process_run.wait-failed: wait failed: {err}"));
            }
        }
    }
}

fn register_hash_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_hash_sha256_hex", Type::Str)
                .param("bytes", Type::Array(Box::new(Type::Int))),
            |args| match args {
                [Value::Array(bytes)] => match bytes_array_to_vec(bytes) {
                    Ok(bytes) => Ok(Value::string(sha256_hex_bytes(&bytes))),
                    Err(message) => Err(string_argument_error_owned(message)),
                },
                _ => unreachable!("static checker guarantees hash.sha256_hex argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_hash_sha256_text", Type::Str)
                .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => Ok(Value::string(sha256_hex_bytes(value.as_bytes()))),
                _ => unreachable!("static checker guarantees hash.sha256_text argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_hash_hmac_sha256_hex", Type::Str)
                .param("key", Type::Array(Box::new(Type::Int)))
                .param("bytes", Type::Array(Box::new(Type::Int))),
            |args| match args {
                [Value::Array(key), Value::Array(bytes)] => {
                    let key = bytes_array_to_vec(key).map_err(string_argument_error_owned)?;
                    let bytes = bytes_array_to_vec(bytes).map_err(string_argument_error_owned)?;
                    Ok(Value::string(hmac_sha256_hex_bytes(&key, &bytes)))
                }
                _ => unreachable!("static checker guarantees hash.hmac_sha256_hex argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_hash_hmac_sha256_text", Type::Str)
                .param("key", Type::Str)
                .param("value", Type::Str),
            |args| match args {
                [Value::String(key), Value::String(value)] => Ok(Value::string(
                    hmac_sha256_hex_bytes(key.as_bytes(), value.as_bytes()),
                )),
                _ => {
                    unreachable!("static checker guarantees hash.hmac_sha256_text argument types")
                }
            },
        )
        .expect("stdlib function registration is static");
}

fn register_encoding_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_encoding_base64_encode", Type::Str)
                .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => Ok(Value::string(base64_encode_bytes(value.as_bytes()))),
                _ => unreachable!("static checker guarantees base64_encode argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_encoding_base64_decode",
                Type::Result {
                    ok: Box::new(Type::Str),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => match base64_decode_bytes(value.as_ref()) {
                    Ok(bytes) => match String::from_utf8(bytes) {
                        Ok(text) => Ok(Value::ok(Type::Str, Type::Str, Value::string(text))),
                        Err(_) => Ok(Value::err(
                            Type::Str,
                            Type::Str,
                            Value::string("decoded bytes are not valid UTF-8"),
                        )),
                    },
                    Err(message) => Ok(Value::err(Type::Str, Type::Str, Value::string(message))),
                },
                _ => unreachable!("static checker guarantees base64_decode argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_encoding_hex_encode", Type::Str)
                .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => Ok(Value::string(hex_encode_bytes(value.as_bytes()))),
                _ => unreachable!("static checker guarantees hex_encode argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_encoding_hex_decode",
                Type::Result {
                    ok: Box::new(Type::Str),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => match hex_decode_bytes(value.as_ref()) {
                    Ok(bytes) => match String::from_utf8(bytes) {
                        Ok(text) => Ok(Value::ok(Type::Str, Type::Str, Value::string(text))),
                        Err(_) => Ok(Value::err(
                            Type::Str,
                            Type::Str,
                            Value::string("decoded bytes are not valid UTF-8"),
                        )),
                    },
                    Err(message) => Ok(Value::err(Type::Str, Type::Str, Value::string(message))),
                },
                _ => unreachable!("static checker guarantees hex_decode argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn parse_dotenv(source: &str) -> Result<BTreeMap<String, Value>, String> {
    let mut entries = BTreeMap::new();
    for (lineno, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let working = trimmed
            .strip_prefix("export ")
            .map(str::trim_start)
            .unwrap_or(trimmed);
        let Some(eq_idx) = working.find('=') else {
            return Err(format!("line {}: missing '=' separator", lineno + 1));
        };
        let key = working[..eq_idx].trim();
        if key.is_empty() {
            return Err(format!("line {}: empty key", lineno + 1));
        }
        if !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        {
            return Err(format!(
                "line {}: invalid characters in key '{}'",
                lineno + 1,
                key
            ));
        }
        let raw_value = working[eq_idx + 1..].trim();
        let is_double_quoted =
            raw_value.starts_with('"') && raw_value.ends_with('"') && raw_value.len() >= 2;
        let is_single_quoted =
            raw_value.starts_with('\'') && raw_value.ends_with('\'') && raw_value.len() >= 2;
        let value = if is_double_quoted || is_single_quoted {
            raw_value[1..raw_value.len() - 1].to_string()
        } else {
            match raw_value.find('#') {
                Some(idx) => raw_value[..idx].trim_end().to_string(),
                None => raw_value.to_string(),
            }
        };
        entries.insert(key.to_string(), Value::string(value));
    }
    Ok(entries)
}

fn register_dotenv_stdlib(engine: &mut Engine) {
    let map_type = Type::Map(Box::new(Type::Str));
    let ok_type = map_type.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_dotenv_parse",
                Type::Result {
                    ok: Box::new(ok_type),
                    err: Box::new(Type::Str),
                },
            )
            .param("source", Type::Str),
            move |args| match args {
                [Value::String(source)] => match parse_dotenv(source.as_ref()) {
                    Ok(entries) => {
                        let map_value = Value::map(Type::Str, entries);
                        Ok(Value::ok(map_type.clone(), Type::Str, map_value))
                    }
                    Err(message) => Ok(Value::err(
                        map_type.clone(),
                        Type::Str,
                        Value::string(message),
                    )),
                },
                _ => unreachable!("static checker guarantees dotenv.parse argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn parse_ini(source: &str) -> Result<BTreeMap<String, Value>, String> {
    let mut sections: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    let mut current_section = String::new();
    sections.insert(current_section.clone(), BTreeMap::new());

    for (lineno, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if trimmed.starts_with('[') {
            if !trimmed.ends_with(']') {
                return Err(format!("line {}: unterminated section header", lineno + 1));
            }
            let section = trimmed[1..trimmed.len() - 1].trim();
            if section.is_empty() {
                return Err(format!("line {}: empty section name", lineno + 1));
            }
            current_section = section.to_string();
            sections.entry(current_section.clone()).or_default();
            continue;
        }

        let sep = match (trimmed.find('='), trimmed.find(':')) {
            (Some(eq), Some(colon)) => Some(eq.min(colon)),
            (Some(eq), None) => Some(eq),
            (None, Some(colon)) => Some(colon),
            (None, None) => None,
        };
        let Some(sep) = sep else {
            return Err(format!("line {}: missing key/value separator", lineno + 1));
        };
        let key = trimmed[..sep].trim();
        if key.is_empty() {
            return Err(format!("line {}: empty key", lineno + 1));
        }
        if !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return Err(format!(
                "line {}: invalid characters in key '{}'",
                lineno + 1,
                key
            ));
        }
        let raw_value = trimmed[sep + 1..].trim();
        let value = parse_ini_value(raw_value);
        sections
            .entry(current_section.clone())
            .or_default()
            .insert(key.to_string(), Value::string(value));
    }

    Ok(sections
        .into_iter()
        .map(|(name, entries)| (name, Value::map(Type::Str, entries)))
        .collect())
}

fn parse_ini_value(raw_value: &str) -> String {
    let is_double_quoted =
        raw_value.starts_with('"') && raw_value.ends_with('"') && raw_value.len() >= 2;
    let is_single_quoted =
        raw_value.starts_with('\'') && raw_value.ends_with('\'') && raw_value.len() >= 2;
    if is_double_quoted || is_single_quoted {
        raw_value[1..raw_value.len() - 1].to_string()
    } else {
        let comment = raw_value
            .find('#')
            .into_iter()
            .chain(raw_value.find(';'))
            .min();
        match comment {
            Some(idx) => raw_value[..idx].trim_end().to_string(),
            None => raw_value.to_string(),
        }
    }
}

fn register_ini_stdlib(engine: &mut Engine) {
    let section_type = Type::Map(Box::new(Type::Str));
    let map_type = Type::Map(Box::new(section_type.clone()));
    let ok_type = map_type.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_ini_parse",
                Type::Result {
                    ok: Box::new(ok_type),
                    err: Box::new(Type::Str),
                },
            )
            .param("source", Type::Str),
            move |args| match args {
                [Value::String(source)] => match parse_ini(source.as_ref()) {
                    Ok(entries) => {
                        let map_value = Value::map(section_type.clone(), entries);
                        Ok(Value::ok(map_type.clone(), Type::Str, map_value))
                    }
                    Err(message) => Ok(Value::err(
                        map_type.clone(),
                        Type::Str,
                        Value::string(message),
                    )),
                },
                _ => unreachable!("static checker guarantees ini.parse argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn parse_toml(source: &str) -> Result<JsonValue, String> {
    let mut root = BTreeMap::new();
    let mut section: Vec<String> = Vec::new();

    for (lineno, line) in source.lines().enumerate() {
        let line_no = lineno + 1;
        let trimmed = strip_toml_comment(line).trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('[') {
            if !trimmed.ends_with(']') || trimmed.starts_with("[[") {
                return Err(format!("line {line_no}: unsupported TOML section header"));
            }
            section = parse_toml_key_path(trimmed[1..trimmed.len() - 1].trim(), line_no)?;
            let _ = toml_table_mut(&mut root, &section, line_no)?;
            continue;
        }
        let Some(eq_idx) = find_toml_separator(&trimmed, '=') else {
            return Err(format!("line {line_no}: missing '=' separator"));
        };
        let key_path = parse_toml_key_path(trimmed[..eq_idx].trim(), line_no)?;
        if key_path.is_empty() {
            return Err(format!("line {line_no}: empty key"));
        }
        let value = parse_toml_value(trimmed[eq_idx + 1..].trim(), line_no)?;
        let (parents, key) = key_path.split_at(key_path.len() - 1);
        let mut full_path = section.clone();
        full_path.extend_from_slice(parents);
        let table = toml_table_mut(&mut root, &full_path, line_no)?;
        if table.insert(key[0].clone(), value).is_some() {
            return Err(format!("line {line_no}: duplicate key '{}'", key[0]));
        }
    }

    Ok(JsonValue::Object(root))
}

fn strip_toml_comment(line: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in line.chars() {
        if let Some(q) = quote {
            out.push(ch);
            if q == '"' && ch == '\\' && !escaped {
                escaped = true;
                continue;
            }
            if ch == q && !escaped {
                quote = None;
            }
            escaped = false;
            continue;
        }
        if ch == '#' {
            break;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
        }
        out.push(ch);
    }
    out
}

fn find_toml_separator(line: &str, needle: char) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if let Some(q) = quote {
            if q == '"' && ch == '\\' && !escaped {
                escaped = true;
                continue;
            }
            if ch == q && !escaped {
                quote = None;
            }
            escaped = false;
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch == needle {
            return Some(idx);
        }
    }
    None
}

fn parse_toml_key_path(input: &str, line_no: usize) -> Result<Vec<String>, String> {
    if input.is_empty() {
        return Err(format!("line {line_no}: empty key"));
    }
    let mut parts = Vec::new();
    for raw in input.split('.') {
        let part = raw.trim();
        if part.is_empty() {
            return Err(format!("line {line_no}: empty dotted key segment"));
        }
        if !part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(format!(
                "line {line_no}: invalid characters in key segment '{part}'"
            ));
        }
        parts.push(part.to_string());
    }
    Ok(parts)
}

fn toml_table_mut<'a>(
    table: &'a mut BTreeMap<String, JsonValue>,
    path: &[String],
    line_no: usize,
) -> Result<&'a mut BTreeMap<String, JsonValue>, String> {
    let Some((head, rest)) = path.split_first() else {
        return Ok(table);
    };
    let entry = table
        .entry(head.clone())
        .or_insert_with(|| JsonValue::Object(BTreeMap::new()));
    match entry {
        JsonValue::Object(child) => toml_table_mut(child, rest, line_no),
        _ => Err(format!(
            "line {line_no}: key '{head}' already exists and is not a table"
        )),
    }
}

fn parse_toml_value(input: &str, line_no: usize) -> Result<JsonValue, String> {
    if input.is_empty() {
        return Err(format!("line {line_no}: empty value"));
    }
    if input.starts_with('"') || input.starts_with('\'') {
        return parse_toml_string(input, line_no).map(JsonValue::String);
    }
    if input == "true" {
        return Ok(JsonValue::Bool(true));
    }
    if input == "false" {
        return Ok(JsonValue::Bool(false));
    }
    if input.starts_with('[') {
        return parse_toml_array(input, line_no);
    }
    let normalized = input.replace('_', "");
    if normalized.contains('.') || normalized.contains('e') || normalized.contains('E') {
        let value: f64 = normalized
            .parse()
            .map_err(|_| format!("line {line_no}: invalid TOML number '{input}'"))?;
        if value.is_finite() {
            return Ok(JsonValue::Number(value));
        }
        return Err(format!("line {line_no}: TOML number is not finite"));
    }
    let value: i64 = normalized
        .parse()
        .map_err(|_| format!("line {line_no}: unsupported TOML value '{input}'"))?;
    Ok(JsonValue::Number(value as f64))
}

fn parse_toml_string(input: &str, line_no: usize) -> Result<String, String> {
    let quote = input
        .chars()
        .next()
        .ok_or_else(|| format!("line {line_no}: empty string value"))?;
    if !input.ends_with(quote) || input.len() < 2 {
        return Err(format!("line {line_no}: unterminated string"));
    }
    let inner = &input[1..input.len() - 1];
    if quote == '\'' {
        return Ok(inner.to_string());
    }
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(escaped) = chars.next() else {
            return Err(format!("line {line_no}: trailing string escape"));
        };
        match escaped {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            other => {
                return Err(format!(
                    "line {line_no}: unsupported string escape '\\{other}'"
                ));
            }
        }
    }
    Ok(out)
}

fn parse_toml_array(input: &str, line_no: usize) -> Result<JsonValue, String> {
    if !input.ends_with(']') {
        return Err(format!("line {line_no}: unterminated array"));
    }
    let inner = &input[1..input.len() - 1];
    if inner.trim().is_empty() {
        return Ok(JsonValue::Array(Vec::new()));
    }
    let mut items = Vec::new();
    for item in split_toml_array_items(inner, line_no)? {
        items.push(parse_toml_value(item.trim(), line_no)?);
    }
    Ok(JsonValue::Array(items))
}

fn split_toml_array_items(input: &str, line_no: usize) -> Result<Vec<String>, String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut depth = 0usize;
    for ch in input.chars() {
        if let Some(q) = quote {
            current.push(ch);
            if q == '"' && ch == '\\' && !escaped {
                escaped = true;
                continue;
            }
            if ch == q && !escaped {
                quote = None;
            }
            escaped = false;
            continue;
        }
        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                current.push(ch);
            }
            '[' => {
                depth += 1;
                current.push(ch);
            }
            ']' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| format!("line {line_no}: unmatched ']' inside TOML array"))?;
                current.push(ch);
            }
            ',' if depth == 0 => {
                items.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if quote.is_some() {
        return Err(format!("line {line_no}: unterminated string in array"));
    }
    if !current.trim().is_empty() {
        items.push(current.trim().to_string());
    }
    Ok(items)
}

fn register_toml_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_toml_parse",
                Type::Result {
                    ok: Box::new(Type::Json),
                    err: Box::new(Type::Str),
                },
            )
            .param("source", Type::Str),
            |args| match args {
                [Value::String(source)] => match parse_toml(source.as_ref()) {
                    Ok(value) => Ok(Value::ok(Type::Json, Type::Str, Value::json(value))),
                    Err(message) => Ok(Value::err(Type::Json, Type::Str, Value::string(message))),
                },
                _ => unreachable!("static checker guarantees toml.parse argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

#[derive(Debug, Clone)]
struct YamlLine {
    line_no: usize,
    indent: usize,
    content: String,
}

fn parse_yaml(source: &str) -> Result<JsonValue, String> {
    let lines = preprocess_yaml_lines(source)?;
    if lines.is_empty() {
        return Ok(JsonValue::Null);
    }
    if lines[0].indent != 0 {
        return Err(format!(
            "line {}: YAML document must start at indentation 0",
            lines[0].line_no
        ));
    }

    let mut index = 0usize;
    let value = parse_yaml_block(&lines, &mut index, lines[0].indent)?;
    if index != lines.len() {
        return Err(format!(
            "line {}: unexpected content after YAML document",
            lines[index].line_no
        ));
    }
    Ok(value)
}

fn preprocess_yaml_lines(source: &str) -> Result<Vec<YamlLine>, String> {
    let mut lines = Vec::new();
    let mut ended = false;
    for (offset, raw) in source.lines().enumerate() {
        let line_no = offset + 1;
        let indent = raw.chars().take_while(|ch| *ch == ' ').count();
        if raw
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .any(|ch| ch == '\t')
        {
            return Err(format!(
                "line {line_no}: tabs are not allowed in indentation"
            ));
        }

        let trimmed = strip_yaml_comment(&raw[indent..], line_no)?
            .trim()
            .to_string();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "---" {
            if !lines.is_empty() {
                return Err(format!(
                    "line {line_no}: multiple YAML documents are unsupported"
                ));
            }
            continue;
        }
        if trimmed == "..." {
            ended = true;
            continue;
        }
        if ended {
            return Err(format!(
                "line {line_no}: content after YAML document end marker"
            ));
        }
        lines.push(YamlLine {
            line_no,
            indent,
            content: trimmed,
        });
    }
    Ok(lines)
}

fn strip_yaml_comment(input: &str, line_no: usize) -> Result<String, String> {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in input.chars() {
        if let Some(q) = quote {
            out.push(ch);
            if q == '"' && ch == '\\' && !escaped {
                escaped = true;
                continue;
            }
            if ch == q && !escaped {
                quote = None;
            }
            escaped = false;
            continue;
        }
        if ch == '#' {
            break;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
        }
        out.push(ch);
    }
    if quote.is_some() {
        return Err(format!("line {line_no}: unterminated quoted scalar"));
    }
    Ok(out)
}

fn parse_yaml_block(
    lines: &[YamlLine],
    index: &mut usize,
    indent: usize,
) -> Result<JsonValue, String> {
    let Some(line) = lines.get(*index) else {
        return Ok(JsonValue::Null);
    };
    if line.indent != indent {
        return Err(format!(
            "line {}: expected indentation {indent}, got {}",
            line.line_no, line.indent
        ));
    }
    if is_yaml_sequence_item(&line.content) {
        return parse_yaml_sequence(lines, index, indent);
    }
    if find_yaml_separator(&line.content, ':').is_some() {
        return parse_yaml_mapping(lines, index, indent);
    }

    *index += 1;
    parse_yaml_scalar(&line.content, line.line_no)
}

fn parse_yaml_mapping(
    lines: &[YamlLine],
    index: &mut usize,
    indent: usize,
) -> Result<JsonValue, String> {
    let mut entries = BTreeMap::new();
    while let Some(line) = lines.get(*index) {
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(format!(
                "line {}: unexpected indentation {}; expected {indent}",
                line.line_no, line.indent
            ));
        }
        if is_yaml_sequence_item(&line.content) {
            return Err(format!(
                "line {}: cannot mix YAML sequence items into a mapping block",
                line.line_no
            ));
        }

        let Some(separator) = find_yaml_separator(&line.content, ':') else {
            return Err(format!("line {}: missing ':' separator", line.line_no));
        };
        let key = parse_yaml_key(line.content[..separator].trim(), line.line_no)?;
        if entries.contains_key(&key) {
            return Err(format!("line {}: duplicate key '{}'", line.line_no, key));
        }
        let rest = line.content[separator + 1..].trim();
        if rest.is_empty() {
            *index += 1;
            let value = if let Some(next) = lines.get(*index) {
                if next.indent > indent {
                    parse_yaml_block(lines, index, next.indent)?
                } else {
                    JsonValue::Null
                }
            } else {
                JsonValue::Null
            };
            entries.insert(key, value);
        } else {
            entries.insert(key, parse_yaml_scalar(rest, line.line_no)?);
            *index += 1;
        }
    }
    Ok(JsonValue::Object(entries))
}

fn parse_yaml_sequence(
    lines: &[YamlLine],
    index: &mut usize,
    indent: usize,
) -> Result<JsonValue, String> {
    let mut items = Vec::new();
    while let Some(line) = lines.get(*index) {
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(format!(
                "line {}: unexpected indentation {}; expected {indent}",
                line.line_no, line.indent
            ));
        }
        if !is_yaml_sequence_item(&line.content) {
            return Err(format!(
                "line {}: cannot mix YAML mapping entries into a sequence block",
                line.line_no
            ));
        }

        let rest = if line.content == "-" {
            ""
        } else {
            line.content
                .strip_prefix("- ")
                .expect("sequence item prefix checked")
                .trim()
        };
        if rest.is_empty() {
            *index += 1;
            let value = if let Some(next) = lines.get(*index) {
                if next.indent > indent {
                    parse_yaml_block(lines, index, next.indent)?
                } else {
                    JsonValue::Null
                }
            } else {
                JsonValue::Null
            };
            items.push(value);
        } else {
            items.push(parse_yaml_scalar(rest, line.line_no)?);
            *index += 1;
        }
    }
    Ok(JsonValue::Array(items))
}

fn is_yaml_sequence_item(content: &str) -> bool {
    content == "-" || content.starts_with("- ")
}

fn find_yaml_separator(input: &str, needle: char) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (idx, ch) in input.char_indices() {
        if let Some(q) = quote {
            if q == '"' && ch == '\\' && !escaped {
                escaped = true;
                continue;
            }
            if ch == q && !escaped {
                quote = None;
            }
            escaped = false;
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch == needle {
            return Some(idx);
        }
    }
    None
}

fn parse_yaml_key(input: &str, line_no: usize) -> Result<String, String> {
    if input.is_empty() {
        return Err(format!("line {line_no}: empty key"));
    }
    let key = if input.starts_with('"') || input.starts_with('\'') {
        parse_yaml_quoted_string(input, line_no)?
    } else {
        input.to_string()
    };
    if key.is_empty() {
        return Err(format!("line {line_no}: empty key"));
    }
    Ok(key)
}

fn parse_yaml_scalar(input: &str, line_no: usize) -> Result<JsonValue, String> {
    if input.is_empty() || input == "null" || input == "~" {
        return Ok(JsonValue::Null);
    }
    if input.starts_with('"') || input.starts_with('\'') {
        return parse_yaml_quoted_string(input, line_no).map(JsonValue::String);
    }
    if input == "true" {
        return Ok(JsonValue::Bool(true));
    }
    if input == "false" {
        return Ok(JsonValue::Bool(false));
    }
    if input.starts_with('[') {
        return parse_yaml_inline_array(input, line_no);
    }
    if yaml_number_candidate(input) {
        let value: f64 = input
            .parse()
            .map_err(|_| format!("line {line_no}: invalid YAML number '{input}'"))?;
        if value.is_finite() {
            return Ok(JsonValue::Number(value));
        }
        return Err(format!("line {line_no}: YAML number is not finite"));
    }
    Ok(JsonValue::String(input.to_string()))
}

fn yaml_number_candidate(input: &str) -> bool {
    let mut has_digit = false;
    for ch in input.chars() {
        if ch.is_ascii_digit() {
            has_digit = true;
            continue;
        }
        if matches!(ch, '+' | '-' | '.' | 'e' | 'E') {
            continue;
        }
        return false;
    }
    has_digit
}

fn parse_yaml_quoted_string(input: &str, line_no: usize) -> Result<String, String> {
    let quote = input
        .chars()
        .next()
        .ok_or_else(|| format!("line {line_no}: empty quoted scalar"))?;
    if !input.ends_with(quote) || input.len() < 2 {
        return Err(format!("line {line_no}: unterminated quoted scalar"));
    }
    let inner = &input[1..input.len() - 1];
    if quote == '\'' {
        return Ok(inner.replace("''", "'"));
    }

    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(escaped) = chars.next() else {
            return Err(format!("line {line_no}: trailing string escape"));
        };
        match escaped {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            '/' => out.push('/'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            other => {
                return Err(format!(
                    "line {line_no}: unsupported string escape '\\{other}'"
                ));
            }
        }
    }
    Ok(out)
}

fn parse_yaml_inline_array(input: &str, line_no: usize) -> Result<JsonValue, String> {
    if !input.ends_with(']') {
        return Err(format!("line {line_no}: unterminated inline array"));
    }
    let inner = &input[1..input.len() - 1];
    if inner.trim().is_empty() {
        return Ok(JsonValue::Array(Vec::new()));
    }
    let mut items = Vec::new();
    for item in split_yaml_inline_items(inner, line_no)? {
        if item.trim().is_empty() {
            return Err(format!("line {line_no}: empty inline array item"));
        }
        items.push(parse_yaml_scalar(item.trim(), line_no)?);
    }
    Ok(JsonValue::Array(items))
}

fn split_yaml_inline_items(input: &str, line_no: usize) -> Result<Vec<String>, String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut depth = 0usize;
    for ch in input.chars() {
        if let Some(q) = quote {
            current.push(ch);
            if q == '"' && ch == '\\' && !escaped {
                escaped = true;
                continue;
            }
            if ch == q && !escaped {
                quote = None;
            }
            escaped = false;
            continue;
        }
        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                current.push(ch);
            }
            '[' => {
                depth += 1;
                current.push(ch);
            }
            ']' => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    format!("line {line_no}: unmatched ']' inside YAML inline array")
                })?;
                current.push(ch);
            }
            ',' if depth == 0 => {
                items.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if quote.is_some() {
        return Err(format!(
            "line {line_no}: unterminated string in inline array"
        ));
    }
    if !current.trim().is_empty() {
        items.push(current.trim().to_string());
    }
    Ok(items)
}

fn register_yaml_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_yaml_parse",
                Type::Result {
                    ok: Box::new(Type::Json),
                    err: Box::new(Type::Str),
                },
            )
            .param("source", Type::Str),
            |args| match args {
                [Value::String(source)] => match parse_yaml(source.as_ref()) {
                    Ok(value) => Ok(Value::ok(Type::Json, Type::Str, Value::json(value))),
                    Err(message) => Ok(Value::err(Type::Json, Type::Str, Value::string(message))),
                },
                _ => unreachable!("static checker guarantees yaml.parse argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn actual_preview(value: &str) -> String {
    const MAX: usize = 160;
    if value.chars().count() <= MAX {
        value.to_string()
    } else {
        let head: String = value.chars().take(MAX).collect();
        format!("{head}... [{} more chars]", value.chars().count() - MAX)
    }
}

fn register_test_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_test_fail", Type::Null)
                .param("label", Type::Str)
                .param("message", Type::Str),
            |args| match args {
                [Value::String(label), Value::String(message)] => Err(Diagnostic::new(
                    format!("[{label}] {message}"),
                    Span { start: 0, end: 0 },
                )
                .with_code("test.assertion-failed")),
                _ => unreachable!("static checker guarantees test.fail argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_test_assert_snapshot", Type::Null)
                .param("label", Type::Str)
                .param("actual", Type::Str)
                .param("expected", Type::Str),
            |args| match args {
                [Value::String(label), Value::String(actual), Value::String(expected)] => {
                    if actual == expected {
                        Ok(Value::Null)
                    } else {
                        let actual_text = actual_preview(actual.as_ref());
                        let expected_text = actual_preview(expected.as_ref());
                        Err(Diagnostic::new(
                            format!(
                                "[{label}] snapshot mismatch:\n  actual:   {actual_text}\n  expected: {expected_text}"
                            ),
                            Span { start: 0, end: 0 },
                        )
                        .with_code("test.assertion-failed"))
                    }
                }
                _ => unreachable!(
                    "static checker guarantees test.assert_snapshot argument types"
                ),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_test_assert_contains", Type::Null)
                .param("haystack", Type::Str)
                .param("needle", Type::Str)
                .param("label", Type::Str),
            |args| match args {
                [Value::String(haystack), Value::String(needle), Value::String(label)] => {
                    if haystack.contains(needle.as_ref()) {
                        Ok(Value::Null)
                    } else {
                        Err(Diagnostic::new(
                            format!(
                                "[{label}] assert_contains failed: '{needle}' not found in '{haystack}'"
                            ),
                            Span { start: 0, end: 0 },
                        )
                        .with_code("test.assertion-failed"))
                    }
                }
                _ => unreachable!(
                    "static checker guarantees test.assert_contains argument types"
                ),
            },
        )
        .expect("stdlib function registration is static");
}

fn register_time_analysis_stubs(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_now_unix", Type::Int),
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
            HostFunctionBuilder::new("__nox_std_time_now_unix_ms", Type::Int),
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
            HostFunctionBuilder::new("__nox_std_time_duration_ms", Type::Int)
                .param("start", Type::Int)
                .param("end", Type::Int),
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
            HostFunctionBuilder::new("__nox_std_time_format_unix", Type::Str)
                .param("ts", Type::Int)
                .param("fmt", Type::Str),
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
                "__nox_std_time_parse_unix",
                Type::Result {
                    ok: Box::new(Type::Int),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str)
            .param("fmt", Type::Str),
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
            HostFunctionBuilder::new("__nox_std_time_iso8601_format", Type::Str)
                .param("unix_seconds", Type::Int),
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
                "__nox_std_time_iso8601_parse",
                Type::Result {
                    ok: Box::new(Type::Int),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");

    for name in [
        "__nox_std_time_year_of",
        "__nox_std_time_month_of",
        "__nox_std_time_day_of",
        "__nox_std_time_weekday_of",
    ] {
        engine
            .register_host_function(
                HostFunctionBuilder::new(name, Type::Int).param("unix_seconds", Type::Int),
                |_| {
                    Err(Diagnostic::new(
                        "LSP stdlib stubs are only available for static analysis",
                        Span { start: 0, end: 0 },
                    ))
                },
            )
            .expect("stdlib function registration is static");
    }

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_add_months", Type::Int)
                .param("unix_seconds", Type::Int)
                .param("months", Type::Int),
            |_| {
                Err(Diagnostic::new(
                    "LSP stdlib stubs are only available for static analysis",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");
}

fn register_time_intrinsics_with_clock(
    engine: &mut Engine,
    mock_clock: Option<Rc<RefCell<Option<i64>>>>,
) {
    let last_ms = Rc::new(RefCell::new(0_i64));
    let now_unix_last = last_ms.clone();
    let mock_for_now_unix = mock_clock.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_now_unix", Type::Int),
            move |_| {
                if let Some(clock) = &mock_for_now_unix {
                    if let Some(value) = *clock.borrow() {
                        return Ok(Value::Int(value));
                    }
                }
                Ok(Value::Int(monotonic_unix_ms(&now_unix_last)? / 1000))
            },
        )
        .expect("stdlib function registration is static");
    let now_unix_ms_last = last_ms;
    let mock_for_now_unix_ms = mock_clock;
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_now_unix_ms", Type::Int),
            move |_| {
                if let Some(clock) = &mock_for_now_unix_ms {
                    if let Some(value) = *clock.borrow() {
                        return Ok(Value::Int(value.saturating_mul(1000)));
                    }
                }
                Ok(Value::Int(monotonic_unix_ms(&now_unix_ms_last)?))
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_duration_ms", Type::Int)
                .param("start", Type::Int)
                .param("end", Type::Int),
            |args| match args {
                [Value::Int(start), Value::Int(end)] => end
                    .checked_sub(*start)
                    .map(Value::Int)
                    .ok_or_else(|| time_argument_error("duration_ms result is out of range")),
                _ => unreachable!("static checker guarantees time.duration_ms argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_format_unix", Type::Str)
                .param("ts", Type::Int)
                .param("fmt", Type::Str),
            |args| match args {
                [Value::Int(ts), Value::String(fmt)] => format_unix_utc(*ts, fmt.as_ref()),
                _ => unreachable!("static checker guarantees time.format_unix argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_time_parse_unix",
                Type::Result {
                    ok: Box::new(Type::Int),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str)
            .param("fmt", Type::Str),
            |args| match args {
                [Value::String(value), Value::String(fmt)] => match parse_unix_utc(value, fmt) {
                    Ok(ts) => Ok(Value::ok(Type::Int, Type::Str, Value::Int(ts))),
                    Err(message) => Ok(Value::err(Type::Int, Type::Str, Value::string(message))),
                },
                _ => unreachable!("static checker guarantees time.parse_unix argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_iso8601_format", Type::Str)
                .param("unix_seconds", Type::Int),
            |args| match args {
                [Value::Int(ts)] => {
                    let parts = unix_to_utc(*ts);
                    Ok(Value::string(format!(
                        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                        parts.year, parts.month, parts.day, parts.hour, parts.minute, parts.second
                    )))
                }
                _ => unreachable!("static checker guarantees iso8601_format argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_time_iso8601_parse",
                Type::Result {
                    ok: Box::new(Type::Int),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => match parse_iso8601_utc(value.as_ref()) {
                    Ok(ts) => Ok(Value::ok(Type::Int, Type::Str, Value::Int(ts))),
                    Err(message) => Ok(Value::err(Type::Int, Type::Str, Value::string(message))),
                },
                _ => unreachable!("static checker guarantees iso8601_parse argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_add_months", Type::Int)
                .param("unix_seconds", Type::Int)
                .param("months", Type::Int),
            |args| match args {
                [Value::Int(unix_seconds), Value::Int(months)] => {
                    let parts = unix_to_utc(*unix_seconds);
                    let total_month = (parts.year * 12 + (parts.month - 1)) + months;
                    let new_year = total_month.div_euclid(12);
                    let new_month = total_month.rem_euclid(12) + 1;
                    let last_day = days_in_month(new_year, new_month);
                    let new_day = parts.day.min(last_day);
                    let new_parts = UtcDateTime {
                        year: new_year,
                        month: new_month,
                        day: new_day,
                        hour: parts.hour,
                        minute: parts.minute,
                        second: parts.second,
                    };
                    Ok(Value::Int(utc_to_unix(new_parts)))
                }
                _ => unreachable!("static checker guarantees add_months argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_year_of", Type::Int)
                .param("unix_seconds", Type::Int),
            |args| match args {
                [Value::Int(unix_seconds)] => Ok(Value::Int(unix_to_utc(*unix_seconds).year)),
                _ => unreachable!("static checker guarantees year_of argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_month_of", Type::Int)
                .param("unix_seconds", Type::Int),
            |args| match args {
                [Value::Int(unix_seconds)] => Ok(Value::Int(unix_to_utc(*unix_seconds).month)),
                _ => unreachable!("static checker guarantees month_of argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_day_of", Type::Int)
                .param("unix_seconds", Type::Int),
            |args| match args {
                [Value::Int(unix_seconds)] => Ok(Value::Int(unix_to_utc(*unix_seconds).day)),
                _ => unreachable!("static checker guarantees day_of argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_time_weekday_of", Type::Int)
                .param("unix_seconds", Type::Int),
            |args| match args {
                [Value::Int(unix_seconds)] => {
                    let days = unix_seconds.div_euclid(86_400);
                    let weekday = (days + 3).rem_euclid(7);
                    Ok(Value::Int(weekday))
                }
                _ => unreachable!("static checker guarantees weekday_of argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn parse_iso8601_utc(value: &str) -> Result<i64, String> {
    let trimmed = value.trim();
    let stripped = trimmed
        .strip_suffix('Z')
        .or_else(|| trimmed.strip_suffix("+00:00"))
        .ok_or_else(|| "ISO-8601 timestamp must end with 'Z' or '+00:00' (UTC only)".to_string())?;
    let mut parts = stripped.splitn(2, 'T');
    let date_part = parts
        .next()
        .ok_or_else(|| "ISO-8601 timestamp missing date".to_string())?;
    let time_part = parts.next().unwrap_or("00:00:00");
    let date_components: Vec<&str> = date_part.split('-').collect();
    if date_components.len() != 3 {
        return Err(format!("invalid ISO-8601 date '{date_part}'"));
    }
    let year: i64 = date_components[0]
        .parse()
        .map_err(|_| format!("invalid year in '{date_part}'"))?;
    let month: i64 = date_components[1]
        .parse()
        .map_err(|_| format!("invalid month in '{date_part}'"))?;
    let day: i64 = date_components[2]
        .parse()
        .map_err(|_| format!("invalid day in '{date_part}'"))?;
    let time_no_frac = match time_part.find('.') {
        Some(idx) => &time_part[..idx],
        None => time_part,
    };
    let time_components: Vec<&str> = time_no_frac.split(':').collect();
    if time_components.len() != 3 {
        return Err(format!("invalid ISO-8601 time '{time_part}'"));
    }
    let hour: i64 = time_components[0]
        .parse()
        .map_err(|_| format!("invalid hour in '{time_part}'"))?;
    let minute: i64 = time_components[1]
        .parse()
        .map_err(|_| format!("invalid minute in '{time_part}'"))?;
    let second: i64 = time_components[2]
        .parse()
        .map_err(|_| format!("invalid second in '{time_part}'"))?;
    let parts = UtcDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
    };
    validate_utc(parts)?;
    Ok(utc_to_unix(parts))
}

fn monotonic_unix_ms(last_ms: &Rc<RefCell<i64>>) -> Result<i64, Diagnostic> {
    let current = current_unix_ms()?;
    let mut last = last_ms.borrow_mut();
    if current > *last {
        *last = current;
    }
    Ok(*last)
}

fn push_runtime_trace_event(
    events: &Rc<RefCell<Option<Vec<RuntimeTraceEvent>>>>,
    event: &str,
    fields: impl IntoIterator<Item = (&'static str, RuntimeTraceValue)>,
) {
    let mut events = events.borrow_mut();
    let Some(events) = events.as_mut() else {
        return;
    };
    events.push(RuntimeTraceEvent {
        event: event.to_string(),
        fields: fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    });
}

fn push_filesystem_trace_event(
    events: &Rc<RefCell<Option<Vec<RuntimeTraceEvent>>>>,
    operation: &'static str,
    path: Option<&str>,
    allowed: bool,
    status: Option<&'static str>,
    bytes: Option<u64>,
    entries: Option<u64>,
) {
    let mut fields = vec![
        (
            "stream",
            RuntimeTraceValue::String("filesystem".to_string()),
        ),
        (
            "operation",
            RuntimeTraceValue::String(operation.to_string()),
        ),
        ("allowed", RuntimeTraceValue::Bool(allowed)),
    ];
    if let Some(path) = path {
        fields.push(("path", RuntimeTraceValue::String(path.to_string())));
    }
    if let Some(status) = status {
        fields.push(("status", RuntimeTraceValue::String(status.to_string())));
    }
    if let Some(bytes) = bytes {
        fields.push(("bytes", RuntimeTraceValue::UInt(bytes)));
    }
    if let Some(entries) = entries {
        fields.push(("entries", RuntimeTraceValue::UInt(entries)));
    }
    push_runtime_trace_event(events, "io", fields);
}

impl Runtime {
    fn install_stdlib(&mut self) {
        let task_runtime = self.task_runtime.clone();
        let runtime_trace_events = self.runtime_trace_events.clone();

        let stdout = self.stdout.clone();
        let stderr_for_snapshot = self.stderr.clone();
        self.engine.set_test_output_snapshot(move || {
            (
                stdout.borrow().clone(),
                stderr_for_snapshot.borrow().clone(),
            )
        });
        let stdout = self.stdout.clone();
        let capture_stdout = self.capture_stdout.clone();
        let trace_stdout_events = runtime_trace_events.clone();
        register_print_intrinsic(&mut self.engine, move |text| {
            push_runtime_trace_event(
                &trace_stdout_events,
                "io",
                [
                    ("stream", RuntimeTraceValue::String("stdout".to_string())),
                    ("operation", RuntimeTraceValue::String("write".to_string())),
                    ("bytes", RuntimeTraceValue::UInt(text.len() as u64 + 1)),
                ],
            );
            if *capture_stdout.borrow() {
                let mut stdout = stdout.borrow_mut();
                stdout.push_str(text);
                stdout.push('\n');
            } else {
                println!("{text}");
            }
        });
        register_to_str_intrinsics(&mut self.engine);

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

        let process_args = self.args.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new(
                    "__nox_std_process_argv",
                    Type::Array(Box::new(Type::Str)),
                ),
                move |_| {
                    let values = process_args
                        .borrow()
                        .iter()
                        .cloned()
                        .map(Value::string)
                        .collect();
                    Ok(Value::array(Type::Str, values))
                },
            )
            .expect("stdlib function registration is static");

        let stdin = self.stdin.clone();
        let trace_stdin_events = runtime_trace_events.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("__nox_std_process_read_stdin", Type::Str),
                move |_| {
                    let mut input = stdin.borrow_mut();
                    if let Some(value) = input.clone() {
                        push_runtime_trace_event(
                            &trace_stdin_events,
                            "io",
                            [
                                ("stream", RuntimeTraceValue::String("stdin".to_string())),
                                ("operation", RuntimeTraceValue::String("read".to_string())),
                                ("bytes", RuntimeTraceValue::UInt(value.len() as u64)),
                                ("cached", RuntimeTraceValue::Bool(true)),
                            ],
                        );
                        return Ok(Value::string(value));
                    }
                    let mut value = String::new();
                    io::stdin().read_to_string(&mut value).map_err(|err| {
                        Diagnostic::new(
                            format!("failed to read stdin: {err}"),
                            Span { start: 0, end: 0 },
                        )
                    })?;
                    *input = Some(value.clone());
                    push_runtime_trace_event(
                        &trace_stdin_events,
                        "io",
                        [
                            ("stream", RuntimeTraceValue::String("stdin".to_string())),
                            ("operation", RuntimeTraceValue::String("read".to_string())),
                            ("bytes", RuntimeTraceValue::UInt(value.len() as u64)),
                            ("cached", RuntimeTraceValue::Bool(false)),
                        ],
                    );
                    Ok(Value::string(value))
                },
            )
            .expect("stdlib function registration is static");

        let stderr = self.stderr.clone();
        let trace_stderr_events = runtime_trace_events.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("__nox_std_process_print_err", Type::Null)
                    .param("value", Type::Str),
                move |args| match args {
                    [Value::String(value)] => {
                        push_runtime_trace_event(
                            &trace_stderr_events,
                            "io",
                            [
                                ("stream", RuntimeTraceValue::String("stderr".to_string())),
                                ("operation", RuntimeTraceValue::String("write".to_string())),
                                (
                                    "bytes",
                                    RuntimeTraceValue::UInt(value.as_ref().len() as u64 + 1),
                                ),
                            ],
                        );
                        let mut stderr = stderr.borrow_mut();
                        stderr.push_str(value.as_ref());
                        stderr.push('\n');
                        Ok(Value::Null)
                    }
                    _ => unreachable!("static checker guarantees process.print_err argument type"),
                },
            )
            .expect("stdlib function registration is static");

        let exit_code = self.exit_code.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("__nox_std_process_exit", Type::Null)
                    .param("code", Type::Int),
                move |args| match args {
                    [Value::Int(code)] if (0..=255).contains(code) => {
                        *exit_code.borrow_mut() = Some(*code);
                        Ok(Value::Null)
                    }
                    [Value::Int(_)] => Err(Diagnostic::new(
                        "exit code must be between 0 and 255",
                        Span { start: 0, end: 0 },
                    )),
                    _ => unreachable!("static checker guarantees process.exit argument type"),
                },
            )
            .expect("stdlib function registration is static");

        register_math_intrinsics(&mut self.engine);

        let filesystem_read_allowed = self.permissions.filesystem;
        let filesystem_read_roots = self.permissions.filesystem_read_roots.clone();
        let mock_filesystem_for_read_text = self.mock_filesystem.clone();
        let trace_read_text_events = runtime_trace_events.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("read_text", Type::Str)
                    .param("path", Type::Str)
                    .docstring("Read a UTF-8 text file through the host filesystem boundary.")
                    .capability("filesystem"),
                move |args| {
                    if !filesystem_read_allowed {
                        push_runtime_trace_event(
                            &trace_read_text_events,
                            "io",
                            [
                                (
                                    "stream",
                                    RuntimeTraceValue::String("filesystem".to_string()),
                                ),
                                (
                                    "operation",
                                    RuntimeTraceValue::String("read_text".to_string()),
                                ),
                                ("allowed", RuntimeTraceValue::Bool(false)),
                            ],
                        );
                        return Err(call_capability_required("filesystem", "read_text"));
                    }
                    match args {
                        [Value::String(path)] => {
                            let result = read_text_with_mock(
                                path.as_ref(),
                                &filesystem_read_roots,
                                Some(&mock_filesystem_for_read_text),
                            );
                            match &result {
                                Ok(Value::String(contents)) => push_runtime_trace_event(
                                    &trace_read_text_events,
                                    "io",
                                    [
                                        (
                                            "stream",
                                            RuntimeTraceValue::String("filesystem".to_string()),
                                        ),
                                        (
                                            "operation",
                                            RuntimeTraceValue::String("read_text".to_string()),
                                        ),
                                        (
                                            "path",
                                            RuntimeTraceValue::String(path.as_ref().to_string()),
                                        ),
                                        ("allowed", RuntimeTraceValue::Bool(true)),
                                        ("status", RuntimeTraceValue::String("ok".to_string())),
                                        (
                                            "bytes",
                                            RuntimeTraceValue::UInt(contents.as_ref().len() as u64),
                                        ),
                                    ],
                                ),
                                Err(_) => push_runtime_trace_event(
                                    &trace_read_text_events,
                                    "io",
                                    [
                                        (
                                            "stream",
                                            RuntimeTraceValue::String("filesystem".to_string()),
                                        ),
                                        (
                                            "operation",
                                            RuntimeTraceValue::String("read_text".to_string()),
                                        ),
                                        (
                                            "path",
                                            RuntimeTraceValue::String(path.as_ref().to_string()),
                                        ),
                                        ("allowed", RuntimeTraceValue::Bool(true)),
                                        ("status", RuntimeTraceValue::String("error".to_string())),
                                    ],
                                ),
                                _ => {}
                            }
                            result
                        }
                        _ => unreachable!("static checker guarantees read_text argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let filesystem_read_allowed = self.permissions.filesystem;
        let filesystem_read_roots = self.permissions.filesystem_read_roots.clone();
        let mock_filesystem_for_exists = self.mock_filesystem.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("exists", Type::Bool).param("path", Type::Str),
                move |args| {
                    if !filesystem_read_allowed {
                        return Err(call_capability_required("filesystem", "exists"));
                    }
                    match args {
                        [Value::String(path)] => fs_exists(
                            path.as_ref(),
                            &filesystem_read_roots,
                            Some(&mock_filesystem_for_exists),
                        ),
                        _ => unreachable!("static checker guarantees exists argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let filesystem_read_allowed = self.permissions.filesystem;
        let filesystem_read_roots = self.permissions.filesystem_read_roots.clone();
        let mock_filesystem_for_is_file = self.mock_filesystem.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("__nox_std_fs_is_file", Type::Bool)
                    .param("path", Type::Str),
                move |args| {
                    if !filesystem_read_allowed {
                        return Err(call_capability_required("filesystem", "is_file"));
                    }
                    match args {
                        [Value::String(path)] => fs_is_file(
                            path.as_ref(),
                            &filesystem_read_roots,
                            Some(&mock_filesystem_for_is_file),
                        ),
                        _ => unreachable!("static checker guarantees is_file argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let filesystem_read_allowed = self.permissions.filesystem;
        let filesystem_read_roots = self.permissions.filesystem_read_roots.clone();
        let mock_filesystem_for_is_dir = self.mock_filesystem.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("__nox_std_fs_is_dir", Type::Bool)
                    .param("path", Type::Str),
                move |args| {
                    if !filesystem_read_allowed {
                        return Err(call_capability_required("filesystem", "is_dir"));
                    }
                    match args {
                        [Value::String(path)] => fs_is_dir(
                            path.as_ref(),
                            &filesystem_read_roots,
                            Some(&mock_filesystem_for_is_dir),
                        ),
                        _ => unreachable!("static checker guarantees is_dir argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let filesystem_read_allowed = self.permissions.filesystem;
        let filesystem_read_roots = self.permissions.filesystem_read_roots.clone();
        let mock_filesystem_for_list_dir = self.mock_filesystem.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new(
                    "__nox_std_fs_list_dir",
                    Type::Result {
                        ok: Box::new(Type::Array(Box::new(Type::Str))),
                        err: Box::new(Type::Str),
                    },
                )
                .param("path", Type::Str),
                move |args| {
                    if !filesystem_read_allowed {
                        return Err(call_capability_required("filesystem", "list_dir"));
                    }
                    match args {
                        [Value::String(path)] => fs_list_dir(
                            path.as_ref(),
                            &filesystem_read_roots,
                            Some(&mock_filesystem_for_list_dir),
                        ),
                        _ => unreachable!("static checker guarantees list_dir argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let filesystem_write_allowed = self.permissions.filesystem_write;
        let filesystem_write_roots = self.permissions.filesystem_write_roots.clone();
        let mock_filesystem_for_write_text = self.mock_filesystem.clone();
        let trace_write_text_events = runtime_trace_events.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("write_text", Type::Null)
                    .param("path", Type::Str)
                    .param("contents", Type::Str),
                move |args| {
                    if !filesystem_write_allowed {
                        push_runtime_trace_event(
                            &trace_write_text_events,
                            "io",
                            [
                                (
                                    "stream",
                                    RuntimeTraceValue::String("filesystem".to_string()),
                                ),
                                (
                                    "operation",
                                    RuntimeTraceValue::String("write_text".to_string()),
                                ),
                                ("allowed", RuntimeTraceValue::Bool(false)),
                            ],
                        );
                        return Err(call_capability_required("filesystem write", "write_text"));
                    }
                    match args {
                        [Value::String(path), Value::String(contents)] => {
                            let path_ref = Path::new(path.as_ref());
                            let result = (|| {
                                check_filesystem_write(path_ref, &filesystem_write_roots)?;
                                if write_mock_file(
                                    path_ref,
                                    contents.as_ref().as_bytes(),
                                    Some(&mock_filesystem_for_write_text),
                                )? {
                                    return Ok(Value::Null);
                                }
                                fs::write(path_ref, contents.as_ref())
                                    .map(|_| Value::Null)
                                    .map_err(|err| {
                                        Diagnostic::new(
                                            format!("failed to write '{path}': {err}"),
                                            Span { start: 0, end: 0 },
                                        )
                                    })
                            })();
                            push_runtime_trace_event(
                                &trace_write_text_events,
                                "io",
                                [
                                    (
                                        "stream",
                                        RuntimeTraceValue::String("filesystem".to_string()),
                                    ),
                                    (
                                        "operation",
                                        RuntimeTraceValue::String("write_text".to_string()),
                                    ),
                                    ("path", RuntimeTraceValue::String(path.as_ref().to_string())),
                                    ("allowed", RuntimeTraceValue::Bool(true)),
                                    (
                                        "status",
                                        RuntimeTraceValue::String(
                                            if result.is_ok() { "ok" } else { "error" }.to_string(),
                                        ),
                                    ),
                                    (
                                        "bytes",
                                        RuntimeTraceValue::UInt(contents.as_ref().len() as u64),
                                    ),
                                ],
                            );
                            result
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
        let trace_timer_events = runtime_trace_events.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("sleep_ms", Type::Null).param("ms", Type::Int),
                move |args| {
                    if !timers_allowed {
                        push_runtime_trace_event(
                            &trace_timer_events,
                            "timer",
                            [
                                ("operation", RuntimeTraceValue::String("sleep".to_string())),
                                ("allowed", RuntimeTraceValue::Bool(false)),
                            ],
                        );
                        return Err(call_capability_required("timer", "sleep_ms"));
                    }

                    match args {
                        [Value::Int(ms)] if *ms >= 0 => {
                            push_runtime_trace_event(
                                &trace_timer_events,
                                "timer",
                                [
                                    ("operation", RuntimeTraceValue::String("sleep".to_string())),
                                    ("allowed", RuntimeTraceValue::Bool(true)),
                                    ("duration_ms", RuntimeTraceValue::Int(*ms)),
                                ],
                            );
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
        let mock_network_for_tcp = self.mock_network.clone();
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
                            if let Some(mock) = mock_network_for_tcp.borrow().as_ref() {
                                return Ok(Value::Bool(mock.tcp_connect(host.as_ref(), port)));
                            }
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
        let async_task_max_pending = self.permissions.async_task_max_pending;
        let task_runtime_for_spawn = task_runtime.clone();
        let trace_task_spawn_events = runtime_trace_events.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("task_sleep_ms", Type::Int).param("ms", Type::Int),
                move |args| {
                    if !async_allowed {
                        push_runtime_trace_event(
                            &trace_task_spawn_events,
                            "task",
                            [
                                ("operation", RuntimeTraceValue::String("spawn".to_string())),
                                ("allowed", RuntimeTraceValue::Bool(false)),
                            ],
                        );
                        return Err(call_capability_required("async task", "task_sleep_ms"));
                    }

                    match args {
                        [Value::Int(ms)] if *ms >= 0 => {
                            let mut task_runtime = task_runtime_for_spawn.borrow_mut();
                            if let Some(max) = async_task_max_pending {
                                if task_runtime.pending_count() >= max {
                                    return Err(async_task_cap_exceeded(max));
                                }
                            }
                            let id = task_runtime.spawn_sleep(Duration::from_millis(*ms as u64));
                            push_runtime_trace_event(
                                &trace_task_spawn_events,
                                "task",
                                [
                                    ("operation", RuntimeTraceValue::String("spawn".to_string())),
                                    ("allowed", RuntimeTraceValue::Bool(true)),
                                    ("task_id", RuntimeTraceValue::UInt(id)),
                                    ("duration_ms", RuntimeTraceValue::Int(*ms)),
                                    (
                                        "pending",
                                        RuntimeTraceValue::UInt(task_runtime.pending_count() as u64),
                                    ),
                                ],
                            );
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
        let async_task_max_pending = self.permissions.async_task_max_pending;
        let task_runtime_for_await_spawn = task_runtime.clone();
        let trace_task_await_spawn_events = runtime_trace_events.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("task_sleep", Type::Task(Box::new(Type::Null)))
                    .param("ms", Type::Int),
                move |args| {
                    if !async_allowed {
                        push_runtime_trace_event(
                            &trace_task_await_spawn_events,
                            "task",
                            [
                                (
                                    "operation",
                                    RuntimeTraceValue::String("spawn-awaitable".to_string()),
                                ),
                                ("allowed", RuntimeTraceValue::Bool(false)),
                            ],
                        );
                        return Err(call_capability_required("async task", "task_sleep"));
                    }

                    match args {
                        [Value::Int(ms)] if *ms >= 0 => {
                            let mut task_runtime = task_runtime_for_await_spawn.borrow_mut();
                            if let Some(max) = async_task_max_pending {
                                if task_runtime.pending_count() >= max {
                                    return Err(async_task_cap_exceeded(max));
                                }
                            }
                            let id = task_runtime.spawn_sleep(Duration::from_millis(*ms as u64));
                            push_runtime_trace_event(
                                &trace_task_await_spawn_events,
                                "task",
                                [
                                    (
                                        "operation",
                                        RuntimeTraceValue::String("spawn-awaitable".to_string()),
                                    ),
                                    ("allowed", RuntimeTraceValue::Bool(true)),
                                    ("task_id", RuntimeTraceValue::UInt(id)),
                                    ("duration_ms", RuntimeTraceValue::Int(*ms)),
                                    (
                                        "pending",
                                        RuntimeTraceValue::UInt(task_runtime.pending_count() as u64),
                                    ),
                                ],
                            );
                            drop(task_runtime);

                            let task_runtime_for_await = task_runtime_for_await_spawn.clone();
                            let trace_task_await_events = trace_task_await_spawn_events.clone();
                            Ok(Value::host_task(Type::Null, move |_span| {
                                loop {
                                    let mut task_runtime = task_runtime_for_await.borrow_mut();
                                    let ready = task_runtime
                                        .poll(id)
                                        .map_err(unknown_async_task_diagnostic)?;
                                    if ready {
                                        push_runtime_trace_event(
                                            &trace_task_await_events,
                                            "task",
                                            [
                                                (
                                                    "operation",
                                                    RuntimeTraceValue::String(
                                                        "await".to_string(),
                                                    ),
                                                ),
                                                ("allowed", RuntimeTraceValue::Bool(true)),
                                                ("task_id", RuntimeTraceValue::UInt(id)),
                                                ("ready", RuntimeTraceValue::Bool(true)),
                                                (
                                                    "pending",
                                                    RuntimeTraceValue::UInt(
                                                        task_runtime.pending_count() as u64,
                                                    ),
                                                ),
                                            ],
                                        );
                                        return Ok(Value::Null);
                                    }
                                    drop(task_runtime);
                                    thread::sleep(Duration::from_millis(1));
                                }
                            }))
                        }
                        [Value::Int(_)] => Err(Diagnostic::new(
                            "task_sleep expects a non-negative duration",
                            Span { start: 0, end: 0 },
                        )),
                        _ => unreachable!("static checker guarantees task_sleep argument type"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let async_allowed = self.permissions.async_tasks;
        let task_runtime_for_ready = task_runtime.clone();
        let trace_task_ready_events = runtime_trace_events.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("task_ready", Type::Bool).param("id", Type::Int),
                move |args| {
                    if !async_allowed {
                        push_runtime_trace_event(
                            &trace_task_ready_events,
                            "task",
                            [
                                ("operation", RuntimeTraceValue::String("poll".to_string())),
                                ("allowed", RuntimeTraceValue::Bool(false)),
                            ],
                        );
                        return Err(call_capability_required("async task", "task_ready"));
                    }

                    match args {
                        [Value::Int(id)] if *id >= 0 => {
                            let mut task_runtime = task_runtime_for_ready.borrow_mut();
                            let ready = task_runtime
                                .poll(*id as u64)
                                .map_err(unknown_async_task_diagnostic)?;
                            push_runtime_trace_event(
                                &trace_task_ready_events,
                                "task",
                                [
                                    ("operation", RuntimeTraceValue::String("poll".to_string())),
                                    ("allowed", RuntimeTraceValue::Bool(true)),
                                    ("task_id", RuntimeTraceValue::Int(*id)),
                                    ("ready", RuntimeTraceValue::Bool(ready)),
                                    (
                                        "pending",
                                        RuntimeTraceValue::UInt(task_runtime.pending_count() as u64),
                                    ),
                                ],
                            );
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
        let trace_task_cancel_events = runtime_trace_events.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("task_cancel", Type::Null).param("id", Type::Int),
                move |args| {
                    if !async_allowed {
                        push_runtime_trace_event(
                            &trace_task_cancel_events,
                            "task",
                            [
                                ("operation", RuntimeTraceValue::String("cancel".to_string())),
                                ("allowed", RuntimeTraceValue::Bool(false)),
                            ],
                        );
                        return Err(call_capability_required("async task", "task_cancel"));
                    }

                    match args {
                        [Value::Int(id)] if *id >= 0 => {
                            let mut task_runtime = task_runtime_for_cancel.borrow_mut();
                            task_runtime
                                .cancel(*id as u64)
                                .map_err(unknown_async_task_diagnostic)?;
                            push_runtime_trace_event(
                                &trace_task_cancel_events,
                                "task",
                                [
                                    ("operation", RuntimeTraceValue::String("cancel".to_string())),
                                    ("allowed", RuntimeTraceValue::Bool(true)),
                                    ("task_id", RuntimeTraceValue::Int(*id)),
                                    (
                                        "pending",
                                        RuntimeTraceValue::UInt(task_runtime.pending_count() as u64),
                                    ),
                                ],
                            );
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

        let async_allowed_join = self.permissions.async_tasks;
        let task_runtime_for_join = self.task_runtime.clone();
        let trace_task_join_events = runtime_trace_events.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("task_join", Type::Bool)
                    .param("id", Type::Int)
                    .param("timeout_ms", Type::Int),
                move |args| {
                    if !async_allowed_join {
                        push_runtime_trace_event(
                            &trace_task_join_events,
                            "task",
                            [
                                ("operation", RuntimeTraceValue::String("join".to_string())),
                                ("allowed", RuntimeTraceValue::Bool(false)),
                            ],
                        );
                        return Err(call_capability_required("async task", "task_join"));
                    }
                    match args {
                        [Value::Int(id), Value::Int(timeout_ms)] if *id >= 0 => {
                            let deadline = if *timeout_ms > 0 {
                                Some(Instant::now() + Duration::from_millis(*timeout_ms as u64))
                            } else {
                                None
                            };
                            loop {
                                let mut task_runtime = task_runtime_for_join.borrow_mut();
                                let ready = task_runtime
                                    .poll(*id as u64)
                                    .map_err(unknown_async_task_diagnostic)?;
                                if ready {
                                    push_runtime_trace_event(
                                        &trace_task_join_events,
                                        "task",
                                        [
                                            (
                                                "operation",
                                                RuntimeTraceValue::String("join".to_string()),
                                            ),
                                            ("allowed", RuntimeTraceValue::Bool(true)),
                                            ("task_id", RuntimeTraceValue::Int(*id)),
                                            ("ready", RuntimeTraceValue::Bool(true)),
                                            (
                                                "pending",
                                                RuntimeTraceValue::UInt(
                                                    task_runtime.pending_count() as u64,
                                                ),
                                            ),
                                        ],
                                    );
                                    return Ok(Value::Bool(true));
                                }
                                drop(task_runtime);
                                if let Some(deadline) = deadline {
                                    if Instant::now() >= deadline {
                                        let _ =
                                            task_runtime_for_join.borrow_mut().cancel(*id as u64);
                                        push_runtime_trace_event(
                                            &trace_task_join_events,
                                            "task",
                                            [
                                                (
                                                    "operation",
                                                    RuntimeTraceValue::String("join".to_string()),
                                                ),
                                                ("allowed", RuntimeTraceValue::Bool(true)),
                                                ("task_id", RuntimeTraceValue::Int(*id)),
                                                ("ready", RuntimeTraceValue::Bool(false)),
                                            ],
                                        );
                                        return Ok(Value::Bool(false));
                                    }
                                }
                                thread::sleep(Duration::from_millis(1));
                            }
                        }
                        [Value::Int(_), Value::Int(_)] => Err(Diagnostic::new(
                            "task_join expects a non-negative task id",
                            Span { start: 0, end: 0 },
                        )),
                        _ => unreachable!("static checker guarantees task_join argument types"),
                    }
                },
            )
            .expect("stdlib function registration is static");

        let async_allowed_count = self.permissions.async_tasks;
        let task_runtime_for_count = self.task_runtime.clone();
        let trace_task_count_events = runtime_trace_events.clone();
        self.engine
            .register_host_function(
                HostFunctionBuilder::new("task_pending_count", Type::Int),
                move |_args| {
                    if !async_allowed_count {
                        push_runtime_trace_event(
                            &trace_task_count_events,
                            "task",
                            [
                                (
                                    "operation",
                                    RuntimeTraceValue::String("pending_count".to_string()),
                                ),
                                ("allowed", RuntimeTraceValue::Bool(false)),
                            ],
                        );
                        return Err(call_capability_required("async task", "task_pending_count"));
                    }
                    let count = task_runtime_for_count.borrow().pending_count();
                    push_runtime_trace_event(
                        &trace_task_count_events,
                        "task",
                        [
                            (
                                "operation",
                                RuntimeTraceValue::String("pending_count".to_string()),
                            ),
                            ("allowed", RuntimeTraceValue::Bool(true)),
                            ("pending", RuntimeTraceValue::UInt(count as u64)),
                        ],
                    );
                    Ok(Value::Int(count as i64))
                },
            )
            .expect("stdlib function registration is static");

        install_std_module_aliases(
            &mut self.engine,
            &self.permissions,
            StdModuleAliasContext {
                mock_clock: Some(self.mock_clock.clone()),
                mock_env: Some(self.mock_env.clone()),
                mock_filesystem: Some(self.mock_filesystem.clone()),
                mock_network: Some(self.mock_network.clone()),
                process_run_active: Some(self.process_run_active.clone()),
                runtime_trace_events: Some(self.runtime_trace_events.clone()),
            },
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UtcDateTime {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
}

fn current_unix_ms() -> Result<i64, Diagnostic> {
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| {
        Diagnostic::new(
            "system clock is before Unix epoch",
            Span { start: 0, end: 0 },
        )
    })?;
    i64::try_from(duration.as_millis()).map_err(|_| {
        Diagnostic::new(
            "current Unix time does not fit in int",
            Span { start: 0, end: 0 },
        )
    })
}

fn format_unix_utc(timestamp: i64, format: &str) -> Result<Value, Diagnostic> {
    let parts = unix_to_utc(timestamp);
    let mut output = String::new();
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }
        let Some(token) = chars.next() else {
            return Err(time_argument_error("format string ends with '%'"));
        };
        match token {
            '%' => output.push('%'),
            'Y' => output.push_str(&format!("{:04}", parts.year)),
            'm' => output.push_str(&format!("{:02}", parts.month)),
            'd' => output.push_str(&format!("{:02}", parts.day)),
            'H' => output.push_str(&format!("{:02}", parts.hour)),
            'M' => output.push_str(&format!("{:02}", parts.minute)),
            'S' => output.push_str(&format!("{:02}", parts.second)),
            _ => return Err(time_argument_error("unsupported time format token")),
        }
    }
    Ok(Value::string(output))
}

fn parse_unix_utc(value: &str, format: &str) -> Result<i64, String> {
    let mut input = value;
    let mut year = None;
    let mut month = None;
    let mut day = None;
    let mut hour = None;
    let mut minute = None;
    let mut second = None;
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            input = input
                .strip_prefix(ch)
                .ok_or_else(|| "time value does not match format".to_string())?;
            continue;
        }
        let token = chars
            .next()
            .ok_or_else(|| "format string ends with '%'".to_string())?;
        match token {
            '%' => {
                input = input
                    .strip_prefix('%')
                    .ok_or_else(|| "time value does not match format".to_string())?;
            }
            'Y' => year = Some(take_fixed_digits(&mut input, 4, "year")?),
            'm' => month = Some(take_fixed_digits(&mut input, 2, "month")?),
            'd' => day = Some(take_fixed_digits(&mut input, 2, "day")?),
            'H' => hour = Some(take_fixed_digits(&mut input, 2, "hour")?),
            'M' => minute = Some(take_fixed_digits(&mut input, 2, "minute")?),
            'S' => second = Some(take_fixed_digits(&mut input, 2, "second")?),
            _ => return Err("unsupported time format token".to_string()),
        }
    }
    if !input.is_empty() {
        return Err("time value has trailing characters".to_string());
    }
    let parts = UtcDateTime {
        year: year.ok_or_else(|| "format is missing %Y".to_string())?,
        month: month.ok_or_else(|| "format is missing %m".to_string())?,
        day: day.ok_or_else(|| "format is missing %d".to_string())?,
        hour: hour.unwrap_or(0),
        minute: minute.unwrap_or(0),
        second: second.unwrap_or(0),
    };
    validate_utc(parts)?;
    Ok(utc_to_unix(parts))
}

fn take_fixed_digits(input: &mut &str, count: usize, label: &str) -> Result<i64, String> {
    if input.len() < count {
        return Err(format!("time value is missing {label} digits"));
    }
    let (digits, rest) = input.split_at(count);
    if !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(format!("time value has invalid {label} digits"));
    }
    *input = rest;
    digits
        .parse::<i64>()
        .map_err(|_| format!("time value has invalid {label} digits"))
}

fn validate_utc(parts: UtcDateTime) -> Result<(), String> {
    if !(1..=12).contains(&parts.month) {
        return Err("month must be between 1 and 12".to_string());
    }
    let days = days_in_month(parts.year, parts.month);
    if parts.day < 1 || parts.day > days {
        return Err("day is out of range for month".to_string());
    }
    if !(0..=23).contains(&parts.hour) {
        return Err("hour must be between 0 and 23".to_string());
    }
    if !(0..=59).contains(&parts.minute) {
        return Err("minute must be between 0 and 59".to_string());
    }
    if !(0..=59).contains(&parts.second) {
        return Err("second must be between 0 and 59".to_string());
    }
    Ok(())
}

fn unix_to_utc(timestamp: i64) -> UtcDateTime {
    let days = timestamp.div_euclid(86_400);
    let seconds = timestamp.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    UtcDateTime {
        year,
        month,
        day,
        hour: seconds / 3_600,
        minute: (seconds % 3_600) / 60,
        second: seconds % 60,
    }
}

fn utc_to_unix(parts: UtcDateTime) -> i64 {
    days_from_civil(parts.year, parts.month, parts.day) * 86_400
        + parts.hour * 3_600
        + parts.minute * 60
        + parts.second
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 }.div_euclid(400);
    let yoe = year - era * 400;
    let month = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * month + 2).div_euclid(5) + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn time_argument_error(message: &'static str) -> Diagnostic {
    Diagnostic::new(message, Span { start: 0, end: 0 })
}

fn register_print_intrinsic<F>(engine: &mut Engine, printer: F)
where
    F: Fn(&str) + 'static,
{
    engine
        .register_host_function(
            HostFunctionBuilder::new("print", Type::Null).param("value", Type::Str),
            move |args| match args {
                [Value::String(value)] => {
                    printer(value.as_ref());
                    Ok(Value::Null)
                }
                _ => unreachable!("static checker guarantees print argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn register_to_str_intrinsics(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new("to_str_int", Type::Str).param("value", Type::Int),
            |args| match args {
                [Value::Int(value)] => Ok(Value::string(value.to_string())),
                _ => unreachable!("static checker guarantees to_str_int argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("to_str_float", Type::Str).param("value", Type::Float),
            |args| match args {
                [Value::Float(value)] => Ok(Value::string(value.to_string())),
                _ => unreachable!("static checker guarantees to_str_float argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("to_str_bool", Type::Str).param("value", Type::Bool),
            |args| match args {
                [Value::Bool(value)] => Ok(Value::string(value.to_string())),
                _ => unreachable!("static checker guarantees to_str_bool argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("to_str_null", Type::Str).param("value", Type::Null),
            |args| match args {
                [Value::Null] => Ok(Value::string("null")),
                _ => unreachable!("static checker guarantees to_str_null argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("to_str_str", Type::Str).param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => Ok(Value::string(value.clone())),
                _ => unreachable!("static checker guarantees to_str_str argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn register_math_intrinsics(engine: &mut Engine) {
    register_unary_math(engine, "abs", f64::abs);
    register_binary_math(engine, "min", f64::min);
    register_binary_math(engine, "max", f64::max);
    register_unary_math_checked(engine, "sqrt", |value| {
        if value < 0.0 {
            return Err(math_argument_error("sqrt expects a non-negative value"));
        }
        Ok(value.sqrt())
    });
    register_binary_math_checked(engine, "pow", |base, exponent| {
        let result = base.powf(exponent);
        if !result.is_finite() {
            return Err(math_argument_error("pow result is not finite"));
        }
        Ok(result)
    });
    register_unary_math(engine, "floor", f64::floor);
    register_unary_math(engine, "ceil", f64::ceil);
    register_unary_math(engine, "round", f64::round);
    register_unary_math_checked(engine, "log", |value| {
        if value <= 0.0 {
            return Err(math_argument_error("log expects a positive value"));
        }
        Ok(value.ln())
    });
    register_unary_math_checked(engine, "log2", |value| {
        if value <= 0.0 {
            return Err(math_argument_error("log2 expects a positive value"));
        }
        Ok(value.log2())
    });
    register_unary_math(engine, "sin", f64::sin);
    register_unary_math(engine, "cos", f64::cos);
    register_unary_math(engine, "tan", f64::tan);
    engine
        .register_host_function(HostFunctionBuilder::new("pi", Type::Float), |_| {
            Ok(Value::Float(std::f64::consts::PI))
        })
        .expect("stdlib function registration is static");
    engine
        .register_host_function(HostFunctionBuilder::new("e", Type::Float), |_| {
            Ok(Value::Float(std::f64::consts::E))
        })
        .expect("stdlib function registration is static");
}

fn register_unary_math(engine: &mut Engine, name: &'static str, function: fn(f64) -> f64) {
    register_unary_math_checked(engine, name, move |value| Ok(function(value)));
}

fn register_unary_math_checked<F>(engine: &mut Engine, name: &'static str, function: F)
where
    F: Fn(f64) -> Result<f64, Diagnostic> + 'static,
{
    engine
        .register_host_function(
            HostFunctionBuilder::new(name, Type::Float).param("value", Type::Float),
            move |args| match args {
                [Value::Float(value)] if value.is_finite() => function(*value).map(Value::Float),
                [Value::Float(_)] => Err(math_argument_error("math input must be finite")),
                _ => unreachable!("static checker guarantees math argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn register_binary_math(engine: &mut Engine, name: &'static str, function: fn(f64, f64) -> f64) {
    register_binary_math_checked(engine, name, move |left, right| Ok(function(left, right)));
}

fn register_binary_math_checked<F>(engine: &mut Engine, name: &'static str, function: F)
where
    F: Fn(f64, f64) -> Result<f64, Diagnostic> + 'static,
{
    engine
        .register_host_function(
            HostFunctionBuilder::new(name, Type::Float)
                .param("left", Type::Float)
                .param("right", Type::Float),
            move |args| match args {
                [Value::Float(left), Value::Float(right)]
                    if left.is_finite() && right.is_finite() =>
                {
                    function(*left, *right).map(Value::Float)
                }
                [Value::Float(_), Value::Float(_)] => {
                    Err(math_argument_error("math input must be finite"))
                }
                _ => unreachable!("static checker guarantees math argument types"),
            },
        )
        .expect("stdlib function registration is static");
}

fn math_argument_error(message: &'static str) -> Diagnostic {
    Diagnostic::new(message, Span { start: 0, end: 0 })
}

fn register_string_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_split", Type::Array(Box::new(Type::Str)))
                .param("value", Type::Str)
                .param("separator", Type::Str),
            |args| match args {
                [Value::String(value), Value::String(separator)] => {
                    if separator.is_empty() {
                        return Err(string_argument_error("split separator cannot be empty"));
                    }
                    let parts = value
                        .split(separator.as_ref())
                        .map(Value::string)
                        .collect::<Vec<_>>();
                    Ok(Value::array(Type::Str, parts))
                }
                _ => unreachable!("static checker guarantees string.split argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_join", Type::Str)
                .param("values", Type::Array(Box::new(Type::Str)))
                .param("separator", Type::Str),
            |args| match args {
                [Value::Array(values), Value::String(separator)] => {
                    string_array_values(&values.elements())
                        .map(|values| Value::string(values.join(separator.as_ref())))
                }
                _ => unreachable!("static checker guarantees string.join argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_substring", Type::Str)
                .param("value", Type::Str)
                .param("start", Type::Int)
                .param("length", Type::Int),
            |args| match args {
                [Value::String(value), Value::Int(start), Value::Int(length)] => {
                    substring_by_char(value.as_ref(), *start, *length)
                }
                _ => unreachable!("static checker guarantees string.substring argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_trim", Type::Str).param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => Ok(Value::string(value.trim().to_string())),
                _ => unreachable!("static checker guarantees string.trim argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_replace", Type::Str)
                .param("value", Type::Str)
                .param("from", Type::Str)
                .param("to", Type::Str),
            |args| match args {
                [Value::String(value), Value::String(from), Value::String(to)] => {
                    if from.is_empty() {
                        return Err(string_argument_error("replace target cannot be empty"));
                    }
                    Ok(Value::string(value.replace(from.as_ref(), to.as_ref())))
                }
                _ => unreachable!("static checker guarantees string.replace argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_starts_with", Type::Bool)
                .param("value", Type::Str)
                .param("prefix", Type::Str),
            |args| match args {
                [Value::String(value), Value::String(prefix)] => {
                    Ok(Value::Bool(value.starts_with(prefix.as_ref())))
                }
                _ => unreachable!("static checker guarantees string.starts_with argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_ends_with", Type::Bool)
                .param("value", Type::Str)
                .param("suffix", Type::Str),
            |args| match args {
                [Value::String(value), Value::String(suffix)] => {
                    Ok(Value::Bool(value.ends_with(suffix.as_ref())))
                }
                _ => unreachable!("static checker guarantees string.ends_with argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_index_of", Type::Int)
                .param("value", Type::Str)
                .param("needle", Type::Str),
            |args| match args {
                [Value::String(value), Value::String(needle)] => {
                    if needle.is_empty() {
                        return Err(string_argument_error("index_of needle cannot be empty"));
                    }
                    let index = value
                        .find(needle.as_ref())
                        .map(|byte_index| value[..byte_index].chars().count() as i64)
                        .unwrap_or(-1);
                    Ok(Value::Int(index))
                }
                _ => unreachable!("static checker guarantees string.index_of argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_contains", Type::Bool)
                .param("value", Type::Str)
                .param("needle", Type::Str),
            |args| match args {
                [Value::String(value), Value::String(needle)] => {
                    if needle.is_empty() {
                        return Err(string_argument_error("contains needle cannot be empty"));
                    }
                    Ok(Value::Bool(value.contains(needle.as_ref())))
                }
                _ => unreachable!("static checker guarantees string.contains argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_last_index_of", Type::Int)
                .param("value", Type::Str)
                .param("needle", Type::Str),
            |args| match args {
                [Value::String(value), Value::String(needle)] => {
                    if needle.is_empty() {
                        return Err(string_argument_error(
                            "last_index_of needle cannot be empty",
                        ));
                    }
                    let index = value
                        .rfind(needle.as_ref())
                        .map(|byte_index| value[..byte_index].chars().count() as i64)
                        .unwrap_or(-1);
                    Ok(Value::Int(index))
                }
                _ => unreachable!("static checker guarantees string.last_index_of argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_repeat", Type::Str)
                .param("value", Type::Str)
                .param("count", Type::Int),
            |args| match args {
                [Value::String(value), Value::Int(count)] => {
                    if *count < 0 {
                        return Err(string_argument_error("repeat count must be non-negative"));
                    }
                    let count = usize::try_from(*count)
                        .map_err(|_| string_argument_error("repeat count is out of range"))?;
                    Ok(Value::string(value.repeat(count)))
                }
                _ => unreachable!("static checker guarantees string.repeat argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_pad_left", Type::Str)
                .param("value", Type::Str)
                .param("width", Type::Int)
                .param("fill", Type::Str),
            |args| match args {
                [Value::String(value), Value::Int(width), Value::String(fill)] => {
                    pad_string(value.as_ref(), *width, fill.as_ref(), PadSide::Left)
                }
                _ => unreachable!("static checker guarantees string.pad_left argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_pad_right", Type::Str)
                .param("value", Type::Str)
                .param("width", Type::Int)
                .param("fill", Type::Str),
            |args| match args {
                [Value::String(value), Value::Int(width), Value::String(fill)] => {
                    pad_string(value.as_ref(), *width, fill.as_ref(), PadSide::Right)
                }
                _ => unreachable!("static checker guarantees string.pad_right argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_string_parse_int",
                Type::Result {
                    ok: Box::new(Type::Int),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => match value.trim().parse::<i64>() {
                    Ok(parsed) => Ok(Value::ok(Type::Int, Type::Str, Value::Int(parsed))),
                    Err(err) => Ok(Value::err(
                        Type::Int,
                        Type::Str,
                        Value::string(format!("invalid int: {err}")),
                    )),
                },
                _ => unreachable!("static checker guarantees string.parse_int argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_string_parse_float",
                Type::Result {
                    ok: Box::new(Type::Float),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => match value.trim().parse::<f64>() {
                    Ok(parsed) if parsed.is_finite() => {
                        Ok(Value::ok(Type::Float, Type::Str, Value::Float(parsed)))
                    }
                    Ok(_) => Ok(Value::err(
                        Type::Float,
                        Type::Str,
                        Value::string("float is not finite"),
                    )),
                    Err(err) => Ok(Value::err(
                        Type::Float,
                        Type::Str,
                        Value::string(format!("invalid float: {err}")),
                    )),
                },
                _ => unreachable!("static checker guarantees string.parse_float argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_lines", Type::Array(Box::new(Type::Str)))
                .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => Ok(Value::array(
                    Type::Str,
                    value.lines().map(Value::string).collect(),
                )),
                _ => unreachable!("static checker guarantees string.lines argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_to_upper", Type::Str)
                .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => Ok(Value::string(value.to_uppercase())),
                _ => unreachable!("static checker guarantees string.to_upper argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_string_to_lower", Type::Str)
                .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => Ok(Value::string(value.to_lowercase())),
                _ => unreachable!("static checker guarantees string.to_lower argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn register_json_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_parse",
                Type::Result {
                    ok: Box::new(Type::Json),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => match JsonParser::new(value.as_ref()).parse() {
                    Ok(value) => Ok(Value::ok(Type::Json, Type::Str, Value::json(value))),
                    Err(message) => Ok(Value::err(Type::Json, Type::Str, Value::string(message))),
                },
                _ => unreachable!("static checker guarantees json.parse argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_json_stringify", Type::Str)
                .param("value", Type::Json),
            |args| match args {
                [Value::Json(value)] => Ok(Value::string(value.to_string())),
                _ => unreachable!("static checker guarantees json.stringify argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_json_kind", Type::Str).param("value", Type::Json),
            |args| match args {
                [Value::Json(value)] => Ok(Value::string(json_kind(value.as_ref()))),
                _ => unreachable!("static checker guarantees json.kind argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_array_len",
                Type::Result {
                    ok: Box::new(Type::Int),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json),
            |args| match args {
                [Value::Json(value)] => match value.as_ref() {
                    JsonValue::Array(values) => Ok(Value::ok(
                        Type::Int,
                        Type::Str,
                        Value::Int(values.len() as i64),
                    )),
                    other => Ok(Value::err(
                        Type::Int,
                        Type::Str,
                        Value::string(format!("expected JSON array, got {}", json_kind(other))),
                    )),
                },
                _ => unreachable!("static checker guarantees json.array_len argument type"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_array_get",
                Type::Result {
                    ok: Box::new(Type::Json),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json)
            .param("index", Type::Int),
            |args| match args {
                [Value::Json(value), Value::Int(index)] => match value.as_ref() {
                    JsonValue::Array(values) => {
                        if *index < 0 {
                            return Ok(Value::err(
                                Type::Json,
                                Type::Str,
                                Value::string("array index must be non-negative"),
                            ));
                        }
                        let index = *index as usize;
                        match values.get(index) {
                            Some(value) => {
                                Ok(Value::ok(Type::Json, Type::Str, Value::json(value.clone())))
                            }
                            None => Ok(Value::err(
                                Type::Json,
                                Type::Str,
                                Value::string("array index out of bounds"),
                            )),
                        }
                    }
                    other => Ok(Value::err(
                        Type::Json,
                        Type::Str,
                        Value::string(format!("expected JSON array, got {}", json_kind(other))),
                    )),
                },
                _ => unreachable!("static checker guarantees json.array_get argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_object_has",
                Type::Result {
                    ok: Box::new(Type::Bool),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json)
            .param("key", Type::Str),
            |args| match args {
                [Value::Json(value), Value::String(key)] => match value.as_ref() {
                    JsonValue::Object(entries) => Ok(Value::ok(
                        Type::Bool,
                        Type::Str,
                        Value::Bool(entries.contains_key(key.as_ref())),
                    )),
                    other => Ok(Value::err(
                        Type::Bool,
                        Type::Str,
                        Value::string(format!("expected JSON object, got {}", json_kind(other))),
                    )),
                },
                _ => unreachable!("static checker guarantees json.object_has argument types"),
            },
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_object_get",
                Type::Result {
                    ok: Box::new(Type::Json),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json)
            .param("key", Type::Str),
            |args| match args {
                [Value::Json(value), Value::String(key)] => match value.as_ref() {
                    JsonValue::Object(entries) => match entries.get(key.as_ref()) {
                        Some(value) => {
                            Ok(Value::ok(Type::Json, Type::Str, Value::json(value.clone())))
                        }
                        None => Ok(Value::err(
                            Type::Json,
                            Type::Str,
                            Value::string(format!("JSON object key '{key}' not found")),
                        )),
                    },
                    other => Ok(Value::err(
                        Type::Json,
                        Type::Str,
                        Value::string(format!("expected JSON object, got {}", json_kind(other))),
                    )),
                },
                _ => unreachable!("static checker guarantees json.object_get argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_require_field",
                Type::Result {
                    ok: Box::new(Type::Json),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json)
            .param("path", Type::Str)
            .param("expected_kind", Type::Str),
            |args| match args {
                [Value::Json(value), Value::String(path), Value::String(expected)] => {
                    match resolve_json_path(value.as_ref(), path.as_ref()) {
                        Ok(found) => {
                            let actual = json_kind(&found);
                            if expected.as_ref() == "any" || actual == expected.as_ref() {
                                Ok(Value::ok(
                                    Type::Json,
                                    Type::Str,
                                    Value::Json(Rc::new(found)),
                                ))
                            } else {
                                Ok(Value::err(
                                    Type::Json,
                                    Type::Str,
                                    Value::string(format!(
                                        "at {path}: expected {expected}, got {actual}"
                                    )),
                                ))
                            }
                        }
                        Err(message) => {
                            Ok(Value::err(Type::Json, Type::Str, Value::string(message)))
                        }
                    }
                }
                _ => unreachable!("static checker guarantees json.require_field argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_validate_schema",
                Type::Result {
                    ok: Box::new(Type::Null),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json)
            .param("required_fields", Type::Array(Box::new(Type::Str))),
            |args| match args {
                [Value::Json(value), Value::Array(required)] => {
                    let JsonValue::Object(obj) = value.as_ref() else {
                        return Ok(Value::err(
                            Type::Null,
                            Type::Str,
                            Value::string(format!(
                                "expected JSON object, got {}",
                                json_kind(value.as_ref())
                            )),
                        ));
                    };
                    let snapshot = required.snapshot();
                    let mut missing: Vec<String> = Vec::new();
                    for field in snapshot {
                        let Value::String(name) = field else {
                            return Err(Diagnostic::new(
                                "schema required_fields must contain strings",
                                Span { start: 0, end: 0 },
                            ));
                        };
                        if !obj.contains_key(name.as_ref()) {
                            missing.push(name.as_ref().to_string());
                        }
                    }
                    if missing.is_empty() {
                        Ok(Value::ok(Type::Null, Type::Str, Value::Null))
                    } else {
                        Ok(Value::err(
                            Type::Null,
                            Type::Str,
                            Value::string(format!(
                                "missing required field(s): {}",
                                missing.join(", ")
                            )),
                        ))
                    }
                }
                _ => unreachable!("static checker guarantees json.validate_schema argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_validate_object",
                Type::Result {
                    ok: Box::new(Type::Null),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json)
            .param("required_fields", Type::Array(Box::new(Type::Str)))
            .param("allowed_fields", Type::Array(Box::new(Type::Str))),
            |args| match args {
                [Value::Json(value), Value::Array(required), Value::Array(allowed)] => {
                    let JsonValue::Object(obj) = value.as_ref() else {
                        return Ok(Value::err(
                            Type::Null,
                            Type::Str,
                            Value::string(format!(
                                "expected JSON object, got {}",
                                json_kind(value.as_ref())
                            )),
                        ));
                    };
                    let required_snapshot = required.snapshot();
                    let allowed_snapshot = allowed.snapshot();
                    let required = string_array_values(&required_snapshot)?;
                    let allowed = string_array_values(&allowed_snapshot)?;
                    let mut missing: Vec<String> = Vec::new();
                    for field in &required {
                        if !obj.contains_key(field) {
                            missing.push(field.clone());
                        }
                    }
                    let allowed_set = allowed
                        .into_iter()
                        .collect::<std::collections::BTreeSet<_>>();
                    let mut unknown: Vec<String> = obj
                        .keys()
                        .filter(|key| !allowed_set.contains(*key))
                        .cloned()
                        .collect();
                    unknown.sort();
                    let mut messages = Vec::new();
                    if !missing.is_empty() {
                        messages.push(format!("missing required field(s): {}", missing.join(", ")));
                    }
                    if !unknown.is_empty() {
                        messages.push(format!("unknown field(s): {}", unknown.join(", ")));
                    }
                    if messages.is_empty() {
                        Ok(Value::ok(Type::Null, Type::Str, Value::Null))
                    } else {
                        Ok(Value::err(
                            Type::Null,
                            Type::Str,
                            Value::string(messages.join("; ")),
                        ))
                    }
                }
                _ => unreachable!("static checker guarantees json.validate_object argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_apply_defaults",
                Type::Result {
                    ok: Box::new(Type::Json),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json)
            .param("defaults", Type::Json),
            |args| match args {
                [Value::Json(value), Value::Json(defaults)] => {
                    let JsonValue::Object(obj) = value.as_ref() else {
                        return Ok(Value::err(
                            Type::Json,
                            Type::Str,
                            Value::string(format!(
                                "expected JSON object, got {}",
                                json_kind(value.as_ref())
                            )),
                        ));
                    };
                    let JsonValue::Object(defaults) = defaults.as_ref() else {
                        return Ok(Value::err(
                            Type::Json,
                            Type::Str,
                            Value::string(format!(
                                "expected defaults object, got {}",
                                json_kind(defaults.as_ref())
                            )),
                        ));
                    };
                    let mut merged = obj.clone();
                    for (key, default_value) in defaults {
                        merged
                            .entry(key.clone())
                            .or_insert_with(|| default_value.clone());
                    }
                    Ok(Value::ok(
                        Type::Json,
                        Type::Str,
                        Value::json(JsonValue::Object(merged)),
                    ))
                }
                _ => unreachable!("static checker guarantees json.apply_defaults argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_apply_defaults_deep",
                Type::Result {
                    ok: Box::new(Type::Json),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json)
            .param("defaults", Type::Json),
            |args| match args {
                [Value::Json(value), Value::Json(defaults)] => {
                    let JsonValue::Object(_) = value.as_ref() else {
                        return Ok(Value::err(
                            Type::Json,
                            Type::Str,
                            Value::string(format!(
                                "expected JSON object, got {}",
                                json_kind(value.as_ref())
                            )),
                        ));
                    };
                    let JsonValue::Object(_) = defaults.as_ref() else {
                        return Ok(Value::err(
                            Type::Json,
                            Type::Str,
                            Value::string(format!(
                                "expected defaults object, got {}",
                                json_kind(defaults.as_ref())
                            )),
                        ));
                    };
                    Ok(Value::ok(
                        Type::Json,
                        Type::Str,
                        Value::json(apply_json_defaults_deep(
                            value.as_ref().clone(),
                            defaults.as_ref(),
                        )),
                    ))
                }
                _ => {
                    unreachable!(
                        "static checker guarantees json.apply_defaults_deep argument types"
                    )
                }
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_json_to_json", Type::Json)
                .type_param("T")
                .param("value", Type::Generic("T".to_string())),
            |args| match args {
                [value] => match value_to_json(value) {
                    Ok(json) => Ok(Value::json(json)),
                    Err(message) => Err(Diagnostic::new(message, Span { start: 0, end: 0 })),
                },
                _ => unreachable!("static checker guarantees json.to_json argument count"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_from_json",
                Type::Result {
                    ok: Box::new(Type::Generic("T".to_string())),
                    err: Box::new(Type::Str),
                },
            )
            .type_param("T")
            .param("value", Type::Json),
            |_| {
                Err(Diagnostic::new(
                    "json.from_json must be compiled with an expected result[T, str] type",
                    Span { start: 0, end: 0 },
                ))
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_variant_name",
                Type::Result {
                    ok: Box::new(Type::Str),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json),
            |args| match args {
                [Value::Json(value)] => match json_variant_name(value.as_ref()) {
                    Ok(name) => Ok(Value::ok(Type::Str, Type::Str, Value::string(name))),
                    Err(message) => Ok(Value::err(Type::Str, Type::Str, Value::string(message))),
                },
                _ => unreachable!("static checker guarantees json.variant_name argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_variant_payload",
                Type::Result {
                    ok: Box::new(Type::Json),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json),
            |args| match args {
                [Value::Json(value)] => match json_variant_payload(value.as_ref()) {
                    Ok(payload) => Ok(Value::ok(Type::Json, Type::Str, Value::json(payload))),
                    Err(message) => Ok(Value::err(Type::Json, Type::Str, Value::string(message))),
                },
                _ => unreachable!("static checker guarantees json.variant_payload argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_as_int",
                Type::Result {
                    ok: Box::new(Type::Int),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json),
            |args| match args {
                [Value::Json(value)] => match value.as_ref() {
                    JsonValue::Number(n) => {
                        if n.fract() != 0.0 || !n.is_finite() {
                            Ok(Value::err(
                                Type::Int,
                                Type::Str,
                                Value::string(format!("expected integer JSON number, got {n}")),
                            ))
                        } else {
                            Ok(Value::ok(Type::Int, Type::Str, Value::Int(*n as i64)))
                        }
                    }
                    other => Ok(Value::err(
                        Type::Int,
                        Type::Str,
                        Value::string(format!("expected JSON number, got {}", json_kind(other))),
                    )),
                },
                _ => unreachable!("static checker guarantees json.as_int argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_as_float",
                Type::Result {
                    ok: Box::new(Type::Float),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json),
            |args| match args {
                [Value::Json(value)] => match value.as_ref() {
                    JsonValue::Number(n) => Ok(Value::ok(Type::Float, Type::Str, Value::Float(*n))),
                    other => Ok(Value::err(
                        Type::Float,
                        Type::Str,
                        Value::string(format!("expected JSON number, got {}", json_kind(other))),
                    )),
                },
                _ => unreachable!("static checker guarantees json.as_float argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_as_str",
                Type::Result {
                    ok: Box::new(Type::Str),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json),
            |args| match args {
                [Value::Json(value)] => match value.as_ref() {
                    JsonValue::String(s) => {
                        Ok(Value::ok(Type::Str, Type::Str, Value::string(s.clone())))
                    }
                    other => Ok(Value::err(
                        Type::Str,
                        Type::Str,
                        Value::string(format!("expected JSON string, got {}", json_kind(other))),
                    )),
                },
                _ => unreachable!("static checker guarantees json.as_str argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_as_bool",
                Type::Result {
                    ok: Box::new(Type::Bool),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json),
            |args| match args {
                [Value::Json(value)] => match value.as_ref() {
                    JsonValue::Bool(b) => Ok(Value::ok(Type::Bool, Type::Str, Value::Bool(*b))),
                    other => Ok(Value::err(
                        Type::Bool,
                        Type::Str,
                        Value::string(format!("expected JSON bool, got {}", json_kind(other))),
                    )),
                },
                _ => unreachable!("static checker guarantees json.as_bool argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_as_array",
                Type::Result {
                    ok: Box::new(Type::Array(Box::new(Type::Json))),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json),
            |args| match args {
                [Value::Json(value)] => match value.as_ref() {
                    JsonValue::Array(items) => {
                        let elements: Vec<Value> = items.iter().cloned().map(Value::json).collect();
                        Ok(Value::ok(
                            Type::Array(Box::new(Type::Json)),
                            Type::Str,
                            Value::array(Type::Json, elements),
                        ))
                    }
                    other => Ok(Value::err(
                        Type::Array(Box::new(Type::Json)),
                        Type::Str,
                        Value::string(format!("expected JSON array, got {}", json_kind(other))),
                    )),
                },
                _ => unreachable!("static checker guarantees json.as_array argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_json_as_object",
                Type::Result {
                    ok: Box::new(Type::Map(Box::new(Type::Json))),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Json),
            |args| match args {
                [Value::Json(value)] => match value.as_ref() {
                    JsonValue::Object(map) => {
                        let entries: BTreeMap<String, Value> = map
                            .iter()
                            .map(|(k, v)| (k.clone(), Value::json(v.clone())))
                            .collect();
                        Ok(Value::ok(
                            Type::Map(Box::new(Type::Json)),
                            Type::Str,
                            Value::map(Type::Json, entries),
                        ))
                    }
                    other => Ok(Value::err(
                        Type::Map(Box::new(Type::Json)),
                        Type::Str,
                        Value::string(format!("expected JSON object, got {}", json_kind(other))),
                    )),
                },
                _ => unreachable!("static checker guarantees json.as_object argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn value_to_json(value: &Value) -> Result<JsonValue, String> {
    match value {
        Value::Null => Ok(JsonValue::Null),
        Value::Bool(b) => Ok(JsonValue::Bool(*b)),
        Value::Int(i) => Ok(JsonValue::Number(*i as f64)),
        Value::Float(f) => Ok(JsonValue::Number(*f)),
        Value::String(s) => Ok(JsonValue::String(s.as_ref().to_string())),
        Value::Json(j) => Ok(j.as_ref().clone()),
        Value::Array(arr) => {
            let snap = arr.snapshot();
            let mut elems = Vec::with_capacity(snap.len());
            for v in &snap {
                elems.push(value_to_json(v)?);
            }
            Ok(JsonValue::Array(elems))
        }
        Value::Tuple(t) => {
            let elements = t.elements();
            let mut out = Vec::with_capacity(elements.len());
            for v in elements {
                out.push(value_to_json(v)?);
            }
            Ok(JsonValue::Array(out))
        }
        Value::Map(m) => {
            let entries = m.entries();
            let mut obj = BTreeMap::new();
            for (k, v) in entries {
                obj.insert(k, value_to_json(&v)?);
            }
            Ok(JsonValue::Object(obj))
        }
        Value::Record(r) => {
            let mut obj = BTreeMap::new();
            for (name, val) in r.fields() {
                obj.insert(name.clone(), value_to_json(val)?);
            }
            Ok(JsonValue::Object(obj))
        }
        Value::Option(opt) => match opt.payload() {
            Some(v) => value_to_json(v),
            None => Ok(JsonValue::Null),
        },
        Value::Result(res) => {
            let mut obj = BTreeMap::new();
            let tag = if res.is_ok() { "ok" } else { "err" };
            obj.insert("_variant".to_string(), JsonValue::String(tag.to_string()));
            obj.insert("payload".to_string(), value_to_json(res.payload())?);
            Ok(JsonValue::Object(obj))
        }
        Value::Enum(e) => match e.payload() {
            Some(payload) => {
                let mut obj = BTreeMap::new();
                obj.insert(
                    "_variant".to_string(),
                    JsonValue::String(e.variant().to_string()),
                );
                obj.insert("payload".to_string(), value_to_json(payload)?);
                Ok(JsonValue::Object(obj))
            }
            None => Ok(JsonValue::String(e.variant().to_string())),
        },
        Value::Function(_) => Err("function values cannot be serialized to JSON".to_string()),
        Value::Task(_) => Err("task values cannot be serialized to JSON".to_string()),
    }
}

fn json_variant_name(value: &JsonValue) -> Result<String, String> {
    match value {
        JsonValue::String(name) => Ok(name.clone()),
        JsonValue::Object(obj) => match obj.get("_variant") {
            Some(JsonValue::String(name)) => Ok(name.clone()),
            Some(other) => Err(format!(
                "expected adjacent enum _variant string, got {}",
                json_kind(other)
            )),
            None => Err("expected adjacent enum object with _variant field".to_string()),
        },
        other => Err(format!(
            "expected adjacent enum string or object, got {}",
            json_kind(other)
        )),
    }
}

fn json_variant_payload(value: &JsonValue) -> Result<JsonValue, String> {
    match value {
        JsonValue::Object(obj) => {
            match obj.get("_variant") {
                Some(JsonValue::String(_)) => {}
                Some(other) => {
                    return Err(format!(
                        "expected adjacent enum _variant string, got {}",
                        json_kind(other)
                    ));
                }
                None => return Err("expected adjacent enum object with _variant field".to_string()),
            }
            obj.get("payload")
                .cloned()
                .ok_or_else(|| "expected adjacent enum object with payload field".to_string())
        }
        JsonValue::String(_) => Err("adjacent enum string has no payload".to_string()),
        other => Err(format!(
            "expected adjacent enum object with payload, got {}",
            json_kind(other)
        )),
    }
}

fn apply_json_defaults_deep(mut value: JsonValue, defaults: &JsonValue) -> JsonValue {
    let (JsonValue::Object(value_obj), JsonValue::Object(default_obj)) = (&mut value, defaults)
    else {
        return value;
    };

    for (key, default_value) in default_obj {
        match value_obj.get_mut(key) {
            Some(existing) => {
                if matches!(existing, JsonValue::Object(_))
                    && matches!(default_value, JsonValue::Object(_))
                {
                    let merged = apply_json_defaults_deep(existing.clone(), default_value);
                    *existing = merged;
                }
            }
            None => {
                value_obj.insert(key.clone(), default_value.clone());
            }
        }
    }

    value
}

fn resolve_json_path(value: &JsonValue, path: &str) -> Result<JsonValue, String> {
    if path.is_empty() {
        return Ok(value.clone());
    }
    let mut current = value.clone();
    let mut chars = path.chars().peekable();
    let mut accumulated = String::new();
    loop {
        let mut segment = String::new();
        while let Some(&c) = chars.peek() {
            if c == '.' || c == '[' {
                break;
            }
            segment.push(c);
            chars.next();
        }
        if !segment.is_empty() {
            if !accumulated.is_empty() {
                accumulated.push('.');
            }
            accumulated.push_str(&segment);
            let JsonValue::Object(obj) = &current else {
                return Err(format!(
                    "at {accumulated}: expected object, got {}",
                    json_kind(&current)
                ));
            };
            current = obj
                .get(&segment)
                .cloned()
                .ok_or_else(|| format!("at {accumulated}: missing key '{segment}'"))?;
        }
        match chars.next() {
            None => return Ok(current),
            Some('.') => continue,
            Some('[') => {
                let mut index_str = String::new();
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    index_str.push(c);
                }
                let index: usize = index_str.parse().map_err(|_| {
                    format!("at {accumulated}: invalid array index '[{index_str}]'")
                })?;
                accumulated.push_str(&format!("[{index}]"));
                let JsonValue::Array(items) = &current else {
                    return Err(format!(
                        "at {accumulated}: expected array, got {}",
                        json_kind(&current)
                    ));
                };
                current = items
                    .get(index)
                    .cloned()
                    .ok_or_else(|| format!("at {accumulated}: index out of range"))?;
                if chars.peek() == Some(&'.') {
                    chars.next();
                }
            }
            Some(other) => return Err(format!("unexpected character '{other}' in path")),
        }
    }
}

fn register_delimited_text_stdlib(engine: &mut Engine) {
    register_parse_line(engine, "__nox_std_csv_parse_line", ',');
    register_format_row(engine, "__nox_std_csv_format_row", ',', false);
    register_parse_rows(engine, "__nox_std_csv_parse_rows", ',');
    register_format_rows(engine, "__nox_std_csv_format_rows", ',', false);
    register_parse_line(engine, "__nox_std_tsv_parse_line", '\t');
    register_format_row(engine, "__nox_std_tsv_format_row", '\t', true);
    register_parse_rows(engine, "__nox_std_tsv_parse_rows", '\t');
    register_format_rows(engine, "__nox_std_tsv_format_rows", '\t', true);
}

fn register_collection_stdlib(engine: &mut Engine) {
    register_array_stdlib(engine);
    register_map_stdlib(engine);
    register_option_result_stdlib(engine);
}

fn url_query_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        let is_unreserved = byte.is_ascii_alphanumeric()
            || byte == b'-'
            || byte == b'_'
            || byte == b'.'
            || byte == b'~';
        if is_unreserved {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

fn url_query_decode(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' => {
                if i + 2 >= bytes.len() {
                    return Err(format!("incomplete percent escape at offset {i}"));
                }
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                match (hi, lo) {
                    (Some(hi), Some(lo)) => {
                        out.push((hi * 16 + lo) as u8);
                        i += 3;
                    }
                    _ => {
                        return Err(format!("invalid percent escape at offset {i}"));
                    }
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|err| format!("decoded bytes are not valid UTF-8: {err}"))
}

fn url_parse(url: &str) -> Result<(String, String, i64, String, String), String> {
    let Some(scheme_end) = url.find("://") else {
        return Err("url is missing '://' scheme separator".to_string());
    };
    let scheme = url[..scheme_end].to_ascii_lowercase();
    if scheme.is_empty() {
        return Err("url scheme is empty".to_string());
    }
    let rest = &url[scheme_end + 3..];
    let (authority, path_and_query) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, ""),
    };
    if authority.is_empty() {
        return Err("url host is empty".to_string());
    }
    let (host, port_text) = match authority.find(':') {
        Some(idx) => (&authority[..idx], &authority[idx + 1..]),
        None => (authority, ""),
    };
    let port: i64 = if port_text.is_empty() {
        match scheme.as_str() {
            "http" => 80,
            "https" => 443,
            _ => -1,
        }
    } else {
        port_text
            .parse::<u16>()
            .map(|p| p as i64)
            .map_err(|_| format!("invalid port '{port_text}'"))?
    };
    let (path, query) = match path_and_query.find('?') {
        Some(idx) => (&path_and_query[..idx], &path_and_query[idx + 1..]),
        None => (path_and_query, ""),
    };
    let path = if path.is_empty() { "/" } else { path };
    Ok((
        scheme,
        host.to_string(),
        port,
        path.to_string(),
        query.to_string(),
    ))
}

fn url_build(scheme: &str, host: &str, port: i64, path: &str, query: &str) -> String {
    let path = if path.is_empty() { "/" } else { path };
    let default_port = match scheme.to_ascii_lowercase().as_str() {
        "http" => 80,
        "https" => 443,
        _ => -1,
    };
    let port_segment = if port == default_port || port < 0 {
        String::new()
    } else {
        format!(":{port}")
    };
    let query_segment = if query.is_empty() {
        String::new()
    } else {
        format!("?{query}")
    };
    format!("{scheme}://{host}{port_segment}{path}{query_segment}")
}

fn xorshift64_step(seed: i64) -> i64 {
    let mut state = seed as u64;
    if state == 0 {
        state = 0x9E37_79B9_7F4A_7C15;
    }
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    state as i64
}

fn register_random_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_random_next_int",
                Type::Tuple(vec![Type::Int, Type::Int]),
            )
            .param("seed", Type::Int)
            .param("min", Type::Int)
            .param("max", Type::Int),
            |args| match args {
                [Value::Int(seed), Value::Int(min), Value::Int(max)] => {
                    if min > max {
                        return Err(Diagnostic::new(
                            format!(
                                "random.next_int requires min <= max, got min={min}, max={max}"
                            ),
                            Span { start: 0, end: 0 },
                        ));
                    }
                    let next_seed = xorshift64_step(*seed);
                    let span = (*max as i128) - (*min as i128) + 1;
                    let unsigned = (next_seed as u64) as i128;
                    let value = *min as i128 + (unsigned.rem_euclid(span));
                    let next_value = value as i64;
                    Ok(Value::tuple(
                        vec![Type::Int, Type::Int],
                        vec![Value::Int(next_seed), Value::Int(next_value)],
                    ))
                }
                _ => unreachable!("static checker guarantees random.next_int argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_random_next_bool",
                Type::Tuple(vec![Type::Int, Type::Bool]),
            )
            .param("seed", Type::Int),
            |args| match args {
                [Value::Int(seed)] => {
                    let next_seed = xorshift64_step(*seed);
                    let bit = ((next_seed as u64) & 1) == 1;
                    Ok(Value::tuple(
                        vec![Type::Int, Type::Bool],
                        vec![Value::Int(next_seed), Value::Bool(bit)],
                    ))
                }
                _ => unreachable!("static checker guarantees random.next_bool argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_random_next_float_unit",
                Type::Tuple(vec![Type::Int, Type::Float]),
            )
            .param("seed", Type::Int),
            |args| match args {
                [Value::Int(seed)] => {
                    let next_seed = xorshift64_step(*seed);
                    let unsigned = (next_seed as u64) >> 11;
                    let denom = (1u64 << 53) as f64;
                    let value = unsigned as f64 / denom;
                    Ok(Value::tuple(
                        vec![Type::Int, Type::Float],
                        vec![Value::Int(next_seed), Value::Float(value)],
                    ))
                }
                _ => unreachable!("static checker guarantees random.next_float_unit argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn register_url_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_url_query_encode", Type::Str)
                .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => Ok(Value::string(url_query_encode(value.as_ref()))),
                _ => unreachable!("static checker guarantees url.query_encode argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_url_query_decode",
                Type::Result {
                    ok: Box::new(Type::Str),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str),
            |args| match args {
                [Value::String(value)] => match url_query_decode(value.as_ref()) {
                    Ok(decoded) => Ok(Value::ok(Type::Str, Type::Str, Value::string(decoded))),
                    Err(message) => Ok(Value::err(Type::Str, Type::Str, Value::string(message))),
                },
                _ => unreachable!("static checker guarantees url.query_decode argument type"),
            },
        )
        .expect("stdlib function registration is static");

    let parse_ok_type = Type::Tuple(vec![Type::Str, Type::Str, Type::Int, Type::Str, Type::Str]);
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_url_parse",
                Type::Result {
                    ok: Box::new(parse_ok_type.clone()),
                    err: Box::new(Type::Str),
                },
            )
            .param("url", Type::Str),
            move |args| match args {
                [Value::String(url)] => match url_parse(url.as_ref()) {
                    Ok((scheme, host, port, path, query)) => {
                        let element_types =
                            vec![Type::Str, Type::Str, Type::Int, Type::Str, Type::Str];
                        let elements = vec![
                            Value::string(scheme),
                            Value::string(host),
                            Value::Int(port),
                            Value::string(path),
                            Value::string(query),
                        ];
                        Ok(Value::ok(
                            parse_ok_type.clone(),
                            Type::Str,
                            Value::tuple(element_types, elements),
                        ))
                    }
                    Err(message) => Ok(Value::err(
                        parse_ok_type.clone(),
                        Type::Str,
                        Value::string(message),
                    )),
                },
                _ => unreachable!("static checker guarantees url.parse argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_url_build", Type::Str)
                .param("scheme", Type::Str)
                .param("host", Type::Str)
                .param("port", Type::Int)
                .param("path", Type::Str)
                .param("query", Type::Str),
            |args| match args {
                [
                    Value::String(scheme),
                    Value::String(host),
                    Value::Int(port),
                    Value::String(path),
                    Value::String(query),
                ] => Ok(Value::string(url_build(
                    scheme.as_ref(),
                    host.as_ref(),
                    *port,
                    path.as_ref(),
                    query.as_ref(),
                ))),
                _ => unreachable!("static checker guarantees url.build argument types"),
            },
        )
        .expect("stdlib function registration is static");
}

const HTTP_MAX_RESPONSE_BYTES: usize = 1_048_576;

#[derive(Debug, Clone)]
struct HttpResponseParts {
    status: i64,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn http_request(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: Option<&str>,
    timeout_ms: i64,
) -> Result<(i64, String), String> {
    let body_bytes = body.map(|s| s.as_bytes().to_vec());
    let response = http_request_bytes(method, url, headers, body_bytes.as_deref(), timeout_ms)?;
    let body_text = String::from_utf8_lossy(&response.body).to_string();
    Ok((response.status, body_text))
}

fn http_request_bytes(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: Option<&[u8]>,
    timeout_ms: i64,
) -> Result<HttpResponseParts, String> {
    validate_http_method(method)?;
    validate_http_headers(headers)?;
    let (scheme, host, port, path, query) = url_parse(url)?;
    if scheme != "http" {
        return Err(format!(
            "scheme '{scheme}' is not supported; only 'http' is implemented"
        ));
    }
    if port <= 0 || port > 65535 {
        return Err(format!("invalid port: {port}"));
    }
    let port = port as u16;
    let timeout = if timeout_ms > 0 {
        Some(Duration::from_millis(timeout_ms as u64))
    } else {
        Some(Duration::from_secs(30))
    };

    let mut stream = std::net::TcpStream::connect_timeout(
        &((host.as_str(), port)
            .to_socket_addrs()
            .map_err(|err| format!("resolve '{host}' failed: {err}"))?
            .next()
            .ok_or_else(|| format!("no addresses for '{host}'"))?),
        timeout.unwrap_or_else(|| Duration::from_secs(30)),
    )
    .map_err(|err| format!("connect failed: {err}"))?;
    stream
        .set_read_timeout(timeout)
        .map_err(|err| format!("set_read_timeout failed: {err}"))?;
    stream
        .set_write_timeout(timeout)
        .map_err(|err| format!("set_write_timeout failed: {err}"))?;

    let request_path = if query.is_empty() {
        path.clone()
    } else {
        format!("{path}?{query}")
    };
    let body_bytes = body.unwrap_or(&[]);
    let mut request = format!(
        "{method} {request_path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: nox/0.0.x\r\nAccept: */*\r\nConnection: close\r\n"
    );
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("content-length") {
            return Err("custom Content-Length header is not supported".to_string());
        }
        if has_default_http_header(name) {
            continue;
        }
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    if !body_bytes.is_empty() {
        request.push_str(&format!("Content-Length: {}\r\n", body_bytes.len()));
    }
    request.push_str("\r\n");
    use std::io::Write as _;
    let mut request_bytes = request.into_bytes();
    request_bytes.extend_from_slice(body_bytes);
    stream
        .write_all(&request_bytes)
        .map_err(|err| format!("write failed: {err}"))?;
    stream
        .flush()
        .map_err(|err| format!("flush failed: {err}"))?;

    use std::io::Read as _;
    let mut response: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = stream
            .read(&mut buf)
            .map_err(|err| format!("read failed: {err}"))?;
        if n == 0 {
            break;
        }
        if response.len() + n > HTTP_MAX_RESPONSE_BYTES {
            return Err(format!(
                "response exceeds {} byte cap",
                HTTP_MAX_RESPONSE_BYTES
            ));
        }
        response.extend_from_slice(&buf[..n]);
    }

    let header_end = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "missing header terminator".to_string())?;
    let header_text = std::str::from_utf8(&response[..header_end])
        .map_err(|_| "non-UTF-8 response headers".to_string())?;
    let mut header_lines = header_text.split("\r\n");
    let status_line = header_lines
        .next()
        .ok_or_else(|| "missing status line".to_string())?;
    let mut status_parts = status_line.splitn(3, ' ');
    status_parts.next();
    let status_code: i64 = status_parts
        .next()
        .and_then(|s| s.parse::<i64>().ok())
        .ok_or_else(|| format!("invalid status line: {status_line}"))?;
    let headers = parse_http_response_headers(header_lines)?;

    let body_bytes = response[header_end + 4..].to_vec();
    Ok(HttpResponseParts {
        status: status_code,
        headers,
        body: body_bytes,
    })
}

fn validate_http_method(method: &str) -> Result<(), String> {
    if method.is_empty() {
        return Err("HTTP method cannot be empty".to_string());
    }
    if !method.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'!' | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'.'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'|'
                    | b'~'
            )
    }) {
        return Err("HTTP method contains an invalid character".to_string());
    }
    Ok(())
}

fn validate_http_headers(headers: &BTreeMap<String, String>) -> Result<(), String> {
    for (name, value) in headers {
        if name.is_empty() {
            return Err("HTTP header name cannot be empty".to_string());
        }
        if !name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        }) {
            return Err(format!(
                "HTTP header name '{name}' contains an invalid character"
            ));
        }
        if value.bytes().any(|byte| matches!(byte, b'\r' | b'\n')) {
            return Err(format!("HTTP header '{name}' contains a newline"));
        }
    }
    Ok(())
}

fn has_default_http_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("host")
        || name.eq_ignore_ascii_case("user-agent")
        || name.eq_ignore_ascii_case("accept")
        || name.eq_ignore_ascii_case("connection")
}

fn parse_http_response_headers<'a>(
    lines: impl Iterator<Item = &'a str>,
) -> Result<BTreeMap<String, String>, String> {
    let mut headers = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(format!("invalid response header: {line}"));
        };
        let normalized = name.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Err("empty response header name".to_string());
        }
        let value = value.trim().to_string();
        headers
            .entry(normalized)
            .and_modify(|existing: &mut String| {
                existing.push_str(", ");
                existing.push_str(&value);
            })
            .or_insert(value);
    }
    Ok(headers)
}

fn normalize_http_headers(headers: BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut normalized = BTreeMap::new();
    for (name, value) in headers {
        let key = name.trim().to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        normalized
            .entry(key)
            .and_modify(|existing: &mut String| {
                existing.push_str(", ");
                existing.push_str(value.trim());
            })
            .or_insert_with(|| value.trim().to_string());
    }
    normalized
}

fn http_mock_response(
    method: &str,
    url: &str,
    mock_network: Option<&MockNetworkHandle>,
) -> Result<Option<HttpResponseParts>, String> {
    let Some(mock_network) = mock_network else {
        return Ok(None);
    };
    let (scheme, _, port, _, _) = url_parse(url)?;
    if scheme != "http" {
        return Err(format!(
            "scheme '{scheme}' is not supported; only 'http' is implemented"
        ));
    }
    if port <= 0 || port > 65535 {
        return Err(format!("invalid port: {port}"));
    }
    if let Some(mock) = mock_network.borrow().as_ref() {
        return match mock.http_response(method, url) {
            Some(response) => Ok(Some(HttpResponseParts {
                status: response.status,
                headers: response.headers,
                body: response.body,
            })),
            None => Err(format!("mock network has no {method} response for '{url}'")),
        };
    }
    Ok(None)
}

fn http_request_with_mock(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: Option<&str>,
    timeout_ms: i64,
    mock_network: Option<&MockNetworkHandle>,
) -> Result<(i64, String), String> {
    if let Some(response) = http_mock_response(method, url, mock_network)? {
        let body_text = String::from_utf8_lossy(&response.body).to_string();
        return Ok((response.status, body_text));
    }
    http_request(method, url, headers, body, timeout_ms)
}

fn http_request_bytes_with_mock(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: Option<&[u8]>,
    timeout_ms: i64,
    mock_network: Option<&MockNetworkHandle>,
) -> Result<HttpResponseParts, String> {
    if let Some(response) = http_mock_response(method, url, mock_network)? {
        return Ok(response);
    }
    http_request_bytes(method, url, headers, body, timeout_ms)
}

fn string_map_to_btree(map: &nox_core::Map) -> Result<BTreeMap<String, String>, String> {
    let mut headers = BTreeMap::new();
    for (key, value) in map.entries() {
        match value {
            Value::String(text) => {
                headers.insert(key, text.as_ref().to_string());
            }
            _ => return Err("header map values must be strings".to_string()),
        }
    }
    Ok(headers)
}

fn header_map_value(headers: BTreeMap<String, String>) -> Value {
    let entries = headers
        .into_iter()
        .map(|(name, value)| (name, Value::string(value)))
        .collect();
    Value::map(Type::Str, entries)
}

fn http_response_value(response: HttpResponseParts, binary: bool) -> Value {
    if binary {
        Value::tuple(
            vec![
                Type::Int,
                Type::Map(Box::new(Type::Str)),
                Type::Array(Box::new(Type::Int)),
            ],
            vec![
                Value::Int(response.status),
                header_map_value(response.headers),
                bytes_vec_to_array(response.body),
            ],
        )
    } else {
        Value::tuple(
            vec![Type::Int, Type::Map(Box::new(Type::Str)), Type::Str],
            vec![
                Value::Int(response.status),
                header_map_value(response.headers),
                Value::string(String::from_utf8_lossy(&response.body).to_string()),
            ],
        )
    }
}

fn register_http_stdlib(
    engine: &mut Engine,
    permissions: &RuntimePermissions,
    mock_network: Option<MockNetworkHandle>,
) {
    let network_allowed_get = permissions.network;
    let response_type = Type::Tuple(vec![Type::Int, Type::Str]);
    let empty_headers = BTreeMap::new();
    let response_type_get = response_type.clone();
    let mock_network_for_get = mock_network.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_http_get",
                Type::Result {
                    ok: Box::new(response_type.clone()),
                    err: Box::new(Type::Str),
                },
            )
            .param("url", Type::Str)
            .param("timeout_ms", Type::Int),
            move |args| {
                if !network_allowed_get {
                    return Err(call_capability_required("network", "http.get"));
                }
                match args {
                    [Value::String(url), Value::Int(timeout_ms)] => {
                        match http_request_with_mock(
                            "GET",
                            url.as_ref(),
                            &empty_headers,
                            None,
                            *timeout_ms,
                            mock_network_for_get.as_ref(),
                        ) {
                            Ok((status, body)) => {
                                let tuple = Value::tuple(
                                    vec![Type::Int, Type::Str],
                                    vec![Value::Int(status), Value::string(body)],
                                );
                                Ok(Value::ok(response_type_get.clone(), Type::Str, tuple))
                            }
                            Err(message) => Ok(Value::err(
                                response_type_get.clone(),
                                Type::Str,
                                Value::string(message),
                            )),
                        }
                    }
                    _ => unreachable!("static checker guarantees http.get argument types"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let network_allowed_post = permissions.network;
    let response_type_post = response_type.clone();
    let mock_network_for_post = mock_network.clone();
    let empty_headers_post = BTreeMap::new();
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_http_post",
                Type::Result {
                    ok: Box::new(response_type),
                    err: Box::new(Type::Str),
                },
            )
            .param("url", Type::Str)
            .param("body", Type::Str)
            .param("timeout_ms", Type::Int),
            move |args| {
                if !network_allowed_post {
                    return Err(call_capability_required("network", "http.post"));
                }
                match args {
                    [Value::String(url), Value::String(body), Value::Int(timeout_ms)] => {
                        match http_request_with_mock(
                            "POST",
                            url.as_ref(),
                            &empty_headers_post,
                            Some(body.as_ref()),
                            *timeout_ms,
                            mock_network_for_post.as_ref(),
                        ) {
                            Ok((status, body)) => {
                                let tuple = Value::tuple(
                                    vec![Type::Int, Type::Str],
                                    vec![Value::Int(status), Value::string(body)],
                                );
                                Ok(Value::ok(response_type_post.clone(), Type::Str, tuple))
                            }
                            Err(message) => Ok(Value::err(
                                response_type_post.clone(),
                                Type::Str,
                                Value::string(message),
                            )),
                        }
                    }
                    _ => unreachable!("static checker guarantees http.post argument types"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let network_allowed_request = permissions.network;
    let request_response_type =
        Type::Tuple(vec![Type::Int, Type::Map(Box::new(Type::Str)), Type::Str]);
    let request_response_type_inner = request_response_type.clone();
    let mock_network_for_request = mock_network.clone();
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_http_request",
                Type::Result {
                    ok: Box::new(request_response_type.clone()),
                    err: Box::new(Type::Str),
                },
            )
            .param("method", Type::Str)
            .param("url", Type::Str)
            .param("headers", Type::Map(Box::new(Type::Str)))
            .param("body", Type::Str)
            .param("timeout_ms", Type::Int),
            move |args| {
                if !network_allowed_request {
                    return Err(call_capability_required("network", "http.request"));
                }
                match args {
                    [
                        Value::String(method),
                        Value::String(url),
                        Value::Map(headers),
                        Value::String(body),
                        Value::Int(timeout_ms),
                    ] => match string_map_to_btree(headers) {
                        Ok(headers) => match http_request_bytes_with_mock(
                            method.as_ref(),
                            url.as_ref(),
                            &headers,
                            Some(body.as_ref().as_bytes()),
                            *timeout_ms,
                            mock_network_for_request.as_ref(),
                        ) {
                            Ok(response) => Ok(Value::ok(
                                request_response_type_inner.clone(),
                                Type::Str,
                                http_response_value(response, false),
                            )),
                            Err(message) => Ok(Value::err(
                                request_response_type_inner.clone(),
                                Type::Str,
                                Value::string(message),
                            )),
                        },
                        Err(message) => Ok(Value::err(
                            request_response_type_inner.clone(),
                            Type::Str,
                            Value::string(message),
                        )),
                    },
                    _ => unreachable!("static checker guarantees http.request argument types"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let network_allowed_get_binary = permissions.network;
    let binary_response_type = Type::Tuple(vec![Type::Int, Type::Array(Box::new(Type::Int))]);
    let binary_response_type_get = binary_response_type.clone();
    let mock_network_for_get_binary = mock_network.clone();
    let empty_headers_get_binary = BTreeMap::new();
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_http_get_binary",
                Type::Result {
                    ok: Box::new(binary_response_type.clone()),
                    err: Box::new(Type::Str),
                },
            )
            .param("url", Type::Str)
            .param("timeout_ms", Type::Int),
            move |args| {
                if !network_allowed_get_binary {
                    return Err(call_capability_required("network", "http.get_binary"));
                }
                match args {
                    [Value::String(url), Value::Int(timeout_ms)] => {
                        match http_request_bytes_with_mock(
                            "GET",
                            url.as_ref(),
                            &empty_headers_get_binary,
                            None,
                            *timeout_ms,
                            mock_network_for_get_binary.as_ref(),
                        ) {
                            Ok(response) => {
                                let tuple = Value::tuple(
                                    vec![Type::Int, Type::Array(Box::new(Type::Int))],
                                    vec![
                                        Value::Int(response.status),
                                        bytes_vec_to_array(response.body),
                                    ],
                                );
                                Ok(Value::ok(
                                    binary_response_type_get.clone(),
                                    Type::Str,
                                    tuple,
                                ))
                            }
                            Err(message) => Ok(Value::err(
                                binary_response_type_get.clone(),
                                Type::Str,
                                Value::string(message),
                            )),
                        }
                    }
                    _ => unreachable!("static checker guarantees http.get_binary argument types"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let network_allowed_post_binary = permissions.network;
    let binary_response_type_post = binary_response_type;
    let mock_network_for_post_binary = mock_network.clone();
    let empty_headers_post_binary = BTreeMap::new();
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_http_post_binary",
                Type::Result {
                    ok: Box::new(binary_response_type_post.clone()),
                    err: Box::new(Type::Str),
                },
            )
            .param("url", Type::Str)
            .param("body", Type::Array(Box::new(Type::Int)))
            .param("timeout_ms", Type::Int),
            move |args| {
                if !network_allowed_post_binary {
                    return Err(call_capability_required("network", "http.post_binary"));
                }
                match args {
                    [Value::String(url), Value::Array(body), Value::Int(timeout_ms)] => {
                        match bytes_array_to_vec(body) {
                            Ok(body_bytes) => match http_request_bytes_with_mock(
                                "POST",
                                url.as_ref(),
                                &empty_headers_post_binary,
                                Some(&body_bytes),
                                *timeout_ms,
                                mock_network_for_post_binary.as_ref(),
                            ) {
                                Ok(response) => {
                                    let tuple = Value::tuple(
                                        vec![Type::Int, Type::Array(Box::new(Type::Int))],
                                        vec![
                                            Value::Int(response.status),
                                            bytes_vec_to_array(response.body),
                                        ],
                                    );
                                    Ok(Value::ok(
                                        binary_response_type_post.clone(),
                                        Type::Str,
                                        tuple,
                                    ))
                                }
                                Err(message) => Ok(Value::err(
                                    binary_response_type_post.clone(),
                                    Type::Str,
                                    Value::string(message),
                                )),
                            },
                            Err(message) => Ok(Value::err(
                                binary_response_type_post.clone(),
                                Type::Str,
                                Value::string(message),
                            )),
                        }
                    }
                    _ => unreachable!("static checker guarantees http.post_binary argument types"),
                }
            },
        )
        .expect("stdlib function registration is static");

    let network_allowed_request_binary = permissions.network;
    let request_binary_response_type = Type::Tuple(vec![
        Type::Int,
        Type::Map(Box::new(Type::Str)),
        Type::Array(Box::new(Type::Int)),
    ]);
    let request_binary_response_type_inner = request_binary_response_type.clone();
    let mock_network_for_request_binary = mock_network;
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_http_request_binary",
                Type::Result {
                    ok: Box::new(request_binary_response_type.clone()),
                    err: Box::new(Type::Str),
                },
            )
            .param("method", Type::Str)
            .param("url", Type::Str)
            .param("headers", Type::Map(Box::new(Type::Str)))
            .param("body", Type::Array(Box::new(Type::Int)))
            .param("timeout_ms", Type::Int),
            move |args| {
                if !network_allowed_request_binary {
                    return Err(call_capability_required("network", "http.request_binary"));
                }
                match args {
                    [
                        Value::String(method),
                        Value::String(url),
                        Value::Map(headers),
                        Value::Array(body),
                        Value::Int(timeout_ms),
                    ] => match (string_map_to_btree(headers), bytes_array_to_vec(body)) {
                        (Ok(headers), Ok(body_bytes)) => match http_request_bytes_with_mock(
                            method.as_ref(),
                            url.as_ref(),
                            &headers,
                            Some(&body_bytes),
                            *timeout_ms,
                            mock_network_for_request_binary.as_ref(),
                        ) {
                            Ok(response) => Ok(Value::ok(
                                request_binary_response_type_inner.clone(),
                                Type::Str,
                                http_response_value(response, true),
                            )),
                            Err(message) => Ok(Value::err(
                                request_binary_response_type_inner.clone(),
                                Type::Str,
                                Value::string(message),
                            )),
                        },
                        (Err(message), _) | (_, Err(message)) => Ok(Value::err(
                            request_binary_response_type_inner.clone(),
                            Type::Str,
                            Value::string(message),
                        )),
                    },
                    _ => unreachable!("static checker guarantees http.request_binary argument types"),
                }
            },
        )
        .expect("stdlib function registration is static");
}

fn register_http_lsp_stubs(engine: &mut Engine) {
    let response_type = Type::Tuple(vec![Type::Int, Type::Str]);
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_http_get",
                Type::Result {
                    ok: Box::new(response_type.clone()),
                    err: Box::new(Type::Str),
                },
            )
            .param("url", Type::Str)
            .param("timeout_ms", Type::Int),
            |_| Err(call_capability_required("network", "http.get")),
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_http_post",
                Type::Result {
                    ok: Box::new(response_type),
                    err: Box::new(Type::Str),
                },
            )
            .param("url", Type::Str)
            .param("body", Type::Str)
            .param("timeout_ms", Type::Int),
            |_| Err(call_capability_required("network", "http.post")),
        )
        .expect("stdlib function registration is static");

    let request_response_type =
        Type::Tuple(vec![Type::Int, Type::Map(Box::new(Type::Str)), Type::Str]);
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_http_request",
                Type::Result {
                    ok: Box::new(request_response_type),
                    err: Box::new(Type::Str),
                },
            )
            .param("method", Type::Str)
            .param("url", Type::Str)
            .param("headers", Type::Map(Box::new(Type::Str)))
            .param("body", Type::Str)
            .param("timeout_ms", Type::Int),
            |_| Err(call_capability_required("network", "http.request")),
        )
        .expect("stdlib function registration is static");

    let binary_response_type = Type::Tuple(vec![Type::Int, Type::Array(Box::new(Type::Int))]);
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_http_get_binary",
                Type::Result {
                    ok: Box::new(binary_response_type.clone()),
                    err: Box::new(Type::Str),
                },
            )
            .param("url", Type::Str)
            .param("timeout_ms", Type::Int),
            |_| Err(call_capability_required("network", "http.get_binary")),
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_http_post_binary",
                Type::Result {
                    ok: Box::new(binary_response_type),
                    err: Box::new(Type::Str),
                },
            )
            .param("url", Type::Str)
            .param("body", Type::Array(Box::new(Type::Int)))
            .param("timeout_ms", Type::Int),
            |_| Err(call_capability_required("network", "http.post_binary")),
        )
        .expect("stdlib function registration is static");

    let request_binary_response_type = Type::Tuple(vec![
        Type::Int,
        Type::Map(Box::new(Type::Str)),
        Type::Array(Box::new(Type::Int)),
    ]);
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_http_request_binary",
                Type::Result {
                    ok: Box::new(request_binary_response_type),
                    err: Box::new(Type::Str),
                },
            )
            .param("method", Type::Str)
            .param("url", Type::Str)
            .param("headers", Type::Map(Box::new(Type::Str)))
            .param("body", Type::Array(Box::new(Type::Int)))
            .param("timeout_ms", Type::Int),
            |_| Err(call_capability_required("network", "http.request_binary")),
        )
        .expect("stdlib function registration is static");
}

fn register_path_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_path_join", Type::Str)
                .param("left", Type::Str)
                .param("right", Type::Str),
            |args| match args {
                [Value::String(left), Value::String(right)] => Ok(Value::string(
                    Path::new(left.as_ref())
                        .join(right.as_ref())
                        .to_string_lossy()
                        .into_owned(),
                )),
                _ => unreachable!("static checker guarantees path.join argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_path_basename", Type::Str).param("path", Type::Str),
            |args| match args {
                [Value::String(path)] => Ok(Value::string(
                    Path::new(path.as_ref())
                        .file_name()
                        .map(|value| value.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                )),
                _ => unreachable!("static checker guarantees path.basename argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_path_dirname", Type::Str).param("path", Type::Str),
            |args| match args {
                [Value::String(path)] => Ok(Value::string(
                    Path::new(path.as_ref())
                        .parent()
                        .map(|value| value.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                )),
                _ => unreachable!("static checker guarantees path.dirname argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_path_extension", Type::Str)
                .param("path", Type::Str),
            |args| match args {
                [Value::String(path)] => Ok(Value::string(
                    Path::new(path.as_ref())
                        .extension()
                        .map(|value| value.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                )),
                _ => unreachable!("static checker guarantees path.extension argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_path_normalize", Type::Str)
                .param("path", Type::Str),
            |args| match args {
                [Value::String(path)] => Ok(Value::string(
                    normalize_lexical(Path::new(path.as_ref()))
                        .to_string_lossy()
                        .into_owned(),
                )),
                _ => unreachable!("static checker guarantees path.normalize argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn register_lsp_process_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_process_argv", Type::Array(Box::new(Type::Str))),
            |_| Ok(Value::array(Type::Str, Vec::new())),
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_process_read_stdin", Type::Str),
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
            HostFunctionBuilder::new("__nox_std_process_print_err", Type::Null)
                .param("value", Type::Str),
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
            HostFunctionBuilder::new("__nox_std_process_exit", Type::Null).param("code", Type::Int),
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
                "__nox_std_process_run",
                Type::Result {
                    ok: Box::new(Type::Tuple(vec![Type::Int, Type::Str, Type::Str])),
                    err: Box::new(Type::Str),
                },
            )
            .param("program", Type::Str)
            .param("args", Type::Array(Box::new(Type::Str)))
            .param("stdin", Type::Str)
            .param("timeout_ms", Type::Int),
            |_| Err(call_capability_required("process run", "process.run")),
        )
        .expect("stdlib function registration is static");
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_process_run_with",
                Type::Result {
                    ok: Box::new(Type::Tuple(vec![Type::Int, Type::Str, Type::Str])),
                    err: Box::new(Type::Str),
                },
            )
            .param("program", Type::Str)
            .param("args", Type::Array(Box::new(Type::Str)))
            .param("stdin", Type::Str)
            .param("timeout_ms", Type::Int)
            .param("cwd", Type::Str)
            .param(
                "env_pairs",
                Type::Array(Box::new(Type::Tuple(vec![Type::Str, Type::Str]))),
            ),
            |_| Err(call_capability_required("process run", "process.run")),
        )
        .expect("stdlib function registration is static");
}

fn register_array_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_array_len", Type::Int)
                .type_param("T")
                .param(
                    "values",
                    Type::Array(Box::new(Type::Generic("T".to_string()))),
                ),
            |args| match args {
                [Value::Array(values)] => Ok(Value::Int(values.elements().len() as i64)),
                _ => unreachable!("static checker guarantees array.len argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_array_is_empty", Type::Bool)
                .type_param("T")
                .param(
                    "values",
                    Type::Array(Box::new(Type::Generic("T".to_string()))),
                ),
            |args| match args {
                [Value::Array(values)] => Ok(Value::Bool(values.elements().is_empty())),
                _ => unreachable!("static checker guarantees array.is_empty argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_array_push_copy",
                Type::Array(Box::new(Type::Generic("T".to_string()))),
            )
            .type_param("T")
            .param(
                "values",
                Type::Array(Box::new(Type::Generic("T".to_string()))),
            )
            .param("value", Type::Generic("T".to_string())),
            |args| match args {
                [Value::Array(values), value] => {
                    let mut elements = values.elements().to_vec();
                    elements.push(value.clone());
                    Ok(Value::array(values.element_type().clone(), elements))
                }
                _ => unreachable!("static checker guarantees array.push_copy argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_array_concat",
                Type::Array(Box::new(Type::Generic("T".to_string()))),
            )
            .type_param("T")
            .param(
                "left",
                Type::Array(Box::new(Type::Generic("T".to_string()))),
            )
            .param(
                "right",
                Type::Array(Box::new(Type::Generic("T".to_string()))),
            ),
            |args| match args {
                [Value::Array(left), Value::Array(right)] => {
                    let mut elements = left.elements();
                    elements.extend(right.elements());
                    Ok(Value::array(left.element_type().clone(), elements))
                }
                _ => unreachable!("static checker guarantees array.concat argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_array_slice_copy",
                Type::Result {
                    ok: Box::new(Type::Array(Box::new(Type::Generic("T".to_string())))),
                    err: Box::new(Type::Str),
                },
            )
            .type_param("T")
            .param(
                "values",
                Type::Array(Box::new(Type::Generic("T".to_string()))),
            )
            .param("start", Type::Int)
            .param("length", Type::Int),
            |args| match args {
                [Value::Array(values), Value::Int(start), Value::Int(length)] => {
                    let ok_type = Type::Array(Box::new(values.element_type().clone()));
                    match slice_array(&values.elements(), *start, *length) {
                        Ok(elements) => Ok(Value::ok(
                            ok_type.clone(),
                            Type::Str,
                            Value::array(values.element_type().clone(), elements),
                        )),
                        Err(message) => Ok(Value::err(ok_type, Type::Str, Value::string(message))),
                    }
                }
                _ => unreachable!("static checker guarantees array.slice_copy argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_array_reverse_copy",
                Type::Array(Box::new(Type::Generic("T".to_string()))),
            )
            .type_param("T")
            .param(
                "values",
                Type::Array(Box::new(Type::Generic("T".to_string()))),
            ),
            |args| match args {
                [Value::Array(values)] => {
                    let mut elements = values.elements();
                    elements.reverse();
                    Ok(Value::array(values.element_type().clone(), elements))
                }
                _ => unreachable!("static checker guarantees array.reverse_copy argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_array_sort_copy_int",
                Type::Array(Box::new(Type::Int)),
            )
            .param("values", Type::Array(Box::new(Type::Int))),
            |args| match args {
                [Value::Array(values)] => {
                    let mut ints = values
                        .elements()
                        .iter()
                        .map(|value| match value {
                            Value::Int(value) => Ok(*value),
                            _ => unreachable!("static checker guarantees int array elements"),
                        })
                        .collect::<Result<Vec<_>, Diagnostic>>()?;
                    ints.sort_unstable();
                    Ok(Value::array(
                        Type::Int,
                        ints.into_iter().map(Value::Int).collect(),
                    ))
                }
                _ => unreachable!("static checker guarantees array.sort_copy_int argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_array_sort_copy_str",
                Type::Array(Box::new(Type::Str)),
            )
            .param("values", Type::Array(Box::new(Type::Str))),
            |args| match args {
                [Value::Array(values)] => {
                    let mut strings = string_array_values(&values.elements())?;
                    strings.sort();
                    Ok(Value::array(
                        Type::Str,
                        strings.into_iter().map(Value::string).collect(),
                    ))
                }
                _ => unreachable!("static checker guarantees array.sort_copy_str argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_array_set",
                Type::Result {
                    ok: Box::new(Type::Null),
                    err: Box::new(Type::Str),
                },
            )
            .type_param("T")
            .param(
                "values",
                Type::Array(Box::new(Type::Generic("T".to_string()))),
            )
            .param("index", Type::Int)
            .param("value", Type::Generic("T".to_string())),
            |args| match args {
                [Value::Array(values), Value::Int(index), value] => {
                    let ok_type = Type::Null;
                    let err_type = Type::Str;
                    let len = values.len();
                    let idx = match usize::try_from(*index) {
                        Ok(i) if i < len => i,
                        _ => {
                            return Ok(Value::err(
                                ok_type,
                                err_type,
                                Value::string(format!(
                                    "index {index} out of bounds for array of length {len}"
                                )),
                            ));
                        }
                    };
                    values.set(idx, value.clone()).ok();
                    Ok(Value::ok(ok_type, err_type, Value::Null))
                }
                _ => unreachable!("static checker guarantees array.set argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_array_append", Type::Null)
                .type_param("T")
                .param(
                    "values",
                    Type::Array(Box::new(Type::Generic("T".to_string()))),
                )
                .param("value", Type::Generic("T".to_string())),
            |args| match args {
                [Value::Array(values), value] => {
                    values.try_push(value.clone()).map_err(|max| {
                        Diagnostic::new(
                            format!("array length would exceed configured cap of {max} elements"),
                            Span { start: 0, end: 0 },
                        )
                        .with_code("runtime.array-length-cap")
                    })?;
                    Ok(Value::Null)
                }
                _ => unreachable!("static checker guarantees array.append argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_array_pop",
                Type::Option(Box::new(Type::Generic("T".to_string()))),
            )
            .type_param("T")
            .param(
                "values",
                Type::Array(Box::new(Type::Generic("T".to_string()))),
            ),
            |args| match args {
                [Value::Array(values)] => {
                    let payload_type = values.element_type().clone();
                    match values.pop() {
                        Some(value) => Ok(Value::some(payload_type, value)),
                        None => Ok(Value::none(payload_type)),
                    }
                }
                _ => unreachable!("static checker guarantees array.pop argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn register_map_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_map_entries",
                Type::Array(Box::new(Type::Tuple(vec![
                    Type::Str,
                    Type::Generic("T".to_string()),
                ]))),
            )
            .type_param("T")
            .param(
                "values",
                Type::Map(Box::new(Type::Generic("T".to_string()))),
            ),
            |args| match args {
                [Value::Map(values)] => Ok(map_entries(values.as_ref())),
                _ => unreachable!("static checker guarantees map.entries argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_map_merge",
                Type::Map(Box::new(Type::Generic("T".to_string()))),
            )
            .type_param("T")
            .param("left", Type::Map(Box::new(Type::Generic("T".to_string()))))
            .param("right", Type::Map(Box::new(Type::Generic("T".to_string())))),
            |args| match args {
                [Value::Map(left), Value::Map(right)] => {
                    let mut entries = left.entries().clone();
                    entries.extend(right.entries().clone());
                    Ok(Value::map(left.value_type().clone(), entries))
                }
                _ => unreachable!("static checker guarantees map.merge argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new(
                "__nox_std_map_remove_copy",
                Type::Map(Box::new(Type::Generic("T".to_string()))),
            )
            .type_param("T")
            .param(
                "values",
                Type::Map(Box::new(Type::Generic("T".to_string()))),
            )
            .param("key", Type::Str),
            |args| match args {
                [Value::Map(values), Value::String(key)] => {
                    let mut entries = values.entries().clone();
                    entries.remove(key.as_ref());
                    Ok(Value::map(values.value_type().clone(), entries))
                }
                _ => unreachable!("static checker guarantees map.remove_copy argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_map_set", Type::Null)
                .type_param("T")
                .param(
                    "values",
                    Type::Map(Box::new(Type::Generic("T".to_string()))),
                )
                .param("key", Type::Str)
                .param("value", Type::Generic("T".to_string())),
            |args| match args {
                [Value::Map(values), Value::String(key), value] => {
                    values
                        .try_set(key.as_ref().to_string(), value.clone())
                        .map_err(|max| {
                            Diagnostic::new(
                                format!("map size would exceed configured cap of {max} entries"),
                                Span { start: 0, end: 0 },
                            )
                            .with_code("runtime.map-size-cap")
                        })?;
                    Ok(Value::Null)
                }
                _ => unreachable!("static checker guarantees map.set argument types"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_map_delete", Type::Bool)
                .type_param("T")
                .param(
                    "values",
                    Type::Map(Box::new(Type::Generic("T".to_string()))),
                )
                .param("key", Type::Str),
            |args| match args {
                [Value::Map(values), Value::String(key)] => {
                    Ok(Value::Bool(values.delete(key.as_ref())))
                }
                _ => unreachable!("static checker guarantees map.delete argument types"),
            },
        )
        .expect("stdlib function registration is static");
}

fn register_option_result_stdlib(engine: &mut Engine) {
    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_option_is_some", Type::Bool)
                .type_param("T")
                .param(
                    "value",
                    Type::Option(Box::new(Type::Generic("T".to_string()))),
                ),
            |args| match args {
                [Value::Option(value)] => Ok(Value::Bool(value.payload().is_some())),
                _ => unreachable!("static checker guarantees option.is_some argument type"),
            },
        )
        .expect("stdlib function registration is static");

    engine
        .register_host_function(
            HostFunctionBuilder::new("__nox_std_result_is_ok", Type::Bool)
                .type_param("T")
                .type_param("E")
                .param(
                    "value",
                    Type::Result {
                        ok: Box::new(Type::Generic("T".to_string())),
                        err: Box::new(Type::Generic("E".to_string())),
                    },
                ),
            |args| match args {
                [Value::Result(value)] => Ok(Value::Bool(value.is_ok())),
                _ => unreachable!("static checker guarantees result.is_ok argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn register_parse_line(engine: &mut Engine, name: &'static str, delimiter: char) {
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                name,
                Type::Result {
                    ok: Box::new(Type::Array(Box::new(Type::Str))),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str),
            move |args| match args {
                [Value::String(value)] => match parse_delimited_line(value.as_ref(), delimiter) {
                    Ok(fields) => Ok(Value::ok(
                        Type::Array(Box::new(Type::Str)),
                        Type::Str,
                        Value::array(Type::Str, fields.into_iter().map(Value::string).collect()),
                    )),
                    Err(message) => Ok(Value::err(
                        Type::Array(Box::new(Type::Str)),
                        Type::Str,
                        Value::string(message),
                    )),
                },
                _ => unreachable!("static checker guarantees delimited parse argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn register_format_row(engine: &mut Engine, name: &'static str, delimiter: char, fallible: bool) {
    let return_type = if fallible {
        Type::Result {
            ok: Box::new(Type::Str),
            err: Box::new(Type::Str),
        }
    } else {
        Type::Str
    };
    engine
        .register_host_function(
            HostFunctionBuilder::new(name, return_type)
                .param("values", Type::Array(Box::new(Type::Str))),
            move |args| match args {
                [Value::Array(values)] => {
                    let values = string_array_values(&values.elements())?;
                    match format_delimited_row(&values, delimiter) {
                        Ok(row) if fallible => {
                            Ok(Value::ok(Type::Str, Type::Str, Value::string(row)))
                        }
                        Ok(row) => Ok(Value::string(row)),
                        Err(message) if fallible => {
                            Ok(Value::err(Type::Str, Type::Str, Value::string(message)))
                        }
                        Err(message) => Err(string_argument_error_owned(message)),
                    }
                }
                _ => unreachable!("static checker guarantees delimited format argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn register_parse_rows(engine: &mut Engine, name: &'static str, delimiter: char) {
    let row_type = Type::Array(Box::new(Type::Str));
    let rows_type = Type::Array(Box::new(row_type.clone()));
    engine
        .register_host_function(
            HostFunctionBuilder::new(
                name,
                Type::Result {
                    ok: Box::new(rows_type.clone()),
                    err: Box::new(Type::Str),
                },
            )
            .param("value", Type::Str),
            move |args| match args {
                [Value::String(value)] => match parse_delimited_rows(value.as_ref(), delimiter) {
                    Ok(rows) => {
                        let row_values = rows
                            .into_iter()
                            .map(|row| {
                                Value::array(
                                    Type::Str,
                                    row.into_iter().map(Value::string).collect(),
                                )
                            })
                            .collect();
                        Ok(Value::ok(
                            rows_type.clone(),
                            Type::Str,
                            Value::array(row_type.clone(), row_values),
                        ))
                    }
                    Err((line, message)) => Ok(Value::err(
                        rows_type.clone(),
                        Type::Str,
                        Value::string(format!("line {line}: {message}")),
                    )),
                },
                _ => unreachable!("static checker guarantees delimited parse_rows argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

fn register_format_rows(engine: &mut Engine, name: &'static str, delimiter: char, fallible: bool) {
    let return_type = if fallible {
        Type::Result {
            ok: Box::new(Type::Str),
            err: Box::new(Type::Str),
        }
    } else {
        Type::Str
    };
    engine
        .register_host_function(
            HostFunctionBuilder::new(name, return_type).param(
                "values",
                Type::Array(Box::new(Type::Array(Box::new(Type::Str)))),
            ),
            move |args| match args {
                [Value::Array(rows)] => {
                    let mut lines = Vec::new();
                    for (index, row) in rows.elements().iter().enumerate() {
                        let Value::Array(row) = row else {
                            return Err(Diagnostic::new(
                                "format_rows values must contain string rows",
                                Span { start: 0, end: 0 },
                            ));
                        };
                        let row_values = string_array_values(&row.elements())?;
                        match format_delimited_row(&row_values, delimiter) {
                            Ok(line) => lines.push(line),
                            Err(message) if fallible => {
                                return Ok(Value::err(
                                    Type::Str,
                                    Type::Str,
                                    Value::string(format!("line {}: {message}", index + 1)),
                                ));
                            }
                            Err(message) => return Err(string_argument_error_owned(message)),
                        }
                    }
                    let text = lines.join("\n");
                    if fallible {
                        Ok(Value::ok(Type::Str, Type::Str, Value::string(text)))
                    } else {
                        Ok(Value::string(text))
                    }
                }
                _ => unreachable!("static checker guarantees delimited format_rows argument type"),
            },
        )
        .expect("stdlib function registration is static");
}

struct JsonParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn parse(mut self) -> Result<JsonValue, String> {
        let value = self.parse_value()?;
        self.skip_ws();
        if self.pos != self.input.len() {
            return Err("unexpected trailing JSON input".to_string());
        }
        Ok(value)
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.parse_literal("null", JsonValue::Null),
            Some(b't') => self.parse_literal("true", JsonValue::Bool(true)),
            Some(b'f') => self.parse_literal("false", JsonValue::Bool(false)),
            Some(b'"') => self.parse_string().map(JsonValue::String),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-' | b'0'..=b'9') => self.parse_number().map(JsonValue::Number),
            Some(_) => Err("expected JSON value".to_string()),
            None => Err("expected JSON value".to_string()),
        }
    }

    fn parse_literal(&mut self, literal: &str, value: JsonValue) -> Result<JsonValue, String> {
        if self.input[self.pos..].starts_with(literal) {
            self.pos += literal.len();
            Ok(value)
        } else {
            Err(format!("expected JSON literal '{literal}'"))
        }
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect_byte(b'[')?;
        let mut values = Vec::new();
        self.skip_ws();
        if self.consume_byte(b']') {
            return Ok(JsonValue::Array(values));
        }
        loop {
            values.push(self.parse_value()?);
            self.skip_ws();
            if self.consume_byte(b']') {
                break;
            }
            self.expect_byte(b',')?;
        }
        Ok(JsonValue::Array(values))
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect_byte(b'{')?;
        let mut entries = BTreeMap::new();
        self.skip_ws();
        if self.consume_byte(b'}') {
            return Ok(JsonValue::Object(entries));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect_byte(b':')?;
            let value = self.parse_value()?;
            entries.insert(key, value);
            self.skip_ws();
            if self.consume_byte(b'}') {
                break;
            }
            self.expect_byte(b',')?;
        }
        Ok(JsonValue::Object(entries))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect_byte(b'"')?;
        let mut output = String::new();
        while let Some(byte) = self.next() {
            match byte {
                b'"' => return Ok(output),
                b'\\' => {
                    let escaped = self
                        .next()
                        .ok_or_else(|| "unterminated JSON escape".to_string())?;
                    match escaped {
                        b'"' => output.push('"'),
                        b'\\' => output.push('\\'),
                        b'/' => output.push('/'),
                        b'b' => output.push('\u{0008}'),
                        b'f' => output.push('\u{000c}'),
                        b'n' => output.push('\n'),
                        b'r' => output.push('\r'),
                        b't' => output.push('\t'),
                        b'u' => output.push(self.parse_unicode_escape()?),
                        _ => return Err("unsupported JSON string escape".to_string()),
                    }
                }
                0x00..=0x1f => return Err("JSON string contains control character".to_string()),
                _ => {
                    let ch = self.input[self.pos - 1..]
                        .chars()
                        .next()
                        .expect("byte came from input");
                    output.push(ch);
                    self.pos += ch.len_utf8() - 1;
                }
            }
        }
        Err("unterminated JSON string".to_string())
    }

    fn parse_unicode_escape(&mut self) -> Result<char, String> {
        let mut value = 0_u32;
        for _ in 0..4 {
            let byte = self
                .next()
                .ok_or_else(|| "unterminated JSON unicode escape".to_string())?;
            value = value
                .checked_mul(16)
                .and_then(|value| byte.to_digit(16).map(|digit| value + digit))
                .ok_or_else(|| "invalid JSON unicode escape".to_string())?;
        }
        char::from_u32(value).ok_or_else(|| "invalid JSON unicode scalar".to_string())
    }

    fn parse_number(&mut self) -> Result<f64, String> {
        let start = self.pos;
        self.consume_byte(b'-');
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
            }
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err("invalid JSON number".to_string()),
        }
        if self.consume_byte(b'.') {
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err("invalid JSON number".to_string());
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            let _ = self.consume_byte(b'+') || self.consume_byte(b'-');
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err("invalid JSON number".to_string());
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let number: f64 = self.input[start..self.pos]
            .parse()
            .map_err(|_| "invalid JSON number".to_string())?;
        if number.is_finite() {
            Ok(number)
        } else {
            Err("JSON number is not finite".to_string())
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.pos += 1;
        }
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), String> {
        if self.consume_byte(expected) {
            Ok(())
        } else {
            Err(format!("expected '{}'", expected as char))
        }
    }

    fn consume_byte(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        Some(byte)
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }
}

trait JsonHexDigit {
    fn to_digit(self, radix: u32) -> Option<u32>;
}

impl JsonHexDigit for u8 {
    fn to_digit(self, radix: u32) -> Option<u32> {
        (self as char).to_digit(radix)
    }
}

fn substring_by_char(value: &str, start: i64, length: i64) -> Result<Value, Diagnostic> {
    if start < 0 || length < 0 {
        return Err(string_argument_error(
            "substring start and length must be non-negative",
        ));
    }

    let start = start as usize;
    let length = length as usize;
    let char_count = value.chars().count();
    let Some(end) = start.checked_add(length) else {
        return Err(string_argument_error("substring range is out of bounds"));
    };
    if start > char_count || end > char_count {
        return Err(string_argument_error("substring range is out of bounds"));
    }

    let start_byte = byte_index_for_char(value, start);
    let end_byte = byte_index_for_char(value, end);
    Ok(Value::string(value[start_byte..end_byte].to_string()))
}

fn byte_index_for_char(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(byte_index, _)| byte_index)
        .unwrap_or(value.len())
}

#[derive(Copy, Clone)]
enum PadSide {
    Left,
    Right,
}

fn pad_string(value: &str, width: i64, fill: &str, side: PadSide) -> Result<Value, Diagnostic> {
    if width < 0 {
        return Err(string_argument_error("pad width must be non-negative"));
    }
    let width =
        usize::try_from(width).map_err(|_| string_argument_error("pad width is out of range"))?;
    if fill.chars().count() != 1 {
        return Err(string_argument_error(
            "pad fill must be exactly one character",
        ));
    }
    let current_width = value.chars().count();
    if current_width >= width {
        return Ok(Value::string(value));
    }
    let padding = fill.repeat(width - current_width);
    Ok(match side {
        PadSide::Left => Value::string(format!("{padding}{value}")),
        PadSide::Right => Value::string(format!("{value}{padding}")),
    })
}

fn string_array_values(values: &[Value]) -> Result<Vec<String>, Diagnostic> {
    values
        .iter()
        .map(|value| match value {
            Value::String(value) => Ok(value.as_ref().to_string()),
            other => Err(string_argument_error_owned(format!(
                "expected str array element, got {}",
                other.kind_name()
            ))),
        })
        .collect()
}

fn slice_array(values: &[Value], start: i64, length: i64) -> Result<Vec<Value>, String> {
    if start < 0 || length < 0 {
        return Err("slice_copy expects non-negative start and length".to_string());
    }
    let start = usize::try_from(start).map_err(|_| "slice_copy start is too large".to_string())?;
    let length =
        usize::try_from(length).map_err(|_| "slice_copy length is too large".to_string())?;
    let end = start
        .checked_add(length)
        .ok_or_else(|| "slice_copy range overflow".to_string())?;
    if end > values.len() {
        return Err(format!(
            "slice_copy range {start}..{end} exceeds array length {}",
            values.len()
        ));
    }
    Ok(values[start..end].to_vec())
}

fn map_entries(values: &Map) -> Value {
    let entries = values
        .entries()
        .iter()
        .map(|(key, value)| {
            Value::tuple(
                vec![Type::Str, values.value_type().clone()],
                vec![Value::string(key.clone()), value.clone()],
            )
        })
        .collect();
    Value::array(
        Type::Tuple(vec![Type::Str, values.value_type().clone()]),
        entries,
    )
}

fn json_kind(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

fn parse_delimited_line(input: &str, delimiter: char) -> Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quotes = false;
    let mut field_started = false;
    while let Some(ch) = chars.next() {
        if in_quotes {
            match ch {
                '"' if matches!(chars.peek(), Some('"')) => {
                    let _ = chars.next();
                    current.push('"');
                }
                '"' => {
                    in_quotes = false;
                    field_started = true;
                }
                _ => current.push(ch),
            }
            continue;
        }
        if ch == delimiter {
            fields.push(current);
            current = String::new();
            field_started = false;
        } else if ch == '"' && !field_started && current.is_empty() {
            in_quotes = true;
            field_started = true;
        } else if ch == '"' {
            return Err("unexpected quote in unquoted field".to_string());
        } else {
            field_started = true;
            current.push(ch);
        }
    }
    if in_quotes {
        return Err("unterminated quoted field".to_string());
    }
    fields.push(current);
    Ok(fields)
}

fn parse_delimited_rows(input: &str, delimiter: char) -> Result<Vec<Vec<String>>, (usize, String)> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quotes = false;
    let mut field_started = false;
    let mut row_started = false;
    let mut current_line = 1usize;
    let mut row_start_line = 1usize;
    let mut saw_any = false;

    while let Some(ch) = chars.next() {
        saw_any = true;
        if in_quotes {
            match ch {
                '"' if matches!(chars.peek(), Some('"')) => {
                    let _ = chars.next();
                    current.push('"');
                }
                '"' => {
                    in_quotes = false;
                    field_started = true;
                    row_started = true;
                }
                '\n' => {
                    current.push(ch);
                    current_line += 1;
                }
                '\r' => {
                    current.push(ch);
                    if matches!(chars.peek(), Some('\n')) {
                        let _ = chars.next();
                        current.push('\n');
                    }
                    current_line += 1;
                }
                _ => current.push(ch),
            }
            continue;
        }

        if ch == delimiter {
            row.push(current);
            current = String::new();
            field_started = false;
            row_started = true;
        } else if ch == '\n' || ch == '\r' {
            row.push(current);
            rows.push(row);
            row = Vec::new();
            current = String::new();
            field_started = false;
            row_started = false;
            current_line += 1;
            if ch == '\r' && matches!(chars.peek(), Some('\n')) {
                let _ = chars.next();
            }
            row_start_line = current_line;
        } else if ch == '"' && !field_started && current.is_empty() {
            in_quotes = true;
            field_started = true;
            row_started = true;
        } else if ch == '"' {
            return Err((
                current_line,
                "unexpected quote in unquoted field".to_string(),
            ));
        } else {
            field_started = true;
            row_started = true;
            current.push(ch);
        }
    }

    if in_quotes {
        return Err((row_start_line, "unterminated quoted field".to_string()));
    }
    if saw_any && (row_started || field_started || !current.is_empty() || !row.is_empty()) {
        row.push(current);
        rows.push(row);
    }
    Ok(rows)
}

fn format_delimited_row(values: &[String], delimiter: char) -> Result<String, String> {
    let mut row = String::new();
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            row.push(delimiter);
        }
        if delimiter == '\t' && value.contains('\t') {
            return Err("TSV field contains tab".to_string());
        }
        let needs_quotes = value.contains(delimiter)
            || value.contains('"')
            || value.contains('\n')
            || value.contains('\r');
        if needs_quotes {
            row.push('"');
            for ch in value.chars() {
                if ch == '"' {
                    row.push('"');
                }
                row.push(ch);
            }
            row.push('"');
        } else {
            row.push_str(value);
        }
    }
    Ok(row)
}

fn string_argument_error(message: &'static str) -> Diagnostic {
    Diagnostic::new(message, Span { start: 0, end: 0 })
}

fn string_argument_error_owned(message: String) -> Diagnostic {
    Diagnostic::new(message, Span { start: 0, end: 0 })
}

#[cfg(test)]
#[path = "lib/tests.rs"]
mod tests;
