#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

fail() {
    printf 'package guardrail: %s\n' "$*" >&2
    exit 1
}

require_contains() {
    file=$1
    pattern=$2
    description=$3
    if ! grep -Eq "$pattern" "$file"; then
        fail "$description missing from $file"
    fi
}

require_contains docs/zh_CN/package-manifest-design.md '\[lock\]' 'lockfile schema section'
require_contains docs/zh_CN/package-manifest-design.md 'version = "1"' 'lockfile schema version'
require_contains docs/zh_CN/package-manifest-design.md 'content_hash.*sha256:<64 hex>' 'content hash rule'
require_contains docs/zh_CN/package-manifest-design.md 'cache_key' 'cache key rule'
require_contains docs/zh_CN/package-manifest-design.md 'tool = "nox ' 'tool provenance rule'
require_contains docs/zh_CN/package-manifest-design.md 'private repo cookbook' 'private source follow-up boundary'
require_contains docs/zh_CN/package-manifest-design.md '不做 package registry' 'registry deferral'
require_contains docs/zh_CN/package-manifest-design.md '缺少 lockfile、cache miss、cache corrupt 或' 'offline failure boundary'
require_contains docs/zh_CN/package-manifest-design.md 'hash mismatch' 'hash mismatch failure boundary'
require_contains docs/zh_CN/package-manifest-design.md '不会触发联网下载' 'no implicit network import rule'
require_contains docs/zh_CN/package-manifest-design.md 'NOX_MODULE_CACHE' 'cache override rule'
require_contains docs/zh_CN/package-manifest-design.md 'nox.project-check.v1' 'project check JSON schema'

require_contains docs/zh_CN/decisions/0026-github-git-module-ecosystem.md '不做自建 registry' 'registry ADR boundary'
require_contains docs/zh_CN/decisions/0026-github-git-module-ecosystem.md '不允许 branch/default-branch' 'floating pin rejection'
require_contains docs/zh_CN/decisions/0026-github-git-module-ecosystem.md 'private repo cookbook' 'private source ADR follow-up'
require_contains docs/zh_CN/decisions/0026-github-git-module-ecosystem.md 'cache miss' 'offline/cache diagnostics ADR'

require_contains scripts/release-gate.sh 'module dependency lockfile guardrail' 'release gate lockfile guardrail'
require_contains scripts/release-gate.sh 'module ecosystem regression: fetch offline/cache' 'fetch offline/cache gate'
require_contains scripts/release-gate.sh 'module ecosystem regression: external import/cache/hash' 'external cache/hash gate'
require_contains scripts/release-gate.sh 'package and lockfile guardrail' 'package guardrail release gate entry'

printf 'package guardrail: ok\n'
