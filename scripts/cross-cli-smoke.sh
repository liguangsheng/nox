#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
TARGET=${NOX_CROSS_CLI_TARGET:-x86_64-unknown-linux-musl}
TOOLCHAIN=${NOX_CROSS_CLI_TOOLCHAIN:-stable}

cd "$ROOT"

printf 'cross CLI smoke: toolchain %s target %s\n' "$TOOLCHAIN" "$TARGET"
rustup target add --toolchain "$TOOLCHAIN" "$TARGET" >/dev/null
cargo +"$TOOLCHAIN" build --release --target "$TARGET" -p nox >/dev/null

BIN="$ROOT/target/$TARGET/release/nox"
if [ ! -x "$BIN" ]; then
    printf 'cross CLI smoke: missing executable %s\n' "$BIN" >&2
    exit 1
fi

"$BIN" --version | grep -E '^nox [0-9]+\.[0-9]+\.[0-9]+$' >/dev/null
output=$("$BIN" run "$ROOT/examples/hello.nox")
if [ "$output" != "84" ]; then
    printf 'cross CLI smoke: expected hello output 84, got %s\n' "$output" >&2
    exit 1
fi

printf 'cross CLI smoke: ok\n'
