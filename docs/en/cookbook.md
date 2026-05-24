# Cookbook

This page groups the existing Nox surface by common scripting tasks. The linked
examples are runnable repository files; short snippets show the shape of the
API when a full example would add noise.

## Start a Project

Create a minimal project:

```sh
nox new demo_app
cd demo_app
nox project check
nox run
nox test
nox fmt --check
```

The scaffold contains `nox.toml`, `src/main.nox`, `tests/main_test.nox`, and a
small README. Use `nox new demo_app --dir path/to/project` when the directory
name should differ from the package name.

## CLI Input and Output

Use `std/process.nox` for command-line scripts:

```nox
import "std/process.nox" as process;

let argv: [str] = process.argv();
let input: str = process.read_stdin();

if (len(argv) > 0 && input != "") {
    print("processed " + argv[0]);
    process.print_err("ok");
    process.exit(0);
} else {
    process.print_err("missing input");
    process.exit(2);
}
```

Runnable example: [`../../examples/process-stdio.nox`](../../examples/process-stdio.nox).

## JSON and TOML Config

Use `std/toml.nox` or `std/json.nox` to parse configuration into `json`, then
decode to a typed record with `json.from_json<T>` when the schema should be
strict:

```nox
import "std/json.nox" as json;
import "std/toml.nox" as toml;

record Config {
    name: str,
    retries: int,
}

let raw: result[json, str] = toml.parse("name = \"nox\"\nretries = 3");
match (raw) {
    ok(value) => {
        let decoded: result[Config, str] = json.from_json(value);
        match (decoded) {
            ok(config) => {
                config.name;
            }
            err(message) => {
                "bad config: " + message;
            }
        }
    }
    err(message) => {
        "bad TOML: " + message;
    }
}
```

JSON object walking example: [`../../examples/json.nox`](../../examples/json.nox).

## Files and Permissions

Runtime filesystem helpers require explicit capability grants from the host or
CLI integration. The project manifest can declare expected permissions, but it
does not grant them by itself. Keep capability-bound calls separated from pure
logic so tests can run without filesystem access.

Reference project: [`../../examples/projects/health-check`](../../examples/projects/health-check).

## HTTP Requests

`std/http.nox` supports plain `http://` GET/POST helpers returning
`result[(int, str), str]`, generic request helpers with custom headers and
response headers, and binary-body variants. Handle both transport errors and
non-2xx status codes explicitly:

```nox
import "std/http.nox" as http;

let response: result[(int, map[str, str], str), str] = http.request(
    "GET",
    "http://example.test/data",
    {"accept": "application/json"},
    "",
    1000,
);
match (response) {
    ok(pair) => {
        let (status, headers, body) = pair;
        if (status >= 200 && status < 300) {
            body;
        } else {
            "unexpected status";
        }
    }
    err(message) => {
        "request failed: " + message;
    }
}
```

HTTP requires the `network` capability. Embedding tests should prefer
`MockNetwork`; see [`embedding.md`](embedding.md). Response header names are
lowercased, and duplicate response headers are folded with `", "`.

## Delimited Data and JSONL-Style Data

Use `std/csv.nox` and `std/tsv.nox` for single-row or eager multi-row parsing
and formatting.
Runnable example: [`../../examples/delimited-text.nox`](../../examples/delimited-text.nox).

For JSON Lines input, use `std/jsonl.nox`. The helper is eager rather than
streaming, and parse errors include 1-based line numbers:

```nox
import "std/json.nox" as json;
import "std/jsonl.nox" as jsonl;

let values: result[[json], str] = jsonl.parse_lines("{\"ok\":true}\n{\"ok\":false}");
```

Runnable JSONL example: [`../../examples/jsonl.nox`](../../examples/jsonl.nox).

For deterministic digests, use `std/hash.nox`:

```nox
import "std/hash.nox" as hash;

let digest: str = hash.sha256_text("abc");
```

Runnable hash example: [`../../examples/hash.nox`](../../examples/hash.nox).

## Propagate Recoverable Errors

Use `result` or `option` for failures the script can handle, and reserve runtime
diagnostics for permission, resource, type, or host-boundary failures:

```nox
import "std/json.nox" as json;

fn normalize_config(source: str) -> result[str, str] {
    let value: json = json.parse(source)?;
    return ok(json.stringify(value));
}
```

Nox currently has no `try/catch/finally` exception channel and no `try {}`
block. Extract small functions when a local chain needs `?`, or use
`std/result.nox` / `std/option.nox` `map` and `and_then` for small value
transformations.

## Deduplicate Custom Values

`std/array.nox` keeps the older `Equatable` helpers and also exposes the first
trait-bound helpers through `Eq`. Implement `Eq` for a record when equality
should use a domain key rather than every field:

```nox
import "std/array.nox";

record User {
    id: int,
    name: str,
}

impl Eq for User {
    fn equals(self: User, other: User) -> bool {
        return self.id == other.id;
    }
}

let users: [User] = [
    User { id: 1, name: "ada" },
    User { id: 1, name: "ada-lovelace" },
];
let unique: [User] = dedupe_equal(users);
let found: bool = contains_equal(users, User { id: 1, name: "alias" });
```

## Tests, Snapshots, and Properties

Use ordinary `fn test_*() -> bool` functions for basic tests. Import
`std/test.nox` when a test should fail with richer diagnostics:

```nox
import "std/test.nox" as test;

fn test_snapshot() -> bool {
    test.assert_snapshot("greeting", "hello", "hello");
    return true;
}

fn test_property() -> bool {
    test.assert_property_int("non-negative square", 1, 8, 0, 10, fn(value: int) -> bool {
        return value * value >= 0;
    });
    return true;
}
```

The stdlib surface fixture exercises these helpers:
[`../../tests/fixtures/stdlib-surface.nox`](../../tests/fixtures/stdlib-surface.nox).

## Embedding a Host Function

For C embedding, compile against `crates/nox_core/include/nox_core.h` and
register host callbacks before evaluation. The minimal smoke is maintained as a
runnable C example: [`../../examples/embed/c_embedding.c`](../../examples/embed/c_embedding.c).

For Rust embedding, use `nox_core::Engine` when only language evaluation is
needed, or `nox::Runtime` when scripts need the default runtime modules,
permissions, and mocks. See [`embedding.md`](embedding.md) for ownership,
diagnostics, and mock filesystem/network examples.
