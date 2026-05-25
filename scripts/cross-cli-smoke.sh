#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
TARGET=${NOX_CROSS_CLI_TARGET:-x86_64-unknown-linux-musl}
TOOLCHAIN=${NOX_CROSS_CLI_TOOLCHAIN:-stable}

cd "$ROOT"

NOX_CLI_SMOKE_TARGET="$TARGET" NOX_CLI_SMOKE_TOOLCHAIN="$TOOLCHAIN" scripts/cli-smoke.sh
printf 'cross CLI smoke: ok\n'
