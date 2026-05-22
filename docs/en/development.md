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

The release gate covers Cargo checks, CLI smoke tests, project checks, embedding regression, robustness smoke, benchmark smoke, Markdown link checks, and whitespace checks.

Do not commit generated build outputs, internal handoff notes, personal tool caches, or temporary binaries.

The detailed Chinese development guide is available in [`../zh_CN/development.md`](../zh_CN/development.md).
