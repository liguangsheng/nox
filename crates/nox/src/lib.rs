use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap},
    env, fs, io,
    io::Read,
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
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

use manifest::Manifest;

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

    pub fn with_http_binary_response(
        mut self,
        method: impl Into<String>,
        url: impl Into<String>,
        status: i64,
        body: Vec<u8>,
    ) -> Self {
        self.http_responses.insert(
            (method.into().to_ascii_uppercase(), url.into()),
            MockHttpResponse { status, body },
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

export fn is_file(path: str) -> bool {
    return __nox_std_fs_is_file(path);
}

export fn is_dir(path: str) -> bool {
    return __nox_std_fs_is_dir(path);
}

export fn list_dir(path: str) -> result[[str], str] {
    return __nox_std_fs_list_dir(path);
}

export fn write_text(path: str, contents: str) -> null {
    return __nox_std_fs_write_text(path, contents);
}

export fn read_binary(path: str) -> result[[int], str] {
    return __nox_std_fs_read_binary(path);
}

export fn write_binary(path: str, bytes: [int]) -> result[null, str] {
    return __nox_std_fs_write_binary(path, bytes);
}

export fn canonicalize(path: str) -> result[str, str] {
    return __nox_std_fs_canonicalize(path);
}
"#
        }
        "std/path.nox" => {
            r#"export fn join(left: str, right: str) -> str {
    return __nox_std_path_join(left, right);
}

export fn basename(path: str) -> str {
    return __nox_std_path_basename(path);
}

export fn dirname(path: str) -> str {
    return __nox_std_path_dirname(path);
}

export fn extension(path: str) -> str {
    return __nox_std_path_extension(path);
}

export fn normalize(path: str) -> str {
    return __nox_std_path_normalize(path);
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
        "std/process.nox" => {
            r#"export fn argv() -> [str] {
    return __nox_std_process_argv();
}

export fn read_stdin() -> str {
    return __nox_std_process_read_stdin();
}

export fn print_err(value: str) -> null {
    return __nox_std_process_print_err(value);
}

export fn exit(code: int) -> null {
    return __nox_std_process_exit(code);
}

export fn run(program: str, args: [str], stdin: str, timeout_ms: int) -> result[(int, str, str), str] {
    return __nox_std_process_run(program, args, stdin, timeout_ms);
}

export fn run_with(program: str, args: [str], stdin: str, timeout_ms: int, cwd: str, env_pairs: [(str, str)]) -> result[(int, str, str), str] {
    return __nox_std_process_run_with(program, args, stdin, timeout_ms, cwd, env_pairs);
}
"#
        }
        "std/time.nox" => {
            r#"export fn sleep_ms(ms: int) -> null {
    return __nox_std_time_sleep_ms(ms);
}

export fn now_unix() -> int {
    return __nox_std_time_now_unix();
}

export fn now_unix_ms() -> int {
    return __nox_std_time_now_unix_ms();
}

export fn duration_ms(start: int, end: int) -> int {
    return __nox_std_time_duration_ms(start, end);
}

export fn format_unix(ts: int, fmt: str) -> str {
    return __nox_std_time_format_unix(ts, fmt);
}

export fn parse_unix(value: str, fmt: str) -> result[int, str] {
    return __nox_std_time_parse_unix(value, fmt);
}

export fn from_seconds(seconds: int) -> int {
    return seconds * 1000;
}

export fn from_minutes(minutes: int) -> int {
    return minutes * 60000;
}

export fn from_hours(hours: int) -> int {
    return hours * 3600000;
}

export fn to_seconds(ms: int) -> int {
    return ms / 1000;
}

export fn to_minutes(ms: int) -> int {
    return ms / 60000;
}

export fn to_hours(ms: int) -> int {
    return ms / 3600000;
}

export fn iso8601_format(unix_seconds: int) -> str {
    return __nox_std_time_iso8601_format(unix_seconds);
}

export fn iso8601_parse(value: str) -> result[int, str] {
    return __nox_std_time_iso8601_parse(value);
}

export fn add_days(unix_seconds: int, days: int) -> int {
    return unix_seconds + days * 86400;
}

export fn add_months(unix_seconds: int, months: int) -> int {
    return __nox_std_time_add_months(unix_seconds, months);
}

export fn year_of(unix_seconds: int) -> int {
    return __nox_std_time_year_of(unix_seconds);
}

export fn month_of(unix_seconds: int) -> int {
    return __nox_std_time_month_of(unix_seconds);
}

export fn day_of(unix_seconds: int) -> int {
    return __nox_std_time_day_of(unix_seconds);
}

export fn weekday_of(unix_seconds: int) -> int {
    return __nox_std_time_weekday_of(unix_seconds);
}

export fn deadline_ms(timeout_ms: int) -> int {
    return __nox_std_time_now_unix_ms() + timeout_ms;
}

export fn is_past_deadline_ms(deadline_ms: int) -> bool {
    return __nox_std_time_now_unix_ms() >= deadline_ms;
}
"#
        }
        "std/string.nox" => {
            r#"export fn split(value: str, separator: str) -> [str] {
    return __nox_std_string_split(value, separator);
}

export fn join(values: [str], separator: str) -> str {
    return __nox_std_string_join(values, separator);
}

export fn substring(value: str, start: int, length: int) -> str {
    return __nox_std_string_substring(value, start, length);
}

export fn trim(value: str) -> str {
    return __nox_std_string_trim(value);
}

export fn replace(value: str, from: str, to: str) -> str {
    return __nox_std_string_replace(value, from, to);
}

export fn starts_with(value: str, prefix: str) -> bool {
    return __nox_std_string_starts_with(value, prefix);
}

export fn ends_with(value: str, suffix: str) -> bool {
    return __nox_std_string_ends_with(value, suffix);
}

export fn index_of(value: str, needle: str) -> int {
    return __nox_std_string_index_of(value, needle);
}

export fn contains(value: str, needle: str) -> bool {
    return __nox_std_string_contains(value, needle);
}

export fn last_index_of(value: str, needle: str) -> int {
    return __nox_std_string_last_index_of(value, needle);
}

export fn repeat(value: str, count: int) -> str {
    return __nox_std_string_repeat(value, count);
}

export fn pad_left(value: str, width: int, fill: str) -> str {
    return __nox_std_string_pad_left(value, width, fill);
}

export fn pad_right(value: str, width: int, fill: str) -> str {
    return __nox_std_string_pad_right(value, width, fill);
}

export fn parse_int(value: str) -> result[int, str] {
    return __nox_std_string_parse_int(value);
}

export fn parse_float(value: str) -> result[float, str] {
    return __nox_std_string_parse_float(value);
}

export fn lines(value: str) -> [str] {
    return __nox_std_string_lines(value);
}

export fn to_upper(value: str) -> str {
    return __nox_std_string_to_upper(value);
}

export fn to_lower(value: str) -> str {
    return __nox_std_string_to_lower(value);
}
"#
        }
        "std/json.nox" => {
            r#"export fn parse(value: str) -> result[json, str] {
    return __nox_std_json_parse(value);
}

export fn stringify(value: json) -> str {
    return __nox_std_json_stringify(value);
}

export fn kind(value: json) -> str {
    return __nox_std_json_kind(value);
}

export fn array_len(value: json) -> result[int, str] {
    return __nox_std_json_array_len(value);
}

export fn array_get(value: json, index: int) -> result[json, str] {
    return __nox_std_json_array_get(value, index);
}

export fn object_has(value: json, key: str) -> result[bool, str] {
    return __nox_std_json_object_has(value, key);
}

export fn object_get(value: json, key: str) -> result[json, str] {
    return __nox_std_json_object_get(value, key);
}

export fn require_field(value: json, path: str, expected_kind: str) -> result[json, str] {
    return __nox_std_json_require_field(value, path, expected_kind);
}

fn __json_join_path(base: str, field: str) -> str {
    if (base == "") {
        return field;
    }
    return base + "." + field;
}

fn __json_prefix_error(path: str, message: str) -> str {
    if (path == "") {
        return message;
    }
    return path + ": " + message;
}

fn __json_payload_or_null(value: json) -> result[json, str] {
    let payload: result[json, str] = variant_payload(value);
    match (payload) {
        ok(found) => {
            return ok(found);
        }
        err(_) => {
            return parse("null");
        }
    }
}

export fn decode_record3<T>(value: json, path: str, field1: str, kind1: str, field2: str, kind2: str, field3: str, kind3: str, build: fn(json, json, json) -> result[T, str]) -> result[T, str] {
    let value1: result[json, str] = require_field(value, __json_join_path(path, field1), kind1);
    match (value1) {
        ok(found1) => {
            let value2: result[json, str] = require_field(value, __json_join_path(path, field2), kind2);
            match (value2) {
                ok(found2) => {
                    let value3: result[json, str] = require_field(value, __json_join_path(path, field3), kind3);
                    match (value3) {
                        ok(found3) => {
                            return build(found1, found2, found3);
                        }
                        err(message) => {
                            return err(message);
                        }
                    }
                }
                err(message) => {
                    return err(message);
                }
            }
        }
        err(message) => {
            return err(message);
        }
    }
}

export fn decode_adjacent_enum3<T>(value: json, path: str, variant1: str, build1: fn(json) -> result[T, str], variant2: str, build2: fn(json) -> result[T, str], variant3: str, build3: fn(json) -> result[T, str]) -> result[T, str] {
    let name_result: result[str, str] = variant_name(value);
    match (name_result) {
        ok(name) => {
            let payload_result: result[json, str] = __json_payload_or_null(value);
            match (payload_result) {
                ok(payload) => {
                    if (name == variant1) {
                        return build1(payload);
                    }
                    if (name == variant2) {
                        return build2(payload);
                    }
                    if (name == variant3) {
                        return build3(payload);
                    }
                    return err(__json_prefix_error(path, "unknown variant " + name));
                }
                err(message) => {
                    return err(__json_prefix_error(path, message));
                }
            }
        }
        err(message) => {
            return err(__json_prefix_error(path, message));
        }
    }
}

export fn validate_schema(value: json, required_fields: [str]) -> result[null, str] {
    return __nox_std_json_validate_schema(value, required_fields);
}

export fn validate_object(value: json, required_fields: [str], allowed_fields: [str]) -> result[null, str] {
    return __nox_std_json_validate_object(value, required_fields, allowed_fields);
}

export fn apply_defaults(value: json, defaults: json) -> result[json, str] {
    return __nox_std_json_apply_defaults(value, defaults);
}

export fn apply_defaults_deep(value: json, defaults: json) -> result[json, str] {
    return __nox_std_json_apply_defaults_deep(value, defaults);
}

export fn to_json<T>(value: T) -> json {
    return __nox_std_json_to_json(value);
}

export fn from_json<T>(value: json) -> result[T, str] {
    return __nox_std_json_from_json(value);
}

export fn variant_name(value: json) -> result[str, str] {
    return __nox_std_json_variant_name(value);
}

export fn variant_payload(value: json) -> result[json, str] {
    return __nox_std_json_variant_payload(value);
}

export fn as_int(value: json) -> result[int, str] {
    return __nox_std_json_as_int(value);
}

export fn as_float(value: json) -> result[float, str] {
    return __nox_std_json_as_float(value);
}

export fn as_str(value: json) -> result[str, str] {
    return __nox_std_json_as_str(value);
}

export fn as_bool(value: json) -> result[bool, str] {
    return __nox_std_json_as_bool(value);
}

export fn as_array(value: json) -> result[[json], str] {
    return __nox_std_json_as_array(value);
}

export fn as_object(value: json) -> result[map[str, json], str] {
    return __nox_std_json_as_object(value);
}
"#
        }
        "std/csv.nox" => {
            r#"export fn parse_line(value: str) -> result[[str], str] {
    return __nox_std_csv_parse_line(value);
}

export fn format_row(values: [str]) -> str {
    return __nox_std_csv_format_row(values);
}
"#
        }
        "std/tsv.nox" => {
            r#"export fn parse_line(value: str) -> result[[str], str] {
    return __nox_std_tsv_parse_line(value);
}

export fn format_row(values: [str]) -> result[str, str] {
    return __nox_std_tsv_format_row(values);
}
"#
        }
        "std/array.nox" => {
            r#"export fn len<T>(values: [T]) -> int {
    return __nox_std_array_len(values);
}

export fn is_empty<T>(values: [T]) -> bool {
    return __nox_std_array_is_empty(values);
}

export fn push_copy<T>(values: [T], value: T) -> [T] {
    return __nox_std_array_push_copy(values, value);
}

export fn concat<T>(left: [T], right: [T]) -> [T] {
    return __nox_std_array_concat(left, right);
}

export fn slice_copy<T>(values: [T], start: int, length: int) -> result[[T], str] {
    return __nox_std_array_slice_copy(values, start, length);
}

export fn reverse_copy<T>(values: [T]) -> [T] {
    return __nox_std_array_reverse_copy(values);
}

export fn sort_copy_int(values: [int]) -> [int] {
    return __nox_std_array_sort_copy_int(values);
}

export fn sort_copy_str(values: [str]) -> [str] {
    return __nox_std_array_sort_copy_str(values);
}

export fn set<T>(values: [T], index: int, value: T) -> result[null, str] {
    return __nox_std_array_set(values, index, value);
}

export fn append<T>(values: [T], value: T) -> null {
    return __nox_std_array_append(values, value);
}

export fn pop<T>(values: [T]) -> option[T] {
    return __nox_std_array_pop(values);
}

export fn map_fn<T, U>(values: [T], f: fn(T) -> U) -> [U] {
    let result: [U] = [];
    let i: int = 0;
    let n: int = __nox_std_array_len(values);
    while (i < n) {
        __nox_std_array_append(result, f(values[i]));
        i = i + 1;
    }
    return result;
}

export fn filter_fn<T>(values: [T], f: fn(T) -> bool) -> [T] {
    let result: [T] = [];
    let i: int = 0;
    let n: int = __nox_std_array_len(values);
    while (i < n) {
        let element: T = values[i];
        if (f(element)) {
            __nox_std_array_append(result, element);
        }
        i = i + 1;
    }
    return result;
}

export fn reduce<T, A>(values: [T], init: A, f: fn(A, T) -> A) -> A {
    let acc: A = init;
    let i: int = 0;
    let n: int = __nox_std_array_len(values);
    while (i < n) {
        acc = f(acc, values[i]);
        i = i + 1;
    }
    return acc;
}

export fn for_each<T>(values: [T], f: fn(T) -> null) -> null {
    let i: int = 0;
    let n: int = __nox_std_array_len(values);
    while (i < n) {
        f(values[i]);
        i = i + 1;
    }
    return null;
}

export fn contains_value<T: Equatable>(values: [T], target: T) -> bool {
    let i: int = 0;
    let n: int = __nox_std_array_len(values);
    while (i < n) {
        if (values[i] == target) {
            return true;
        }
        i = i + 1;
    }
    return false;
}

export fn dedupe<T: Equatable>(values: [T]) -> [T] {
    let result: [T] = [];
    let i: int = 0;
    let n: int = __nox_std_array_len(values);
    while (i < n) {
        let element: T = values[i];
        if (!contains_value(result, element)) {
            __nox_std_array_append(result, element);
        }
        i = i + 1;
    }
    return result;
}
"#
        }
        "std/map.nox" => {
            r#"export fn keys<T>(values: map[str, T]) -> [str] {
    return map_keys(values);
}

export fn values<T>(values: map[str, T]) -> [T] {
    return map_values(values);
}

export fn entries<T>(values: map[str, T]) -> [(str, T)] {
    return __nox_std_map_entries(values);
}

export fn merge<T>(left: map[str, T], right: map[str, T]) -> map[str, T] {
    return __nox_std_map_merge(left, right);
}

export fn remove_copy<T>(values: map[str, T], key: str) -> map[str, T] {
    return __nox_std_map_remove_copy(values, key);
}

export fn get_or<T>(values: map[str, T], key: str, fallback: T) -> T {
    let found: option[T] = map_get(values, key);
    match (found) {
        some(value) => {
            return value;
        }
        none => {
            return fallback;
        }
    }
}

export fn set<T>(values: map[str, T], key: str, value: T) -> null {
    return __nox_std_map_set(values, key, value);
}

export fn delete<T>(values: map[str, T], key: str) -> bool {
    return __nox_std_map_delete(values, key);
}
"#
        }
        "std/option.nox" => {
            r#"export fn is_some<T>(value: option[T]) -> bool {
    return __nox_std_option_is_some(value);
}

export fn is_none<T>(value: option[T]) -> bool {
    return !__nox_std_option_is_some(value);
}

export fn unwrap_or<T>(value: option[T], fallback: T) -> T {
    match (value) {
        some(payload) => {
            return payload;
        }
        none => {
            return fallback;
        }
    }
}
"#
        }
        "std/result.nox" => {
            r#"export fn is_ok<T, E>(value: result[T, E]) -> bool {
    return __nox_std_result_is_ok(value);
}

export fn is_err<T, E>(value: result[T, E]) -> bool {
    return !__nox_std_result_is_ok(value);
}

export fn unwrap_or<T, E>(value: result[T, E], fallback: T) -> T {
    match (value) {
        ok(payload) => {
            return payload;
        }
        err(_) => {
            return fallback;
        }
    }
}

export fn map_err_to_str<T>(value: result[T, str]) -> result[T, str] {
    return value;
}
"#
        }
        "std/term.nox" => {
            r#"export fn is_tty_stdout() -> bool {
    return __nox_std_term_is_tty_stdout();
}

export fn is_tty_stderr() -> bool {
    return __nox_std_term_is_tty_stderr();
}

export fn color_enabled() -> bool {
    return __nox_std_term_color_enabled();
}

export fn style_color(value: str, color: str) -> str {
    return __nox_std_term_style_color(value, color);
}

export fn style_bold(value: str) -> str {
    return __nox_std_term_style_color(value, "bold");
}

export fn prompt(message: str) -> result[str, str] {
    return __nox_std_term_prompt(message);
}

export fn confirm(message: str, default_yes: bool) -> result[bool, str] {
    return __nox_std_term_confirm(message, default_yes);
}

export fn pad_column(value: str, width: int) -> str {
    return __nox_std_term_pad_column(value, width);
}

export fn select(message: str, items: [str], default_index: int) -> result[int, str] {
    return __nox_std_term_select(message, items, default_index);
}

export fn progress(current: int, total: int, width: int) -> str {
    return __nox_std_term_progress(current, total, width);
}

export fn prompt_password(message: str) -> result[str, str] {
    return __nox_std_term_prompt_password(message);
}
"#
        }
        "std/bytes.nox" => {
            r#"export fn encode_utf8(text: str) -> [int] {
    return __nox_std_bytes_encode_utf8(text);
}

export fn decode_utf8(values: [int]) -> result[str, str] {
    return __nox_std_bytes_decode_utf8(values);
}

export fn len(values: [int]) -> int {
    return __nox_std_bytes_len(values);
}

export fn get(values: [int], index: int) -> result[int, str] {
    return __nox_std_bytes_get(values, index);
}

export fn slice_copy(values: [int], start: int, length: int) -> result[[int], str] {
    return __nox_std_bytes_slice_copy(values, start, length);
}

export fn equal(left: [int], right: [int]) -> bool {
    return __nox_std_bytes_equal(left, right);
}

export fn base64_encode(values: [int]) -> str {
    return __nox_std_bytes_base64_encode(values);
}

export fn base64_decode(value: str) -> result[[int], str] {
    return __nox_std_bytes_base64_decode(value);
}

export fn hex_encode(values: [int]) -> str {
    return __nox_std_bytes_hex_encode(values);
}

export fn hex_decode(value: str) -> result[[int], str] {
    return __nox_std_bytes_hex_decode(value);
}
"#
        }
        "std/encoding.nox" => {
            r#"export fn base64_encode(value: str) -> str {
    return __nox_std_encoding_base64_encode(value);
}

export fn base64_decode(value: str) -> result[str, str] {
    return __nox_std_encoding_base64_decode(value);
}

export fn hex_encode(value: str) -> str {
    return __nox_std_encoding_hex_encode(value);
}

export fn hex_decode(value: str) -> result[str, str] {
    return __nox_std_encoding_hex_decode(value);
}
"#
        }
        "std/dotenv.nox" => {
            r#"export fn parse(source: str) -> result[map[str, str], str] {
    return __nox_std_dotenv_parse(source);
}
"#
        }
        "std/ini.nox" => {
            r#"export fn parse(source: str) -> result[map[str, map[str, str]], str] {
    return __nox_std_ini_parse(source);
}
"#
        }
        "std/toml.nox" => {
            r#"export fn parse(source: str) -> result[json, str] {
    return __nox_std_toml_parse(source);
}
"#
        }
        "std/test.nox" => {
            r#"export fn assert_eq<T: Equatable>(actual: T, expected: T, label: str) -> null {
    if (actual == expected) {
        return null;
    }
    return __nox_std_test_fail(label, "assert_eq failed");
}

export fn assert_ne<T: Equatable>(actual: T, unexpected: T, label: str) -> null {
    if (actual != unexpected) {
        return null;
    }
    return __nox_std_test_fail(label, "assert_ne failed");
}

export fn assert_true(condition: bool, label: str) -> null {
    if (condition) {
        return null;
    }
    return __nox_std_test_fail(label, "assert_true failed");
}

export fn assert_false(condition: bool, label: str) -> null {
    if (!condition) {
        return null;
    }
    return __nox_std_test_fail(label, "assert_false failed");
}

export fn assert_contains(haystack: str, needle: str, label: str) -> null {
    return __nox_std_test_assert_contains(haystack, needle, label);
}

export fn fail(label: str, message: str) -> null {
    return __nox_std_test_fail(label, message);
}

export fn assert_snapshot(label: str, actual: str, expected: str) -> null {
    return __nox_std_test_assert_snapshot(label, actual, expected);
}

export fn assert_table_row<T: Equatable>(label: str, index: int, actual: T, expected: T) -> null {
    if (actual == expected) {
        return null;
    }
    return __nox_std_test_fail(label, "table row mismatch at index");
}

fn __property_abs(value: int) -> int {
    if (value < 0) {
        return 0 - value;
    }
    return value;
}

fn __property_mod(value: int, modulus: int) -> int {
    let positive: int = __property_abs(value);
    return positive - ((positive / modulus) * modulus);
}

fn __property_next_seed(seed: int) -> int {
    let normalized: int = __property_mod(seed, 1000000);
    return __property_mod((normalized * 73) + 19, 1000000);
}

export fn gen_int(seed: int, min: int, max: int) -> (int, int) {
    if (min > max) {
        __nox_std_test_fail("property.gen_int", "min must be <= max");
        return (seed, min);
    }
    let next_seed: int = __property_next_seed(seed);
    let span: int = (max - min) + 1;
    let value: int = min + __property_mod(next_seed, span);
    return (next_seed, value);
}

export fn gen_bool(seed: int) -> (int, bool) {
    let next_seed: int = __property_next_seed(seed);
    return (next_seed, __property_mod(next_seed, 2) == 1);
}

export fn gen_string(seed: int, max_len: int) -> (int, str) {
    if (max_len < 0) {
        __nox_std_test_fail("property.gen_string", "max_len must be >= 0");
        return (seed, "");
    }
    let pair: (int, int) = gen_int(seed, 0, max_len);
    let (next_seed, len) = pair;
    let out: str = "";
    let current_seed: int = next_seed;
    let i: int = 0;
    while (i < len) {
        let char_pair: (int, int) = gen_int(current_seed, 0, 2);
        let (updated_seed, ch) = char_pair;
        current_seed = updated_seed;
        if (ch == 0) {
            out = out + "a";
        } else if (ch == 1) {
            out = out + "b";
        } else {
            out = out + "c";
        }
        i = i + 1;
    }
    return (current_seed, out);
}

export fn gen_int_array(seed: int, len: int, min: int, max: int) -> (int, [int]) {
    if (len < 0) {
        __nox_std_test_fail("property.gen_int_array", "len must be >= 0");
        return (seed, []);
    }
    let out: [int] = [];
    let current_seed: int = seed;
    let i: int = 0;
    while (i < len) {
        let pair: (int, int) = gen_int(current_seed, min, max);
        let (next_seed, value) = pair;
        current_seed = next_seed;
        __nox_std_array_append(out, value);
        i = i + 1;
    }
    return (current_seed, out);
}

export fn gen_int_map(seed: int, len: int, min: int, max: int) -> (int, map[str, int]) {
    if (len < 0) {
        __nox_std_test_fail("property.gen_int_map", "len must be >= 0");
        return (seed, {});
    }
    let out: map[str, int] = {};
    let current_seed: int = seed;
    let i: int = 0;
    while (i < len) {
        let pair: (int, int) = gen_int(current_seed, min, max);
        let (next_seed, value) = pair;
        current_seed = next_seed;
        out["k${i}"] = value;
        i = i + 1;
    }
    return (current_seed, out);
}

export fn gen_record3<T>(seed: int, min: int, max: int, build: fn(int, str, bool) -> T) -> (int, T) {
    let int_pair: (int, int) = gen_int(seed, min, max);
    let (seed_after_int, id) = int_pair;
    let string_pair: (int, str) = gen_string(seed_after_int, 8);
    let (seed_after_string, name) = string_pair;
    let bool_pair: (int, bool) = gen_bool(seed_after_string);
    let (next_seed, active) = bool_pair;
    return (next_seed, build(id, name, active));
}

export fn gen_enum3<T>(seed: int, min: int, max: int, max_len: int, build_int: fn(int) -> T, build_str: fn(str) -> T, build_bool: fn(bool) -> T) -> (int, T) {
    let variant_pair: (int, int) = gen_int(seed, 0, 2);
    let (seed_after_variant, variant) = variant_pair;
    if (variant == 0) {
        let int_pair: (int, int) = gen_int(seed_after_variant, min, max);
        let (next_seed, value) = int_pair;
        return (next_seed, build_int(value));
    }
    if (variant == 1) {
        let string_pair: (int, str) = gen_string(seed_after_variant, max_len);
        let (next_seed, value) = string_pair;
        return (next_seed, build_str(value));
    }
    let bool_pair: (int, bool) = gen_bool(seed_after_variant);
    let (next_seed, value) = bool_pair;
    return (next_seed, build_bool(value));
}

fn __property_shrink_int(value: int, property: fn(int) -> bool) -> int {
    let current: int = value;
    let step: int = __property_abs(value) / 2;
    while (step > 0) {
        let candidate: int = current;
        if (current > 0) {
            candidate = current - step;
        } else if (current < 0) {
            candidate = current + step;
        }
        if (!property(candidate)) {
            current = candidate;
        }
        step = step / 2;
    }
    if (current != 0 && !property(0)) {
        return 0;
    }
    return current;
}

fn __property_shrink_string(value: str, property: fn(str) -> bool) -> str {
    if (value != "" && !property("")) {
        return "";
    }
    return value;
}

fn __property_array_prefix(values: [int], length: int) -> [int] {
    let out: [int] = [];
    let i: int = 0;
    let n: int = __nox_std_array_len(values);
    while (i < length && i < n) {
        __nox_std_array_append(out, values[i]);
        i = i + 1;
    }
    return out;
}

fn __property_array_with_value(values: [int], index: int, value: int) -> [int] {
    let out: [int] = [];
    let i: int = 0;
    let n: int = __nox_std_array_len(values);
    while (i < n) {
        if (i == index) {
            __nox_std_array_append(out, value);
        } else {
            __nox_std_array_append(out, values[i]);
        }
        i = i + 1;
    }
    return out;
}

fn __property_shrink_int_array(values: [int], property: fn([int]) -> bool) -> [int] {
    let current: [int] = __property_array_prefix(values, __nox_std_array_len(values));
    let length: int = __nox_std_array_len(current);
    let prefix_len: int = 0;
    while (prefix_len < length) {
        let prefix: [int] = __property_array_prefix(current, prefix_len);
        if (!property(prefix)) {
            current = prefix;
            prefix_len = length;
        } else {
            prefix_len = prefix_len + 1;
        }
    }
    let i: int = 0;
    while (i < __nox_std_array_len(current)) {
        let step: int = __property_abs(current[i]) / 2;
        while (step > 0) {
            let candidate_value: int = current[i];
            if (current[i] > 0) {
                candidate_value = current[i] - step;
            } else if (current[i] < 0) {
                candidate_value = current[i] + step;
            }
            let candidate: [int] = __property_array_with_value(current, i, candidate_value);
            if (!property(candidate)) {
                current = candidate;
            }
            step = step / 2;
        }
        if (current[i] != 0) {
            let zero_candidate: [int] = __property_array_with_value(current, i, 0);
            if (!property(zero_candidate)) {
                current = zero_candidate;
            }
        }
        i = i + 1;
    }
    return current;
}

fn __property_map_prefix(values: map[str, int], length: int) -> map[str, int] {
    let out: map[str, int] = {};
    let i: int = 0;
    while (i < length) {
        let key: str = "k${i}";
        let found: option[int] = map_get(values, key);
        match (found) {
            some(value) => {
                out[key] = value;
            }
            none => {}
        }
        i = i + 1;
    }
    return out;
}

fn __property_map_with_value(values: map[str, int], key: str, value: int) -> map[str, int] {
    let out: map[str, int] = {};
    let keys: [str] = map_keys(values);
    let i: int = 0;
    while (i < __nox_std_array_len(keys)) {
        let current_key: str = keys[i];
        let found: option[int] = map_get(values, current_key);
        match (found) {
            some(current_value) => {
                if (current_key == key) {
                    out[current_key] = value;
                } else {
                    out[current_key] = current_value;
                }
            }
            none => {}
        }
        i = i + 1;
    }
    return out;
}

fn __property_shrink_int_map(values: map[str, int], property: fn(map[str, int]) -> bool) -> map[str, int] {
    let current: map[str, int] = __property_map_prefix(values, __nox_std_array_len(map_keys(values)));
    let length: int = __nox_std_array_len(map_keys(current));
    let prefix_len: int = 0;
    while (prefix_len < length) {
        let prefix: map[str, int] = __property_map_prefix(current, prefix_len);
        if (!property(prefix)) {
            current = prefix;
            prefix_len = length;
        } else {
            prefix_len = prefix_len + 1;
        }
    }
    let keys: [str] = map_keys(current);
    let i: int = 0;
    while (i < __nox_std_array_len(keys)) {
        let key: str = keys[i];
        let found: option[int] = map_get(current, key);
        match (found) {
            some(value) => {
                let step: int = __property_abs(value) / 2;
                while (step > 0) {
                    let candidate_value: int = value;
                    if (value > 0) {
                        candidate_value = value - step;
                    } else if (value < 0) {
                        candidate_value = value + step;
                    }
                    let candidate: map[str, int] = __property_map_with_value(current, key, candidate_value);
                    if (!property(candidate)) {
                        current = candidate;
                        value = candidate_value;
                    }
                    step = step / 2;
                }
                if (value != 0) {
                    let zero_candidate: map[str, int] = __property_map_with_value(current, key, 0);
                    if (!property(zero_candidate)) {
                        current = zero_candidate;
                    }
                }
            }
            none => {}
        }
        i = i + 1;
    }
    return current;
}

export fn assert_property_record3<T>(label: str, seed: int, cases: int, min: int, max: int, build: fn(int, str, bool) -> T, property: fn(T) -> bool) -> null {
    if (cases <= 0) {
        return __nox_std_test_fail(label, "property cases must be > 0");
    }
    if (min > max) {
        return __nox_std_test_fail(label, "property min must be <= max");
    }
    let current_seed: int = seed;
    let i: int = 0;
    while (i < cases) {
        let int_pair: (int, int) = gen_int(current_seed, min, max);
        let (seed_after_int, id) = int_pair;
        let string_pair: (int, str) = gen_string(seed_after_int, 8);
        let (seed_after_string, name) = string_pair;
        let bool_pair: (int, bool) = gen_bool(seed_after_string);
        let (next_seed, active) = bool_pair;
        current_seed = next_seed;
        let value: T = build(id, name, active);
        if (!property(value)) {
            let minimized_id: int = __property_shrink_int(id, fn(candidate: int) -> bool {
                return property(build(candidate, name, active));
            });
            let minimized_name: str = __property_shrink_string(name, fn(candidate: str) -> bool {
                return property(build(minimized_id, candidate, active));
            });
            let minimized_active: bool = active;
            if (active && !property(build(minimized_id, minimized_name, false))) {
                minimized_active = false;
            }
            return __nox_std_test_fail(
                label,
                "property failed seed=${seed} case=${i} record_fields=3 minimized_int=${minimized_id} minimized_str_len=${len(minimized_name)} minimized_bool=${minimized_active} replay=\"${label}:record3:int=${minimized_id}:strlen=${len(minimized_name)}:bool=${minimized_active}\""
            );
        }
        i = i + 1;
    }
    return null;
}

export fn assert_property_enum3<T>(label: str, seed: int, cases: int, min: int, max: int, max_len: int, build_int: fn(int) -> T, build_str: fn(str) -> T, build_bool: fn(bool) -> T, property: fn(T) -> bool) -> null {
    if (cases <= 0) {
        return __nox_std_test_fail(label, "property cases must be > 0");
    }
    if (min > max) {
        return __nox_std_test_fail(label, "property min must be <= max");
    }
    if (max_len < 0) {
        return __nox_std_test_fail(label, "property max_len must be >= 0");
    }
    let current_seed: int = seed;
    let i: int = 0;
    while (i < cases) {
        let variant_pair: (int, int) = gen_int(current_seed, 0, 2);
        let (seed_after_variant, variant) = variant_pair;
        let next_seed: int = seed_after_variant;
        let failed: bool = false;
        let minimized_variant: int = variant;
        let minimized_int: int = 0;
        let minimized_str_len: int = 0;
        let minimized_bool: bool = false;
        if (variant == 0) {
            let int_pair: (int, int) = gen_int(seed_after_variant, min, max);
            let (seed_after_value, payload) = int_pair;
            next_seed = seed_after_value;
            if (!property(build_int(payload))) {
                failed = true;
                minimized_int = __property_shrink_int(payload, fn(candidate: int) -> bool {
                    return property(build_int(candidate));
                });
            }
        } else if (variant == 1) {
            let string_pair: (int, str) = gen_string(seed_after_variant, max_len);
            let (seed_after_value, payload) = string_pair;
            next_seed = seed_after_value;
            if (!property(build_str(payload))) {
                failed = true;
                let minimized_str: str = __property_shrink_string(payload, fn(candidate: str) -> bool {
                    return property(build_str(candidate));
                });
                minimized_str_len = len(minimized_str);
            }
        } else {
            let bool_pair: (int, bool) = gen_bool(seed_after_variant);
            let (seed_after_value, payload) = bool_pair;
            next_seed = seed_after_value;
            if (!property(build_bool(payload))) {
                failed = true;
                minimized_bool = payload;
                if (payload && !property(build_bool(false))) {
                    minimized_bool = false;
                }
            }
        }
        current_seed = next_seed;
        if (failed) {
            if (variant != 0 && !property(build_int(0))) {
                minimized_variant = 0;
                minimized_int = 0;
                minimized_str_len = 0;
                minimized_bool = false;
            }
            return __nox_std_test_fail(
                label,
                "property failed seed=${seed} case=${i} enum_variant=${variant} minimized_variant=${minimized_variant} minimized_int=${minimized_int} minimized_str_len=${minimized_str_len} minimized_bool=${minimized_bool} replay=\"${label}:enum3:variant=${minimized_variant}:int=${minimized_int}:strlen=${minimized_str_len}:bool=${minimized_bool}\""
            );
        }
        i = i + 1;
    }
    return null;
}

export fn assert_property_int(label: str, seed: int, cases: int, min: int, max: int, property: fn(int) -> bool) -> null {
    if (cases <= 0) {
        return __nox_std_test_fail(label, "property cases must be > 0");
    }
    if (min > max) {
        return __nox_std_test_fail(label, "property min must be <= max");
    }
    let current_seed: int = seed;
    let i: int = 0;
    while (i < cases) {
        let pair: (int, int) = gen_int(current_seed, min, max);
        let (next_seed, value) = pair;
        current_seed = next_seed;
        if (!property(value)) {
            let minimized: int = __property_shrink_int(value, property);
            return __nox_std_test_fail(
                label,
                "property failed seed=${seed} case=${i} value=${value} minimized=${minimized} replay=\"${label}:${minimized}\""
            );
        }
        i = i + 1;
    }
    return null;
}

export fn assert_property_int_array(label: str, seed: int, cases: int, len: int, min: int, max: int, property: fn([int]) -> bool) -> null {
    if (cases <= 0) {
        return __nox_std_test_fail(label, "property cases must be > 0");
    }
    if (len < 0) {
        return __nox_std_test_fail(label, "property len must be >= 0");
    }
    if (min > max) {
        return __nox_std_test_fail(label, "property min must be <= max");
    }
    let current_seed: int = seed;
    let i: int = 0;
    while (i < cases) {
        let pair: (int, [int]) = gen_int_array(current_seed, len, min, max);
        let (next_seed, value) = pair;
        current_seed = next_seed;
        if (!property(value)) {
            let minimized: [int] = __property_shrink_int_array(value, property);
            let value_len: int = __nox_std_array_len(value);
            let minimized_len: int = __nox_std_array_len(minimized);
            let minimized_first: int = 0;
            if (minimized_len > 0) {
                minimized_first = minimized[0];
            }
            return __nox_std_test_fail(
                label,
                "property failed seed=${seed} case=${i} value_len=${value_len} minimized_len=${minimized_len} minimized_first=${minimized_first} replay=\"${label}:len=${minimized_len}:first=${minimized_first}\""
            );
        }
        i = i + 1;
    }
    return null;
}

export fn assert_property_int_map(label: str, seed: int, cases: int, len: int, min: int, max: int, property: fn(map[str, int]) -> bool) -> null {
    if (cases <= 0) {
        return __nox_std_test_fail(label, "property cases must be > 0");
    }
    if (len < 0) {
        return __nox_std_test_fail(label, "property len must be >= 0");
    }
    if (min > max) {
        return __nox_std_test_fail(label, "property min must be <= max");
    }
    let current_seed: int = seed;
    let i: int = 0;
    while (i < cases) {
        let pair: (int, map[str, int]) = gen_int_map(current_seed, len, min, max);
        let (next_seed, value) = pair;
        current_seed = next_seed;
        if (!property(value)) {
            let minimized: map[str, int] = __property_shrink_int_map(value, property);
            let value_len: int = __nox_std_array_len(map_keys(value));
            let minimized_len: int = __nox_std_array_len(map_keys(minimized));
            let minimized_first: int = 0;
            let first: option[int] = map_get(minimized, "k0");
            match (first) {
                some(first_value) => {
                    minimized_first = first_value;
                }
                none => {}
            }
            return __nox_std_test_fail(
                label,
                "property failed seed=${seed} case=${i} value_len=${value_len} minimized_len=${minimized_len} minimized_first=${minimized_first} replay=\"${label}:len=${minimized_len}:k0=${minimized_first}\""
            );
        }
        i = i + 1;
    }
    return null;
}
"#
        }
        "std/task.nox" => {
            r#"export fn sleep_ms(ms: int) -> int {
    return task_sleep_ms(ms);
}

export fn is_ready(id: int) -> bool {
    return task_ready(id);
}

export fn cancel(id: int) -> null {
    return task_cancel(id);
}

export fn wait(id: int) -> bool {
    return task_join(id, 0);
}

export fn wait_or_timeout(id: int, timeout_ms: int) -> bool {
    return task_join(id, timeout_ms);
}

export fn pending_count() -> int {
    return task_pending_count();
}
"#
        }
        "std/http.nox" => {
            r#"export fn get(url: str, timeout_ms: int) -> result[(int, str), str] {
    return __nox_std_http_get(url, timeout_ms);
}

export fn post(url: str, body: str, timeout_ms: int) -> result[(int, str), str] {
    return __nox_std_http_post(url, body, timeout_ms);
}

export fn get_binary(url: str, timeout_ms: int) -> result[(int, [int]), str] {
    return __nox_std_http_get_binary(url, timeout_ms);
}

export fn post_binary(url: str, body: [int], timeout_ms: int) -> result[(int, [int]), str] {
    return __nox_std_http_post_binary(url, body, timeout_ms);
}
"#
        }
        "std/random.nox" => {
            r#"export fn next_int(seed: int, min: int, max: int) -> (int, int) {
    return __nox_std_random_next_int(seed, min, max);
}

export fn next_bool(seed: int) -> (int, bool) {
    return __nox_std_random_next_bool(seed);
}

export fn next_float_unit(seed: int) -> (int, float) {
    return __nox_std_random_next_float_unit(seed);
}
"#
        }
        "std/url.nox" => {
            r#"export fn parse(url: str) -> result[(str, str, int, str, str), str] {
    return __nox_std_url_parse(url);
}

export fn build(scheme: str, host: str, port: int, path: str, query: str) -> str {
    return __nox_std_url_build(scheme, host, port, path, query);
}

export fn query_encode(value: str) -> str {
    return __nox_std_url_query_encode(value);
}

export fn query_decode(value: str) -> result[str, str] {
    return __nox_std_url_query_decode(value);
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
    register_dotenv_stdlib(engine);
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
    register_dotenv_stdlib(engine);
    register_ini_stdlib(engine);
    register_toml_stdlib(engine);
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

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode_bytes(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);
    for chunk in chunks.by_ref() {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
        out.push(BASE64_ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(BASE64_ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(BASE64_ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        out.push(BASE64_ALPHABET[(n & 0x3f) as usize] as char);
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        let mut n: u32 = 0;
        for (i, b) in rem.iter().enumerate() {
            n |= (*b as u32) << (16 - 8 * i);
        }
        out.push(BASE64_ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(BASE64_ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        if rem.len() == 2 {
            out.push(BASE64_ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        out.push('=');
    }
    out
}

fn base64_decode_bytes(input: &str) -> Result<Vec<u8>, String> {
    let bytes = input.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err(format!(
            "base64 input length {} is not a multiple of 4",
            bytes.len()
        ));
    }
    fn lookup(byte: u8) -> Result<u8, String> {
        match byte {
            b'A'..=b'Z' => Ok(byte - b'A'),
            b'a'..=b'z' => Ok(byte - b'a' + 26),
            b'0'..=b'9' => Ok(byte - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            other => Err(format!("invalid base64 character '{}'", other as char)),
        }
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad_count = chunk.iter().rev().take_while(|b| **b == b'=').count();
        if pad_count > 2 {
            return Err("base64 chunk has more than two padding bytes".to_string());
        }
        let mut value: u32 = 0;
        for (i, byte) in chunk.iter().enumerate() {
            if (*byte == b'=') && i >= 4 - pad_count {
                value <<= 6;
            } else {
                let digit = lookup(*byte)? as u32;
                value = (value << 6) | digit;
            }
        }
        out.push(((value >> 16) & 0xff) as u8);
        if pad_count < 2 {
            out.push(((value >> 8) & 0xff) as u8);
        }
        if pad_count < 1 {
            out.push((value & 0xff) as u8);
        }
    }
    Ok(out)
}

fn hex_encode_bytes(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for byte in input {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

fn hex_decode_bytes(input: &str) -> Result<Vec<u8>, String> {
    let bytes = input.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err(format!("hex input length {} is odd", bytes.len()));
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks(2) {
        let high = (chunk[0] as char)
            .to_digit(16)
            .ok_or_else(|| format!("invalid hex character '{}'", chunk[0] as char))?;
        let low = (chunk[1] as char)
            .to_digit(16)
            .ok_or_else(|| format!("invalid hex character '{}'", chunk[1] as char))?;
        out.push(((high << 4) | low) as u8);
    }
    Ok(out)
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
                                    return Err(Diagnostic::new(
                                        format!(
                                            "async task pending count would exceed configured cap of {max}"
                                        ),
                                        Span { start: 0, end: 0 },
                                    )
                                    .with_code("runtime.task-pending-cap"));
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
                                .map_err(|msg| Diagnostic::new(msg, Span { start: 0, end: 0 }))?;
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
                                .map_err(|msg| Diagnostic::new(msg, Span { start: 0, end: 0 }))?;
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
                                let ready = task_runtime.poll(*id as u64).map_err(|msg| {
                                    Diagnostic::new(msg, Span { start: 0, end: 0 })
                                })?;
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
    register_parse_line(engine, "__nox_std_tsv_parse_line", '\t');
    register_format_row(engine, "__nox_std_tsv_format_row", '\t', true);
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

fn http_request(
    method: &str,
    url: &str,
    body: Option<&str>,
    timeout_ms: i64,
) -> Result<(i64, String), String> {
    let body_bytes = body.map(|s| s.as_bytes().to_vec());
    let (status, body) = http_request_bytes(method, url, body_bytes.as_deref(), timeout_ms)?;
    let body_text = String::from_utf8_lossy(&body).to_string();
    Ok((status, body_text))
}

fn http_request_bytes(
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    timeout_ms: i64,
) -> Result<(i64, Vec<u8>), String> {
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

    let body_bytes = response[header_end + 4..].to_vec();
    Ok((status_code, body_bytes))
}

fn http_mock_response(
    method: &str,
    url: &str,
    mock_network: Option<&MockNetworkHandle>,
) -> Result<Option<(i64, Vec<u8>)>, String> {
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
            Some(response) => Ok(Some((response.status, response.body))),
            None => Err(format!("mock network has no {method} response for '{url}'")),
        };
    }
    Ok(None)
}

fn http_request_with_mock(
    method: &str,
    url: &str,
    body: Option<&str>,
    timeout_ms: i64,
    mock_network: Option<&MockNetworkHandle>,
) -> Result<(i64, String), String> {
    if let Some((status, body)) = http_mock_response(method, url, mock_network)? {
        let body_text = String::from_utf8_lossy(&body).to_string();
        return Ok((status, body_text));
    }
    http_request(method, url, body, timeout_ms)
}

fn http_request_bytes_with_mock(
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    timeout_ms: i64,
    mock_network: Option<&MockNetworkHandle>,
) -> Result<(i64, Vec<u8>), String> {
    if let Some(response) = http_mock_response(method, url, mock_network)? {
        return Ok(response);
    }
    http_request_bytes(method, url, body, timeout_ms)
}

fn register_http_stdlib(
    engine: &mut Engine,
    permissions: &RuntimePermissions,
    mock_network: Option<MockNetworkHandle>,
) {
    let network_allowed_get = permissions.network;
    let response_type = Type::Tuple(vec![Type::Int, Type::Str]);
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

    let network_allowed_get_binary = permissions.network;
    let binary_response_type = Type::Tuple(vec![Type::Int, Type::Array(Box::new(Type::Int))]);
    let binary_response_type_get = binary_response_type.clone();
    let mock_network_for_get_binary = mock_network.clone();
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
                            None,
                            *timeout_ms,
                            mock_network_for_get_binary.as_ref(),
                        ) {
                            Ok((status, body)) => {
                                let tuple = Value::tuple(
                                    vec![Type::Int, Type::Array(Box::new(Type::Int))],
                                    vec![Value::Int(status), bytes_vec_to_array(body)],
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
    let mock_network_for_post_binary = mock_network;
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
                                Some(&body_bytes),
                                *timeout_ms,
                                mock_network_for_post_binary.as_ref(),
                            ) {
                                Ok((status, resp_body)) => {
                                    let tuple = Value::tuple(
                                        vec![Type::Int, Type::Array(Box::new(Type::Int))],
                                        vec![Value::Int(status), bytes_vec_to_array(resp_body)],
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
mod tests {
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
        "std/csv.nox",
        "std/tsv.nox",
        "std/array.nox",
        "std/map.nox",
        "std/option.nox",
        "std/result.nox",
        "std/term.nox",
        "std/bytes.nox",
        "std/encoding.nox",
        "std/dotenv.nox",
        "std/ini.nox",
        "std/toml.nox",
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

    #[test]
    fn stdlib_index_documents_every_exported_helper() {
        assert_docs_cover_every_export(
            "../../docs/zh_CN/runtime.md",
            "../../docs/zh_CN/stdlib-index.md",
            "docs/zh_CN/{runtime,stdlib-index}.md",
        );
    }

    #[test]
    fn english_stdlib_index_documents_every_exported_helper() {
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
        let dir =
            std::env::temp_dir().join(format!("nox-process-run-with-cwd-{}", std::process::id()));
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
        let dir =
            std::env::temp_dir().join(format!("nox-mock-fs-{}-{}", std::process::id(), line!()));
        let allowed = dir.join("allowed");
        fs::create_dir_all(&allowed).unwrap();
        let text_path = allowed.join("note.txt");
        let binary_path = allowed.join("raw.bin");
        let nested = allowed.join("nested");
        let nested_file = nested.join("deep.txt");

        let mut runtime = Runtime::with_permissions(
            RuntimePermissions::none().allow_filesystem_read_under(&allowed),
        );
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
    fn mock_filesystem_does_not_bypass_read_allowlist() {
        let dir = std::env::temp_dir().join(format!(
            "nox-mock-fs-allow-{}-{}",
            std::process::id(),
            line!()
        ));
        let allowed = dir.join("allowed");
        fs::create_dir_all(&allowed).unwrap();
        let outside = dir.join("outside.txt");

        let mut runtime = Runtime::with_permissions(
            RuntimePermissions::none().allow_filesystem_read_under(&allowed),
        );
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
                .with_http_binary_response(
                    "POST",
                    "http://example.test/upload",
                    204,
                    vec![1, 2, 255],
                ),
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
    fn array_stdlib_dedupe_and_contains_value_use_equatable_constraint() {
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

                if (array.len(d) == 4 && d[0] == 1 && d[3] == 4 && found && !missing) {
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

                if (
                    option.is_some(present) &&
                    option.is_none(missing) &&
                    option.unwrap_or(present, 0) == 7 &&
                    option.unwrap_or(missing, 5) == 5 &&
                    result.is_ok(loaded) &&
                    result.is_err(failed) &&
                    result.unwrap_or(loaded, 0) == 9 &&
                    result.unwrap_or(failed, 4) == 4 &&
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
