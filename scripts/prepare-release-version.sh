#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

usage() {
    cat >&2 <<'EOF'
Usage:
  scripts/prepare-release-version.sh <version> <yyyy-mm-dd>
  scripts/prepare-release-version.sh --check-only <version> <yyyy-mm-dd>
  scripts/prepare-release-version.sh --self-test

Prepare the local release-prep diff for a production release.
This updates version identity files only. It does not commit, tag, push, build
assets, upload a GitHub Release, or run release gates.

With --check-only, verify that the planned release-prep anchors exist without
editing files or running cargo build.

Example:
  scripts/prepare-release-version.sh 0.0.5 2026-05-24
  scripts/prepare-release-version.sh --check-only 0.0.5 2026-05-24
EOF
}

fail() {
    printf 'prepare release version: %s\n' "$*" >&2
    exit 1
}

is_release_version() {
    printf '%s\n' "$1" | grep -Eq '^0\.0\.(0|[1-9][0-9]*)$'
}

is_iso_date() {
    printf '%s\n' "$1" | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2}$'
}

if [ "${1:-}" = "--self-test" ]; then
    is_release_version 0.0.5 || fail "self-test rejected valid version"
    ! is_release_version 1.0.0 || fail "self-test accepted non-0.0.x version"
    ! is_release_version 0.0.5-dev || fail "self-test accepted suffixed version"
    ! is_release_version 0.0.05 || fail "self-test accepted leading-zero patch"
    is_iso_date 2026-05-24 || fail "self-test rejected valid date"
    ! is_iso_date 2026-5-24 || fail "self-test accepted non-padded date"
    ! is_iso_date 2026-05-24x || fail "self-test accepted suffixed date"
    printf 'prepare release version: self-test ok\n'
    exit 0
fi

MODE=apply
if [ "${1:-}" = "--check-only" ]; then
    MODE=check
    shift
fi

[ $# -eq 2 ] || {
    usage
    exit 2
}

VERSION=$1
DATE=$2
TAG="v$VERSION"

is_release_version "$VERSION" || fail "expected a 0.0.x release version, got $VERSION"
is_iso_date "$DATE" || fail "expected date as yyyy-mm-dd, got $DATE"

CURRENT=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
[ -n "$CURRENT" ] || fail "could not read workspace version"

[ "$CURRENT" != "$VERSION" ] || fail "workspace version is already $VERSION"
grep -Fq '## [未发布]' CHANGELOG.md || fail "CHANGELOG.md is missing [未发布]"

if [ "$MODE" = "apply" ] && git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    fail "tag $TAG already exists"
fi

current_tag="v$CURRENT"

replace_fixed() {
    file=$1
    from=$2
    to=$3
    if ! grep -Fq "$from" "$file"; then
        fail "expected text not found in $file: $from"
    fi
    [ "$MODE" = "check" ] && return 0
    FROM=$from TO=$to perl -0pi -e 's/\Q$ENV{FROM}\E/$ENV{TO}/g' "$file"
}

replace_fixed Cargo.toml "version = \"$CURRENT\"" "version = \"$VERSION\""
replace_fixed crates/nox/Cargo.toml \
    "nox_core = { version = \"$CURRENT\", path = \"../nox_core\" }" \
    "nox_core = { version = \"$VERSION\", path = \"../nox_core\" }"

if [ "$MODE" != "check" ]; then
    VERSION=$VERSION DATE=$DATE perl -0pi -e '
        my $version = $ENV{VERSION};
        my $date = $ENV{DATE};
        s/^## \[未发布\]\n/## [未发布]\n\n## [$version] — $date\n/m
            or die "CHANGELOG.md: missing [未发布] heading\n";
    ' CHANGELOG.md
fi

replace_fixed README.md "latest production release is \`$current_tag\`" "latest production release is \`$TAG\`"
replace_fixed README_zh_CN.md "最新正式发布版本是 \`$current_tag\`" "最新正式发布版本是 \`$TAG\`"
replace_fixed docs/en/README.md "current production release is \`$current_tag\`" "current production release is \`$TAG\`"
replace_fixed docs/en/release-checklist.md "The latest production release is \`$current_tag\`." "The latest production release is \`$TAG\`."
zh_current_identity=$(printf '当前 checkout 的 Cargo workspace 版本、`nox --version` 和 CHANGELOG 最新已发布节均为\n`%s`。' "$CURRENT")
zh_next_identity=$(printf '当前 checkout 的 Cargo workspace 版本、`nox --version` 和 CHANGELOG 最新已发布节均为\n`%s`。' "$VERSION")
replace_fixed docs/zh_CN/release-checklist.md "$zh_current_identity" "$zh_next_identity"

replace_fixed README.md "$current_tag/" "$TAG/"
replace_fixed README.md "$current_tag " "$TAG "
replace_fixed README.md "nox-cli-$current_tag-" "nox-cli-$TAG-"
replace_fixed README.md "nox-embed-$current_tag-" "nox-embed-$TAG-"

replace_fixed README_zh_CN.md "$current_tag/" "$TAG/"
replace_fixed README_zh_CN.md "$current_tag " "$TAG "
replace_fixed README_zh_CN.md "nox-cli-$current_tag-" "nox-cli-$TAG-"
replace_fixed README_zh_CN.md "nox-embed-$current_tag-" "nox-embed-$TAG-"

if [ "$MODE" = "check" ]; then
    printf 'prepare release version: check-only ok for %s -> %s (%s)\n' "$CURRENT" "$VERSION" "$DATE"
    exit 0
fi

cargo build >/dev/null

printf 'prepare release version: updated %s -> %s (%s)\n' "$CURRENT" "$VERSION" "$DATE"
printf 'prepare release version: review diff, then run release gate before committing\n'
