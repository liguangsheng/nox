#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
TARGET=${NOX_CLI_SMOKE_TARGET:-}
TOOLCHAIN=${NOX_CLI_SMOKE_TOOLCHAIN:-stable}

cd "$ROOT"

if [ "${1:-}" = "--self-test" ]; then
    grep -q 'examples/hello.nox' "$0"
    grep -q 'check "$ROOT/examples/hello.nox"' "$0"
    printf 'cli smoke: self-test ok\n'
    exit 0
fi

if [ -n "$TARGET" ]; then
    printf 'cli smoke: toolchain %s target %s\n' "$TOOLCHAIN" "$TARGET"
    rustup target add --toolchain "$TOOLCHAIN" "$TARGET" >/dev/null
    cargo +"$TOOLCHAIN" build --release --target "$TARGET" -p nox >/dev/null
    BIN="$ROOT/target/$TARGET/release/nox"
else
    printf 'cli smoke: toolchain %s host target\n' "$TOOLCHAIN"
    cargo +"$TOOLCHAIN" build --release -p nox >/dev/null
    BIN="$ROOT/target/release/nox"
fi

if [ ! -x "$BIN" ] && [ -x "$BIN.exe" ]; then
    BIN="$BIN.exe"
fi

if [ ! -x "$BIN" ]; then
    printf 'cli smoke: missing executable %s\n' "$BIN" >&2
    exit 1
fi

"$BIN" --version | grep -E '^nox [0-9]+\.[0-9]+\.[0-9]+$' >/dev/null
output=$("$BIN" run "$ROOT/examples/hello.nox")
if [ "$output" != "84" ]; then
    printf 'cli smoke: expected hello output 84, got %s\n' "$output" >&2
    exit 1
fi
"$BIN" check "$ROOT/examples/hello.nox" >/dev/null

printf 'cli smoke: ok\n'
