#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
DIST=${NOX_LOCAL_DIST_DIR:-}
CC=${CC:-cc}

if [ -z "$DIST" ]; then
    DIST=$(mktemp -d "${TMPDIR:-/tmp}/nox-local-dist.XXXXXX")
else
    rm -rf "$DIST"
    mkdir -p "$DIST"
fi

cd "$ROOT"

BUILD="$ROOT/target/release"
VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys
for package in json.load(sys.stdin)["packages"]:
    if package["name"] == "nox":
        print(package["version"])
        break
else:
    raise SystemExit("missing nox package metadata")')

printf 'local dist smoke: build release artifacts\n'
cargo build --release -p nox -p nox_core >/dev/null

printf 'local dist smoke: stage %s\n' "$DIST"
mkdir -p "$DIST/bin" "$DIST/include" "$DIST/lib" "$DIST/examples" "$DIST/examples/projects"
cp "$BUILD/nox" "$DIST/bin/nox"
cp "$ROOT/crates/nox_core/include/nox_core.h" "$DIST/include/nox_core.h"
cp "$ROOT/examples/hello.nox" "$DIST/examples/hello.nox"
cp "$ROOT/examples/math.nox" "$DIST/examples/math.nox"
cp "$ROOT/examples/maps.nox" "$DIST/examples/maps.nox"
cp -R "$ROOT/examples/projects/scoreboard" "$DIST/examples/projects/scoreboard"

if [ -f "$BUILD/libnox_core.so" ]; then
    cp "$BUILD/libnox_core.so" "$DIST/lib/libnox_core.so"
elif [ -f "$BUILD/libnox_core.dylib" ]; then
    cp "$BUILD/libnox_core.dylib" "$DIST/lib/libnox_core.dylib"
else
    printf 'local dist smoke: missing release dynamic library for nox_core\n' >&2
    exit 1
fi

printf 'local dist smoke: version\n'
"$DIST/bin/nox" --version | grep -x "nox $VERSION" >/dev/null

printf 'local dist smoke: run example\n'
output=$("$DIST/bin/nox" run "$DIST/examples/hello.nox")
if [ "$output" != "84" ]; then
    printf 'local dist smoke: expected hello output 84, got %s\n' "$output" >&2
    exit 1
fi

printf 'local dist smoke: run map_get example\n'
output=$("$DIST/bin/nox" run "$DIST/examples/maps.nox")
if [ "$output" != "42" ]; then
    printf 'local dist smoke: expected maps output 42, got %s\n' "$output" >&2
    exit 1
fi

printf 'local dist smoke: project check json\n'
(cd "$DIST/examples/projects/scoreboard" && "$DIST/bin/nox" project check --json) \
    | grep -q '"schema":"nox.project-check.v1"'

cat >"$DIST/c_header_smoke.c" <<'C'
#include <stdint.h>
#include <stdio.h>
#include "nox_core.h"

int main(void) {
    NoxCoreEngine *engine = nox_core_engine_new();
    if (engine == NULL) {
        return 1;
    }

    NoxCoreValue value = {0};
    NoxCoreStatus status = nox_core_engine_eval(engine, "21 + 21;", &value);
    if (status != NOX_CORE_OK || value.kind != NOX_CORE_VALUE_INT || value.int_value != 42) {
        const char *error = nox_core_engine_last_error(engine);
        fprintf(stderr, "%s\n", error != NULL ? error : "unexpected C smoke result");
        nox_core_engine_free(engine);
        return 1;
    }

    printf("nox_core %s\n", nox_core_version());
    nox_core_engine_free(engine);
    return 0;
}
C

printf 'local dist smoke: c header\n'
"$CC" -I"$DIST/include" "$DIST/c_header_smoke.c" \
    -L"$DIST/lib" -lnox_core -Wl,-rpath,"$DIST/lib" -o "$DIST/c_header_smoke"
"$DIST/c_header_smoke" >/dev/null

printf 'local dist smoke: ok (%s)\n' "$DIST"
