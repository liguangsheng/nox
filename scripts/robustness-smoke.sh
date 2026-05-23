#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if [ "${NOX_BIN+x}" ]; then
    NOX_BIN=${NOX_BIN}
else
    NOX_BIN="$ROOT/target/debug/nox"
    cargo build -p nox >/dev/null
fi

printf 'case\tcommand\tstatus\tresult\n'

run_expect_status() {
    name=$1
    expected=$2
    shift 2
    tmp_stdout=$(mktemp)
    tmp_stderr=$(mktemp)
    set +e
    "$@" >"$tmp_stdout" 2>"$tmp_stderr"
    status=$?
    set -e
    rm -f "$tmp_stdout" "$tmp_stderr"
    if [ "$status" -eq "$expected" ]; then
        result=ok
    else
        result="expected-$expected-got-$status"
    fi
    printf '%s\t%s\t%s\t%s\n' "$name" "$*" "$status" "$result"
    [ "$result" = ok ]
}

run_expect_status_in_dir() {
    name=$1
    expected=$2
    dir=$3
    shift 3
    tmp_stdout=$(mktemp)
    tmp_stderr=$(mktemp)
    set +e
    (cd "$dir" && "$@") >"$tmp_stdout" 2>"$tmp_stderr"
    status=$?
    set -e
    rm -f "$tmp_stdout" "$tmp_stderr"
    if [ "$status" -eq "$expected" ]; then
        result=ok
    else
        result="expected-$expected-got-$status"
    fi
    printf '%s\t%s\t%s\t%s\n' "$name" "(cd $dir && $*)" "$status" "$result"
    [ "$result" = ok ]
}

run_expect_status_stdin() {
    name=$1
    expected=$2
    stdin_file=$3
    shift 3
    tmp_stdout=$(mktemp)
    tmp_stderr=$(mktemp)
    set +e
    "$@" <"$stdin_file" >"$tmp_stdout" 2>"$tmp_stderr"
    status=$?
    set -e
    rm -f "$tmp_stdout" "$tmp_stderr"
    if [ "$status" -eq "$expected" ]; then
        result=ok
    else
        result="expected-$expected-got-$status"
    fi
    printf '%s\t%s\t%s\t%s\n' "$name" "$* < stdin" "$status" "$result"
    [ "$result" = ok ]
}

for file in \
    "$ROOT/tests/malformed/unterminated-string.nox" \
    "$ROOT/tests/malformed/deep-nesting.nox" \
    "$ROOT/tests/malformed/illegal-token.nox" \
    "$ROOT/tests/malformed/half-import.nox" \
    "$ROOT/tests/malformed/bad-record.nox" \
    "$ROOT/tests/malformed/lsp-half-source.nox"
do
    file_name=$(basename "$file")
    run_expect_status "check:$file_name" 1 "$NOX_BIN" check "$file"
    run_expect_status "fmt:$file_name" 1 "$NOX_BIN" fmt "$file"
done

for file in \
    "$ROOT/tests/malformed/type-mismatch.nox" \
    "$ROOT/tests/malformed/namespace-missing-member.nox" \
    "$ROOT/tests/malformed/deep-record-map.nox" \
    "$ROOT/tests/malformed/stdlib-string-bad-call.nox" \
    "$ROOT/tests/malformed/stdlib-array-bad-call.nox" \
    "$ROOT/tests/malformed/stdlib-process-exit-bad-type.nox" \
    "$ROOT/tests/malformed/stdlib-path-misspelled.nox"
do
    file_name=$(basename "$file")
    run_expect_status "check:$file_name" 1 "$NOX_BIN" check "$file"
    run_expect_status "fmt:$file_name" 0 "$NOX_BIN" fmt "$file"
done

for dir in \
    "$ROOT/tests/malformed/manifest-missing-version" \
    "$ROOT/tests/malformed/manifest-unknown-permission"
do
    dir_name=$(basename "$dir")
    run_expect_status_in_dir "check:$dir_name" 2 "$dir" "$NOX_BIN" check
    run_expect_status_in_dir "fmt:$dir_name" 2 "$dir" "$NOX_BIN" fmt --check
done

lsp_input=$(mktemp)
trap 'rm -f "$lsp_input"' EXIT
python3 - "$ROOT/tests/malformed/lsp-half-source.nox" >"$lsp_input" <<'PY'
import json
import pathlib
import sys

source = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
messages = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
    {
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///malformed/lsp-half-source.nox",
                "languageId": "nox",
                "version": 1,
                "text": source,
            }
        },
    },
    {"jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": None},
    {"jsonrpc": "2.0", "method": "exit", "params": None},
]

for message in messages:
    body = json.dumps(message, separators=(",", ":"))
    payload = body.encode("utf-8")
    sys.stdout.write(f"Content-Length: {len(payload)}\r\n\r\n{body}")
PY
run_expect_status_stdin "lsp:lsp-half-source.nox" 0 "$lsp_input" "$NOX_BIN" lsp
