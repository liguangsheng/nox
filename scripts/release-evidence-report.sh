#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

current_version=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
VERSION=${NOX_RELEASE_VERSION:-$current_version}
TAG=${NOX_RELEASE_TAG:-"v$VERSION"}
DIST=${NOX_RELEASE_ASSET_DIR:-"/tmp/nox-release-assets-$TAG"}
CI_EVIDENCE=${NOX_RELEASE_CI_EVIDENCE:-}

usage() {
    cat >&2 <<'EOF'
Usage:
  scripts/release-evidence-report.sh
  scripts/release-evidence-report.sh --self-test

Print a read-only Phase 77 release evidence report.
This script does not edit files, commit, tag, push, build assets, create a
GitHub Release, or upload files.
EOF
}

emit_report() {
    cargo_version=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
    head_commit=$(git rev-parse HEAD)
    if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
        tag_commit=$(git rev-list -n 1 "$TAG")
    else
        tag_commit="<missing>"
    fi
    if git diff --quiet -- . ':!target'; then
        worktree_state="clean"
    else
        worktree_state="dirty"
    fi
    status_json=$(scripts/release-cutover-status.sh --json 2>/dev/null || true)

    printf '# Nox Phase 77 Release Evidence\n\n'
    printf '%s\n' "- version: \`$VERSION\`"
    printf '%s\n' "- tag: \`$TAG\`"
    printf '%s\n' "- cargo_version: \`$cargo_version\`"
    printf '%s\n' "- head: \`$head_commit\`"
    printf '%s\n' "- tag_commit: \`$tag_commit\`"
    printf '%s\n' "- worktree: \`$worktree_state\`"
    printf '%s\n' "- asset_dir: \`$DIST\`"
    if [ -n "$CI_EVIDENCE" ]; then
        printf '%s\n' "- ci_evidence: \`$CI_EVIDENCE\`"
    else
        printf '%s\n' '- ci_evidence: `<missing>`'
    fi

    printf '\n## Cutover Status JSON\n\n'
    printf '```json\n%s\n```\n' "$status_json"

    printf '\n## Toolchain Status JSON\n\n'
    printf '```json\n'
    NOX_RELEASE_VERSION="$VERSION" \
        NOX_RELEASE_TAG="$TAG" \
        scripts/release-toolchain-status.sh --json
    printf '```\n'

    printf '\n## Release Asset Manifest JSON\n\n'
    printf '```json\n'
    NOX_RELEASE_VERSION="$VERSION" \
        NOX_RELEASE_TAG="$TAG" \
        scripts/release-asset-manifest.sh --json
    printf '```\n'

    printf '\n## Required Assets\n\n'
    while IFS= read -r asset; do
        printf '%s\n' "- \`$asset.tar.gz\` + \`$asset.sha256\`"
    done <<EOF_ASSETS
$(NOX_RELEASE_TAG="$TAG" scripts/release-asset-manifest.sh)
EOF_ASSETS

    printf '\n## Command Plan\n\n'
    printf '```sh\n'
    NOX_RELEASE_VERSION="$VERSION" \
        NOX_RELEASE_TAG="$TAG" \
        NOX_RELEASE_ASSET_DIR="$DIST" \
        NOX_RELEASE_CI_EVIDENCE="${CI_EVIDENCE:-<run-url-after-ci-passes>}" \
        scripts/release-command-plan.sh
    printf '```\n'
}

if [ "${1:-}" = "--self-test" ]; then
    output=$(NOX_RELEASE_VERSION=0.0.5 "$0")
    for expected in \
        '# Nox Phase 77 Release Evidence' \
        '## Cutover Status JSON' \
        '## Toolchain Status JSON' \
        '## Release Asset Manifest JSON' \
        'nox.release-cutover-status.v1' \
        'nox.release-toolchain-status.v1' \
        'nox.release-asset-manifest.v1' \
        'nox-cli-v0.0.5-x86_64-unknown-linux-gnu.tar.gz' \
        '"commitment":"cli-only"' \
        'scripts/release-cutover-check.sh'; do
        printf '%s' "$output" | grep -Fq "$expected" || {
            printf 'release evidence report: self-test missing %s in output:\n%s\n' "$expected" "$output" >&2
            exit 1
        }
    done
    printf 'release evidence report: self-test ok\n'
    exit 0
fi

[ $# -eq 0 ] || {
    usage
    exit 2
}

emit_report
