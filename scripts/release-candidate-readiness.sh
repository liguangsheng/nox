#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

fail() {
    printf 'release candidate readiness: %s\n' "$*" >&2
    exit 1
}

require_file_contains() {
    file=$1
    pattern=$2
    description=$3
    if ! grep -Eq "$pattern" "$file"; then
        fail "$description missing from $file"
    fi
}

require_file_contains_fixed() {
    file=$1
    text=$2
    description=$3
    if ! grep -Fq "$text" "$file"; then
        fail "$description missing from $file"
    fi
}

workspace_version=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
mode=${NOX_RELEASE_READINESS_MODE:-auto}
if [ "$mode" = "auto" ]; then
    if [ "$workspace_version" = "0.0.4" ]; then
        mode=candidate
    else
        mode=cutover
    fi
fi

case "$mode" in
    candidate)
        [ "$workspace_version" = "0.0.4" ] || fail "workspace version must stay 0.0.4 before release-prep commit, got $workspace_version"
        require_file_contains_fixed crates/nox/Cargo.toml 'nox_core = { version = "0.0.4", path = "../nox_core" }' "exact nox_core dependency"

        require_file_contains README.md 'latest production release is `v0\.0\.4`' "English latest production release wording"
        require_file_contains README_zh_CN.md '最新正式发布版本是 `v0\.0\.4`' "Chinese latest production release wording"
        require_file_contains docs/en/README.md 'current production release is `v0\.0\.4`' "English docs production release wording"
        require_file_contains docs/zh_CN/release-checklist.md '下一轮候选版本从 `v0\.0\.5` 开始' "next release candidate version"
        require_file_contains docs/zh_CN/release-checklist.md '`\[workspace\.package\]\.version` 仍是上一个已准备发布版本' "pre-release version identity rule"
        require_file_contains docs/en/release-checklist.md 'next release candidate starts at' "English next release candidate wording"
        require_file_contains docs/en/release-checklist.md '`v0\.0\.5`, but candidate audits' "English next release candidate version"
        require_file_contains docs/en/release-checklist.md 'keep `\[workspace\.package\]\.version` at' "English pre-release version identity wording"
        require_file_contains docs/en/release-checklist.md 'the previous prepared version' "English pre-release version identity rule"

        require_file_contains CHANGELOG.md '^## \[未发布\]' "unreleased changelog section"
        ;;
    cutover)
        expected_version=${NOX_RELEASE_CUTOVER_VERSION:-0.0.5}
        [ "$workspace_version" = "$expected_version" ] || fail "cutover workspace version must be $expected_version, got $workspace_version"
        version_re=$(printf '%s' "$workspace_version" | sed 's/\./\\./g')
        require_file_contains_fixed crates/nox/Cargo.toml "nox_core = { version = \"$workspace_version\", path = \"../nox_core\" }" "exact nox_core dependency"
        require_file_contains README.md "latest production release is \`v$version_re\`" "English latest production release wording"
        require_file_contains README_zh_CN.md "最新正式发布版本是 \`v$version_re\`" "Chinese latest production release wording"
        require_file_contains docs/en/README.md "current production release is \`v$version_re\`" "English docs production release wording"
        require_file_contains CHANGELOG.md "^## \\[$version_re\\]" "release changelog section"
        ;;
    *)
        fail "unknown NOX_RELEASE_READINESS_MODE '$mode' (expected auto, candidate, or cutover)"
        ;;
esac

require_file_contains CHANGELOG.md 'scripts/cross-cli-smoke\.sh' "cross CLI smoke changelog entry"
require_file_contains CHANGELOG.md 'CLI_ONLY_TARGET_TRIPLES' "CLI-only target changelog entry"
require_file_contains CHANGELOG.md '暂缓 crates\.io 发布' "crates.io deferral changelog entry"
require_file_contains CHANGELOG.md 'scripts/compatibility-golden\.sh' "compatibility golden changelog entry"
require_file_contains CHANGELOG.md '静态 trait MVP' "trait MVP changelog entry"
require_file_contains CHANGELOG.md '不提供 IO reactor' "async staged-boundary changelog entry"
require_file_contains CHANGELOG.md '暂缓内建宏系统' "macro deferral changelog entry"
require_file_contains CHANGELOG.md '不引入 `throw` / `catch` / `finally`' "exception model deferral changelog entry"

require_file_contains docs/en/language-v0.md 'experimental static trait MVP' "English trait experimental status"
require_file_contains docs/zh_CN/language-v0.md '当前 trait 能力是实验性的纯静态 MVP' "Chinese trait experimental status"
require_file_contains docs/en/runtime.md 'experimental `Eq` trait' "English Eq experimental status"
require_file_contains docs/zh_CN/runtime.md '实验性 `Eq` trait' "Chinese Eq experimental status"
require_file_contains docs/en/stdlib-index.md '\| Collections \| `std/array\.nox` \| experimental \| Eq trait / dedupe_equal / contains_equal' "English stdlib Eq experimental index row"
require_file_contains docs/zh_CN/stdlib-index.md '\| 集合 \| `std/array\.nox` \| experimental \| Eq trait / dedupe_equal / contains_equal' "Chinese stdlib Eq experimental index row"
require_file_contains docs/en/diagnostics.md 'experimental static trait MVP' "English trait diagnostic experimental status"
require_file_contains docs/zh_CN/diagnostics.md 'trait\.not-found` \| 实验' "Chinese trait diagnostic experimental status"

require_file_contains docs/en/language-v0.md 'does not expose `throw` / `catch` / `finally` exceptions' "English exception deferral"
require_file_contains docs/zh_CN/language-v0.md '不提供用户可见的 `throw` / `catch` / `finally`' "Chinese exception deferral"
require_file_contains docs/en/runtime.md 'YAML remains deferred' "English YAML deferral"
require_file_contains docs/zh_CN/runtime.md 'result / option 错误模型与 try block 暂缓' "Chinese try-block deferral"
require_file_contains docs/zh_CN/decisions/0029-defer-macro-system.md 'Nox 暂缓内建宏系统' "macro deferral ADR"
require_file_contains docs/zh_CN/decisions/0030-staged-async-await.md '无 IO reactor' "async staged ADR"

require_file_contains scripts/build-release-assets.sh 'CLI_ONLY_TARGET_TRIPLES' "CLI-only release asset support"
require_file_contains scripts/cross-cli-smoke.sh 'x86_64-unknown-linux-musl' "musl cross CLI smoke target"
require_file_contains docs/en/release-checklist.md 'x86_64-unknown-linux-musl.*CLI-only' "English musl CLI-only release checklist"
require_file_contains docs/zh_CN/release-checklist.md 'x86_64-unknown-linux-musl.*CLI-only' "Chinese musl CLI-only release checklist"
require_file_contains docs/en/release-checklist.md 'withdrawn|deprecated|hotfix' "English rollback terms"
require_file_contains docs/zh_CN/release-checklist.md '撤回|hotfix|下游升级路径' "Chinese rollback terms"
require_file_contains docs/en/release-checklist.md 'Publishing to crates.io is deferred' "English crates.io deferral"
require_file_contains docs/zh_CN/release-checklist.md '暂缓 crates.io 发布' "Chinese crates.io deferral"

require_file_contains scripts/release-gate.sh 'compatibility regression: machine-readable golden surfaces' "compatibility golden release gate"
require_file_contains scripts/release-audit.sh 'PLAN 第 75 项: compatibility golden regression bus wired' "compatibility golden release audit"

standalone_lsp_hits=/tmp/nox-standalone-lsp-hits.$$
if rg -n 'nox[-_]lsp|nox_language_server|language[-_]server.*package|lsp.*standalone' \
    Cargo.toml crates/*/Cargo.toml scripts/build-release-assets.sh .github/workflows/ci.yml \
    >"$standalone_lsp_hits"; then
    fail "standalone LSP binary/package marker found; see $standalone_lsp_hits"
else
    rm -f "$standalone_lsp_hits"
fi

printf 'release candidate readiness: ok\n'
