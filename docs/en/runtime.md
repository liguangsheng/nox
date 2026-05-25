# Runtime

The `nox` crate provides the default runtime on top of `nox_core`.

By default, runtime permissions are conservative. The CLI grants only the filesystem read access needed to load the entry file and imports. Environment variables, timers, network access, filesystem writes, process execution, and async task helpers require explicit permission.

Prefer static standard library modules:

```nox
import "std/fs.nox" as fs;
import "std/env.nox" as env;
import "std/time.nox" as time;
import "std/string.nox" as string;
import "std/json.nox" as json;
import "std/jsonl.nox" as jsonl;
import "std/hash.nox" as hash;
import "std/yaml.nox" as yaml;
import "std/xml.nox" as xml;
```

Older global functions remain available as compatibility surface, but new code should prefer namespace imports.

## Error Model

Recoverable failures are ordinary `result[T, E]` or `option[T]` values. Use
postfix `?` inside functions to propagate `err` or `none`, and use `match`,
`if let`, `while let`, or `let ... else` when the script should handle both
branches locally.

Runtime diagnostics are not catchable exceptions. Permission denials, allowlist
escapes, resource caps, parser/typechecker failures, host callback panics,
host callback type mismatches, and VM diagnostics terminate the current eval or
test case. Standard library helpers only return `result.err` or `none` for
ordinary recoverable failures after the relevant capability and safety checks
have passed.

`std/string.nox` is pure computation and requires no runtime capability. It exposes
`split`, `substring`, `trim`, `replace`, `starts_with`, `ends_with`, `index_of`,
`contains`, `last_index_of`, `join`, `repeat`, `pad_left`, `pad_right`,
`parse_int`, `parse_float`, `lines`, `to_upper`, and `to_lower`. Fallible parsers
return `result` values instead of runtime diagnostics.

`std/json.nox` is pure computation and requires no runtime capability. It exposes
`parse(value: str) -> result[json, str]` and `stringify(value: json) -> str`
for number, string, bool, null, array, and object JSON values. It also exposes
`kind`, `array_len`, `array_get`, `object_has`, and `object_get`; helper failures
return `result.err(message)`.

`std/jsonl.nox` is pure computation for JSON Lines text. `parse_lines(source) ->
result[[json], str]` parses one JSON value per line and prefixes parse failures
with a 1-based line number such as `line 2: expected JSON value`.
`format_lines(values) -> str` joins `json.stringify` output with `\n` and does
not append a trailing newline.

`std/csv.nox` and `std/tsv.nox` provide line-oriented helpers. `parse_line` returns
`result[[str], str]`; `csv.format_row(values: [str]) -> str` quotes fields as
needed, and `tsv.format_row(values: [str]) -> result[str, str]` rejects fields that
contain tabs. `parse_rows(source) -> result[[[str]], str]` and `format_rows(rows)`
operate on in-memory multi-row text; they are not streaming parsers.

`std/hash.nox` is pure computation for deterministic SHA-256 and HMAC-SHA256
digests. `sha256_text(value) -> str` hashes UTF-8 text, and
`sha256_hex(bytes) -> str` hashes `[int]` byte arrays whose elements must be in
`0..=255`. `hmac_sha256_text(key, value) -> str` signs UTF-8 text with a UTF-8
key, and `hmac_sha256_hex(key, bytes) -> str` signs `[int]` byte arrays with a
`[int]` key. All helpers return lowercase hexadecimal digest strings.

`std/array.nox`, `std/map.nox`, `std/option.nox`, and `std/result.nox` are pure
computation modules for common data transformations. Most array and map helpers
return copies, while the explicitly named mutation helpers are documented later
in this page. Array higher-order helpers accept `fn(...) -> ...` function values
or lambda literals; result and option helpers expose status checks, fallbacks,
lazy fallbacks, `option.ok_or`, `option.filter`, `map`, `result.map_or`,
`map_err`, `and_then`, and `or_else` composition.
`std/array.nox` also exports the experimental `Eq` trait plus
`contains_equal<T: Eq>` and `dedupe_equal<T: Eq>`, while the older
`contains_value<T: Equatable>` and `dedupe<T: Equatable>` helpers remain
available.
`std/traits.nox` is the small experimental trait core for code that wants trait
abstractions without importing collection helpers. It exports `Eq`, `Display`,
`equal<T: Eq>`, `not_equal<T: Eq>`, `display<T: Display>`, and
`display_label<T: Display>` with built-in primitive impls, and it does not
create a prelude or implicit import.

`std/url.nox` provides pure URL helpers: `parse(url) -> result[(scheme, host, port,
path, query), str]`, `build(scheme, host, port, path, query) -> str`, `query_encode`,
and `query_decode` (the percent decoder maps `+` to space and returns
`result.err` for malformed escapes).

`std/json.nox` gains two schema helpers: `require_field(value, path, expected_kind)
-> result[json, str]` walks a dotted / `[idx]` path and asserts the resolved
value's `kind` matches the expected string (e.g. `"number"`, `"string"`, `"object"`,
`"array"`, `"any"`). Missing keys, out-of-range indices, and kind mismatches return
`result.err` with the offending path. `validate_schema(value, required_fields)
-> result[null, str]` checks that a JSON object contains every required key
(non-recursive); missing keys are concatenated into the error message.
`validate_object(value, required_fields, allowed_fields) -> result[null, str]`
adds an allowed-field list and reports both missing required keys and unknown
object keys in the error message. `apply_defaults(value, defaults) -> result[
json, str]` accepts two JSON objects and returns a copy of `value` with missing
top-level keys filled from `defaults`; existing keys are never overwritten.
`apply_defaults_deep(value, defaults) -> result[json, str]` applies the same
rule recursively for nested JSON objects, so `server.port` can be supplied from
defaults while `server.host` remains user-provided.

`std/random.nox` is a pure-computation seeded PRNG built on xorshift64.
`next_int(seed, min, max) -> (int, int)` returns `(new_seed, value)` with the
value in `[min, max]`; `min > max` raises a runtime diagnostic. `next_bool(seed)
-> (int, bool)` and `next_float_unit(seed) -> (int, float)` (with the value in
`[0.0, 1.0)`) follow the same pattern. The seed pair lets scripts thread state
through deterministic property-test loops. `std/test.nox` builds on this with
property helpers and shrinking for int properties; automatic record/enum
generation remains deferred to a future ADR.

`std/json.nox` also exposes `to_json<T>(value: T) -> json` — a one-way serializer
that converts any Nox value into a `json` value. Records become objects keyed by
field name; enum variants become either a bare string (no payload) or an object
shaped `{"_variant": "Name", "payload": ...}` (with payload); tuples become
arrays; maps become objects; options serialize their payload directly (None →
null); result values become `{"_variant": "ok"|"err", "payload": ...}`. Function
values are rejected with a runtime diagnostic.
The adjacent enum shape is the stable `to_json` contract; alternate encodings
would need a separate explicit helper instead of changing this default.
`variant_name(value) -> result[str, str]` and `variant_payload(value) -> result[
json, str]` read that adjacent shape back: a no-payload enum string yields its
variant name, while a payload enum object yields both `_variant` and `payload`.

For the reverse direction, `std/json.nox` exposes `from_json<T>(value: json) ->
result[T, str]`. The call must have an expected `result[T, str]` type, for
example `let decoded: result[Config, str] = json.from_json(value);`; the
compiler records that target type in bytecode and the VM decodes JSON into the
typed value. Records map from JSON objects by field name, reject unknown object
keys, require every record field, and report path-aware errors such as
`server.port: expected number, got string`. Enums use the same adjacent contract
as `to_json`: no-payload variants decode from a bare string, while payload
variants decode from `{"_variant": "Name", "payload": ...}`.

`std/json.nox` also provides typed scalar extractors so scripts can walk a
parsed JSON value into records or enums by hand:
`as_int(value) -> result[int, str]`, `as_float(value) -> result[float, str]`,
`as_str(value) -> result[str, str]`, `as_bool(value) -> result[bool, str]`,
`as_array(value) -> result[[json], str]`, and `as_object(value) -> result[map[
str, json], str]`. Each returns `result.err` when the value's JSON kind does not
match (e.g. `expected JSON number, got string`); `as_int` additionally requires
the number to be a whole, finite value. `decode_record3<T>(value, path, field1,
kind1, field2, kind2, field3, kind3, build)` validates three path-aware fields
and passes their JSON values to an explicit builder returning `result[T, str]`.
`decode_adjacent_enum3<T>(value, path, variant1, build1, variant2, build2,
variant3, build3)` reads the stable adjacent enum shape, dispatches by variant
name, and passes the payload JSON to the selected builder; no-payload strings
receive JSON null.

`std/bytes.nox` ships a minimum bytes surface without introducing a dedicated
`Type::Bytes` yet. Byte arrays are represented as `[int]` with elements in the
`0..=255` range. `encode_utf8(text) -> [int]` and `decode_utf8(values) -> result[str,
str]` perform UTF-8 conversion (decode returns err for invalid UTF-8).
`len(values) -> int`, `get(values, index) -> result[int, str]`,
`slice_copy(values, start, length) -> result[[int], str]`, and
`equal(left, right) -> bool` provide the helper-form length, index, copy-slice,
and comparison operations for the byte-array representation. Display and wire
formatting should use `hex_encode` or `base64_encode`; there is no implicit
binary display syntax.
`base64_encode(values) -> str` / `base64_decode(value) -> result[[int], str]` and
`hex_encode(values) -> str` / `hex_decode(value) -> result[[int], str]` apply the
same encoders as `std/encoding.nox` but directly to byte arrays. Out-of-range
byte values raise a runtime diagnostic.

`std/fs.nox` exposes binary helpers using the same `[int]` byte representation:
`read_binary(path) -> result[[int], str]` reads a file as a byte array, and
`write_binary(path, bytes) -> result[null, str]` writes a byte array to disk.
Both helpers reuse the `filesystem` (read) and `filesystem_write` capabilities
and their respective allowlist checks. Reads return `result.err` for missing
files or permission failures; writes return `result.err` when any element of
`bytes` falls outside `0..=255`.

`std/fs.nox` also exposes `canonicalize(path) -> result[str, str]`: it returns
the absolute canonical path with symlinks resolved. The input path is gated by
the `filesystem` read capability and read allowlist; the resolved output is
only returned to the script. Returns `result.err` when the underlying
`fs::canonicalize` fails (missing file, permission denied at the OS level).

`std/fs.nox` provides async-friendly wrappers with the `_async` suffix:
`read_text_async`, `try_read_text_async`, `exists_async`, `is_file_async`,
`is_dir_async`, `list_dir_async`, `write_text_async`, `read_binary_async`,
`write_binary_async`, and `canonicalize_async`. They are regular `async fn`
wrappers around the synchronous helpers, so calls return `task[T]` at the call
site and can be awaited inside `async fn` bodies. They do not add an IO reactor
or background filesystem scheduler; every awaited operation uses the same
`filesystem` / `filesystem_write` capability checks, allowlists, mock
filesystem, return values, and diagnostics as the synchronous helper it wraps.

Embedding hosts can call `Runtime::set_mock_filesystem(Some(MockFilesystem))`
to replace the read side of `std/fs.nox` for one runtime instance. The mock
covers text reads, binary reads, existence/type checks, directory listing, and
canonicalize, plus `write_text` and `write_binary`. Permission checks are
unchanged: every mocked read or write still requires the matching filesystem
capability and must pass the matching allowlist before mock lookup or mutation.
When the mock is enabled, writes update the mock storage and do not touch the
real filesystem.

`std/term.nox` provides interactive-CLI helpers without bringing in a TUI
framework: `is_tty_stdout() -> bool`, `is_tty_stderr() -> bool` (Unix uses
`isatty`; Windows uses `GetConsoleMode`), `color_enabled() -> bool` (false when
`NO_COLOR` is set or stdout is not a TTY),
`progress(current, total, width) -> str` (pure ASCII bar string
`[####-----] 4/10 (40%)`, current is clamped to `[0, total]`, total = 0 renders
as 0%, width must be >= 0; non-TTY-safe because it only returns a string),
`style_color(value, color) -> str` and
`style_bold(value) -> str` (transparent passthrough when colour is disabled),
`pad_column(value, width) -> str`, `prompt(message) -> result[str, str]` (writes
to stderr, reads one line from stdin), `confirm(message, default_yes) ->
result[bool, str]`, and `select(message, items, default_index) -> result[int,
str]` (renders a numbered menu to stderr and reads a 1-based selection from
stdin; empty input or EOF with a valid `default_index` returns that default,
otherwise returns `result.err`). `prompt_password(message) -> result[str, str]`
reads a line from stdin without echoing it. On Linux it disables echo directly
with termios (`tcgetattr` / `tcsetattr`) and restores the original terminal mode
after the read. If echo control is unavailable, the helper refuses to fall back
to echoed input and returns `term.prompt-password.echo-disable-failed: ...`.
Other failure prefixes are `term.prompt-password.eof` and
`term.prompt-password.read-failed`. The prompt message is written to stderr. TUI
framework still remains deferred.

`std/process.nox` adds `run(program, args, stdin, timeout_ms) -> result[(int, str,
str), str]` returning `(exit_code, stdout, stderr)`. Requires the new
`process_run` capability. The runtime applies a 4 MiB cap on each of stdout and
stderr (exceeding bytes return `result.err` after killing the child). Setting
the optional `process_run_allowlist` on `RuntimePermissions` restricts which
program names are allowed; an empty allowlist permits any program.
`RuntimePermissions::process_run_max_concurrent` defaults to `Some(8)` and
rejects new children once the per-runtime running count reaches the limit.
Shell expansion, pipes, PTY, and background services are intentionally not
supported.

`std/process.nox` also exposes `run_with(program, args, stdin, timeout_ms, cwd,
env_pairs) -> result[(int, str, str), str]` for callers that need to override
the working directory or augment environment variables. `cwd` is an empty
string when the child should inherit the parent's working directory; any
non-empty string is passed to `Command::current_dir`. `env_pairs` is a `[(str,
str)]` list of additive key/value overrides applied on top of the inherited
environment (empty list = inherit unchanged). A value of `"<unset>"` removes
that key from the child environment; an empty string sets the variable to an
empty value. The same capability, allowlist, output cap, and timeout rules
apply as for `run`.

`run` and `run_with` use `result.err` messages prefixed with a stable failure
code so tools and tests can disambiguate causes. The codes are:

- `process_run.allowlist-denied` — program name is not in `process_run_allowlist`.
- `process_run.concurrent-limit` — the per-runtime concurrent child-process
  limit has been reached.
- `process_run.spawn-failed` — the OS rejected the spawn (command not found,
  permission denied, missing executable bit, etc.).
- `process_run.timeout` — child exceeded `timeout_ms`; it was killed.
- `process_run.output-cap-stdout` / `process_run.output-cap-stderr` — output
  exceeded the 4 MiB cap; child was killed.
- `process_run.stdin-write-failed` — writing `stdin` to the child pipe failed.
- `process_run.wait-failed` — the OS reported an error while waiting on the child.

The portion after the colon is the human-readable detail; consumers should
split on `: ` once to separate code from message.

`std/time.nox` gains duration converters (`from_seconds` / `from_minutes` /
`from_hours` to milliseconds, and `to_seconds` / `to_minutes` / `to_hours` back),
ISO-8601 helpers (`iso8601_format(unix_seconds) -> str` produces
`YYYY-MM-DDTHH:MM:SSZ`; `iso8601_parse(value) -> result[int, str]` only accepts
UTC with `Z` or `+00:00` suffixes), and deadline helpers (`deadline_ms`,
`is_past_deadline_ms`). All entries are pure computation. Locale formatting and
non-UTC timezone arithmetic remain deferred per stage-36 boundaries.

`std/time.nox` also exposes UTC calendar helpers for date arithmetic:
`add_days(unix_seconds, days) -> int`, `add_months(unix_seconds, months) -> int`
(month-end days are clamped to the new month's length: e.g. Jan 31 + 1 month
becomes Feb 28 or 29 depending on the year), `year_of(unix_seconds) -> int`,
`month_of(unix_seconds) -> int` (1-12), `day_of(unix_seconds) -> int` (1-31),
and `weekday_of(unix_seconds) -> int` (ISO weekday, 0 = Monday … 6 = Sunday).
All helpers use UTC; locale-aware formatting and non-UTC timezone arithmetic
remain deferred.

`std/encoding.nox` provides pure encoding helpers: `base64_encode(str) -> str`,
`base64_decode(str) -> result[str, str]`, `hex_encode(str) -> str`,
`hex_decode(str) -> result[str, str]`. Decoders return `result.err` for malformed
input or when the decoded bytes are not valid UTF-8. No capability required.

`std/dotenv.nox` provides `parse(source) -> result[map[str, str], str]`: parses
`KEY=value` lines with optional `#` comments, `export` prefix, and double / single
quoted values. Returns `result.err` on missing `=` or invalid identifier
characters. Pure computation; no capability required.

`std/ini.nox` provides `parse(source) -> result[map[str, map[str, str]], str]`
for simple INI files. Section headers use `[section]`; key/value lines accept
either `=` or `:` separators; `#` and `;` start comments outside quoted values.
Top-level keys before the first section are stored under the empty-string
section name.

`std/toml.nox` provides a minimum reader: `parse(source) -> result[json, str]`.
It supports tables (`[package]`), dotted tables / keys, strings, booleans,
numbers, and arrays of supported scalar values. It intentionally rejects TOML
features outside that subset, including datetime values and arrays of tables.

`std/yaml.nox` provides an experimental minimum reader:
`parse(source) -> result[json, str]`. It supports one document, indentation-based
mappings, scalar sequences, inline arrays, quoted strings, booleans, finite
numbers, null values, and comments. Anchors, aliases, tags, flow mappings,
multi-document streams, block scalars, and schema-specific YAML coercions remain
unsupported and return `result.err` for malformed structure.

`std/xml.nox` is pure computation for safe XML text generation. It exposes
`validate_name(name) -> result[str, str]`, `escape_text(value) -> str`,
`escape_attr(value) -> str`, `unescape_text(value) -> str`,
`comment(value) -> result[str, str]`, `text_element(name, value) -> result[str, str]`,
`attr(name, value) -> result[str, str]`, `attrs(values) -> result[str, str]`,
`qname(prefix, local) -> result[str, str]`, `xmlns(prefix, uri) -> result[str, str]`,
`xmlns_default(uri) -> result[str, str]`, `empty_element(name, attrs) -> result[str, str]`,
`text_element_attrs(name, attrs, value) -> result[str, str]`,
`empty_element_ns(prefix, local, attrs) -> result[str, str]`, and
`text_element_ns(prefix, local, attrs, value) -> result[str, str]`. The helpers
validate element/attribute names and namespace prefix/local-name text before
constructing tags. `comment` rejects `--` and trailing `-` content. These helpers
do not parse XML documents, resolve namespace scopes, validate schemas, or
stream large documents.

Compression/archive formats, protobuf, SQLite/database drivers, and HTTPS/TLS
remain deferred. They either need larger dependencies, runtime capabilities, or
mock/error-model work that does not fit the current conservative stdlib surface.

`std/test.nox` ships assertion helpers for `nox test` scripts:
`assert_eq<T: Equatable>(actual, expected, label) -> null`,
`assert_ne<T: Equatable>(actual, unexpected, label) -> null`,
`assert_true(condition, label) -> null`,
`assert_false(condition, label) -> null`,
`assert_contains(haystack, needle, label) -> null`, and
`fail(label, message) -> null`. Failure raises a diagnostic with stable code
`test.assertion-failed`. Test functions may now return `null` (assertions raise
on failure) in addition to the existing `bool` return; either signature is
accepted by `nox test`. `nox test --filter <substr>` limits execution to test
function names that contain the substring.

For deterministic property-style tests, `std/test.nox` exposes
`gen_int(seed, min, max) -> (int, int)`, `gen_bool(seed) -> (int, bool)`,
`gen_string(seed, max_len) -> (int, str)`, `gen_int_array(seed, len, min, max)
-> (int, [int])`, `gen_int_map(seed, len, min, max) -> (int, map[str,
int])`, `gen_record3(seed, min, max, build)`, and `gen_enum3(seed, min, max,
max_len, build_int, build_str, build_bool)`. `assert_property_int(label, seed, cases, min, max, property)` runs an
`fn(int) -> bool` property over generated int cases; on failure it shrinks the
failing value toward a smaller counterexample and reports seed, case index,
original value, minimized value, and replay metadata in the diagnostic message.
`assert_property_int_array(label, seed, cases, len, min, max, property)` runs
the same deterministic loop for `[int]` cases, first shrinking to a shorter
failing prefix and then shrinking individual elements toward zero.
`assert_property_int_map(label, seed, cases, len, min, max, property)` runs
the same loop for generated `map[str, int]` cases with deterministic `k0`,
`k1`, ... keys, shrinking to a shorter key prefix and then shrinking values
toward zero.
`assert_property_record3<T>(label, seed, cases, min, max, build, property)`
uses an explicit `fn(int, str, bool) -> T` builder so tests can generate record
values without reflection; failures shrink the int field, then string length,
then bool payload. `assert_property_enum3<T>(...)` uses three explicit variant
builders (`fn(int) -> T`, `fn(str) -> T`, `fn(bool) -> T`) and shrinks both the
payload and variant choice toward the first variant.

`std/task.nox` wraps the runtime's sleep-based async task primitives:
`sleep_ms(ms) -> int` spawns a timer and returns a task id, `sleep(ms) ->
task[null]` creates an awaitable sleep task for `async fn` bodies,
`is_ready(id)` polls without blocking, `cancel(id)` removes a pending task,
`wait(id) -> bool` blocks until the task completes (or forever for an unknown
id), `wait_or_timeout(id, timeout_ms) -> bool` blocks up to the timeout and
cancels the task on expiration returning `false`, and `pending_count() -> int`
exposes the current outstanding task count. `delay<T>(ms, value) -> task[T]`
waits on a sleep task and then returns `value`; `join2<T, U>(left, right) ->
task[(T, U)]` and `join3<T, U, V>(first, second, third) -> task[(T, U, V)]`
await already-created task values and return tuples. `map<T, U>(value, f) ->
task[U]` awaits an already-created task and applies `f`; `and_then<T, U>(value,
f) -> task[U]` awaits an already-created task, calls `f`, and awaits the task it
returns. The sleep/id helpers and `delay` require the `async task` capability
because they create or inspect the runtime sleep task table. `join2`, `join3`,
`map`, and `and_then` do not create runtime tasks by themselves; they only await
the tasks their caller passes in or that their callback returns.
`RuntimePermissions::async_task_max_pending` defaults to `Some(1024)` and makes
`task_sleep_ms` and `task_sleep` / `task.sleep` fail with
`runtime.task-pending-cap` before creating a new task when the current pending
count is already at the configured limit. Set it to `None` only for trusted
embedding hosts that provide their own task accounting.
Rust embedding hosts can drive the same single-runtime task table through
`Runtime::spawn_sleep_task`, `Runtime::poll_async_task`, and
`Runtime::cancel_async_task`. These APIs use the same `async_tasks` permission,
pending cap, unknown-id diagnostic, and cleanup rules as the script helpers.
If an `async fn` fails after creating awaitable sleep tasks, top-level
`Runtime::eval` cleanup removes tasks created by that call while preserving
tasks that existed before the call. Diagnostics raised at an awaitable task
boundary retain both host and script stack frames.
The C ABI does not expose runtime task handles in this stage.
The staged async boundary keeps this model deliberately small: no IO
reactor, multithread runtime, top-level await, async traits, language-level
cancellation tokens, generic `select` / `race`, or C ABI task handles are part
of the current runtime surface. Additional `std/task.nox` helpers must keep
composing already-created tasks and reuse the same permissions, pending-task
cap, cleanup rules, trace events, mocks, and diagnostics instead of adding a
background scheduler.

`std/http.nox` provides a minimal HTTP/1.1 client over plain TCP. The simple
helpers `get(url, timeout_ms)` and `post(url, body, timeout_ms)` return
`result[(int, str), str]` with the HTTP status and response body. The generic
helpers `request(method, url, headers, body, timeout_ms) -> result[(int,
map[str, str], str), str]` and `request_binary(method, url, headers, body,
timeout_ms) -> result[(int, map[str, str], [int]), str]` also accept custom
request headers and return response headers. Response header names are lowercased;
duplicate response headers are folded by joining values with `", "`.
Custom `Content-Length` is rejected because Nox computes it from the body.
Custom `Host`, `User-Agent`, `Accept`, and `Connection` are ignored in favor of
the runtime defaults.

Only `http://` URLs are supported in the current runtime; HTTPS, chunked
transfer-encoding, keep-alive, cookie jars, redirects, auth frameworks, and
streaming bodies are not implemented yet and require a future ADR before adding
TLS. All HTTP calls require the `network` capability, are capped at 1 MiB of
response data, default to a 30s timeout when `timeout_ms <= 0`, and always send
`Connection: close`.

`std/http.nox` also keeps the simple binary-body variants:
`get_binary(url, timeout_ms) -> result[(int, [int]), str]` and
`post_binary(url, body, timeout_ms) -> result[(int, [int]), str]`. Both share
the same `network` capability, 1 MiB cap, default timeout, and `Connection:
close` semantics; bodies are exchanged as `[int]` byte arrays without lossy
UTF-8 conversion, suitable for images, archives, or binary protocols. The
existing string-bodied `get` / `post` continue to work and now share their
implementation with the binary variants under the hood.

`std/http.nox` also exposes async-friendly wrappers with the `_async` suffix:
`get_async`, `post_async`, `request_async`, `get_binary_async`,
`post_binary_async`, and `request_binary_async`. These wrappers are regular
`async fn` functions around the existing blocking HTTP helpers. Awaiting them
does not grant network access, skip mocks, or change timeout / response-size
rules; it uses the same `network` capability and the same HTTP implementation
as the synchronous helper.

Embedding hosts can call `Runtime::set_mock_network(Some(MockNetwork))` to
replace `tcp_connect` and `std/http.nox` for one runtime instance. The mock is
still gated by the `network` capability. Missing mocked HTTP responses return
`result.err` and do not fall back to the real network.

`std/array.nox` and `std/map.nox` also expose a small in-place mutation surface
that updates the underlying storage shared by all aliases: `array.set(values,
index, value) -> result[null, str]` (returns `result.err(message)` when `index`
is out of range), `array.append(values, value) -> null`, `array.pop(values) ->
option[T]`, `map.set(values, key, value) -> null`, and `map.delete(values, key)
-> bool`. The language also accepts `arr[i] = value` and `map[key] = value`
index-assignment syntax; both forms compile to a runtime `IndexAssign`
instruction and mutate the same underlying storage. Out-of-range array writes
raise the stable diagnostic code `runtime.index-out-of-range`; non-array,
non-map LHS expressions are rejected at typecheck with `type.assign-target`.

`std/process.nox` exposes command-line script helpers. `argv() -> [str]` returns
the script arguments after the entry path and does not include the script path.
`read_stdin() -> str` returns all stdin text, `print_err(value: str) -> null`
writes one line to stderr, and `exit(code: int) -> null` records a `0..255` exit
code that `nox run` uses after the script finishes successfully.

`std/path.nox` is pure computation and provides `join`, `basename`, `dirname`,
`extension`, and lexical `normalize`. It does not access the filesystem.
`std/fs.nox` also provides `is_file`, `is_dir`, and `list_dir`; these use the
same filesystem read capability and allowlist checks as `read_text` and `exists`.

`std/time.nox` exposes `now_unix() -> int`, `now_unix_ms() -> int`,
`duration_ms(start: int, end: int) -> int`, `format_unix(ts: int, fmt: str) -> str`,
and `parse_unix(value: str, fmt: str) -> result[int, str]` in addition to
`sleep_ms(ms: int) -> null`. The Unix formatting/parsing helpers use UTC and support
the minimal token set `%Y`, `%m`, `%d`, `%H`, `%M`, `%S`, and `%%`.
Only `sleep_ms` requires the `timers` capability.

The default runtime also exposes pure global math intrinsics:

```text
abs(value: float) -> float
min(left: float, right: float) -> float
max(left: float, right: float) -> float
sqrt(value: float) -> float
pow(left: float, right: float) -> float
floor(value: float) -> float
ceil(value: float) -> float
round(value: float) -> float
log(value: float) -> float
log2(value: float) -> float
sin(value: float) -> float
cos(value: float) -> float
tan(value: float) -> float
pi() -> float
e() -> float
```

These functions require no runtime capability. `sqrt` rejects negative values, and
`log` / `log2` reject non-positive values with runtime diagnostics.

The detailed Chinese runtime guide is available in [`../zh_CN/runtime.md`](../zh_CN/runtime.md).
