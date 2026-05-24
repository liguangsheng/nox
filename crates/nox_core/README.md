# nox_core

`nox_core` is the embeddable core of the Nox scripting language.

It owns the lexer, parser, static type checker, bytecode compiler, VM, value
model, diagnostics, Rust embedding API, host function API, and C ABI. The CLI
runtime crate `nox` builds on top of this crate and adds project loading,
standard library modules, permissions, and command-line tools.

Embedding hosts can use the Rust API from this crate or the C ABI declared in
`include/nox_core.h`. The release tarball named `nox-embed-vX.Y.Z-<target>.tar.gz`
contains the dynamic library, C header, and C embedding example.

The package name is not currently a release commitment. crates.io resolves
`nox_core` to the existing `nox-core` crate, so this repository must not publish
the core crate until a registry name or ownership path is explicitly chosen.
Use GitHub tags, release assets, or a local checkout for now.

Before any future registry publish attempt, run:

```sh
cargo publish --dry-run -p nox_core
```
