#!/usr/bin/env sh
set -eu

# Build production release assets (CLI + embed tarballs + sha256 sidecars) for
# a given Nox release tag. Mirrors what was done by hand to recover the v0.0.3
# release after assets were missing on first publish.
#
# Usage:
#   scripts/build-release-assets.sh                  # uses workspace version: v<Cargo.toml [workspace.package].version>
#   scripts/build-release-assets.sh v0.0.4           # explicit tag override
#   NOX_DIST_DIR=/path scripts/build-release-assets.sh  # custom output dir
#   TARGET_TRIPLE=aarch64-apple-darwin scripts/...   # cross-publish naming override
#
# Output: four files under $NOX_DIST_DIR (default /tmp/nox-release-assets-<tag>):
#   nox-cli-<tag>-<triple>.tar.gz       (bin/nox + examples/ minus embed/)
#   nox-cli-<tag>-<triple>.sha256
#   nox-embed-<tag>-<triple>.tar.gz     (lib + include + embed/c_embedding.c + README/CHANGELOG)
#   nox-embed-<tag>-<triple>.sha256
#
# Does NOT push, tag, or call gh release upload. The release operator must
# inspect outputs and upload manually with `gh release upload <tag> <files...>`.
# Builds happen inside an isolated git worktree on the release tag so main HEAD
# is untouched.

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

TARGET_TRIPLE=${TARGET_TRIPLE:-x86_64-unknown-linux-gnu}

if [ $# -gt 0 ]; then
    TAG=$1
else
    VERSION=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
    TAG="v$VERSION"
fi

if ! git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    printf 'build-release-assets: tag %s does not exist; create it first\n' "$TAG" >&2
    exit 1
fi

DIST=${NOX_DIST_DIR:-"/tmp/nox-release-assets-$TAG"}
rm -rf "$DIST"
mkdir -p "$DIST"

WORKTREE=$(mktemp -d "${TMPDIR:-/tmp}/nox-release-build.XXXXXX")
rmdir "$WORKTREE"
git worktree add --detach "$WORKTREE" "$TAG" >/dev/null
trap 'git worktree remove --force "$WORKTREE" >/dev/null 2>&1 || true' EXIT

printf 'build-release-assets: building %s at %s\n' "$TAG" "$WORKTREE"
(cd "$WORKTREE" && cargo build --release -p nox -p nox_core >/dev/null)

LIB="$WORKTREE/target/release/libnox_core.so"
if [ ! -f "$LIB" ]; then
    LIB="$WORKTREE/target/release/libnox_core.dylib"
fi
if [ ! -f "$LIB" ]; then
    printf 'build-release-assets: missing release dynamic library for nox_core under target/release\n' >&2
    exit 1
fi

CLI_NAME="nox-cli-$TAG-$TARGET_TRIPLE"
CLI_DIR="$DIST/$CLI_NAME"
mkdir -p "$CLI_DIR/bin" "$CLI_DIR/examples"
cp "$WORKTREE/target/release/nox" "$CLI_DIR/bin/nox"
(cd "$WORKTREE/examples" && tar c --exclude='./embed' .) | (cd "$CLI_DIR/examples" && tar x)

EMBED_NAME="nox-embed-$TAG-$TARGET_TRIPLE"
EMBED_DIR="$DIST/$EMBED_NAME"
mkdir -p "$EMBED_DIR/lib" "$EMBED_DIR/include" "$EMBED_DIR/examples/embed"
cp "$LIB" "$EMBED_DIR/lib/$(basename "$LIB")"
cp "$WORKTREE/crates/nox_core/include/nox_core.h" "$EMBED_DIR/include/nox_core.h"
cp "$WORKTREE/examples/embed/c_embedding.c" "$EMBED_DIR/examples/embed/c_embedding.c"
cp "$WORKTREE/README.md" "$EMBED_DIR/README.md"
cp "$WORKTREE/README_zh_CN.md" "$EMBED_DIR/README_zh_CN.md"
cp "$WORKTREE/CHANGELOG.md" "$EMBED_DIR/CHANGELOG.md"

(cd "$DIST" && tar czf "$CLI_NAME.tar.gz" "$CLI_NAME")
(cd "$DIST" && tar czf "$EMBED_NAME.tar.gz" "$EMBED_NAME")
(cd "$DIST" && sha256sum "$CLI_NAME.tar.gz" > "$CLI_NAME.sha256")
(cd "$DIST" && sha256sum "$EMBED_NAME.tar.gz" > "$EMBED_NAME.sha256")

rm -rf "$CLI_DIR" "$EMBED_DIR"

printf 'build-release-assets: ok\n'
printf 'build-release-assets: outputs in %s\n' "$DIST"
ls -lh "$DIST"
