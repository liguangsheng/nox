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

    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$mode" "$*" "$status" "$real" "$output"
    [ "$status" = ok ]
}

run_script_case() {
    name=$1
    file=$2
    run_case "$name" check "$file: ok" "$NOX_BIN" check "$file"
    run_case "$name" compile "0000" "$NOX_BIN" inspect-bytecode --compact "$file"
    run_case "$name" e2e "$3" "$NOX_BIN" run "$file"
}

run_script_case recursion "$ROOT/examples/bench-fib.nox" fib-ok
run_script_case loop "$ROOT/examples/bench-loop.nox" loop-ok
run_script_case containers "$ROOT/examples/bench-containers.nox" containers-ok
run_script_case modules "$ROOT/examples/bench-modules.nox" modules-ok
run_case nox-test e2e "summary: 2 tests, 2 passed, 0 failed" "$NOX_BIN" test "$ROOT/examples/example_test.nox"
