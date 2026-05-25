#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

current_version=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
VERSION=${NOX_RELEASE_VERSION:-$current_version}
TAG=${NOX_RELEASE_TAG:-"v$VERSION"}

usage() {
    cat >&2 <<'EOF'
Usage:
  scripts/release-asset-manifest.sh
  scripts/release-asset-manifest.sh --json
  scripts/release-asset-manifest.sh --self-test

Print the required Phase 77 release asset base names, without extensions.
This is the shared source for release cutover status and strict cutover checks.
EOF
}

json_escape() {
    python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'
}

emit_manifest() {
    tag=$1
    printf 'nox-cli-%s-x86_64-unknown-linux-gnu\n' "$tag"
    printf 'nox-embed-%s-x86_64-unknown-linux-gnu\n' "$tag"
    printf 'nox-cli-%s-x86_64-unknown-linux-musl\n' "$tag"
}

asset_rows() {
    tag=$1
    printf 'nox-cli-%s-x86_64-unknown-linux-gnu|cli|x86_64-unknown-linux-gnu|full-sdk|false\n' "$tag"
    printf 'nox-embed-%s-x86_64-unknown-linux-gnu|embed|x86_64-unknown-linux-gnu|full-sdk|true\n' "$tag"
    printf 'nox-cli-%s-x86_64-unknown-linux-musl|cli|x86_64-unknown-linux-musl|cli-only|false\n' "$tag"
}

emit_json_manifest() {
    tag=$1
    printf '{'
    printf '"schema":"nox.release-asset-manifest.v1",'
    printf '"version":%s,' "$(printf '%s' "$VERSION" | json_escape)"
    printf '"tag":%s,' "$(printf '%s' "$tag" | json_escape)"
    printf '"assets":['
    first=1
    asset_rows "$tag" | while IFS='|' read -r name kind target commitment c_abi_smoke_required; do
        if [ "$first" -eq 1 ]; then
            first=0
        else
            printf ','
        fi
        printf '{"name":%s,"kind":%s,"target":%s,"commitment":%s,"c_abi_smoke_required":%s}' \
            "$(printf '%s' "$name" | json_escape)" \
            "$(printf '%s' "$kind" | json_escape)" \
            "$(printf '%s' "$target" | json_escape)" \
            "$(printf '%s' "$commitment" | json_escape)" \
            "$c_abi_smoke_required"
    done
    printf ']}'
    printf '\n'
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
    json=$(NOX_RELEASE_VERSION=0.0.5 NOX_RELEASE_TAG=v0.0.5 "$0" --json)
    printf '%s' "$json" | python3 -c 'import json,sys
data=json.load(sys.stdin)
assert data["schema"] == "nox.release-asset-manifest.v1"
assert data["version"] == "0.0.5"
assert data["tag"] == "v0.0.5"
assets=data["assets"]
assert [asset["name"] for asset in assets] == [
    "nox-cli-v0.0.5-x86_64-unknown-linux-gnu",
    "nox-embed-v0.0.5-x86_64-unknown-linux-gnu",
    "nox-cli-v0.0.5-x86_64-unknown-linux-musl",
]
gnu_cli, gnu_embed, musl_cli = assets
assert gnu_cli["kind"] == "cli"
assert gnu_cli["target"] == "x86_64-unknown-linux-gnu"
assert gnu_cli["commitment"] == "full-sdk"
assert gnu_cli["c_abi_smoke_required"] is False
assert gnu_embed["kind"] == "embed"
assert gnu_embed["target"] == "x86_64-unknown-linux-gnu"
assert gnu_embed["commitment"] == "full-sdk"
assert gnu_embed["c_abi_smoke_required"] is True
assert musl_cli["kind"] == "cli"
assert musl_cli["target"] == "x86_64-unknown-linux-musl"
assert musl_cli["commitment"] == "cli-only"
assert musl_cli["c_abi_smoke_required"] is False
'
    printf 'release asset manifest: self-test ok\n'
    exit 0
fi

case "${1:-}" in
    "") emit_manifest "$TAG" ;;
    --json) emit_json_manifest "$TAG" ;;
    -h|--help) usage ;;
    *) usage; exit 2 ;;
esac
