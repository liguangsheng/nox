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

printf 'embedding regression: c abi symbols\n'
LIB="$ROOT/target/debug/libnox_core.so"
if [ ! -f "$LIB" ]; then
    LIB="$ROOT/target/debug/libnox_core.dylib"
fi
if [ ! -f "$LIB" ]; then
    printf 'missing nox_core dynamic library under target/debug\n' >&2
    exit 1
fi
HEADER_SYMBOLS=$(sed -n 's/^[A-Za-z_][A-Za-z0-9_ *]*\(nox_core_[A-Za-z0-9_]*\)(.*/\1/p' "$ROOT/crates/nox_core/include/nox_core.h" | sort -u)
EXPORTED_SYMBOLS=$(nm -g "$LIB" | sed -n 's/^.*[[:space:]]\(_\{0,1\}nox_core_[A-Za-z0-9_]*\)$/\1/p' | sed 's/^_//' | sort -u)
for symbol in $HEADER_SYMBOLS; do
    if ! printf '%s\n' "$EXPORTED_SYMBOLS" | grep -qx "$symbol"; then
        printf 'header declares %s but %s does not export it\n' "$symbol" "$LIB" >&2
        exit 1
    fi
done

printf 'embedding regression: c abi smoke\n'
"$CC" -I"$ROOT/crates/nox_core/include" "$ROOT/examples/embed/c_embedding.c" \
    -L"$ROOT/target/debug" -lnox_core -Wl,-rpath,"$ROOT/target/debug" -o "$OUT"
"$OUT"

printf 'embedding regression: ok\n'
