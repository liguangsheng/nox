#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

fail() {
    printf 'release cutover check: %s\n' "$*" >&2
    exit 1
}

ok() {
    printf 'release cutover check: ok: %s\n' "$*"
}

is_ci_evidence() {
    case "$1" in
        http://*|https://*|*[0-9]*) return 0 ;;
        *) return 1 ;;
    esac
}

is_release_version() {
    printf '%s\n' "$1" | grep -Eq '^0\.0\.(0|[1-9][0-9]*)$'
}

require_asset() {
    base=$1
    file="$DIST/$base.tar.gz"
    sum="$DIST/$base.sha256"
    [ -f "$file" ] || fail "missing asset $file"
    [ -f "$sum" ] || fail "missing sha256 sidecar $sum"
    (cd "$DIST" && sha256sum -c "$base.sha256" >/dev/null) || fail "sha256 check failed for $base"
    ok "asset verified: $base"
}

if [ "${1:-}" = "--self-test" ]; then
    is_ci_evidence "https://github.com/example/actions/runs/1" || fail "self-test rejected CI URL"
    is_ci_evidence "12345" || fail "self-test rejected CI run id"
    ! is_ci_evidence "pending" || fail "self-test accepted placeholder CI evidence"

    DIST=$(mktemp -d "${TMPDIR:-/tmp}/nox-release-cutover-check.XXXXXX")
    trap 'rm -rf "$DIST"' EXIT HUP INT TERM
    TAG=v0.0.5
    asset="nox-cli-$TAG-x86_64-unknown-linux-gnu"
    printf 'asset\n' > "$DIST/$asset.tar.gz"
    (cd "$DIST" && sha256sum "$asset.tar.gz" > "$asset.sha256")
    require_asset "$asset" >/dev/null
    rm -f "$DIST/$asset.sha256"
    ! (require_asset "$asset" >/dev/null 2>&1) || fail "self-test accepted missing sha256 sidecar"

    printf 'release cutover check: self-test ok\n'
    exit 0
fi

VERSION=${NOX_RELEASE_VERSION:-}
if [ -z "$VERSION" ]; then
    VERSION=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
fi
TAG=${NOX_RELEASE_TAG:-"v$VERSION"}
CI_EVIDENCE=${NOX_RELEASE_CI_EVIDENCE:-}
DIST=${NOX_RELEASE_ASSET_DIR:-"/tmp/nox-release-assets-$TAG"}

is_release_version "$VERSION" || fail "expected release cutover version 0.0.x, got $VERSION"

if git diff --quiet -- . ':!target'; then
    ok "worktree source diff is clean"
else
    fail "worktree has source diffs; commit the release-prep changes before cutover"
fi

if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    tag_commit=$(git rev-list -n 1 "$TAG")
    head_commit=$(git rev-parse HEAD)
    [ "$tag_commit" = "$head_commit" ] || fail "$TAG points at $tag_commit, not HEAD $head_commit"
    ok "$TAG points at HEAD"
else
    fail "missing release tag $TAG"
fi

[ -n "$CI_EVIDENCE" ] || fail "missing NOX_RELEASE_CI_EVIDENCE"
if is_ci_evidence "$CI_EVIDENCE"; then
    ok "remote CI evidence provided: $CI_EVIDENCE"
else
    fail "NOX_RELEASE_CI_EVIDENCE must be a run URL or id"
fi

NOX_RELEASE_READINESS_MODE=cutover NOX_RELEASE_CUTOVER_VERSION="$VERSION" \
    scripts/release-candidate-readiness.sh >/dev/null
ok "cutover readiness guard passes"

NOX_RELEASE_VERSION="$VERSION" NOX_RELEASE_CI_EVIDENCE="$CI_EVIDENCE" \
    scripts/release-audit.sh >/dev/null
ok "strict release audit passes"

[ -d "$DIST" ] || fail "missing release asset directory $DIST"

NOX_RELEASE_TAG="$TAG" scripts/release-asset-manifest.sh | while IFS= read -r asset; do
    require_asset "$asset"
done

printf 'release cutover check: ok\n'
