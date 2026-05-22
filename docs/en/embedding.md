# Embedding

Use `nox_core` when embedding Nox into another application. The engine exposes a Rust API and a C ABI.

The C header is distributed as:

```text
include/nox_core.h
```

The release embedding SDK contains:

```text
lib/libnox_core.so
include/nox_core.h
examples/embed/c_embedding.c
```

Compile the C embedding smoke from the SDK:

```sh
cc -Inox-embed-v0.0.2-x86_64-unknown-linux-gnu/include \
  nox-embed-v0.0.2-x86_64-unknown-linux-gnu/examples/embed/c_embedding.c \
  -Lnox-embed-v0.0.2-x86_64-unknown-linux-gnu/lib -lnox_core \
  -Wl,-rpath,"$PWD/nox-embed-v0.0.2-x86_64-unknown-linux-gnu/lib" \
  -o /tmp/nox-c-embedding-smoke
/tmp/nox-c-embedding-smoke
```

Public ABI expectations include stable enum values, explicit handle ownership, engine-owned error strings, and documented host callback boundaries.

The detailed Chinese embedding guide is available in [`../zh_CN/embedding.md`](../zh_CN/embedding.md).
