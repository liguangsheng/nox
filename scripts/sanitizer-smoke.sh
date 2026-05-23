#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
CARGO_NIGHTLY=${CARGO_NIGHTLY:-"$HOME/.cargo/bin/cargo"}
RUST_TARGET=${NOX_SANITIZER_TARGET:-x86_64-unknown-linux-gnu}
CC=${CC:-cc}
VALGRIND=${VALGRIND:-valgrind}
OUT=${NOX_VALGRIND_C_EMBEDDING_OUT:-"$ROOT/target/debug/c_embedding_smoke_valgrind"}

cd "$ROOT"

printf 'sanitizer smoke: ASan heap/C ABI regressions\n'
for test_name in \
    c_abi_handles_keep_heap_objects_until_freed \
    repeated_c_abi_handle_free_collects_nested_heap_values \
    c_abi_option_and_result_handles_keep_nested_heap_values_until_freed
do
    RUSTFLAGS="-Z sanitizer=address" \
        "$CARGO_NIGHTLY" +nightly test -p nox_core "$test_name" --target "$RUST_TARGET"
done

printf 'sanitizer smoke: TSan host callback regression\n'
RUSTFLAGS="-Z sanitizer=thread" \
    "$CARGO_NIGHTLY" +nightly test -Z build-std -p nox_core \
    repeated_host_callback_returns_do_not_accumulate_heap_values \
    --target "$RUST_TARGET"

if ! command -v "$VALGRIND" >/dev/null 2>&1; then
    printf 'sanitizer smoke: valgrind not found: %s\n' "$VALGRIND" >&2
    exit 1
fi

printf 'sanitizer smoke: build nox_core for Valgrind C embedding smoke\n'
cargo build -p nox_core >/dev/null

LIB="$ROOT/target/debug/libnox_core.so"
if [ ! -f "$LIB" ]; then
    LIB="$ROOT/target/debug/libnox_core.dylib"
fi
if [ ! -f "$LIB" ]; then
    printf 'sanitizer smoke: missing nox_core dynamic library under target/debug\n' >&2
    exit 1
fi

printf 'sanitizer smoke: compile C embedding smoke\n'
"$CC" -I"$ROOT/crates/nox_core/include" "$ROOT/examples/embed/c_embedding.c" \
    -L"$ROOT/target/debug" -lnox_core -Wl,-rpath,"$ROOT/target/debug" -o "$OUT"

printf 'sanitizer smoke: Valgrind C embedding smoke\n'
"$VALGRIND" \
    --quiet \
    --leak-check=full \
    --show-leak-kinds=definite,indirect,possible \
    --errors-for-leak-kinds=definite,indirect,possible \
    --error-exitcode=99 \
    "$OUT" >/dev/null

printf 'sanitizer smoke: ok\n'
