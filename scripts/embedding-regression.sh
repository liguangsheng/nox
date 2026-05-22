#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
CC=${CC:-cc}
OUT=${NOX_C_EMBEDDING_OUT:-"$ROOT/target/debug/c_embedding_smoke"}

printf 'embedding regression: rust api\n'
cargo test -p nox_core >/dev/null

printf 'embedding regression: runtime api\n'
cargo test -p nox session_and_runtime --lib >/dev/null

printf 'embedding regression: rust embedding example\n'
cargo run -p nox --example rust_embedding >/dev/null

printf 'embedding regression: build nox_core\n'
cargo build -p nox_core >/dev/null

printf 'embedding regression: c abi smoke\n'
"$CC" -I"$ROOT/crates/nox_core/include" "$ROOT/examples/embed/c_embedding.c" \
    -L"$ROOT/target/debug" -lnox_core -Wl,-rpath,"$ROOT/target/debug" -o "$OUT"
"$OUT"

printf 'embedding regression: ok\n'
