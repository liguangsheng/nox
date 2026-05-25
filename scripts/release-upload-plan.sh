#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

current_version=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
VERSION=${NOX_RELEASE_VERSION:-$current_version}
TAG=${NOX_RELEASE_TAG:-"v$VERSION"}
DIST=${NOX_RELEASE_ASSET_DIR:-"/tmp/nox-release-assets-$TAG"}

usage() {
    cat >&2 <<'EOF'
Usage:
  scripts/release-upload-plan.sh
  scripts/release-upload-plan.sh --self-test

Print the GitHub Release upload command for the required Phase 77 assets.
This script is read-only: it does not build assets, call gh, create releases,
push commits, or upload files.
EOF
}

shell_quote() {
    printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

emit_plan() {
    missing=0
    files=""
    while IFS= read -r asset; do
        tarball="$DIST/$asset.tar.gz"
        sidecar="$DIST/$asset.sha256"
        if [ ! -f "$tarball" ]; then
            printf 'release upload plan: missing: %s\n' "$tarball" >&2
            missing=$((missing + 1))
        else
            files="$files $tarball"
        fi
        if [ ! -f "$sidecar" ]; then
            printf 'release upload plan: missing: %s\n' "$sidecar" >&2
            missing=$((missing + 1))
        else
            if ! (cd "$DIST" && sha256sum -c "$asset.sha256" >/dev/null 2>&1); then
                printf 'release upload plan: invalid sha256 sidecar: %s\n' "$sidecar" >&2
                missing=$((missing + 1))
            else
                files="$files $sidecar"
            fi
        fi
    done <<EOF_MANIFEST
$(NOX_RELEASE_TAG="$TAG" scripts/release-asset-manifest.sh)
EOF_MANIFEST

    if [ "$missing" -ne 0 ]; then
        printf 'release upload plan: %s file(s) pending before upload\n' "$missing" >&2
        return 1
    fi

    printf 'gh release upload '
    shell_quote "$TAG"
    for file in $files; do
        printf ' '
        shell_quote "$file"
    done
    printf '\n'
}

if [ "${1:-}" = "--self-test" ]; then
    tmp=$(mktemp -d "${TMPDIR:-/tmp}/nox-release-upload-plan.XXXXXX")
    trap 'rm -rf "$tmp"' EXIT HUP INT TERM
    TAG=v0.0.5
    DIST=$tmp
    while IFS= read -r asset; do
        printf 'asset\n' > "$DIST/$asset.tar.gz"
        (cd "$DIST" && sha256sum "$asset.tar.gz" > "$asset.sha256")
    done <<EOF_MANIFEST
$(NOX_RELEASE_TAG="$TAG" scripts/release-asset-manifest.sh)
EOF_MANIFEST
    plan=$(emit_plan)
    case "$plan" in
        *"gh release upload 'v0.0.5'"*"$tmp/nox-cli-v0.0.5-x86_64-unknown-linux-gnu.tar.gz"*"$tmp/nox-cli-v0.0.5-x86_64-unknown-linux-musl.sha256"*) ;;
        *)
            printf 'release upload plan: self-test unexpected plan:\n%s\n' "$plan" >&2
            exit 1
            ;;
    esac
    rm -f "$DIST/nox-cli-v0.0.5-x86_64-unknown-linux-musl.sha256"
    ! emit_plan >/dev/null 2>/dev/null || {
        printf 'release upload plan: self-test accepted missing sidecar\n' >&2
        exit 1
    }
    (cd "$DIST" && sha256sum nox-cli-v0.0.5-x86_64-unknown-linux-gnu.tar.gz > nox-cli-v0.0.5-x86_64-unknown-linux-gnu.sha256)
    printf 'changed\n' >> "$DIST/nox-cli-v0.0.5-x86_64-unknown-linux-gnu.tar.gz"
    ! emit_plan >/dev/null 2>/dev/null || {
        printf 'release upload plan: self-test accepted invalid sidecar\n' >&2
        exit 1
    }
    printf 'release upload plan: self-test ok\n'
    exit 0
fi

[ $# -eq 0 ] || {
    usage
    exit 2
}

emit_plan
