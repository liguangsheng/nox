# Development

Run the standard local checks before publishing or merging substantial changes:

```sh
cargo fmt --all --check
cargo test --all
cargo clippy --all-targets -- -D warnings
git diff --check HEAD
```

Release-related changes should also run:

```sh
scripts/release-gate.sh
scripts/local-dist-smoke.sh
```

The release gate covers Cargo checks, CLI smoke tests, project checks,
compatibility golden checks, embedding regression, robustness smoke, benchmark
smoke, Markdown link checks, and whitespace checks. The compatibility golden
checks live in `scripts/compatibility-golden.sh`; they pin parser/formatter
surface, CLI diagnostic JSON, LSP diagnostic JSON, `nox doc` output, project
lockfile JSON, and host-metadata API JSON. Focused release-gate tests also keep
the parser AST golden, C ABI enum values, and async Rust API task behavior
explicitly visible.

## Editor Tooling

The TextMate grammar lives at `tools/nox.tmLanguage.json`, and the VS Code extension lives under `tools/vscode-nox/`. The extension contributes the `.nox` language, syntax highlighting, LSP startup, and a DAP debug configuration. It starts `nox lsp` and `nox dap` from the `nox.binaryPath` setting, `NOX_BINARY`, or `nox` on `PATH`.

Run the extension checks and package smoke with:

```sh
npm install --prefix tools/vscode-nox
npm run --prefix tools/vscode-nox smoke
npm run --prefix tools/vscode-nox package
```

The `.vsix` package includes runtime dependencies. After installation, `.nox` files should have highlighting, LSP hover/signature help/code actions/diagnostics/completion/formatting, and the `Debug Nox script` launch configuration.

## Fuzzing

The `fuzz/` workspace contains cargo-fuzz targets for long-running compiler-path stress tests:

- `parser`: lexing plus `parse_all`.
- `typecheck`: lexing, parsing, and type checking.
- `verifier`: lexing, parsing, type checking, bytecode compilation, and bytecode verification.

Install cargo-fuzz before running them:

```sh
cargo install cargo-fuzz
```

Short local smoke:

```sh
RUSTFLAGS="--cfg fuzzing" cargo check --manifest-path fuzz/Cargo.toml
cargo +nightly fuzz run parser -- -max_total_time=60
cargo +nightly fuzz run typecheck -- -max_total_time=60
cargo +nightly fuzz run verifier -- -max_total_time=60
```

The normal release gate does not run fuzzing by default. To opt in:

```sh
NOX_RELEASE_GATE_FUZZ=1 NOX_FUZZ_TIME=60 scripts/release-gate.sh
```

Property failure export and coverage checks are also opt-in release-gate layers.
They keep the default gate fast while giving CI/release jobs stable switches for
deeper test evidence:

```sh
NOX_RELEASE_GATE_PROPERTY=1 scripts/release-gate.sh
NOX_RELEASE_GATE_COVERAGE=1 scripts/release-gate.sh
```

## Sanitizer / Valgrind Smoke

Run the stage 15 sanitizer smoke before release-prep quality reviews:

```sh
scripts/sanitizer-smoke.sh
```

It runs heap and C ABI ownership regressions under ASan, a host-callback regression under TSan
with `-Z build-std`, then compiles `examples/embed/c_embedding.c` and runs it under Valgrind leak
checking. TSan requires the nightly `rust-src` component. The normal release gate does not run this
by default. To opt in:

```sh
NOX_RELEASE_GATE_SANITIZER=1 scripts/release-gate.sh
```

Set `CARGO_NIGHTLY`, `NOX_SANITIZER_TARGET`, or `VALGRIND` to override local tool paths.

Do not commit generated build outputs, internal handoff notes, personal tool caches, or temporary binaries.

The detailed Chinese development guide is available in [`../zh_CN/development.md`](../zh_CN/development.md).
