#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

VERSION=${NOX_RELEASE_VERSION:-0.0.5}
TAG=${NOX_RELEASE_TAG:-"v$VERSION"}

usage() {
    cat >&2 <<'EOF'
Usage:
  scripts/release-toolchain-status.sh
  scripts/release-toolchain-status.sh --json
  scripts/release-toolchain-status.sh --self-test

Report the local Rust/toolchain prerequisites for the Phase 77 release asset
matrix. This script is read-only: it does not install targets, build assets,
commit, tag, push, or call GitHub.
EOF
}

json_escape() {
    python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'
}

rust_host() {
    rustc -vV | awk '/^host: /{print $2; exit}'
}

target_installed() {
    target=$1
    rustup target list --installed 2>/dev/null | grep -Fxq "$target"
}

rustup_available() {
    command -v rustup >/dev/null 2>&1
}

check_required_target() {
    target=$1
    kind=$2
    if target_installed "$target"; then
        printf '%s|%s|ok|installed\n' "$target" "$kind"
    else
        printf '%s|%s|missing|run: rustup target add %s\n' "$target" "$kind" "$target"
    fi
}

emit_rows() {
    manifest=$(scripts/release-asset-manifest.sh)
    printf '%s\n' "$manifest" | while IFS= read -r asset; do
        case "$asset" in
            *-x86_64-unknown-linux-gnu)
                target=x86_64-unknown-linux-gnu
                ;;
            *-x86_64-unknown-linux-musl)
                target=x86_64-unknown-linux-musl
                ;;
            *)
                target=unknown
                ;;
        esac
        case "$asset" in
            nox-embed-*) kind=embed ;;
            *) kind=cli ;;
        esac
        if [ "$target" = unknown ]; then
            printf '%s|%s|missing|unknown target for asset %s\n' "$target" "$kind" "$asset"
        else
            check_required_target "$target" "$kind"
        fi
    done | awk -F'|' '!seen[$1 "|" $2]++ {print}'
}

emit_text() {
    host=$(rust_host)
    printf 'release toolchain status for %s\n' "$TAG"
    printf 'host: %s\n' "$host"
    if rustup_available; then
        printf 'rustup: available\n'
    else
        printf 'rustup: missing\n'
    fi
    pending=0
    emit_rows | while IFS='|' read -r target kind state message; do
        printf '%s %s: %s (%s)\n' "$kind" "$target" "$state" "$message"
        [ "$state" = ok ] || pending=$((pending + 1))
    done
}

emit_json() {
    host=$(rust_host)
    rustup_state=missing
    rustup_available && rustup_state=available
    rows=$(mktemp "${TMPDIR:-/tmp}/nox-release-toolchain-rows.XXXXXX")
    trap 'rm -f "$rows"' EXIT HUP INT TERM
    emit_rows >"$rows"
    pending=$(awk -F'|' '$3 != "ok" {count++} END {print count + 0}' "$rows")
    ok=false
    [ "$pending" -eq 0 ] && ok=true
    printf '{'
    printf '"schema":"nox.release-toolchain-status.v1",'
    printf '"version":%s,' "$(printf '%s' "$VERSION" | json_escape)"
    printf '"tag":%s,' "$(printf '%s' "$TAG" | json_escape)"
    printf '"host":%s,' "$(printf '%s' "$host" | json_escape)"
    printf '"rustup":%s,' "$(printf '%s' "$rustup_state" | json_escape)"
    printf '"ok":%s,' "$ok"
    printf '"pending_count":%s,' "$pending"
    printf '"items":['
    first=1
    while IFS='|' read -r target kind state message; do
        [ -n "$target" ] || continue
        if [ "$first" -eq 1 ]; then
            first=0
        else
            printf ','
        fi
        printf '{"target":%s,"kind":%s,"state":%s,"message":%s}' \
            "$(printf '%s' "$target" | json_escape)" \
            "$(printf '%s' "$kind" | json_escape)" \
            "$(printf '%s' "$state" | json_escape)" \
            "$(printf '%s' "$message" | json_escape)"
    done <"$rows"
    printf ']}'
    printf '\n'
}

if [ "${1:-}" = "--self-test" ]; then
    output=$($0 --json)
    printf '%s' "$output" | python3 -c 'import json,sys
data=json.load(sys.stdin)
assert data["schema"] == "nox.release-toolchain-status.v1"
assert data["tag"].startswith("v")
assert isinstance(data["ok"], bool)
assert isinstance(data["pending_count"], int)
items=data["items"]
assert any(item["target"] == "x86_64-unknown-linux-gnu" and item["kind"] == "cli" for item in items)
assert any(item["target"] == "x86_64-unknown-linux-gnu" and item["kind"] == "embed" for item in items)
assert any(item["target"] == "x86_64-unknown-linux-musl" and item["kind"] == "cli" for item in items)
for item in items:
    assert item["state"] in {"ok", "missing"}
    assert item["message"]
'
    printf 'release toolchain status: self-test ok\n'
    exit 0
fi

case "${1:-}" in
    "") emit_text ;;
    --json) emit_json ;;
    -h|--help) usage ;;
    *) usage; exit 2 ;;
esac
