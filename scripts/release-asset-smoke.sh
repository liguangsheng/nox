#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

VERSION=${NOX_RELEASE_VERSION:-0.0.5}
TAG=${NOX_RELEASE_TAG:-"v$VERSION"}
DIST=${NOX_RELEASE_ASSET_DIR:-"/tmp/nox-release-assets-$TAG"}
CC=${CC:-cc}
RUN_MODE=${NOX_RELEASE_ASSET_SMOKE_RUN:-auto}
COMPILE_EMBED=${NOX_RELEASE_ASSET_SMOKE_COMPILE_EMBED:-auto}

usage() {
    cat >&2 <<'EOF'
Usage:
  scripts/release-asset-smoke.sh
  scripts/release-asset-smoke.sh --self-test

Verify release tarballs and sha256 sidecars after building or downloading
GitHub Release assets. This script is read-only: it does not build assets,
commit, tag, push, upload files, or call GitHub.

Environment:
  NOX_RELEASE_VERSION              expected CLI version, default 0.0.5
  NOX_RELEASE_TAG                  expected tag, default v$NOX_RELEASE_VERSION
  NOX_RELEASE_ASSET_DIR            directory containing *.tar.gz and *.sha256
  NOX_RELEASE_ASSET_SMOKE_RUN      auto, always, or never; default auto
  NOX_RELEASE_ASSET_SMOKE_COMPILE_EMBED auto or never; default auto
EOF
}

fail() {
    printf 'release asset smoke: %s\n' "$*" >&2
    exit 1
}

host_triple() {
    rustc -vV 2>/dev/null | awk '/^host: /{print $2; exit}'
}

manifest_assets() {
    if [ -n "${NOX_RELEASE_ASSET_MANIFEST:-}" ]; then
        printf '%s\n' "$NOX_RELEASE_ASSET_MANIFEST"
    else
        NOX_RELEASE_TAG="$TAG" scripts/release-asset-manifest.sh
    fi
}

target_from_asset() {
    asset=$1
    case "$asset" in
        nox-cli-"$TAG"-*) printf '%s\n' "${asset#nox-cli-$TAG-}" ;;
        nox-embed-"$TAG"-*) printf '%s\n' "${asset#nox-embed-$TAG-}" ;;
        *) fail "asset $asset does not match expected tag $TAG" ;;
    esac
}

can_run_target() {
    target=$1
    host=${HOST_TRIPLE:-unknown}
    case "$RUN_MODE" in
        always) return 0 ;;
        never) return 1 ;;
        auto) ;;
        *) fail "NOX_RELEASE_ASSET_SMOKE_RUN must be auto, always, or never" ;;
    esac
    [ "$target" = "$host" ] && return 0
    [ "$host" = "x86_64-unknown-linux-gnu" ] && [ "$target" = "x86_64-unknown-linux-musl" ] && return 0
    return 1
}

verify_sidecar() {
    asset=$1
    [ -f "$DIST/$asset.tar.gz" ] || fail "missing asset $DIST/$asset.tar.gz"
    [ -f "$DIST/$asset.sha256" ] || fail "missing sha256 sidecar $DIST/$asset.sha256"
    (cd "$DIST" && sha256sum -c "$asset.sha256" >/dev/null) || fail "sha256 check failed for $asset"
}

extract_asset() {
    asset=$1
    out=$2
    mkdir -p "$out"
    tar -xzf "$DIST/$asset.tar.gz" -C "$out"
    [ -d "$out/$asset" ] || fail "asset $asset did not extract to top-level $asset directory"
}

smoke_cli_asset() {
    asset=$1
    dir=$2
    target=$3
    bin="$dir/bin/nox"
    [ -x "$bin" ] || fail "CLI asset $asset is missing executable bin/nox"
    [ -f "$dir/examples/hello.nox" ] || fail "CLI asset $asset is missing examples/hello.nox"

    if can_run_target "$target"; then
        "$bin" --version </dev/null | grep -x "nox $VERSION" >/dev/null || fail "CLI asset $asset reports wrong version"
        output=$("$bin" run "$dir/examples/hello.nox" </dev/null)
        [ "$output" = "84" ] || fail "CLI asset $asset hello smoke expected 84, got $output"
        printf 'release asset smoke: CLI executable ok: %s\n' "$asset"
    else
        printf 'release asset smoke: CLI executable skipped for non-host target: %s\n' "$asset"
    fi
}

smoke_embed_asset() {
    asset=$1
    dir=$2
    target=$3
    [ -f "$dir/include/nox_core.h" ] || fail "embed asset $asset is missing include/nox_core.h"
    [ -d "$dir/lib" ] || fail "embed asset $asset is missing lib/"
    lib=$(find "$dir/lib" -maxdepth 1 \( -name 'libnox_core.so' -o -name 'libnox_core.dylib' \) -print | head -n 1)
    [ -n "$lib" ] || fail "embed asset $asset is missing libnox_core dynamic library"

    [ "$COMPILE_EMBED" = never ] && {
        printf 'release asset smoke: embed compile skipped by configuration: %s\n' "$asset"
        return
    }
    [ "$COMPILE_EMBED" = auto ] || fail "NOX_RELEASE_ASSET_SMOKE_COMPILE_EMBED must be auto or never"

    if can_run_target "$target"; then
        c_file="$TMP_ROOT/$asset-c-smoke.c"
        exe="$TMP_ROOT/$asset-c-smoke"
        cat >"$c_file" <<'C'
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
        nox_core_engine_free(engine);
        return 1;
    }
    nox_core_engine_free(engine);
    return 0;
}
C
        "$CC" -I"$dir/include" "$c_file" -L"$dir/lib" -lnox_core -Wl,-rpath,"$dir/lib" -o "$exe"
        "$exe" </dev/null
        printf 'release asset smoke: embed C ABI ok: %s\n' "$asset"
    else
        printf 'release asset smoke: embed C ABI skipped for non-host target: %s\n' "$asset"
    fi
}

run_smoke() {
    [ -d "$DIST" ] || fail "missing release asset directory $DIST"
    HOST_TRIPLE=$(host_triple)
    HOST_TRIPLE=${HOST_TRIPLE:-unknown}
    TMP_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/nox-release-asset-smoke.XXXXXX")
    export HOST_TRIPLE TMP_ROOT
    trap 'rm -rf "$TMP_ROOT"' EXIT HUP INT TERM
    manifest_file="$TMP_ROOT/manifest.txt"
    manifest_assets >"$manifest_file"

    count=0
    while IFS= read -r asset; do
        [ -n "$asset" ] || continue
        count=$((count + 1))
        target=$(target_from_asset "$asset")
        verify_sidecar "$asset"
        extract_asset "$asset" "$TMP_ROOT"
        dir="$TMP_ROOT/$asset"
        case "$asset" in
            nox-cli-*) smoke_cli_asset "$asset" "$dir" "$target" ;;
            nox-embed-*) smoke_embed_asset "$asset" "$dir" "$target" ;;
            *) fail "unknown release asset kind: $asset" ;;
        esac
    done <"$manifest_file"
    [ "$count" -gt 0 ] || fail "release asset manifest is empty"
    printf 'release asset smoke: ok (%s)\n' "$DIST"
}

if [ "${1:-}" = "--self-test" ]; then
    tmp=$(mktemp -d "${TMPDIR:-/tmp}/nox-release-asset-smoke-self.XXXXXX")
    trap 'rm -rf "$tmp"' EXIT HUP INT TERM
    manifest='nox-cli-v0.0.5-self-test-target
nox-embed-v0.0.5-self-test-target'

    cli_base=nox-cli-v0.0.5-self-test-target
    mkdir -p "$tmp/$cli_base/bin" "$tmp/$cli_base/examples"
    cat >"$tmp/$cli_base/bin/nox" <<'SH'
#!/usr/bin/env sh
if [ "$1" = "--version" ]; then
    printf 'nox 0.0.5\n'
elif [ "$1" = "run" ]; then
    cat >/dev/null
    printf '84\n'
else
    exit 2
fi
SH
    chmod +x "$tmp/$cli_base/bin/nox"
    printf '21 + 21;\n' > "$tmp/$cli_base/examples/hello.nox"
    (cd "$tmp" && tar czf "$cli_base.tar.gz" "$cli_base" && sha256sum "$cli_base.tar.gz" > "$cli_base.sha256")
    rm -rf "$tmp/$cli_base"

    embed_base=nox-embed-v0.0.5-self-test-target
    mkdir -p "$tmp/$embed_base/include" "$tmp/$embed_base/lib"
    printf '/* self-test header */\n' > "$tmp/$embed_base/include/nox_core.h"
    printf 'self-test lib\n' > "$tmp/$embed_base/lib/libnox_core.so"
    (cd "$tmp" && tar czf "$embed_base.tar.gz" "$embed_base" && sha256sum "$embed_base.tar.gz" > "$embed_base.sha256")
    rm -rf "$tmp/$embed_base"

    NOX_RELEASE_VERSION=0.0.5 \
        NOX_RELEASE_TAG=v0.0.5 \
        NOX_RELEASE_ASSET_DIR="$tmp" \
        NOX_RELEASE_ASSET_MANIFEST="$manifest" \
        NOX_RELEASE_ASSET_SMOKE_RUN=always \
        NOX_RELEASE_ASSET_SMOKE_COMPILE_EMBED=never \
        "$0" >/dev/null

    printf 'changed\n' >> "$tmp/$cli_base.tar.gz"
    ! NOX_RELEASE_VERSION=0.0.5 \
        NOX_RELEASE_TAG=v0.0.5 \
        NOX_RELEASE_ASSET_DIR="$tmp" \
        NOX_RELEASE_ASSET_MANIFEST="$manifest" \
        NOX_RELEASE_ASSET_SMOKE_RUN=never \
        NOX_RELEASE_ASSET_SMOKE_COMPILE_EMBED=never \
        "$0" >/dev/null 2>/dev/null || fail "self-test accepted invalid sha256 sidecar"

    printf 'release asset smoke: self-test ok\n'
    exit 0
fi

case "${1:-}" in
    "") run_smoke ;;
    -h|--help) usage ;;
    *) usage; exit 2 ;;
esac
