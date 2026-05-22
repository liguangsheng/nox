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
run_gate_expect_status "CLI JSON diagnostic smoke" 1 "$NOX_BIN" check --json examples/type-error.nox
run_gate "CLI relative module not-found JSON smoke" env NOX_BIN="$NOX_BIN" sh -eu -c '
tmp=$(mktemp -d "${TMPDIR:-/tmp}/nox-missing-module.XXXXXX")
trap "rm -rf \"$tmp\"" EXIT
cat >"$tmp/main.nox" <<'"'"'NOX'"'"'
import "missing.nox" as missing;

missing.answer();
NOX
"$NOX_BIN" check --json "$tmp/main.nox" | grep -q "\"code\":\"module.not-found\""
'
run_gate "CLI test smoke" "$NOX_BIN" test examples/example_test.nox
run_gate "CLI test JSON smoke" "$NOX_BIN" test --json examples/example_test.nox
run_gate "CLI fmt smoke" "$NOX_BIN" fmt examples/hello.nox
run_gate "CLI fmt check smoke" "$NOX_BIN" fmt --check examples/formatter-golden.nox
run_gate "CLI inspect-bytecode smoke" "$NOX_BIN" inspect-bytecode --compact examples/hello.nox
run_gate "CLI map_get smoke" env NOX_BIN="$NOX_BIN" sh -eu -c 'output=$("$NOX_BIN" run examples/maps.nox); [ "$output" = "42" ]'
run_gate "CLI map_get bytecode smoke" env NOX_BIN="$NOX_BIN" sh -eu -c '"$NOX_BIN" inspect-bytecode --compact examples/maps.nox | grep -q MapGet'
run_gate_in_dir "scoreboard project check" examples/projects/scoreboard "$NOX_BIN" project check
run_gate_in_dir "scoreboard project check JSON" examples/projects/scoreboard "$NOX_BIN" project check --json
run_gate_in_dir "scoreboard test JSON" examples/projects/scoreboard "$NOX_BIN" test --json
run_gate_in_dir "scoreboard fmt check" examples/projects/scoreboard "$NOX_BIN" fmt --check
run_gate "scoreboard std module check" "$NOX_BIN" check examples/projects/scoreboard/src/runtime_info.nox
run_gate "scoreboard std module fmt" "$NOX_BIN" fmt --check examples/projects/scoreboard/src/runtime_info.nox

run_gate "embedding regression" scripts/embedding-regression.sh
run_gate "robustness smoke" scripts/robustness-smoke.sh
run_gate "benchmark smoke" env -u NOX_BIN scripts/bench-smoke.sh

run_gate "production release audit blocker smoke" env NOX_RELEASE_AUDIT_EXPECT_BLOCKED=1 scripts/release-audit.sh

run_gate "Markdown link check" python3 -c 'import pathlib,re,sys
roots=[pathlib.Path(p) for p in ["README.md","README_zh_CN.md","docs","examples/README.md"]]
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
