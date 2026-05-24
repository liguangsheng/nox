# Nox

Nox is an embeddable, statically typed scripting engine and default runtime written in Rust. Nox source files use the `.nox` extension.

[中文 README](README_zh_CN.md)

The workspace contains two crates:

- `nox_core`: the embeddable engine. It owns the language frontend, static type checker, bytecode, VM, value model, diagnostics, host functions, Rust API, and C ABI.
- `nox`: the default runtime and CLI built on top of `nox_core`. It owns file loading, permission checks, the standard library surface, LSP support, and command-line behavior.

`nox_core` exposes both a Rust API and a C ABI. The C header is at `crates/nox_core/include/nox_core.h`.

## Status

The latest production release is `v0.0.5`. For that version, the Cargo version, git tag, CHANGELOG, release checklist, GitHub Release, remote CI, local release gate, and distribution smoke tests are aligned.

Future `0.0.x` versions may still evolve the language, runtime, and embedding APIs. Breaking changes must be called out in the CHANGELOG, relevant documentation, and release notes.

Production readiness is defined in engineering release terms: no known high-severity defects, no undocumented compatibility breakage, conservative default permissions, and auditable, rollback-capable release steps. It is not a mathematical zero-risk claim.

The first language slice is implemented: spanned tokens, recursive-descent parsing, static type checking, flat bytecode compilation, a VM, typed variables, typed functions, calls, blocks, `if`, `while`, half-open `for` ranges, `return`, arrays, `map[str, T]`, `json`, named `record` values, relative imports, and `export` visibility.

The default runtime resolves `import "..."` relative to the entry file and installs a small typed standard library. Prefer static module imports for file, environment, and time capabilities, for example `import "std/fs.nox" as fs;`. Older global functions remain available as compatibility surface. Runtime permissions are explicit: the CLI only grants the filesystem read access needed for the entry file and imports by default. Environment variables, timers, networking, and async task helpers require separate permissions.

## Quick Start

### Use Release Packages

GitHub Releases split the command-line tool and embedding SDK starting with `v0.0.3`:

- `nox-cli-v0.0.5-x86_64-unknown-linux-gnu.tar.gz`: for CLI users. It contains `bin/nox`, README files, the CHANGELOG, and script examples.
- `nox-embed-v0.0.5-x86_64-unknown-linux-gnu.tar.gz`: for host applications. It contains `lib/libnox_core.so`, `include/nox_core.h`, README files, the CHANGELOG, and a C embedding example.

Download, verify, and install the CLI to `/usr/local/bin/nox`:

```sh
curl -LO https://github.com/liguangsheng/nox/releases/download/v0.0.5/nox-cli-v0.0.5-x86_64-unknown-linux-gnu.tar.gz
curl -LO https://github.com/liguangsheng/nox/releases/download/v0.0.5/nox-cli-v0.0.5-x86_64-unknown-linux-gnu.sha256
sha256sum -c nox-cli-v0.0.5-x86_64-unknown-linux-gnu.sha256
tar -xzf nox-cli-v0.0.5-x86_64-unknown-linux-gnu.tar.gz
sudo install -m 0755 nox-cli-v0.0.5-x86_64-unknown-linux-gnu/bin/nox /usr/local/bin/nox
nox --version
nox run ./nox-cli-v0.0.5-x86_64-unknown-linux-gnu/examples/hello.nox
```

Download and verify the embedding SDK:

```sh
curl -LO https://github.com/liguangsheng/nox/releases/download/v0.0.5/nox-embed-v0.0.5-x86_64-unknown-linux-gnu.tar.gz
curl -LO https://github.com/liguangsheng/nox/releases/download/v0.0.5/nox-embed-v0.0.5-x86_64-unknown-linux-gnu.sha256
sha256sum -c nox-embed-v0.0.5-x86_64-unknown-linux-gnu.sha256
tar -xzf nox-embed-v0.0.5-x86_64-unknown-linux-gnu.tar.gz
cc -Inox-embed-v0.0.5-x86_64-unknown-linux-gnu/include \
  nox-embed-v0.0.5-x86_64-unknown-linux-gnu/examples/embed/c_embedding.c \
  -Lnox-embed-v0.0.5-x86_64-unknown-linux-gnu/lib -lnox_core \
  -Wl,-rpath,"$PWD/nox-embed-v0.0.5-x86_64-unknown-linux-gnu/lib" \
  -o /tmp/nox-c-embedding-smoke
/tmp/nox-c-embedding-smoke
```

Platform support is split by artifact type:

- `x86_64-unknown-linux-gnu` is the current supported binary target for both
  the CLI and embedding SDK release assets.
- `x86_64-unknown-linux-musl` is covered by CI as a CLI-only cross-build and
  smoke target for the next release line. It does not expand the embedding SDK
  commitment until C ABI smoke coverage exists for that target.
- Other targets are source-build-only or best-effort until their toolchain,
  artifact build, and smoke evidence are part of the release checklist.

### Install with Cargo

The current package names `nox` and `nox_core` are not available as this
project's crates.io release names: `nox` is owned by another project, and
crates.io resolves `nox_core` to the existing `nox-core` crate. Registry
installation is therefore intentionally deferred. After a registry name is
chosen or ownership is resolved, the registry install form will be:

```sh
cargo install <nox-cli-crate> --locked
nox --version
```

Install the CLI directly from a GitHub tag when you need an exact repository
release before, or instead of, a crates.io publish:

```sh
cargo install --git https://github.com/liguangsheng/nox --tag v0.0.5 --locked nox
nox --version
```

Or from a local checkout (useful for tracking `main` or applying patches):

```sh
git clone https://github.com/liguangsheng/nox
cd nox
cargo install --path crates/nox --locked
nox --version
```

All Cargo install forms install to `~/.cargo/bin/nox`. Run `cargo uninstall nox`
to remove it. The `nox_core` C ABI dynamic library is not produced by
`cargo install`; embedding hosts should use the `nox-embed` release tarball or
`cargo build --release -p nox_core`.

### Build From Source

Build the CLI locally:

```sh
cargo build -p nox
target/debug/nox --version
```

Run the main examples:

```sh
cargo run -p nox -- run examples/hello.nox
cargo run -p nox -- check examples/hello.nox
cargo run -p nox -- check --json tests/fixtures/type-error.nox
cargo run -p nox -- test tests/fixtures/example_test.nox
cargo run -p nox -- fmt examples/hello.nox
cargo run -p nox -- inspect-bytecode --compact examples/hello.nox
```

Create and validate a new project:

```sh
target/debug/nox new demo_app
cd demo_app
../target/debug/nox project check
../target/debug/nox run
```

Projects that declare GitHub/git dependencies must also provide a matching
`nox.lock`; use `nox fetch` to populate the module cache and write the lockfile.
`project check` validates the lockfile but does not fetch dependencies.

Run the multi-module sample project:

```sh
cd examples/projects/scoreboard
cargo run -p nox -- project check
```

More examples are available under `examples/`:

- `arrays.nox`: homogeneous arrays, integer indexing, and `len(array)`.
- `maps.nox`: `map[str, T]`, string keys, map indexing, and `map_get`.
- `control-flow.nox`: typed functions, `while`, assignment, and `if`.
- `export-main.nox`: explicit `export` module boundaries.
- `example_test.nox`: a minimal `nox test` file.
- `for-range.nox`: half-open `int` range loops.
- `match.nox`: limited `match` branches.
- `numeric-boundaries.nox`: integer division and explicit numeric conversion boundaries.
- `print.nox`: `print` and `to_str_int` output helpers.
- `recursion.nox`: recursive function calls.
- `records.nox`: named records, record literals, and field access.
- `result-chain.nox`: `?` propagation for `result` and `option` chains.
- `collections-config.nox`: copy-oriented `std/map.nox` helpers for config merging.
- `collections-summary.nox`: `std/array.nox` and `std/map.nox` sorting and summary helpers.
- `error-summary.nox`: `std/option.nox` and `std/result.nox` status and fallback helpers.
- `process-stdio.nox`: `std/process.nox` argv, stdin, stderr, and exit-code helpers.
- `path-summary.nox`: `std/path.nox` join, normalize, basename, dirname, and extension helpers.
- `fs-summary.nox`: `std/fs.nox` file classification and directory listing helpers.
- `strings.nox`: typed strings, concatenation, `${expr}` interpolation, and `std/string.nox` helpers.
- `json.nox`: `std/json.nox` parse/stringify plus kind and array/object helpers.
- `delimited-text.nox`: `std/csv.nox` and `std/tsv.nox` line parsing and formatting helpers.
- `jsonl.nox`: `std/jsonl.nox` JSON Lines parsing and formatting with line-number errors.
- `hash.nox`: `std/hash.nox` SHA-256 and HMAC-SHA256 text and byte-array digest helpers.
- `stdlib.nox`: default runtime host function calls.
- `projects/scoreboard/`: a multi-module project with `nox.toml`, namespace imports, and source/test directories.
- `tests/fixtures/type-error*.nox`, `tests/fixtures/syntax-errors.nox`, and
  `tests/fixtures/runtime-error*.nox`: negative fixtures used by automated checks.

## Documentation

- [docs/en/README.md](docs/en/README.md): English documentation index.
- [docs/en/language-v0.md](docs/en/language-v0.md): implemented language slice.
- [docs/en/cli.md](docs/en/cli.md): command behavior and exit codes.
- [docs/en/cookbook.md](docs/en/cookbook.md): task recipes for projects, CLI scripts, data, HTTP, tests, and embedding.
- [docs/en/runtime.md](docs/en/runtime.md): runtime permissions and standard library.
- [docs/en/embedding.md](docs/en/embedding.md): Rust and C embedding guide.
- [docs/en/diagnostics.md](docs/en/diagnostics.md): machine-readable diagnostic codes.
- [docs/en/benchmarks.md](docs/en/benchmarks.md): benchmark smoke workflow.
- [docs/en/development.md](docs/en/development.md): validation, testing, and iteration notes.
- [docs/en/directory-structure.md](docs/en/directory-structure.md): directory structure and file ownership.
- [docs/zh_CN/README.md](docs/zh_CN/README.md): Chinese documentation index.
