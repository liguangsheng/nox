# nox

`nox` is the command-line runtime and tooling crate for the Nox scripting language.

It provides the `nox` binary for checking, running, testing, formatting, scaffolding,
and inspecting `.nox` projects. The runtime embeds `nox_core` and installs the
standard library modules used by CLI scripts.

The package name `nox` is already occupied on crates.io by another project, and
the current `nox_core` dependency is not available in the registry under this
project's ownership. This repository therefore does not publish the CLI through
crates.io today. After a registry name is chosen or ownership is resolved,
install from crates.io with:

```sh
cargo install <nox-cli-crate> --locked
```

Install from a GitHub release tag:

```sh
cargo install --git https://github.com/liguangsheng/nox --tag v0.0.4 --locked nox
```

The C ABI dynamic library for embedding hosts is produced by `nox_core`, not by
`cargo install nox`. Use the `nox-embed` release tarball or build
`nox_core` directly when a C host needs `libnox_core`.
