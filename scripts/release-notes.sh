#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

current_version=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
VERSION=${NOX_RELEASE_VERSION:-$current_version}
CHANGELOG=${NOX_CHANGELOG:-CHANGELOG.md}

usage() {
    cat >&2 <<'EOF'
Usage:
  scripts/release-notes.sh
  scripts/release-notes.sh --self-test

Print the CHANGELOG section for NOX_RELEASE_VERSION as GitHub Release notes.
This script is read-only: it does not edit files, tag, push, create a release,
or call gh.
EOF
}

extract_notes() {
    version=$1
    changelog=$2
    VERSION=$version CHANGELOG=$changelog python3 - <<'PY'
import os
import re
import sys
from pathlib import Path

version = os.environ["VERSION"]
path = Path(os.environ["CHANGELOG"])
text = path.read_text(encoding="utf-8")
pattern = re.compile(
    r"^## \[" + re.escape(version) + r"\][^\n]*\n(?P<body>.*?)(?=^## \[|\Z)",
    re.M | re.S,
)
match = pattern.search(text)
if not match:
    raise SystemExit(f"release notes: missing CHANGELOG section [{version}]")
body = match.group("body").strip()
if not body:
    raise SystemExit(f"release notes: empty CHANGELOG section [{version}]")
print(body)
PY
}

if [ "${1:-}" = "--self-test" ]; then
    tmp=$(mktemp "${TMPDIR:-/tmp}/nox-release-notes.XXXXXX")
    trap 'rm -f "$tmp"' EXIT HUP INT TERM
    cat > "$tmp" <<'EOF'
# 更新日志

## [未发布]

- next

## [0.0.5] — 2026-05-24

### Added

- release note

## [0.0.4] — 2026-05-20

- old
EOF
    notes=$(extract_notes 0.0.5 "$tmp")
    case "$notes" in
        *"### Added"*"- release note"*) ;;
        *)
            printf 'release notes: self-test unexpected notes:\n%s\n' "$notes" >&2
            exit 1
            ;;
    esac
    ! extract_notes 0.0.6 "$tmp" >/dev/null 2>/dev/null || {
        printf 'release notes: self-test accepted missing version\n' >&2
        exit 1
    }
    printf 'release notes: self-test ok\n'
    exit 0
fi

[ $# -eq 0 ] || {
    usage
    exit 2
}

extract_notes "$VERSION" "$CHANGELOG"
