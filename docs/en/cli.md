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
nox fmt examples/hello.nox
nox fmt --check tests/fixtures/formatter-golden.nox
nox inspect-bytecode --compact examples/hello.nox
```

Project commands use `nox.toml` discovery:

```sh
cd examples/projects/scoreboard
nox project check
nox project check --json
```

Machine-readable command output uses versioned schemas such as `nox.check.v1`, `nox.test.v1`, and `nox.project-check.v1`. Diagnostic `code` values are intended for tools and editors; see [Diagnostics](diagnostics.md).

The detailed Chinese CLI reference is available in [`../zh_CN/cli.md`](../zh_CN/cli.md).
