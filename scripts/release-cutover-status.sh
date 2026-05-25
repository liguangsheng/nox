#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

current_version=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
VERSION=${NOX_RELEASE_VERSION:-$current_version}
TAG=${NOX_RELEASE_TAG:-"v$VERSION"}
CI_EVIDENCE=${NOX_RELEASE_CI_EVIDENCE:-}
DIST=${NOX_RELEASE_ASSET_DIR:-"/tmp/nox-release-assets-$TAG"}

failures=0

usage() {
    cat >&2 <<'EOF'
Usage:
  scripts/release-cutover-status.sh
  scripts/release-cutover-status.sh --json
  scripts/release-cutover-status.sh --self-test

Print the remaining read-only Phase 77 release cutover evidence.
This script does not edit files, commit, tag, push, build assets, upload a
GitHub Release, or run release gates.
EOF
}

JSON=0
if [ "${1:-}" = "--json" ]; then
    JSON=1
    shift
fi

STATUS_FILE=$(mktemp "${TMPDIR:-/tmp}/nox-release-cutover-status-items.XXXXXX")
trap 'rm -f "$STATUS_FILE"' EXIT HUP INT TERM

status() {
    state=$1
    shift
    message=$*
    if [ "$JSON" = "1" ]; then
        printf '%s\t%s\n' "$state" "$message" >> "$STATUS_FILE"
    else
        printf 'release cutover status: %s: %s\n' "$state" "$message"
    fi
}

ok() {
    status ok "$*"
}

missing() {
    failures=$((failures + 1))
    status missing "$*"
}

is_ci_evidence() {
    case "$1" in
        http://*|https://*|*[0-9]*) return 0 ;;
        *) return 1 ;;
    esac
}

asset_ok() {
    base=$1
    [ -f "$DIST/$base.tar.gz" ] || return 1
    [ -f "$DIST/$base.sha256" ] || return 1
    (cd "$DIST" && sha256sum -c "$base.sha256" >/dev/null 2>&1)
}

if [ "${1:-}" = "--self-test" ]; then
    is_ci_evidence "https://github.com/example/actions/runs/1" || {
        printf 'release cutover status: self-test rejected CI URL\n' >&2
        exit 1
    }
    is_ci_evidence "12345" || {
        printf 'release cutover status: self-test rejected CI run id\n' >&2
        exit 1
    }
    ! is_ci_evidence "pending" || {
        printf 'release cutover status: self-test accepted placeholder CI evidence\n' >&2
        exit 1
    }

    tmp=$(mktemp -d "${TMPDIR:-/tmp}/nox-release-cutover-status.XXXXXX")
    trap 'rm -rf "$tmp" "$STATUS_FILE"' EXIT HUP INT TERM
    DIST=$tmp
    TAG=v0.0.5
    asset="nox-cli-$TAG-x86_64-unknown-linux-gnu"
    printf 'asset\n' > "$DIST/$asset.tar.gz"
    (cd "$DIST" && sha256sum "$asset.tar.gz" > "$asset.sha256")
    asset_ok "$asset" || {
        printf 'release cutover status: self-test rejected valid asset\n' >&2
        exit 1
    }
    rm -f "$DIST/$asset.sha256"
    ! asset_ok "$asset" || {
        printf 'release cutover status: self-test accepted missing sha256 sidecar\n' >&2
        exit 1
    }
    printf 'release cutover status: self-test ok\n'
    exit 0
fi

[ $# -eq 0 ] || {
    usage
    exit 2
}

current_version=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
if [ "$current_version" = "$VERSION" ]; then
    ok "workspace version is $VERSION"
else
    missing "workspace version is $current_version, expected $VERSION"
fi

if git diff --quiet -- . ':!target'; then
    ok "worktree source diff is clean"
else
    missing "worktree has source diffs; release-prep commit is still needed"
fi

if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    tag_commit=$(git rev-list -n 1 "$TAG")
    head_commit=$(git rev-parse HEAD)
    if [ "$tag_commit" = "$head_commit" ]; then
        ok "$TAG points at HEAD"
    else
        missing "$TAG points at $tag_commit, not HEAD $head_commit"
    fi
else
    missing "missing release tag $TAG"
fi

if [ -n "$CI_EVIDENCE" ] && is_ci_evidence "$CI_EVIDENCE"; then
    ok "remote CI evidence provided: $CI_EVIDENCE"
else
    missing "missing remote CI evidence; set NOX_RELEASE_CI_EVIDENCE to a run URL or id"
fi

if [ -d "$DIST" ]; then
    ok "release asset directory exists: $DIST"
else
    missing "missing release asset directory $DIST"
fi

while IFS= read -r asset; do
    if asset_ok "$asset"; then
        ok "asset verified: $asset"
    else
        missing "asset or sha256 sidecar missing/invalid: $asset"
    fi
done <<EOF
$(NOX_RELEASE_TAG="$TAG" scripts/release-asset-manifest.sh)
EOF

if [ "$failures" -eq 0 ]; then
    if [ "$JSON" = "1" ]; then
        VERSION="$VERSION" TAG="$TAG" DIST="$DIST" FAILURES="$failures" STATUS_FILE="$STATUS_FILE" python3 - <<'PY'
import json, os
items = []
with open(os.environ["STATUS_FILE"], encoding="utf-8") as fh:
    for line in fh:
        state, message = line.rstrip("\n").split("\t", 1)
        items.append({"state": state, "message": message})
print(json.dumps({
    "schema": "nox.release-cutover-status.v1",
    "ok": True,
    "version": os.environ["VERSION"],
    "tag": os.environ["TAG"],
    "asset_dir": os.environ["DIST"],
    "pending_count": int(os.environ["FAILURES"]),
    "items": items,
}, ensure_ascii=False, sort_keys=True))
PY
    else
        printf 'release cutover status: ready for strict cutover check\n'
    fi
    exit 0
fi

if [ "$JSON" = "1" ]; then
    VERSION="$VERSION" TAG="$TAG" DIST="$DIST" FAILURES="$failures" STATUS_FILE="$STATUS_FILE" python3 - <<'PY'
import json, os
items = []
with open(os.environ["STATUS_FILE"], encoding="utf-8") as fh:
    for line in fh:
        state, message = line.rstrip("\n").split("\t", 1)
        items.append({"state": state, "message": message})
print(json.dumps({
    "schema": "nox.release-cutover-status.v1",
    "ok": False,
    "version": os.environ["VERSION"],
    "tag": os.environ["TAG"],
    "asset_dir": os.environ["DIST"],
    "pending_count": int(os.environ["FAILURES"]),
    "items": items,
}, ensure_ascii=False, sort_keys=True))
PY
else
    printf 'release cutover status: %s item(s) pending before strict cutover check\n' "$failures"
fi
exit 1
