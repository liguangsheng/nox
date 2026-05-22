# Architecture

Nox is split into two crates:

- `nox_core` is the embeddable engine. It contains lexing, parsing, static type checking, bytecode compilation, the VM, diagnostics, heap-managed values, Rust host-function APIs, and the C ABI.
- `nox` is the default runtime and CLI. It builds on `nox_core` and owns file loading, project discovery, runtime permissions, standard library modules, LSP behavior, and command-line output.

The boundary is intentional: embedding hosts can depend on `nox_core` without accepting the default filesystem, network, environment, timer, or CLI policy from `nox`.

The execution pipeline is:

1. Source text is lexed into spanned tokens.
2. Tokens are parsed into an AST.
3. Static checks validate names, types, imports, records, modules, and control-flow requirements.
4. Valid AST is compiled to flat bytecode.
5. The bytecode verifier rejects malformed instruction streams before execution.
6. The VM evaluates bytecode with an optional instruction budget and host callback support.

The default runtime resolves relative imports from the entry file or project manifest roots, then installs typed `std/*` modules and compatibility global functions according to explicit permissions.

Detailed Chinese architecture notes are available in [`../zh_CN/architecture.md`](../zh_CN/architecture.md).
