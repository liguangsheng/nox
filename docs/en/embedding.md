# Embedding

Use `nox_core` when embedding Nox into another application. The engine exposes a Rust API and a C ABI.

The C header is distributed as:

```text
include/nox_core.h
```

The release embedding SDK contains:

```text
lib/libnox_core.so
include/nox_core.h
examples/embed/c_embedding.c
```

Compile the C embedding smoke from the SDK:

```sh
cc -Inox-embed-v0.0.2-x86_64-unknown-linux-gnu/include \
  nox-embed-v0.0.2-x86_64-unknown-linux-gnu/examples/embed/c_embedding.c \
  -Lnox-embed-v0.0.2-x86_64-unknown-linux-gnu/lib -lnox_core \
  -Wl,-rpath,"$PWD/nox-embed-v0.0.2-x86_64-unknown-linux-gnu/lib" \
  -o /tmp/nox-c-embedding-smoke
/tmp/nox-c-embedding-smoke
```

Public ABI expectations include stable enum values, explicit handle ownership, engine-owned error strings, and documented host callback boundaries.
Runtime async tasks belong to the `nox` crate, not `nox_core`; the C ABI does
not expose task handles in this stage.

## C Host Callbacks

The C ABI exposes two registration entry points:

- `nox_core_engine_register_host_function(...)` registers the original
  synchronous callback surface.
- `nox_core_engine_register_host_function_ex(...)` registers the same callback
  plus optional metadata: a copied docstring and a copied list of capability
  names. Use the host namespace convention (`<namespace>__<function>`) for
  grouped callbacks, then expose a script-side wrapper if desired.

Both functions keep the same ownership and execution rules. `name`,
`param_types`, `docstring`, and capability strings are copied during
registration. `ctx` remains host-owned; Nox never dereferences or frees it.
When `ctx == NULL`, the callback receives the current engine userdata pointer.
Callbacks run synchronously on the evaluating thread; the C ABI does not make
an engine thread-safe or reentrant. `nox_core_engine_last_error(engine)` returns
an engine-owned pointer that remains valid until the next operation that
replaces the error slot, `nox_core_engine_clear_error`, or engine free.

## Runtime Stdio Mocks

Embedding hosts that use the `nox` crate can replace stdin and capture stdout on a
single `Runtime` instance:

```rust
use nox::Runtime;

let mut runtime = Runtime::new();
runtime.set_mock_stdin(Some("payload\n".to_string()));
runtime.set_mock_stdout(true);

runtime.eval(r#"
    import "std/process.nox" as process;
    let input: str = process.read_stdin();
    print("seen:" + input);
"#)?;

assert_eq!(runtime.take_stdout(), "seen:payload\n\n");
runtime.set_mock_stdin(None);
runtime.set_mock_stdout(false);
```

`set_mock_stdin(None)` restores real process stdin reads. `set_mock_stdout(true)`
captures script `print(...)` output into the runtime buffer, and `take_stdout()`
drains that buffer. `std/process.nox` stderr capture remains available through
`take_stderr()`.

## Runtime Filesystem Mocks

Embedding hosts can inject a deterministic read-only filesystem into one
`Runtime` instance:

```rust
use nox::{MockFilesystem, Runtime, RuntimePermissions};

let root = std::env::temp_dir().join("nox-embed-fixture");
let script_path = root.join("input.txt");

let mut runtime = Runtime::with_permissions(
    RuntimePermissions::none().allow_filesystem_read_under(&root),
);
runtime.set_mock_filesystem(Some(
    MockFilesystem::new().with_text_file(&script_path, "fixture"),
));

let value = runtime.eval(&format!(r#"read_text("{}");"#, script_path.display()))?;
```

The mock covers `read_text`, `try_read_text`, `exists`, `is_file`, `is_dir`,
`list_dir`, `read_binary`, `write_text`, `write_binary`, and `canonicalize`.
Each helper still checks the matching filesystem capability and allowlist
before consulting or mutating the mock. When the mock is enabled, filesystem
writes update the mock storage and do not touch the real filesystem.

Binary data is exposed to embedding hosts as the normal Nox `[int]` array
representation with each element in `0..=255`. There is no separate bytes
handle or borrowed buffer ABI yet: Rust hosts use the existing `Value::Array`
ownership rules, and C hosts use the existing value/handle lifecycle for arrays.
Stdlib helpers that return binary data allocate a fresh array; helpers that
accept binary data copy and validate the array before passing bytes to the host
operation.

## Runtime Network Mocks

Embedding hosts can inject deterministic network results into one `Runtime`
instance:

```rust
use nox::{MockNetwork, Runtime, RuntimePermissions};

let mut runtime = Runtime::with_permissions(RuntimePermissions {
    network: true,
    ..RuntimePermissions::none()
});
runtime.set_mock_network(Some(
    MockNetwork::new()
        .with_tcp_connect("example.test", 80, true)
        .with_http_text_response("GET", "http://example.test/data", 200, "fixture"),
));

let value = runtime.eval(r#"
    import "std/http.nox" as http;
    http.get("http://example.test/data", 1);
"#)?;
```

The mock covers `tcp_connect` and `std/http.nox` text/binary GET and POST
helpers. Each helper still checks the `network` capability before consulting
the mock. Missing mocked HTTP responses return `result.err` and do not fall
back to the real network.

## Runtime Async Tasks

Embedding hosts that use the `nox` crate can drive the same single-runtime
task table used by `std/task.nox`:

```rust
use nox::{AsyncTaskPoll, Runtime, RuntimePermissions};
use std::time::Duration;

let mut runtime = Runtime::with_permissions(RuntimePermissions {
    async_tasks: true,
    ..RuntimePermissions::none()
});

let id = runtime.spawn_sleep_task(Duration::from_millis(0))?;
assert_eq!(runtime.poll_async_task(id)?, AsyncTaskPoll::Ready);
```

`spawn_sleep_task`, `poll_async_task`, and `cancel_async_task` use the same
`async_tasks` permission, pending-task cap, unknown-id diagnostic, and cleanup
rules as the script helpers. Ready and cancelled tasks are consumed and become
unknown on later polls.
The runtime does not expose generic task status, task payload handles, or C ABI
task handles. Hosts that need richer scheduling should keep that registry in
Rust or behind their own host functions; the built-in runtime only promises the
single-runtime sleep task table and explicit poll/cancel API.

## Heap Limits

`nox_core::Engine` exposes optional memory guardrails for embedding hosts:

```rust
let mut engine = nox_core::Engine::new();
engine.set_max_heap_objects(Some(1024));
engine.set_max_string_length(Some(64 * 1024));
engine.set_max_array_length(Some(4096));
engine.set_max_map_entries(Some(4096));
```

`set_max_heap_objects` counts string, json, container, option, result, enum,
record, and function objects tracked by the engine heap. VM allocations and host
callback return values are both registered. When the limit is exceeded, the
engine returns `runtime.heap-object-cap`. The limit is disabled by default.

The detailed Chinese embedding guide is available in [`../zh_CN/embedding.md`](../zh_CN/embedding.md).
