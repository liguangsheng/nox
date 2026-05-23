#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if [ "${NOX_BIN+x}" ]; then
    NOX_BIN=${NOX_BIN}
else
    NOX_BIN="$ROOT/target/release/nox"
    cargo build --release -p nox >/dev/null
fi

MAX_SECONDS=${NOX_BENCH_SMOKE_MAX_SECONDS:-10}
TIMEOUT_BIN=${NOX_BENCH_SMOKE_TIMEOUT:-}
if [ -z "$TIMEOUT_BIN" ] && command -v timeout >/dev/null 2>&1; then
    TIMEOUT_BIN=timeout
fi

printf 'case\tmode\tcommand\tstatus\treal_seconds\toutput\n'

run_case() {
    name=$1
    mode=$2
    expected_output=$3
    shift 3
    tmp_stdout=$(mktemp)
    tmp_stderr=$(mktemp)
    start=$(date +%s.%N)
    if [ -n "$TIMEOUT_BIN" ]; then
        set -- "$TIMEOUT_BIN" "$MAX_SECONDS" "$@"
    fi
    if "$@" >"$tmp_stdout" 2>"$tmp_stderr"; then
        status=ok
    else
        status=fail
    fi
    end=$(date +%s.%N)
    real=$(awk -v start="$start" -v end="$end" 'BEGIN { printf "%.6f", end - start }')
    output=$(tr '\n' ' ' <"$tmp_stdout" | sed 's/[[:space:]]*$//')
    if [ "$status" = fail ]; then
        err=$(tr '\n' ' ' <"$tmp_stderr" | sed 's/[[:space:]]*$//')
        output="$output $err"
    fi
    rm -f "$tmp_stdout" "$tmp_stderr"

    if [ "$status" = ok ] && [ "$expected_output" != "-" ]; then
        case "$output" in
            *"$expected_output"*) ;;
            *)
                status=fail
                output="$output expected output containing '$expected_output'"
                ;;
        esac
    fi
    if [ "$status" = ok ] && awk -v real="$real" -v max="$MAX_SECONDS" 'BEGIN { exit !(real > max) }'; then
        status=fail
        output="$output exceeded ${MAX_SECONDS}s smoke threshold"
    fi

    # P8.4: per-case e2e budget. Hard ceiling per case applied only to e2e wall time;
    # check/compile modes are not budgeted. Budget passed via BUDGET env var when caller
    # wants tight limits (release-gate); unset means only MAX_SECONDS cap applies.
    if [ "$status" = ok ] && [ "$mode" = e2e ] && [ -n "${BUDGET:-}" ] && awk -v real="$real" -v budget="$BUDGET" 'BEGIN { exit !(real > budget) }'; then
        status=fail
        output="$output exceeded ${BUDGET}s e2e budget"
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$mode" "$*" "$status" "$real" "$output"
    [ "$status" = ok ]
}

run_script_case() {
    name=$1
    file=$2
    expected=$3
    budget=$4
    run_case "$name" check "$file: ok" "$NOX_BIN" check "$file"
    run_case "$name" compile "0000" "$NOX_BIN" inspect-bytecode --compact "$file"
    BUDGET="$budget" run_case "$name" e2e "$expected" "$NOX_BIN" run "$file"
}

# P8.4: per-case e2e budget (seconds). Baselines observed on the v0.0.3 release build:
# bench-fib peaked at ~0.14s, bench-loop ~0.20s, bench-containers ~0.025s,
# bench-modules ~0.019s, bench-lambda ~0.005s, host-capabilities ~0.03s,
# nox-test ~0.09s. Budgets leave headroom for CI shared cores and machine load.
# Tightening or raising any of these requires an independent commit + CHANGELOG + ADR.
NOX_BENCH_BUDGET_FIB=${NOX_BENCH_BUDGET_FIB:-1.0}
NOX_BENCH_BUDGET_LOOP=${NOX_BENCH_BUDGET_LOOP:-1.5}
NOX_BENCH_BUDGET_CONTAINERS=${NOX_BENCH_BUDGET_CONTAINERS:-0.3}
NOX_BENCH_BUDGET_MODULES=${NOX_BENCH_BUDGET_MODULES:-0.3}
NOX_BENCH_BUDGET_NOX_TEST=${NOX_BENCH_BUDGET_NOX_TEST:-1.0}
NOX_BENCH_BUDGET_LAMBDA=${NOX_BENCH_BUDGET_LAMBDA:-0.5}
NOX_BENCH_BUDGET_HOST_CAPABILITIES=${NOX_BENCH_BUDGET_HOST_CAPABILITIES:-0.5}

run_script_case recursion "$ROOT/tests/benchmarks/bench-fib.nox" fib-ok "$NOX_BENCH_BUDGET_FIB"
run_script_case loop "$ROOT/tests/benchmarks/bench-loop.nox" loop-ok "$NOX_BENCH_BUDGET_LOOP"
run_script_case containers "$ROOT/tests/benchmarks/bench-containers.nox" containers-ok "$NOX_BENCH_BUDGET_CONTAINERS"
run_script_case modules "$ROOT/tests/benchmarks/bench-modules.nox" modules-ok "$NOX_BENCH_BUDGET_MODULES"
run_script_case lambda "$ROOT/tests/benchmarks/bench-lambda.nox" lambda-ok "$NOX_BENCH_BUDGET_LAMBDA"
run_script_case host-capabilities "$ROOT/tests/benchmarks/bench-host-capabilities.nox" host-capabilities-ok "$NOX_BENCH_BUDGET_HOST_CAPABILITIES"
BUDGET="$NOX_BENCH_BUDGET_NOX_TEST" run_case nox-test e2e "summary: 2 tests, 2 passed, 0 failed" "$NOX_BIN" test "$ROOT/tests/fixtures/example_test.nox"

if [ "${NOX_BENCH_CRITERION:-0}" = "1" ]; then
    cargo bench -p nox_core --bench core_paths
    cargo bench -p nox --bench runtime_capabilities
fi
