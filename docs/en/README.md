# Nox Documentation

This directory contains the English documentation for Nox. The full Chinese documentation is available under [`../zh_CN`](../zh_CN/README.md).

Start here:

- [Architecture](architecture.md): crate boundaries, execution pipeline, and runtime responsibilities.
- [Language v0](language-v0.md): implemented syntax, types, expressions, modules, and current limits.
- [CLI](cli.md): command behavior, exit codes, and JSON output.
- [Cookbook](cookbook.md): task recipes for projects, CLI scripts, data, HTTP, tests, and embedding.
- [Runtime](runtime.md): permissions, standard library modules, and capability boundaries.
- [Stability and compatibility](stability.md): stable, experimental, deferred, and internal public-surface boundaries.
- [Support and security policy](support-policy.md): supported versions, EOL, hotfixes, withdrawn releases, and vulnerability response.
- [Standard Library Index](stdlib-index.md): stdlib modules grouped by topic with stability tags.
- [Embedding](embedding.md): Rust API, C ABI, host functions, ownership, and error handling.
- [Diagnostics](diagnostics.md): stable machine-readable diagnostic codes.
- [Development](development.md): validation commands and contribution rules.
- [Release checklist](release-checklist.md): release versioning, tags, CI evidence, assets, and rollback expectations.
- [Benchmarks](benchmarks.md): benchmark smoke workflow.
- [Directory structure](directory-structure.md): repository layout and file ownership.

The current production release is `v0.0.6`. Release notes are recorded in [`../../CHANGELOG.md`](../../CHANGELOG.md).

Detailed design notes and ADRs are currently maintained in Chinese under [`../zh_CN`](../zh_CN/README.md). English pages document the supported public surface and link back to the corresponding Chinese deep-dive pages where useful.
