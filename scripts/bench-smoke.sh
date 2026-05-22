#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if [ "${NOX_BIN+x}" ]; then
    NOX_BIN=${NOX_BIN}
else
    NOX_BIN="$ROOT/target/release/nox"
    cargo build --release -p nox >/dev/null
fi

printf 'case\tmode\tcommand\tstatus\treal_seconds\toutput\n'

run_case() {
    name=$1
    mode=$2
    shift 2
    tmp_stdout=$(mktemp)
    tmp_stderr=$(mktemp)
    start=$(date +%s.%N)
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
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$mode" "$*" "$status" "$real" "$output"
    [ "$status" = ok ]
}

run_script_case() {
    name=$1
    file=$2
    run_case "$name" check "$NOX_BIN" check "$file"
    run_case "$name" compile "$NOX_BIN" inspect-bytecode --compact "$file"
    run_case "$name" e2e "$NOX_BIN" run "$file"
}

run_script_case recursion "$ROOT/examples/bench-fib.nox"
run_script_case loop "$ROOT/examples/bench-loop.nox"
run_script_case containers "$ROOT/examples/bench-containers.nox"
run_script_case modules "$ROOT/examples/bench-modules.nox"
run_case nox-test e2e "$NOX_BIN" test "$ROOT/examples/example_test.nox"
