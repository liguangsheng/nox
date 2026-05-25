#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
NOX_BIN=${NOX_BIN:-"$ROOT/target/debug/nox"}

cd "$ROOT"

printf 'release gate: local verification only; no push, tag, or external release\n'

run_gate() {
    name=$1
    shift
    printf 'release gate: %s\n' "$name"
    "$@"
}

run_gate_shell() {
    name=$1
    shift
    printf 'release gate: %s\n' "$name"
    sh -eu -c "$*"
}

run_gate_in_dir() {
    name=$1
    dir=$2
    shift 2
    printf 'release gate: %s\n' "$name"
    (cd "$dir" && "$@")
}

run_gate_expect_status() {
    name=$1
    expected=$2
    shift 2
    printf 'release gate: %s\n' "$name"
    set +e
    "$@"
    status=$?
    set -e
    if [ "$status" -ne "$expected" ]; then
        printf 'release gate: %s expected exit %s, got %s\n' "$name" "$expected" "$status" >&2
        return 1
    fi
}

run_gate "cargo fmt" cargo fmt --all --check
run_gate "cargo test" cargo test --all
run_gate "cargo clippy" cargo clippy --all-targets -- -D warnings
run_gate "debug CLI build" cargo build -p nox

run_gate "CLI version smoke" "$NOX_BIN" --version
run_gate "CLI run smoke" "$NOX_BIN" run examples/hello.nox
run_gate "CLI check smoke" "$NOX_BIN" check examples/hello.nox
run_gate_expect_status "CLI JSON diagnostic smoke" 1 "$NOX_BIN" check --json tests/fixtures/type-error.nox
run_gate "CLI relative module not-found JSON smoke" env NOX_BIN="$NOX_BIN" sh -eu -c '
tmp=$(mktemp -d "${TMPDIR:-/tmp}/nox-missing-module.XXXXXX")
trap "rm -rf \"$tmp\"" EXIT
cat >"$tmp/main.nox" <<'"'"'NOX'"'"'
import "missing.nox" as missing;

missing.answer();
NOX
"$NOX_BIN" check --json "$tmp/main.nox" | grep -q "\"code\":\"module.not-found\""
'
run_gate "CLI test smoke" "$NOX_BIN" test tests/fixtures/example_test.nox
run_gate "CLI test JSON smoke" "$NOX_BIN" test --json tests/fixtures/example_test.nox
run_gate "CLI fmt smoke" "$NOX_BIN" fmt examples/hello.nox
run_gate "CLI fmt check smoke" "$NOX_BIN" fmt --check tests/fixtures/formatter-golden.nox
run_gate "CLI inspect-bytecode smoke" "$NOX_BIN" inspect-bytecode --compact examples/hello.nox
run_gate "CLI map_get smoke" env NOX_BIN="$NOX_BIN" sh -eu -c 'output=$("$NOX_BIN" run examples/maps.nox); [ "$output" = "42" ]'
run_gate "CLI map_get bytecode smoke" env NOX_BIN="$NOX_BIN" sh -eu -c '"$NOX_BIN" inspect-bytecode --compact examples/maps.nox | grep -q MapGet'
run_gate_in_dir "scoreboard project check" examples/projects/scoreboard "$NOX_BIN" project check
run_gate_in_dir "scoreboard project check JSON" examples/projects/scoreboard "$NOX_BIN" project check --json
run_gate_in_dir "scoreboard test JSON" examples/projects/scoreboard "$NOX_BIN" test --json
run_gate_in_dir "scoreboard fmt check" examples/projects/scoreboard "$NOX_BIN" fmt --check
run_gate "scoreboard std module check" "$NOX_BIN" check examples/projects/scoreboard/src/runtime_info.nox
run_gate "scoreboard std module fmt" "$NOX_BIN" fmt --check examples/projects/scoreboard/src/runtime_info.nox
run_gate "module dependency lockfile guardrail" sh -eu -c '
git ls-files | grep -E "(^|/)nox\.toml$" | while IFS= read -r manifest; do
    if grep -q "^\[dependencies\]" "$manifest"; then
        lock="$(dirname "$manifest")/nox.lock"
        if [ ! -f "$lock" ]; then
            printf "dependency lockfile guardrail failed: %s declares [dependencies] but %s is missing\n" "$manifest" "$lock" >&2
            exit 1
        fi
    fi
done
'
run_gate "module ecosystem regression: project check lockfile JSON" cargo test -p nox project_check_json_reports_ --test cli
run_gate "module ecosystem regression: fetch offline/cache" cargo test -p nox fetch_ --test cli
run_gate "module ecosystem regression: external import/cache/hash" cargo test -p nox external_dependency_import --test cli
run_gate "module ecosystem regression: integrated LSP external import" cargo test -p nox lsp_external_dependency_import --test cli
run_gate "compatibility regression: parser AST golden" cargo test -p nox_core parser_ast_golden --lib
run_gate "compatibility regression: C ABI enum values" cargo test -p nox_core c_abi_enum_values_are_stable --lib
run_gate "compatibility regression: async Rust API" cargo test -p nox async_task_rust_api --lib
run_gate "compatibility regression: machine-readable golden surfaces" scripts/compatibility-golden.sh

# Host-friendliness guardrail (P8.5, PLAN 完成定义第 12 项前半).
# embedding-regression covers Rust API/runtime tests, Rust embedding example,
# nox_core dynamic library build, C ABI header↔library symbol parity, and the
# C embedding smoke compile + link + run. We attach a wall-time budget so a
# silent regression that doubles build/link cost surfaces as a release-time
# blocker, not just a slow CI run.
NOX_EMBEDDING_TIME_BUDGET=${NOX_EMBEDDING_TIME_BUDGET:-60}
run_gate "embedding regression (time budget)" env BUDGET="$NOX_EMBEDDING_TIME_BUDGET" sh -eu -c '
start=$(date +%s)
scripts/embedding-regression.sh
end=$(date +%s)
elapsed=$((end - start))
if [ "$elapsed" -gt "$BUDGET" ]; then
    printf "embedding regression: %ss exceeded budget %ss\n" "$elapsed" "$BUDGET" >&2
    exit 1
fi
printf "embedding regression: %ss (budget %ss)\n" "$elapsed" "$BUDGET"
'
run_gate "robustness smoke" scripts/robustness-smoke.sh

# Optional long-running quality-deepening gate (PLAN stage 15).
# Keep it opt-in so normal release-gate runs stay fast and deterministic, but
# make the exact command part of the shared gate instead of tribal knowledge.
if [ "${NOX_RELEASE_GATE_PROPERTY:-0}" = "1" ]; then
    run_gate "property failure export smoke" env NOX_BIN="$NOX_BIN" sh -eu -c '
tmp=$(mktemp -d "${TMPDIR:-/tmp}/nox-property-gate.XXXXXX")
trap "rm -rf \"$tmp\"" EXIT
cat >"$tmp/property_test.nox" <<'"'"'NOX'"'"'
import "std/test.nox" as test;

fn test_property_fails() -> null {
    test.assert_property_int("negative-rejected", 3, 20, -20, 20, fn(value: int) -> bool {
        return value >= 0;
    });
    return null;
}
NOX
set +e
"$NOX_BIN" test --export-failures "$tmp/corpus" "$tmp/property_test.nox" >"$tmp/stdout" 2>"$tmp/stderr"
status=$?
set -e
if [ "$status" -ne 1 ]; then
    printf "expected property test to fail with exit 1, got %s\n" "$status" >&2
    cat "$tmp/stdout" >&2
    cat "$tmp/stderr" >&2
    exit 1
fi
find "$tmp/corpus" -type f -name "*.nox" -print -quit | grep -q .
grep -R "property failed seed=3 case=" "$tmp/corpus" >/dev/null
'
fi

if [ "${NOX_RELEASE_GATE_COVERAGE:-0}" = "1" ]; then
    run_gate "coverage JSON opt-in smoke" env NOX_BIN="$NOX_BIN" sh -eu -c '
"$NOX_BIN" coverage --json tests/benchmarks/bench-fib.nox | grep -q "\"schema\":\"nox.coverage.v1\""
"$NOX_BIN" coverage --ndjson tests/benchmarks/bench-fib.nox | grep -q "\"schema\":\"nox.coverage.event.v1\""
'
fi

if [ "${NOX_RELEASE_GATE_FUZZ:-0}" = "1" ]; then
    NOX_FUZZ_TIME=${NOX_FUZZ_TIME:-60}
    CARGO_NIGHTLY=${CARGO_NIGHTLY:-"$HOME/.cargo/bin/cargo"}
    for target in parser typecheck verifier; do
        run_gate "cargo-fuzz $target (${NOX_FUZZ_TIME}s)" "$CARGO_NIGHTLY" +nightly fuzz run "$target" -- -max_total_time="$NOX_FUZZ_TIME"
    done
fi

if [ "${NOX_RELEASE_GATE_SANITIZER:-0}" = "1" ]; then
    run_gate "sanitizer smoke" scripts/sanitizer-smoke.sh
fi

run_gate "benchmark smoke" env -u NOX_BIN scripts/bench-smoke.sh

# Product-shape guardrail: PLAN 完成定义第 9 项的 release-time 显式断言。
# 该段不替代上面的 cargo test 与 CLI smoke——它们已经隐式覆盖语言、引擎、运行时回归；
# 本段只对 CLI 子命令"对外能力面"做单点显式快速失败：当 nox usage 中任何 GOAL 产品形态四件套
# 的 CLI 子命令名被静默删除时，此 gate 立即失败，避免下游用户在 release 后才发现能力消失。
run_gate "product-shape guardrail: CLI subcommand surface" env NOX_BIN="$NOX_BIN" sh -eu -c '
usage=$("$NOX_BIN" --help 2>&1 || true)
missing=""
for sub in "nox run" "nox check" "nox test" "nox fmt" "nox project check" "nox fetch" "nox repl" "nox lsp" "nox dap" "nox profile" "nox coverage" "nox inspect-bytecode" "nox watch" "nox lint" "nox doc" "nox host-metadata"; do
    case "$usage" in
        *"$sub"*) ;;
        *) missing="$missing\n  $sub" ;;
    esac
done
if [ -n "$missing" ]; then
    printf "product-shape guardrail failed: missing CLI subcommands:%b\n" "$missing" >&2
    printf "captured nox --help output:\n%s\n" "$usage" >&2
    exit 1
fi
'
run_gate "release candidate readiness guard" scripts/release-candidate-readiness.sh
run_gate "release-prep version helper self-test" scripts/prepare-release-version.sh --self-test
run_gate "release asset builder self-test" scripts/build-release-assets.sh --self-test
run_gate "release asset manifest self-test" scripts/release-asset-manifest.sh --self-test
run_gate "release asset smoke self-test" scripts/release-asset-smoke.sh --self-test
run_gate "release toolchain status self-test" scripts/release-toolchain-status.sh --self-test
run_gate "release cutover check self-test" scripts/release-cutover-check.sh --self-test
run_gate "release cutover status self-test" scripts/release-cutover-status.sh --self-test
run_gate "release upload plan self-test" scripts/release-upload-plan.sh --self-test
run_gate "release notes extraction self-test" scripts/release-notes.sh --self-test
run_gate "release command plan self-test" scripts/release-command-plan.sh --self-test
run_gate "release evidence report self-test" scripts/release-evidence-report.sh --self-test
run_gate "release prep dry-run self-test" scripts/release-prep-dry-run.sh --self-test
run_gate "release cutover status JSON smoke" sh -eu -c '
tmp=$(mktemp "${TMPDIR:-/tmp}/nox-release-cutover-status-json.XXXXXX")
trap "rm -f \"$tmp\"" EXIT HUP INT TERM
scripts/release-cutover-status.sh --json >"$tmp" 2>/dev/null || true
python3 - "$tmp" <<'"'"'PY'"'"'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    data = json.load(fh)
assert data["schema"] == "nox.release-cutover-status.v1"
assert isinstance(data["ok"], bool)
assert data["tag"].startswith("v")
assert isinstance(data["pending_count"], int)
assert isinstance(data["items"], list)
for item in data["items"]:
    assert item["state"] in {"ok", "missing"}
    assert isinstance(item["message"], str)
PY
'
run_gate "release-prep version helper check-only" sh -eu -c '
current=$(awk -F"\"" "/^version = /{print \$2; exit}" Cargo.toml)
patch=${current##*.}
next_patch=$((patch + 1))
next_version="0.0.$next_patch"
if grep -q "^## \\[$current\\]" CHANGELOG.md; then
    printf "release-prep version helper check-only: already on %s\n" "$current"
else
    scripts/prepare-release-version.sh --check-only "$next_version" 2026-05-25
fi
'

# Small-footprint guardrail: PLAN 完成定义第 10 项的 release-time 显式断言。
# 阈值由 P8.3 冻结；上调阈值必须独立 commit + CHANGELOG + ADR，不允许在 release-prep 阶段
# 临时上调来掩盖回归。LOC 不设硬阈值，只回显趋势值供 release notes 记录。
NOX_SIZE_CAP_CLI=${NOX_SIZE_CAP_CLI:-3227648}        # 3.078125 MiB; ADR 0025 recalibrates after codegen source-map audit growth
NOX_SIZE_CAP_CORE=${NOX_SIZE_CAP_CORE:-1572864}      # 1.5 MiB; current baseline ~1.0 MiB (tightened from 2.5 MiB at v0.0.3 post-release)
run_gate "small-footprint guardrail: release build" cargo build --release -p nox -p nox_core
run_gate "small-footprint guardrail: CLI binary size cap" env CAP="$NOX_SIZE_CAP_CLI" sh -eu -c '
size=$(wc -c < target/release/nox)
if [ "$size" -gt "$CAP" ]; then
    printf "small-footprint guardrail failed: target/release/nox is %s bytes, cap is %s bytes\n" "$size" "$CAP" >&2
    exit 1
fi
printf "small-footprint guardrail: target/release/nox = %s bytes (cap %s)\n" "$size" "$CAP"
'
run_gate "small-footprint guardrail: libnox_core size cap" env CAP="$NOX_SIZE_CAP_CORE" sh -eu -c '
size=$(wc -c < target/release/libnox_core.so)
if [ "$size" -gt "$CAP" ]; then
    printf "small-footprint guardrail failed: target/release/libnox_core.so is %s bytes, cap is %s bytes\n" "$size" "$CAP" >&2
    exit 1
fi
printf "small-footprint guardrail: target/release/libnox_core.so = %s bytes (cap %s)\n" "$size" "$CAP"
'
run_gate "small-footprint guardrail: zero third-party runtime deps" sh -eu -c '
external=$(cargo tree -p nox -e normal --prefix none 2>/dev/null | grep -vE " \\(/" | wc -l)
if [ "$external" -ne 0 ]; then
    printf "small-footprint guardrail failed: %s non-workspace runtime deps:\n" "$external" >&2
    cargo tree -p nox -e normal --prefix none | grep -vE " \\(/" >&2 || true
    exit 1
fi
printf "small-footprint guardrail: zero third-party runtime deps\n"
'
run_gate "small-footprint guardrail: LOC trend record" sh -eu -c '
loc=$(find crates -name "*.rs" -not -path "*/target/*" -print0 | xargs -0 wc -l | tail -1 | awk "{print \$1}")
printf "small-footprint guardrail: workspace Rust LOC = %s\n" "$loc"
'

# Standard-library surface guardrail (P8.5, PLAN 完成定义第 12 项后半).
# Type-checks tests/fixtures/stdlib-surface.nox; if any std module or global
# stdlib entry (fs/env/time/net/async/math/string/json/csv/tsv/array/map/option/result) is silently removed or has
# its signature changed, this gate fails.
run_gate "stdlib surface guardrail: stdlib entries" "$NOX_BIN" check tests/fixtures/stdlib-surface.nox

# Practical-use guardrail (P8.5, PLAN 完成定义第 12 项后半).
# Drives a real-world sample project that is not the scoreboard calculator,
# proving that the project workflow (manifest + multi-module + tests + fmt)
# works on at least one production-like scenario. health-check uses fs and env
# stdlib modules plus option/result and unit tests.
run_gate_in_dir "health-check project check" examples/projects/health-check "$NOX_BIN" project check
run_gate_in_dir "health-check project check JSON" examples/projects/health-check "$NOX_BIN" project check --json

run_gate "production release audit blocker smoke" env NOX_RELEASE_AUDIT_EXPECT_BLOCKED=1 scripts/release-audit.sh

run_gate "Markdown link check" python3 -c 'import pathlib,re,sys
roots=[pathlib.Path(p) for p in ["README.md","README_zh_CN.md","docs/en","docs/zh_CN","examples/README.md","tests/README.md"]]
files=[]
for root in roots:
    if root.is_dir(): files.extend(root.rglob("*.md"))
    elif root.exists(): files.append(root)
missing=[]
for path in files:
    text=path.read_text()
    for target in re.findall(r"\[[^\]]+\]\(([^)#][^)]+)\)", text):
        if "://" in target or target.startswith("mailto:"): continue
        target_path=(path.parent/target).resolve()
        if not target_path.exists(): missing.append((str(path),target))
if missing:
    print("missing markdown links:")
    [print(f"{p}: {t}") for p,t in missing]
    sys.exit(1)
print("markdown links ok")'

run_gate "whitespace check" git diff --check HEAD

printf 'release gate: ok\n'
