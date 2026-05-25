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
    mode=candidate
fi
workspace_version_re=$(printf '%s' "$workspace_version" | sed 's/\./\\./g')
patch=${workspace_version##*.}
next_patch=$((patch + 1))
next_version="0.0.$next_patch"

case "$mode" in
    candidate)
        require_file_contains_fixed crates/nox/Cargo.toml "nox_core = { version = \"$workspace_version\", path = \"../nox_core\" }" "exact nox_core dependency"

        require_file_contains README.md "latest production release is \`v$workspace_version_re\`" "English latest production release wording"
        require_file_contains README_zh_CN.md "最新正式发布版本是 \`v$workspace_version_re\`" "Chinese latest production release wording"
        require_file_contains docs/en/README.md "current production release is \`v$workspace_version_re\`" "English docs production release wording"
        require_file_contains docs/zh_CN/release-checklist.md '下一轮候选版本从下一个 patch 版本开始' "next release candidate version"
        require_file_contains docs/zh_CN/release-checklist.md '`\[workspace\.package\]\.version` 仍是上一个已准备发布版本' "pre-release version identity rule"
        require_file_contains docs/en/release-checklist.md 'next release candidate starts at' "English next release candidate wording"
        require_file_contains docs/en/release-checklist.md 'next patch version' "English next release candidate version"
        require_file_contains docs/en/release-checklist.md 'keep `\[workspace\.package\]\.version` at' "English pre-release version identity wording"
        require_file_contains docs/en/release-checklist.md 'the previous prepared version' "English pre-release version identity rule"

        require_file_contains CHANGELOG.md '^## \[未发布\]' "unreleased changelog section"
        printf 'release candidate readiness: candidate mode for v%s -> v%s\n' "$workspace_version" "$next_version"
        ;;
    cutover)
        expected_version=${NOX_RELEASE_CUTOVER_VERSION:-$workspace_version}
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
require_file_contains CHANGELOG.md 'source_map.*source_map_hash' "LSP generated source-map metadata changelog entry"
require_file_contains CHANGELOG.md 'nox\.release-asset-manifest\.v1' "release asset manifest JSON changelog entry"
require_file_contains CHANGELOG.md 'Release Asset Manifest JSON' "release evidence report manifest JSON changelog entry"
require_file_contains CHANGELOG.md '稳定性与兼容承诺矩阵' "stability matrix changelog entry"

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
require_file_contains docs/en/runtime.md '`std/yaml\.nox` provides an experimental minimum reader' "English YAML experimental boundary"
require_file_contains docs/zh_CN/runtime.md '`std/yaml\.nox` \| `parse\(source: str\) -> result\[json, str\]` \| 无 \| 实验性最小 YAML reader' "Chinese YAML experimental boundary"
require_file_contains docs/en/runtime.md 'Compression/archive formats, protobuf, SQLite/database drivers, and HTTPS/TLS' "English data-capability deferral"
require_file_contains docs/zh_CN/runtime.md '压缩/归档格式、protobuf、SQLite/database driver 和 HTTPS/TLS 仍暂缓' "Chinese data-capability deferral"
require_file_contains docs/zh_CN/runtime.md 'result / option 错误模型与 try block 暂缓' "Chinese try-block deferral"
require_file_contains docs/zh_CN/decisions/0029-defer-macro-system.md 'Nox 暂缓内建宏系统' "macro deferral ADR"
require_file_contains docs/zh_CN/decisions/0030-staged-async-await.md '无 IO reactor' "async staged ADR"
require_file_contains docs/zh_CN/decisions/0031-integrated-lsp-fourth-round.md 'source_map.*source_map_hash' "LSP source-map metadata ADR"
require_file_contains docs/zh_CN/decisions/0031-integrated-lsp-fourth-round.md '不开放跨文件 rename' "LSP no cross-file rename ADR"
require_file_contains docs/zh_CN/decisions/0033-platform-distribution-third-round.md 'full SDK target 仍只有 `x86_64-unknown-linux-gnu`' "platform full SDK ADR"
require_file_contains docs/zh_CN/decisions/0033-platform-distribution-third-round.md 'CLI-only target 仍只有 `x86_64-unknown-linux-musl`' "platform CLI-only ADR"
require_file_contains docs/zh_CN/decisions/0034-production-evidence-third-round.md 'nox\.release-asset-manifest\.v1' "production evidence asset manifest ADR"

require_file_contains scripts/build-release-assets.sh 'CLI_ONLY_TARGET_TRIPLES' "CLI-only release asset support"
require_file_contains scripts/cross-cli-smoke.sh 'x86_64-unknown-linux-musl' "musl cross CLI smoke target"
require_file_contains scripts/release-asset-smoke.sh 'release asset smoke' "release asset smoke script"
require_file_contains scripts/release-asset-manifest.sh 'nox\.release-asset-manifest\.v1' "release asset manifest JSON schema"
require_file_contains scripts/release-asset-manifest.sh 'c_abi_smoke_required' "release asset manifest C ABI smoke field"
require_file_contains scripts/release-evidence-report.sh 'Release Asset Manifest JSON' "release evidence report asset manifest section"
require_file_contains scripts/release-gate.sh 'release asset smoke self-test' "release asset smoke release gate"
require_file_contains docs/en/release-checklist.md 'x86_64-unknown-linux-musl.*CLI-only' "English musl CLI-only release checklist"
require_file_contains docs/zh_CN/release-checklist.md 'x86_64-unknown-linux-musl.*CLI-only' "Chinese musl CLI-only release checklist"
require_file_contains docs/en/release-checklist.md 'c_abi_smoke_required' "English release asset manifest JSON checklist"
require_file_contains docs/zh_CN/release-checklist.md 'c_abi_smoke_required' "Chinese release asset manifest JSON checklist"
require_file_contains docs/en/release-checklist.md 'release-asset-smoke\.sh' "English release asset smoke checklist"
require_file_contains docs/zh_CN/release-checklist.md 'release-asset-smoke\.sh' "Chinese release asset smoke checklist"
require_file_contains docs/en/release-checklist.md 'withdrawn|deprecated|hotfix' "English rollback terms"
require_file_contains docs/zh_CN/release-checklist.md '撤回|hotfix|下游升级路径' "Chinese rollback terms"
require_file_contains docs/en/release-checklist.md 'Publishing to crates.io is deferred' "English crates.io deferral"
require_file_contains docs/zh_CN/release-checklist.md '暂缓 crates.io 发布' "Chinese crates.io deferral"
require_file_contains docs/en/README.md 'support-policy\.md' "English support policy index link"
require_file_contains docs/zh_CN/README.md 'support-policy\.md' "Chinese support policy index link"
require_file_contains docs/en/support-policy.md 'Supported Versions' "English supported versions policy"
require_file_contains docs/zh_CN/support-policy.md '支持版本' "Chinese supported versions policy"
require_file_contains docs/en/support-policy.md 'Security Response' "English security response policy"
require_file_contains docs/zh_CN/support-policy.md '漏洞响应' "Chinese security response policy"
require_file_contains docs/en/support-policy.md 'EOL' "English EOL policy"
require_file_contains docs/zh_CN/support-policy.md 'EOL' "Chinese EOL policy"
require_file_contains docs/en/support-policy.md 'Hotfix' "English hotfix policy"
require_file_contains docs/zh_CN/support-policy.md 'Hotfix' "Chinese hotfix policy"
require_file_contains docs/en/support-policy.md 'Withdrawn Releases' "English withdrawn release policy"
require_file_contains docs/zh_CN/support-policy.md '撤回 Release' "Chinese withdrawn release policy"
require_file_contains docs/en/README.md 'migration-v0\.0\.6-to-v0\.0\.7\.md' "English migration guide index link"
require_file_contains docs/zh_CN/README.md 'migration-v0\.0\.6-to-v0\.0\.7\.md' "Chinese migration guide index link"
require_file_contains docs/en/migration-v0.0.6-to-v0.0.7.md 'nox\.lock' "English migration lockfile guidance"
require_file_contains docs/zh_CN/migration-v0.0.6-to-v0.0.7.md 'nox\.lock' "Chinese migration lockfile guidance"
require_file_contains .github/workflows/ci.yml 'Platform CLI smoke' "platform CLI smoke CI job"
require_file_contains .github/workflows/ci.yml 'macos-latest' "macOS CLI smoke runner"
require_file_contains .github/workflows/ci.yml 'windows-latest' "Windows CLI smoke runner"
require_file_contains scripts/cli-smoke.sh 'examples/hello\.nox' "shared CLI smoke script"
require_file_contains docs/en/release-checklist.md 'Platform CLI smoke' "English platform CLI smoke checklist"
require_file_contains docs/zh_CN/release-checklist.md 'Platform CLI smoke' "Chinese platform CLI smoke checklist"
require_file_contains docs/zh_CN/decisions/0033-platform-distribution-third-round.md '阶段 131 复审' "platform distribution stage 131 review"
require_file_contains docs/zh_CN/decisions/0026-github-git-module-ecosystem.md '阶段 133-135 复审' "package stage 133-135 review"

require_file_contains scripts/release-gate.sh 'compatibility regression: machine-readable golden surfaces' "compatibility golden release gate"
require_file_contains scripts/release-audit.sh 'PLAN 第 75 项: compatibility golden regression bus wired' "compatibility golden release audit"
require_file_contains scripts/release-gate.sh 'stability and support policy guardrail' "stability/support release gate"
require_file_contains scripts/release-audit.sh 'stability and support policy guardrail wired' "stability/support release audit"
require_file_contains scripts/stability-guardrail.sh 'support-policy\.md' "stability/support guardrail script"
require_file_contains scripts/release-gate.sh 'package and lockfile guardrail' "package guardrail release gate"
require_file_contains scripts/release-audit.sh 'package and lockfile guardrail wired' "package guardrail release audit"
require_file_contains scripts/package-guardrail.sh 'NOX_MODULE_CACHE' "package guardrail cache boundary"

standalone_lsp_hits=/tmp/nox-standalone-lsp-hits.$$
if rg -n 'nox[-_]lsp|nox_language_server|language[-_]server.*package|lsp.*standalone' \
    Cargo.toml crates/*/Cargo.toml scripts/build-release-assets.sh .github/workflows/ci.yml \
    >"$standalone_lsp_hits"; then
    fail "standalone LSP binary/package marker found; see $standalone_lsp_hits"
else
    rm -f "$standalone_lsp_hits"
fi

printf 'release candidate readiness: ok\n'
