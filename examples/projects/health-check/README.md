# health-check

A small Nox project that checks whether a few expected files and environment
variables are present, and reports a single overall status. It demonstrates
real-world stdlib usage outside the `scoreboard` calculator sample:

- `std/fs.nox` for file presence checks (`fs.exists`).
- `std/env.nox` for optional environment lookups (`env.try_get`).
- Pure helpers separated from capability-bound functions so unit tests run
  without requiring runtime capability grants.

## Layout

- `nox.toml` — project manifest.
- `src/checks.nox` — capability-bound helpers plus the pure decision helper.
- `src/main.nox` — entry point that combines results and prints status.
- `tests/checks_test.nox` — unit tests for the pure helpers (no capability grants needed).

## Run from scratch

From the project root (`examples/projects/health-check`):

```sh
# Build the CLI once at the repo root.
cargo build -p nox

# Type-check, run unit tests, and verify formatting in one command.
../../../target/debug/nox project check

# Or run the main script directly (uses default permissive runtime).
../../../target/debug/nox run src/main.nox
```

`nox project check` runs `check`, `test`, and `fmt --check` for every module
listed in `nox.toml`'s `[modules]` section, and reports a single summary.
