#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
NOX_BIN=${NOX_BIN:-"$ROOT/target/debug/nox"}
VERSION=${NOX_RELEASE_VERSION:-}
CI_EVIDENCE=${NOX_RELEASE_CI_EVIDENCE:-}
EXPECT_BLOCKED=${NOX_RELEASE_AUDIT_EXPECT_BLOCKED:-0}

cd "$ROOT"

failures=0

note() {
    printf 'release audit: %s\n' "$*"
}

fail() {
    failures=$((failures + 1))
    printf 'release audit: BLOCKER: %s\n' "$*" >&2
}

pass() {
    printf 'release audit: ok: %s\n' "$*"
}

if [ -z "$VERSION" ]; then
    VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys
for package in json.load(sys.stdin)["packages"]:
    if package["name"] == "nox":
        print(package["version"])
        break
else:
    raise SystemExit("missing nox package metadata")')
fi

TAG="v$VERSION"

note "candidate version $VERSION"

if [ ! -x "$NOX_BIN" ]; then
    note "debug CLI missing at $NOX_BIN; building nox"
    cargo build -p nox >/dev/null
fi

CLI_VERSION=$($NOX_BIN --version)
if [ "$CLI_VERSION" = "nox $VERSION" ]; then
    pass "CLI version matches Cargo version ($CLI_VERSION)"
else
    fail "CLI version '$CLI_VERSION' does not match Cargo version '$VERSION'"
fi

if grep -q "^## \[$VERSION\]" CHANGELOG.md; then
    pass "CHANGELOG has release section [$VERSION]"
else
    fail "CHANGELOG is missing release section [$VERSION]"
fi

if git diff --quiet -- . ':!target'; then
    pass "worktree has no unstaged/staged source diff"
else
    fail "worktree has source diffs; release candidate must be audited on a release commit"
fi

if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    TAG_COMMIT=$(git rev-list -n 1 "$TAG")
    HEAD_COMMIT=$(git rev-parse HEAD)
    if [ "$TAG_COMMIT" = "$HEAD_COMMIT" ]; then
        pass "$TAG points at HEAD ($HEAD_COMMIT)"
    else
        fail "$TAG points at $TAG_COMMIT, not HEAD $HEAD_COMMIT"
    fi
else
    fail "missing release tag $TAG"
fi

if git remote -v | grep -q .; then
    pass "git remote is configured"
else
    fail "git remote is not configured; remote CI and release publishing cannot be verified"
fi

if [ -n "$CI_EVIDENCE" ]; then
    pass "remote CI evidence provided: $CI_EVIDENCE"
else
    fail "missing remote CI evidence; set NOX_RELEASE_CI_EVIDENCE to the run URL/id after CI passes"
fi

if rg -g '!scripts/release-audit.sh' '\.agents|\]\([^)]*(GOAL|PLAN)\.md' README.md README_zh_CN.md docs/en docs/zh_CN examples/README.md tests/README.md scripts .github >/tmp/nox-release-audit-links.$$ 2>/dev/null; then
    fail "formal surfaces link handoff files; see /tmp/nox-release-audit-links.$$"
else
    rm -f /tmp/nox-release-audit-links.$$
    pass "formal docs/scripts do not link .agents/GOAL.md/PLAN.md"
fi

if rg -q 'withdrawn|deprecated|hotfix|下游升级路径|git tag|GitHub Release' docs/zh_CN/release-checklist.md docs/en/release-checklist.md; then
    pass "release checklist includes rollback/tag/release terms"
else
    fail "release checklist is missing rollback/tag/release terms"
fi

if rg -q 'production release|checkpoint|release candidate|Breaking changes' README.md CHANGELOG.md docs/zh_CN/release-checklist.md docs/en/release-checklist.md; then
    pass "release level and breaking-change wording is present"
else
    fail "release level or breaking-change wording is missing"
fi

# PLAN 完成定义 9-13 项 (P8.6 综合断言). 每项检查对应 release-gate / fixture 中的护栏是否仍然
# 在位; 这里只做存在性审计 (PLAN 闭合契约要求), release-gate 负责跑实际护栏内容. 任一项缺失意味着
# 持续维护门槛被破坏, GOAL 实现状态立即回退.

if grep -q "product-shape guardrail" scripts/release-gate.sh; then
    pass "PLAN 第 9 项: product-shape guardrail wired in release-gate"
else
    fail "PLAN 第 9 项: product-shape guardrail missing from release-gate"
fi

if grep -q "small-footprint guardrail" scripts/release-gate.sh; then
    pass "PLAN 第 10 项: small-footprint guardrail wired in release-gate"
else
    fail "PLAN 第 10 项: small-footprint guardrail missing from release-gate"
fi

if grep -q "NOX_BENCH_BUDGET" scripts/bench-smoke.sh; then
    pass "PLAN 第 11 项: bench-smoke per-case budget wired"
else
    fail "PLAN 第 11 项: bench-smoke per-case budget missing"
fi

if grep -q "NOX_EMBEDDING_TIME_BUDGET" scripts/release-gate.sh; then
    pass "PLAN 第 12 项前半: embedding regression time budget wired"
else
    fail "PLAN 第 12 项前半: embedding regression time budget missing"
fi

if [ -f tests/fixtures/stdlib-surface.nox ] && grep -q "stdlib surface guardrail" scripts/release-gate.sh; then
    pass "PLAN 第 12 项后半: stdlib-surface fixture + guardrail wired"
else
    fail "PLAN 第 12 项后半: stdlib-surface fixture or guardrail missing"
fi

if [ -f examples/projects/health-check/nox.toml ] && grep -q "health-check project check" scripts/release-gate.sh; then
    pass "PLAN 第 12 项后半: non-scoreboard sample project wired"
else
    fail "PLAN 第 12 项后半: non-scoreboard sample project missing or not wired"
fi

# PLAN 第 13 项: 暂缓项守护. GOAL.md 由 .gitignore 排除, 不能直接 diff; 守护落在公开 API/CLI 关键词
# grep 上. 任何暂缓项 (mutable array, slice type, closure, higher-order, watch mode, incremental
# typecheck, tracing gc, package registry, lockfile) 在公开 surface 出现, 必须先修 GOAL.md 与
# CHANGELOG breaking-changes 再放行.
DEFERRED_RE='mutable array|slice type|closure|higher-order|watch mode|incremental typecheck|tracing gc|package registry|lockfile'
DEFERRED_HITS=/tmp/nox-deferred-hits.$$
if rg -i "$DEFERRED_RE" crates/nox/src crates/nox_core/src 2>/dev/null > "$DEFERRED_HITS"; then
    if [ -s "$DEFERRED_HITS" ]; then
        fail "PLAN 第 13 项: deferred-item keywords appear in public API/CLI; see $DEFERRED_HITS"
    else
        rm -f "$DEFERRED_HITS"
        pass "PLAN 第 13 项: deferred-item keywords absent from public API/CLI"
    fi
else
    rm -f "$DEFERRED_HITS"
    pass "PLAN 第 13 项: deferred-item keywords absent from public API/CLI"
fi

if [ "$failures" -eq 0 ]; then
    note "production release audit passed for $TAG"
    note "GOAL implementation: ACHIEVED on $TAG (PLAN 完成定义 13 项 + 持续门槛全部成立)"
    exit 0
fi

note "production release audit failed with $failures blocker(s)"
note "GOAL implementation: NOT ACHIEVED until all blockers are cleared"
if [ "$EXPECT_BLOCKED" = "1" ]; then
    note "blocked checkpoint mode accepted current blockers"
    exit 0
fi
exit 1
