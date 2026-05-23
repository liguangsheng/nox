# CLI

The `nox` binary is the default command-line interface for running and checking `.nox` files.

Common commands:

```sh
nox --version
nox run examples/hello.nox
nox check examples/hello.nox
nox check --json tests/fixtures/type-error.nox
nox test tests/fixtures/example_test.nox
nox test --json tests/fixtures/example_test.nox
nox test --export-failures fuzz/corpus/property tests/fixtures/example_test.nox
nox test --export-failures-classified tests/malformed/exported tests/fixtures/example_test.nox
nox fmt examples/hello.nox
nox fmt --check tests/fixtures/formatter-golden.nox
nox repl
nox dap
nox profile tests/benchmarks/bench-fib.nox
nox coverage tests/benchmarks/bench-fib.nox
nox inspect-bytecode --compact examples/hello.nox
```

Project commands use `nox.toml` discovery:

```sh
cd examples/projects/scoreboard
nox project check
nox project check --json
```

The `nox doc <file.nox>` command emits a Markdown document listing every
top-level `fn` declaration (exported and local) together with the `///` doc
comment lines that immediately precede the declaration. A single leading space
after `///` is consumed so block-style comments stay aligned. The current
implementation is a text-based scanner rather than a full AST walk. It covers
top-level `fn`, `record`, `enum`, and `type` declarations (both `export` and
local); each section carries `Kind:` and `Visibility:` labels. Richer LSP hover
integration remains deferred.

The `nox lint` command reports non-blocking quality warnings. The current rule
set covers `lint.unused-variable`, `lint.unused-function`, `lint.unused-import`,
`lint.unreachable-code`, `lint.shadowed-variable`, and `lint.constant-condition`.
Identifiers that start with `_` (e.g. `_ignored`) are excluded from the
unused-variable check. The exit code stays 0 even when warnings are reported;
`--json` emits the `nox.lint.v1` schema with `summary.capabilities` listing the
runtime capabilities the script infers from its `import "std/X.nox"` statements
and call sites (`filesystem`, `filesystem_write`, `process_run`, `environment`,
`network`, `timers`, `async_tasks`). Text mode appends a `capabilities: ...`
line.

The `nox watch` command wraps a single subcommand (`check`, `test`, or `run`) and re-executes it whenever any `.nox` file under the manifest's `source_dirs` / `test_dirs` (or the current directory when no manifest is found) changes. It uses a `stat`-based poll loop with a default 500ms interval and accepts `--interval-ms <ms>`. The loop runs in the foreground until interrupted; daemon, hot reload, and incremental cache are intentionally not provided per ADR 0022. A missing watch path returns the stable diagnostic code `watch.path-not-found`.

Machine-readable command output uses versioned schemas such as `nox.check.v1`, `nox.test.v1`, and `nox.project-check.v1`. Diagnostic `code` values are intended for tools and editors; see [Diagnostics](diagnostics.md). Runtime diagnostics may include an optional `stack_frames` array; old consumers can ignore this additive field.

`nox test --json` emits `nox.test.v1`. Each test record includes `file`, `name`,
`ok`, `attempts`, `retried`, `duration_us`, `stdout`, `stderr`, `diagnostic`,
`snapshot_diff`, `kind`, and `mock_events`.
The top-level `suites` array groups reported cases by test file as
`{file, cases}` so tools can present a suite/case hierarchy without deriving it
from the flat `tests` array.
`kind` classifies the reported case as `unit`, `integration`, or `fixture`.
Paths containing a `fixtures` component report `fixture`; paths under `tests`
report `integration`; all other test files report `unit`.
Output produced by script `print(...)` and `std/process.nox` `print_err(...)` is
captured per test case so the outer command stdout remains valid JSON and the
outer stderr stays reserved for CLI-level failures.
When `std/test.nox` `assert_snapshot` fails, `snapshot_diff` contains
`{label, actual, expected}` previews; otherwise it is `null`.
`mock_events` is an additive array reserved for tests that run through a mocked
capability harness; the plain CLI runner currently reports an empty array.
`--export-failures <dir>` is an opt-in fuzz bridge for property tests. When a
failed test diagnostic contains `std/test.nox` property replay metadata, the
runner writes a `.nox` corpus file into `<dir>` with the original source and
commented source/test/diagnostic metadata. Use a path under `fuzz/corpus/...` or
`tests/malformed/...` depending on whether the exported case should feed
cargo-fuzz or deterministic malformed-source regression tests.
`--export-failures-classified <dir>` keeps `--export-failures` compatibility and
writes exportable failures under `<dir>/<classification>/`. Property replay
failures go to `property`; malformed module failures are classified from their
diagnostic code into `parser`, `typecheck`, `verifier`, or `runtime`.

The LSP server exposes a Nox-specific `nox/testDiscovery` request for editor test
explorers. Pass `params.textDocument.uri`; the response is an array of top-level
`test_*` functions with `uri`, `name`, and `range`. `textDocument/codeLens` uses
the same discovery rule to emit `nox.runTest` commands for individual tests.

`nox project check --json` emits `nox.project-check.v1`. The schema reports the
manifest root, package metadata, manifest schema validation summary, entrypoints,
declared runtime capabilities, the discovered module graph, child step results,
and a summary. The manifest parser rejects unsupported sections and unsupported
keys in fixed sections; the JSON `schema_validation` object records that
contract for valid projects. The module graph lists manifest source roots and
discovered `.nox` files; it does not resolve remote packages or install
dependencies.

`nox run <file.nox> [args...]` passes arguments after the entry path to both
`args()` and `std/process.nox` `argv()`. Neither includes the script path.
`std/process.nox` also provides `read_stdin()`, `print_err(value)`, and
`exit(code)`. `exit(code)` accepts `0..255`; when the script finishes
successfully, `nox run` uses that value as the process exit code.

`nox repl` reads statements and expressions from stdin, keeps successful top-level declarations for the session, prints non-null results, and exits on EOF, `:quit`, or `:exit`.

`nox profile <file.nox>` runs the script and prints a tab-separated function profile with `function`, `call_count`, and `total_us`, followed by operation rows for VM hot paths such as host callbacks, container literals, indexing, match patterns, and map helpers. Recursive script calls are counted through the VM call path. `nox coverage <file.nox>` reuses the same execution data and reports covered functions plus VM span statement execution counts and branch true/false counts. `nox coverage --json` includes additive `statements` and `branches` arrays with byte spans and 1-based source locations; `--ndjson` emits `kind:"statement"` and `kind:"branch"` events. `nox trace [--ndjson] <file.nox>` emits `nox.trace.event.v1` NDJSON events for run start, static capability summary, per-capability `permission_check` requirements, runtime `io` / `timer` / `task` events, captured stdout/stderr, function and operation profile rows, host callback summaries, per-call `host_callback_call` enter/exit events, diagnostics, and run finish. Runtime `io` events cover stdout/stderr writes, stdin reads, top-level file helpers, and `std/fs.nox` filesystem operations. Diagnostic trace events carry the same `trace_id` and `seq` envelope as other events plus `span`, `source`, and runtime `stack_frames` when available.

`nox dap` starts the stdio Debug Adapter Protocol adapter. The VS Code extension uses it for Nox launch configurations, breakpoints, conditional breakpoint evaluation, exception breakpoint filters, stepping requests, stack frames, scopes, and variables. Conditional breakpoints support simple `result == value` / `result != value` checks after launch evaluation; unmatched conditional breakpoints terminate instead of reporting a false stop. With the `raised` exception filter enabled, launch-time runtime errors report a stopped event with `reason:"exception"`. `variables` accepts optional `depth` / `maxDepth` arguments: depth `0` suppresses expandable child references, while larger values expose a depth-limited `debugState` child plus condition/exception debug state.

`nox lsp` starts the stdio Language Server Protocol server. It advertises
diagnostics, hover, formatting, completion, signature help, code actions,
document symbols, and a conservative go-to-definition subset. Document symbols
cover current-document top-level `fn`, `record`, `enum`, `type`, `let`, and
`const` declarations. Diagnostics include `data.trace_id` for tool correlation.
Go-to-definition only resolves those top-level
declarations in the same open document; workspace symbols, rename, cross-file
definition lookup, watch mode, and daemon mode are not exposed as capabilities.

`nox host-metadata --json` emits `nox.host-metadata.v1`, a local-process view of
registered host functions with signatures, docstrings, and declared capability
metadata. The LSP uses the same metadata for completion details, hover text, and
signature help.

The detailed Chinese CLI reference is available in [`../zh_CN/cli.md`](../zh_CN/cli.md).
