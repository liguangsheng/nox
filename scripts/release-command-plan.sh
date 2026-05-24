#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

VERSION=${NOX_RELEASE_VERSION:-0.0.5}
DATE=${NOX_RELEASE_DATE:-2026-05-24}
TAG=${NOX_RELEASE_TAG:-"v$VERSION"}
DIST=${NOX_RELEASE_ASSET_DIR:-"/tmp/nox-release-assets-$TAG"}
CI_EVIDENCE=${NOX_RELEASE_CI_EVIDENCE:-"<run-url-after-ci-passes>"}

usage() {
    cat >&2 <<'EOF'
Usage:
  scripts/release-command-plan.sh
  scripts/release-command-plan.sh --self-test

Print the ordered Phase 77 release cutover commands.
This script is read-only: it does not edit files, commit, tag, push, build
assets, create a GitHub Release, or upload files.
EOF
}

shell_quote() {
    printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

validate_inputs() {
    case "$VERSION" in
        0.0.[1-9]*)
            case "$VERSION" in
                *[!0-9.]*|0.0.0|0.0.0*) return 1 ;;
                *) ;;
            esac
            ;;
        *) return 1 ;;
    esac
    case "$DATE" in
        [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]) ;;
        *) return 1 ;;
    esac
}

emit_plan() {
    validate_inputs || {
        printf 'release command plan: invalid NOX_RELEASE_VERSION or NOX_RELEASE_DATE\n' >&2
        return 2
    }

    printf '# Phase 77 release cutover command plan\n'
    printf '# Read-only output; review and run manually only after release authorization.\n'
    printf '# version=%s tag=%s date=%s asset_dir=%s\n\n' "$VERSION" "$TAG" "$DATE" "$DIST"

    printf '1. Preflight current checkpoint\n'
    printf 'git status -sb\n'
    printf 'scripts/prepare-release-version.sh --check-only '
    shell_quote "$VERSION"
    printf ' '
    shell_quote "$DATE"
    printf '\n'
    printf 'scripts/release-toolchain-status.sh\n'
    printf 'scripts/release-cutover-status.sh\n\n'

    printf '2. Prepare the release commit locally\n'
    printf 'scripts/prepare-release-version.sh '
    shell_quote "$VERSION"
    printf ' '
    shell_quote "$DATE"
    printf '\n'
    printf 'scripts/release-gate.sh\n'
    printf 'scripts/local-dist-smoke.sh\n'
    printf 'git status -sb\n'
    printf 'git commit -m '
    shell_quote "release $TAG"
    printf '\n\n'

    printf '3. Tag, push, and wait for CI\n'
    printf 'git tag '
    shell_quote "$TAG"
    printf '\n'
    printf 'git push origin HEAD\n'
    printf 'git push origin '
    shell_quote "$TAG"
    printf '\n'
    printf '# Wait for the GitHub Actions run for %s to pass, then set NOX_RELEASE_CI_EVIDENCE.\n\n' "$TAG"

    printf '4. Build and inspect release assets\n'
    printf 'NOX_RELEASE_TAG='
    shell_quote "$TAG"
    printf ' NOX_RELEASE_ASSET_DIR='
    shell_quote "$DIST"
    printf ' CLI_ONLY_TARGET_TRIPLES='
    shell_quote "x86_64-unknown-linux-musl"
    printf ' scripts/build-release-assets.sh\n'
    printf 'NOX_RELEASE_VERSION='
    shell_quote "$VERSION"
    printf ' scripts/release-notes.sh\n'
    printf 'NOX_RELEASE_TAG='
    shell_quote "$TAG"
    printf ' NOX_RELEASE_ASSET_DIR='
    shell_quote "$DIST"
    printf ' scripts/release-upload-plan.sh\n\n'

    printf '5. Upload release assets and run strict cutover checks\n'
    printf '# Use the upload command printed by scripts/release-upload-plan.sh after creating the GitHub Release.\n'
    printf 'NOX_RELEASE_CI_EVIDENCE='
    shell_quote "$CI_EVIDENCE"
    printf ' NOX_RELEASE_ASSET_DIR='
    shell_quote "$DIST"
    printf ' scripts/release-cutover-check.sh\n'
    printf 'NOX_RELEASE_CI_EVIDENCE='
    shell_quote "$CI_EVIDENCE"
    printf ' scripts/release-audit.sh\n'
}

if [ "${1:-}" = "--self-test" ]; then
    output=$(NOX_RELEASE_VERSION=0.0.5 NOX_RELEASE_DATE=2026-05-24 NOX_RELEASE_CI_EVIDENCE=https://github.com/example/actions/runs/1 "$0")
    case "$output" in
*"scripts/prepare-release-version.sh --check-only '0.0.5' '2026-05-24'"*\
*"scripts/release-toolchain-status.sh"*\
*"git commit -m 'release v0.0.5'"*\
*"CLI_ONLY_TARGET_TRIPLES='x86_64-unknown-linux-musl' scripts/build-release-assets.sh"*\
*"scripts/release-cutover-check.sh"*\
*"scripts/release-audit.sh"*) ;;
        *)
            printf 'release command plan: self-test unexpected output:\n%s\n' "$output" >&2
            exit 1
            ;;
    esac
    ! NOX_RELEASE_VERSION=1.0.0 "$0" >/dev/null 2>/dev/null || {
        printf 'release command plan: self-test accepted invalid version\n' >&2
        exit 1
    }
    printf 'release command plan: self-test ok\n'
    exit 0
fi

[ $# -eq 0 ] || {
    usage
    exit 2
}

emit_plan
