#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
NOX_BIN=${NOX_BIN:-"$ROOT/target/debug/nox"}

cd "$ROOT"

if [ ! -x "$NOX_BIN" ]; then
    cargo build -p nox >/dev/null
fi

tmp=$(mktemp -d "${TMPDIR:-/tmp}/nox-compat-golden.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

compat="$tmp/compat.nox"
cat >"$compat" <<'NOX'
/// Displayable values.
export trait Display {
    fn to_str(self: Self) -> str;
}

export record User {
    name: str,
}

impl Display for User {
    fn to_str(self: User) -> str {
        return self.name;
    }
}

/// Computes a value asynchronously.
export async fn compute(value: int) -> result[int, str] {
    return ok(value + 1);
}
NOX

printf 'compatibility golden: formatter/parser surface\n'
"$NOX_BIN" fmt "$compat" >"$tmp/fmt.out"
grep -q 'export trait Display {' "$tmp/fmt.out"
grep -q 'impl Display for User {' "$tmp/fmt.out"
grep -q 'export async fn compute(value: int) -> result\[int, str\]' "$tmp/fmt.out"

printf 'compatibility golden: CLI diagnostic JSON\n'
set +e
"$NOX_BIN" check --json tests/fixtures/type-error-question-mark-mismatch.nox >"$tmp/check.json"
status=$?
set -e
if [ "$status" -ne 1 ]; then
    printf 'expected check --json to exit 1, got %s\n' "$status" >&2
    exit 1
fi
python3 - "$tmp/check.json" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
assert data["schema"] == "nox.check.v1"
codes = [diagnostic.get("code") for diagnostic in data["diagnostics"]]
assert "result.question-mark.mismatch" in codes, codes
assert data["summary"]["failed"] == 1
PY

printf 'compatibility golden: doc output\n'
"$NOX_BIN" doc "$compat" >"$tmp/doc.md"
grep -q '## export trait Display' "$tmp/doc.md"
grep -q 'Kind: \*\*trait\*\*\. Visibility: \*\*exported\*\*' "$tmp/doc.md"
grep -q '## export async fn compute() -> result\[int, str\]' "$tmp/doc.md" \
    || grep -q '## export async fn compute(value: int) -> result\[int, str\]' "$tmp/doc.md"
grep -q 'Call return: \*\*task\[result\[int, str\]\]\*\*\.' "$tmp/doc.md"

printf 'compatibility golden: LSP diagnostic JSON\n'
python3 - "$NOX_BIN" "$tmp" <<'PY'
import json
import pathlib
import subprocess
import sys

nox_bin = sys.argv[1]
tmp = pathlib.Path(sys.argv[2])
source = "async fn compute() -> int {\n    return 1;\n}\n\nawait compute();\n"
path = tmp / "lsp-async.nox"
path.write_text(source)
uri = "file://" + str(path)

def frame(payload):
    encoded = payload.encode()
    return f"Content-Length: {len(encoded)}\r\n\r\n".encode() + encoded

messages = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
    {
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "nox",
                "version": 1,
                "text": source,
            }
        },
    },
    {"jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": None},
    {"jsonrpc": "2.0", "method": "exit", "params": None},
]
stdin = b"".join(frame(json.dumps(message, separators=(",", ":"))) for message in messages)
proc = subprocess.run([nox_bin, "lsp"], input=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
assert proc.returncode == 0, proc.stderr.decode()
stdout = proc.stdout.decode()
assert '"method":"textDocument/publishDiagnostics"' in stdout, stdout
assert '"code":"async.await-outside-async"' in stdout, stdout
PY

printf 'compatibility golden: project check lockfile JSON\n'
project="$tmp/project"
mkdir -p "$project/src"
cat >"$project/nox.toml" <<'TOML'
[package]
name = "compat-lockfile"
version = "0.0.1"

[entrypoints]
main = "src/main.nox"

[modules]
source_dirs = ["src"]

[dependencies]
mathx = { github = "owner/mathx", rev = "0123456789abcdef0123456789abcdef01234567" }
tools = { git = "https://github.com/owner/tools.git", tag = "v0.2.0" }
TOML
cat >"$project/nox.lock" <<'LOCK'
[lock]
version = "1"

[dependencies.mathx]
source_kind = "github"
source = "owner/mathx"
pin_kind = "rev"
pin = "0123456789abcdef0123456789abcdef01234567"
resolved = "0123456789abcdef0123456789abcdef01234567"
content_hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
cache_key = "github-owner-mathx-0123456789abcdef0123456789abcdef01234567"
tool = "nox 0.0.4"

[dependencies.tools]
source_kind = "git"
source = "https://github.com/owner/tools.git"
pin_kind = "tag"
pin = "v0.2.0"
resolved = "fedcba9876543210fedcba9876543210fedcba98"
content_hash = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
cache_key = "git-owner-tools-fedcba9876543210fedcba9876543210fedcba98"
tool = "nox 0.0.4"
LOCK
printf '1;\n' >"$project/src/main.nox"
(cd "$project" && "$NOX_BIN" project check --json >"$tmp/project.json")
python3 - "$tmp/project.json" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
assert data["schema"] == "nox.project-check.v1"
assert data["ok"] is True
deps = data["dependencies"]
assert [entry["name"] for entry in deps["declared"]] == ["mathx", "tools"]
assert deps["lockfile"]["ok"] is True
assert deps["lockfile"]["status"] == "ok"
PY

printf 'compatibility golden: host metadata API JSON\n'
"$NOX_BIN" host-metadata --json >"$tmp/host.json"
python3 - "$tmp/host.json" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
assert data["schema"] == "nox.host-metadata.v1"
functions = {entry["name"]: entry for entry in data["functions"]}
assert functions["read_text"]["return_type"] == "str"
assert functions["read_text"]["capabilities"] == ["filesystem"]
assert functions["task_sleep"]["return_type"] == "task[null]"
assert functions["task_ready"]["return_type"] == "bool"
PY

printf 'compatibility golden: release asset manifest JSON\n'
NOX_RELEASE_VERSION=0.0.6 NOX_RELEASE_TAG=v0.0.6 scripts/release-asset-manifest.sh --json >"$tmp/asset-manifest.json"
python3 - "$tmp/asset-manifest.json" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
assert data["schema"] == "nox.release-asset-manifest.v1"
assert data["version"] == "0.0.6"
assert data["tag"] == "v0.0.6"
assets = data["assets"]
assert [asset["name"] for asset in assets] == [
    "nox-cli-v0.0.6-x86_64-unknown-linux-gnu",
    "nox-embed-v0.0.6-x86_64-unknown-linux-gnu",
    "nox-cli-v0.0.6-x86_64-unknown-linux-musl",
]
assert assets[0]["kind"] == "cli"
assert assets[0]["commitment"] == "full-sdk"
assert assets[0]["c_abi_smoke_required"] is False
assert assets[1]["kind"] == "embed"
assert assets[1]["commitment"] == "full-sdk"
assert assets[1]["c_abi_smoke_required"] is True
assert assets[2]["target"] == "x86_64-unknown-linux-musl"
assert assets[2]["commitment"] == "cli-only"
PY

printf 'compatibility golden: ok\n'
