#!/usr/bin/env sh
set -eu

VERSION=${NOX_RELEASE_VERSION:-0.0.5}
TAG=${NOX_RELEASE_TAG:-"v$VERSION"}

usage() {
    cat >&2 <<'EOF'
Usage:
  scripts/release-asset-manifest.sh
  scripts/release-asset-manifest.sh --self-test

Print the required Phase 77 release asset base names, without extensions.
This is the shared source for release cutover status and strict cutover checks.
EOF
}

emit_manifest() {
    tag=$1
    printf 'nox-cli-%s-x86_64-unknown-linux-gnu\n' "$tag"
    printf 'nox-embed-%s-x86_64-unknown-linux-gnu\n' "$tag"
    printf 'nox-cli-%s-x86_64-unknown-linux-musl\n' "$tag"
}

if [ "${1:-}" = "--self-test" ]; then
    expected='nox-cli-v0.0.5-x86_64-unknown-linux-gnu
nox-embed-v0.0.5-x86_64-unknown-linux-gnu
nox-cli-v0.0.5-x86_64-unknown-linux-musl'
    actual=$(emit_manifest v0.0.5)
    [ "$actual" = "$expected" ] || {
        printf 'release asset manifest: self-test mismatch\nexpected:\n%s\nactual:\n%s\n' "$expected" "$actual" >&2
        exit 1
    }
    printf 'release asset manifest: self-test ok\n'
    exit 0
fi

[ $# -eq 0 ] || {
    usage
    exit 2
}

emit_manifest "$TAG"
