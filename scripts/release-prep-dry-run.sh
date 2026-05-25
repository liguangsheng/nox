#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

current_for_default=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
current_patch_for_default=${current_for_default##*.}
VERSION=${NOX_RELEASE_VERSION:-0.0.$((current_patch_for_default + 1))}
DATE=${NOX_RELEASE_DATE:-$(date +%F)}

usage() {
    cat >&2 <<'EOF'
Usage:
  scripts/release-prep-dry-run.sh [version yyyy-mm-dd]
  scripts/release-prep-dry-run.sh --self-test

Run the release-prep version switch in a temporary copy of the current checkout.
This script does not edit the current worktree, commit, tag, push, build release
assets, create a GitHub Release, or upload files.

Set NOX_RELEASE_DRY_RUN_KEEP=1 to keep the temporary copy for inspection.
EOF
}

fail() {
    printf 'release prep dry-run: %s\n' "$*" >&2
    exit 1
}

current_version() {
    awk -F'"' '/^version = /{print $2; exit}' Cargo.toml
}

validate_existing_candidate() {
    NOX_RELEASE_READINESS_MODE=cutover NOX_RELEASE_CUTOVER_VERSION="$VERSION" \
        scripts/release-candidate-readiness.sh >/dev/null
    NOX_RELEASE_VERSION="$VERSION" scripts/release-notes.sh >/dev/null
}

if [ "${1:-}" = "--self-test" ]; then
    current=$(current_version)
    if grep -q "^## \\[$current\\]" CHANGELOG.md; then
        VERSION=$current
        validate_existing_candidate
    else
        patch=${current##*.}
        next_patch=$((patch + 1))
        scripts/prepare-release-version.sh --check-only "0.0.$next_patch" 2026-05-25 >/dev/null
    fi
    ! NOX_RELEASE_VERSION=1.0.0 "$0" --validate-only >/dev/null 2>/dev/null || {
        fail "self-test accepted invalid version"
    }
    printf 'release prep dry-run: self-test ok\n'
    exit 0
fi

if [ "${1:-}" = "--validate-only" ]; then
    current=$(current_version)
    if [ "$current" = "$VERSION" ]; then
        validate_existing_candidate
        exit 0
    fi
    scripts/prepare-release-version.sh --check-only "$VERSION" "$DATE" >/dev/null
    exit 0
fi

if [ $# -eq 2 ]; then
    VERSION=$1
    DATE=$2
elif [ $# -ne 0 ]; then
    usage
    exit 2
fi

current=$(current_version)
if [ "$current" = "$VERSION" ]; then
    validate_existing_candidate
    printf 'release prep dry-run: already prepared for %s (%s); cutover readiness ok\n' "$VERSION" "$DATE"
    exit 0
fi

scripts/prepare-release-version.sh --check-only "$VERSION" "$DATE" >/dev/null

tmp=$(mktemp -d "${TMPDIR:-/tmp}/nox-release-prep-dry-run.XXXXXX")
AGENT_HANDOFF_NAME=agents
AGENT_HANDOFF_DIR=".${AGENT_HANDOFF_NAME}"
cleanup() {
    if [ "${NOX_RELEASE_DRY_RUN_KEEP:-0}" = "1" ]; then
        printf 'release prep dry-run: kept temp checkout: %s\n' "$tmp"
    else
        rm -rf "$tmp"
    fi
}
trap cleanup EXIT HUP INT TERM

printf 'release prep dry-run: copying current checkout to %s\n' "$tmp"
tar \
    --exclude='./.git' \
    --exclude="./$AGENT_HANDOFF_DIR" \
    --exclude='./.codex' \
    --exclude='./target' \
    --exclude='./fuzz/target' \
    --exclude='./fuzz/artifacts' \
    -cf - . | (cd "$tmp" && tar -xf -)

(
    cd "$tmp"
    git init -q
    git config user.email nox-release-dry-run@example.invalid
    git config user.name "Nox Release Dry Run"
    git add .
    git commit -q -m "dry-run baseline"

    scripts/prepare-release-version.sh "$VERSION" "$DATE" >/dev/null
    NOX_RELEASE_READINESS_MODE=cutover NOX_RELEASE_CUTOVER_VERSION="$VERSION" \
        scripts/release-candidate-readiness.sh >/dev/null
    NOX_RELEASE_VERSION="$VERSION" scripts/release-notes.sh >/dev/null

    printf 'release prep dry-run: cutover readiness ok for %s (%s)\n' "$VERSION" "$DATE"
    printf 'release prep dry-run: release-prep changed files:\n'
    git diff --name-only | sed 's/^/  /'
)
