# Diagnostics

Nox diagnostics carry human-readable messages, byte spans, optional source locations, optional runtime `stack_frames`, and machine-readable `code` values. The same codes are used across CLI JSON output and LSP diagnostics where applicable. LSP diagnostics also include `data.trace_id`, a deterministic per-diagnostic identifier that tools can use to associate editor diagnostics with trace/log records.

Important code families include:

- `parse.*` and `type.*` for frontend errors.
- `module.*` for import and module visibility failures.
- `manifest.invalid` and `project.discovery` for project configuration problems.
- `permission.denied` for runtime capability denials.
- `host.callback` for host callback failures without a more specific host-provided code.
- `type-alias.cyclic` for direct or indirect cyclic type aliases.
- `enum.variant-not-found` for user enum constructors or patterns that name a missing variant.
- `generic.infer-failed` for generic function calls whose type parameters cannot be inferred consistently.
- `type.bitwise-non-int` for bitwise operators used with non-`int` operands.
- `control-flow.let-else-fallthrough` for `let ... else` branches that can fall through without returning.
- `type.spread-mismatch` for array/map spread sources that are not matching containers.
- `tuple.arity-mismatch` and `tuple.element-type-mismatch` for tuple shape/type errors.
- `bytecode.verifier` for malformed bytecode rejection.
- `runtime.index-out-of-range` for out-of-range index assignments such as `arr[i] = v`.
- `type.assign-target` for assignment targets that are not a variable, array index, or map index.
- `generic.constraint-unsatisfied` when a generic function's actual type argument does not implement the declared built-in marker (Equatable / Comparable / Stringify / Hashable).
- `generic.constraint-unknown` when a `<T: Marker>` clause references a marker name that is not in the built-in set.
- `parse.reserved-keyword` when source code uses `try`, `catch`, `panic`, `defer`, or `finally` as an identifier; these words are reserved per ADR 0021 for future exception-mechanism evaluation.
- `watch.path-not-found` when `nox watch` is started but a `source_dirs` / `test_dirs` entry declared in the manifest does not exist on disk.
- `test.assertion-failed` when an `assert_*` / `fail` helper in `std/test.nox` rejects a test case.
- `runtime.call-stack-overflow` when script call-stack depth exceeds `Engine::set_max_call_stack_depth`. When the limit is not configured, the runtime falls back to the OS stack and no diagnostic is raised.
- `runtime.string-length-cap` when a string concatenation (`+`) produces a result longer than `Engine::set_max_string_length`. No diagnostic is raised when the cap is not configured.
- `runtime.array-length-cap` when an array literal construction or `array.append` growth exceeds `Engine::set_max_array_length`. No diagnostic is raised when the cap is not configured.
- `runtime.map-size-cap` when a map literal construction, `map.set`, or map index assignment grows past `Engine::set_max_map_entries`. Updating an existing key does not increase the entry count. No diagnostic is raised when the cap is not configured.
- `runtime.heap-object-cap` when VM allocations or host callback return values make the engine heap exceed `Engine::set_max_heap_objects`. No diagnostic is raised when the cap is not configured.
- `runtime.task-pending-cap` when `task_sleep_ms` would exceed `RuntimePermissions::async_task_max_pending`. The default limit is 1024 pending sleep tasks per `Runtime`.
- `lint.unused-variable` / `lint.unused-function` / `lint.unused-import` when `nox lint` detects top-level declarations that are never referenced. Underscore-prefixed variables and the `main` entry function are exempted.
- `lint.unreachable-code` when `nox lint` detects statements after `return`, `break`, or `continue` in the same block (function body, if branch, while/for body, or lambda body).
- `lint.shadowed-variable` when `nox lint` detects an inner `let` declaration that shadows an outer same-named binding (function parameter or outer `let`). Underscore-prefixed names are exempted.
- `lint.constant-condition` when `nox lint` detects a literal `true` or `false` as an `if` condition, or a literal `false` as a `while` condition. `while (true)` is exempted because it is the canonical forever-loop idiom.
- `lint.duplicate-match-arm` when `nox lint` detects two arms in the same `match` statement whose patterns are equal (int / float / str / range / enum variant / some / none / ok / err compared recursively), making the later arm unreachable.

Tooling should prefer diagnostic codes over matching message text. Runtime `stack_frames` are additive and ordered with the most recent call first; each frame carries a `kind` field set to `"script"` for user-defined Nox functions or `"host"` for registered host callbacks. CLI text output renders this as `[script]` / `[host]` tags after the frame name. LSP diagnostics place `trace_id` and any runtime `stack_frames` together under `data`.

For readability, typecheck messages may append `, did you mean 'X'?` to `undefined variable`, `record '...' has no field '...'`, `enum '...' has no variant '...'` and `unknown type '...'` errors when a close (Levenshtein-distance) candidate is in scope. The suffix is advisory only — it does not affect `code` or machine-readable JSON.

The detailed Chinese diagnostics reference is available in [`../zh_CN/diagnostics.md`](../zh_CN/diagnostics.md).
