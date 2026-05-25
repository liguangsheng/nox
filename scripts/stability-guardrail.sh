#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

fail() {
    printf 'stability guardrail: %s\n' "$*" >&2
    exit 1
}

require_file() {
    file=$1
    [ -f "$file" ] || fail "missing $file"
}

require_contains() {
    file=$1
    pattern=$2
    description=$3
    grep -Eq "$pattern" "$file" || fail "$description missing from $file"
}

for file in \
    docs/en/stability.md \
    docs/zh_CN/stability.md \
    docs/en/support-policy.md \
    docs/zh_CN/support-policy.md
do
    require_file "$file"
done

for file in docs/en/stability.md docs/zh_CN/stability.md; do
    require_contains "$file" 'Stable|稳定' "stable status tag"
    require_contains "$file" 'Experimental|实验' "experimental status tag"
    require_contains "$file" 'Deferred|暂缓' "deferred status tag"
    require_contains "$file" 'Internal|内部' "internal status tag"
    require_contains "$file" 'language-v0\.md' "language surface row"
    require_contains "$file" '[Pp]arser/typechecker/VM' "internal engine row"
    require_contains "$file" 'nox run' "CLI command row"
    require_contains "$file" 'nox\.check\.v1' "CLI JSON schema row"
    require_contains "$file" '[Cc]overage/profile/trace|coverage/profile/trace JSON' "coverage/profile/trace row"
    require_contains "$file" 'diagnostics\.md' "diagnostics row"
    require_contains "$file" 'LSP diagnostics' "LSP diagnostics row"
    require_contains "$file" 'nox_core' "Rust nox_core row"
    require_contains "$file" 'Runtime' "Rust runtime row"
    require_contains "$file" 'nox_core\.h' "C ABI row"
    require_contains "$file" 'stdlib-index\.md' "stdlib row"
    require_contains "$file" 'nox\.lock' "lockfile row"
    require_contains "$file" 'sha256|\.sha256' "release asset row"
    require_contains "$file" 'CHANGELOG\.md' "change rule"
    require_contains "$file" 'release gate|release checklist' "release audit rule"
    if grep -Eq '\.agents|\]\([^)]*(GOAL|PLAN)\.md' "$file"; then
        fail "$file must not link or mention agent handoff files"
    fi
done

for file in docs/en/support-policy.md docs/zh_CN/support-policy.md; do
    require_contains "$file" 'Supported Versions|支持版本' "supported versions section"
    require_contains "$file" 'Security Response|漏洞响应' "security response section"
    require_contains "$file" 'Hotfix' "hotfix section"
    require_contains "$file" 'Withdrawn Releases|撤回 Release' "withdrawn release section"
    require_contains "$file" 'Release Train' "release train section"
    require_contains "$file" 'latest production release|最新 production release' "latest production release support rule"
    require_contains "$file" 'security-fix-only' "security fix support window"
    require_contains "$file" 'EOL' "EOL rule"
    require_contains "$file" 'GitHub Security Advisories' "private security report channel"
    require_contains "$file" 'deny-by-default|默认拒绝' "default permission security boundary"
    require_contains "$file" 'CHANGELOG\.md' "changelog requirement"
    require_contains "$file" 'scripts/release-gate\.sh' "release gate requirement"
    require_contains "$file" 'scripts/local-dist-smoke\.sh' "local dist smoke requirement"
    require_contains "$file" 'scripts/release-audit\.sh' "release audit requirement"
    if grep -Eq '\.agents|\]\([^)]*(GOAL|PLAN)\.md' "$file"; then
        fail "$file must not link or mention agent handoff files"
    fi
done

require_contains docs/en/README.md 'stability\.md' "English stability index link"
require_contains docs/zh_CN/README.md 'stability\.md' "Chinese stability index link"
require_contains docs/en/README.md 'support-policy\.md' "English support policy index link"
require_contains docs/zh_CN/README.md 'support-policy\.md' "Chinese support policy index link"
require_contains docs/en/release-checklist.md 'support-policy\.md' "English release checklist support policy link"
require_contains docs/zh_CN/release-checklist.md 'support-policy\.md' "Chinese release checklist support policy link"
require_contains scripts/release-gate.sh 'stability and support policy guardrail' "release gate wiring"
require_contains scripts/release-audit.sh 'stability and support policy guardrail wired' "release audit wiring"
require_contains scripts/release-candidate-readiness.sh 'support-policy\.md' "readiness support policy wiring"

printf 'stability guardrail: ok\n'
