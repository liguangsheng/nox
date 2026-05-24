#!/usr/bin/env sh
set -eu

# Build production release assets (CLI + embed tarballs + sha256 sidecars) for
# a given Nox release tag. Mirrors what was done by hand to recover the v0.0.3
# release after assets were missing on first publish.
#
# Usage:
#   scripts/build-release-assets.sh                  # uses workspace version: v<Cargo.toml [workspace.package].version>
#   scripts/build-release-assets.sh v0.0.4           # explicit tag override
#   NOX_RELEASE_TAG=v0.0.4 scripts/build-release-assets.sh  # environment tag override
#   NOX_RELEASE_ASSET_DIR=/path scripts/build-release-assets.sh  # custom output dir
#   NOX_DIST_DIR=/path scripts/build-release-assets.sh            # legacy custom output dir
#   TARGET_TRIPLES="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu" scripts/...
#   TARGET_TRIPLES="" CLI_ONLY_TARGET_TRIPLES="x86_64-unknown-linux-musl" scripts/...
#   CLI_ONLY_TARGET_TRIPLES="x86_64-unknown-linux-musl" scripts/...
#
# Output: four files per target under $NOX_RELEASE_ASSET_DIR (default /tmp/nox-release-assets-<tag>):
#   nox-cli-<tag>-<triple>.tar.gz       (bin/nox + examples/ minus embed/)
#   nox-cli-<tag>-<triple>.sha256
#   nox-embed-<tag>-<triple>.tar.gz     (lib + include + embed/c_embedding.c + README/CHANGELOG)
#   nox-embed-<tag>-<triple>.sha256
# CLI_ONLY_TARGET_TRIPLES builds only nox-cli tarballs for targets whose C ABI
# dynamic library is not part of the formal binary SDK commitment yet.
#
# Does NOT push, tag, or call gh release upload. The release operator must
# inspect outputs and upload manually with `gh release upload <tag> <files...>`.
# Builds happen inside an isolated git worktree on the release tag so main HEAD
# is untouched.

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

if [ "${1:-}" = "--self-test" ]; then
    tag_from_env=$(NOX_RELEASE_TAG=v0.0.5 sh -eu -c 'if [ $# -gt 0 ]; then tag=$1; elif [ -n "${NOX_RELEASE_TAG:-}" ]; then tag=$NOX_RELEASE_TAG; else tag=v0.0.4; fi; printf "%s" "$tag"')
    tag_from_arg=$(NOX_RELEASE_TAG=v0.0.5 sh -eu -c 'if [ $# -gt 0 ]; then tag=$1; elif [ -n "${NOX_RELEASE_TAG:-}" ]; then tag=$NOX_RELEASE_TAG; else tag=v0.0.4; fi; printf "%s" "$tag"' sh v0.0.6)
    default_dist=$(env -u NOX_RELEASE_ASSET_DIR -u NOX_DIST_DIR sh -eu -c 'TAG=v0.0.5; printf "%s" "${NOX_RELEASE_ASSET_DIR:-${NOX_DIST_DIR:-"/tmp/nox-release-assets-$TAG"}}"')
    legacy_dist=$(env -u NOX_RELEASE_ASSET_DIR NOX_DIST_DIR=/tmp/nox-legacy-assets sh -eu -c 'TAG=v0.0.5; printf "%s" "${NOX_RELEASE_ASSET_DIR:-${NOX_DIST_DIR:-"/tmp/nox-release-assets-$TAG"}}"')
    release_dist=$(NOX_RELEASE_ASSET_DIR=/tmp/nox-release-assets NOX_DIST_DIR=/tmp/nox-legacy-assets sh -eu -c 'TAG=v0.0.5; printf "%s" "${NOX_RELEASE_ASSET_DIR:-${NOX_DIST_DIR:-"/tmp/nox-release-assets-$TAG"}}"')
    [ "$default_dist" = "/tmp/nox-release-assets-v0.0.5" ] || {
        printf 'build-release-assets: self-test default dist mismatch: %s\n' "$default_dist" >&2
        exit 1
    }
    [ "$legacy_dist" = "/tmp/nox-legacy-assets" ] || {
        printf 'build-release-assets: self-test legacy dist mismatch: %s\n' "$legacy_dist" >&2
        exit 1
    }
    [ "$release_dist" = "/tmp/nox-release-assets" ] || {
        printf 'build-release-assets: self-test release dist mismatch: %s\n' "$release_dist" >&2
        exit 1
    }
    [ "$tag_from_env" = "v0.0.5" ] || {
        printf 'build-release-assets: self-test env tag mismatch: %s\n' "$tag_from_env" >&2
        exit 1
    }
    [ "$tag_from_arg" = "v0.0.6" ] || {
        printf 'build-release-assets: self-test arg tag mismatch: %s\n' "$tag_from_arg" >&2
        exit 1
    }
    printf 'build-release-assets: self-test ok\n'
    exit 0
fi

HOST_TRIPLE=$(rustc -vV | awk '/^host: /{print $2; exit}')
if [ "${TARGET_TRIPLES+x}" = x ]; then
    TARGET_TRIPLES=$TARGET_TRIPLES
elif [ "${TARGET_TRIPLE+x}" = x ]; then
    TARGET_TRIPLES=$TARGET_TRIPLE
else
    TARGET_TRIPLES=$HOST_TRIPLE
fi
CLI_ONLY_TARGET_TRIPLES=${CLI_ONLY_TARGET_TRIPLES:-}

if [ $# -gt 0 ]; then
    TAG=$1
elif [ -n "${NOX_RELEASE_TAG:-}" ]; then
    TAG=$NOX_RELEASE_TAG
else
    VERSION=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
    TAG="v$VERSION"
fi

if ! git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    printf 'build-release-assets: tag %s does not exist; create it first\n' "$TAG" >&2
    exit 1
fi

DIST=${NOX_RELEASE_ASSET_DIR:-${NOX_DIST_DIR:-"/tmp/nox-release-assets-$TAG"}}
rm -rf "$DIST"
mkdir -p "$DIST"

WORKTREE=$(mktemp -d "${TMPDIR:-/tmp}/nox-release-build.XXXXXX")
rmdir "$WORKTREE"
git worktree add --detach "$WORKTREE" "$TAG" >/dev/null
trap 'git worktree remove --force "$WORKTREE" >/dev/null 2>&1 || true' EXIT

printf 'build-release-assets: building %s at %s\n' "$TAG" "$WORKTREE"
for TARGET_TRIPLE in $TARGET_TRIPLES; do
    printf 'build-release-assets: target %s\n' "$TARGET_TRIPLE"
    if [ "$TARGET_TRIPLE" = "$HOST_TRIPLE" ]; then
        (cd "$WORKTREE" && cargo build --release -p nox -p nox_core >/dev/null)
        BUILD_DIR="$WORKTREE/target/release"
    else
        (cd "$WORKTREE" && cargo build --release --target "$TARGET_TRIPLE" -p nox -p nox_core >/dev/null)
        BUILD_DIR="$WORKTREE/target/$TARGET_TRIPLE/release"
    fi

    LIB="$BUILD_DIR/libnox_core.so"
    if [ ! -f "$LIB" ]; then
        LIB="$BUILD_DIR/libnox_core.dylib"
    fi
    if [ ! -f "$LIB" ]; then
        printf 'build-release-assets: missing release dynamic library for nox_core under %s\n' "$BUILD_DIR" >&2
        exit 1
    fi

    CLI_NAME="nox-cli-$TAG-$TARGET_TRIPLE"
    CLI_DIR="$DIST/$CLI_NAME"
    mkdir -p "$CLI_DIR/bin" "$CLI_DIR/examples"
    cp "$BUILD_DIR/nox" "$CLI_DIR/bin/nox"
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
done

for TARGET_TRIPLE in $CLI_ONLY_TARGET_TRIPLES; do
    printf 'build-release-assets: cli-only target %s\n' "$TARGET_TRIPLE"
    (cd "$WORKTREE" && cargo build --release --target "$TARGET_TRIPLE" -p nox >/dev/null)
    BUILD_DIR="$WORKTREE/target/$TARGET_TRIPLE/release"

    CLI_NAME="nox-cli-$TAG-$TARGET_TRIPLE"
    CLI_DIR="$DIST/$CLI_NAME"
    mkdir -p "$CLI_DIR/bin" "$CLI_DIR/examples"
    cp "$BUILD_DIR/nox" "$CLI_DIR/bin/nox"
    (cd "$WORKTREE/examples" && tar c --exclude='./embed' .) | (cd "$CLI_DIR/examples" && tar x)
    cp "$WORKTREE/README.md" "$CLI_DIR/README.md"
    cp "$WORKTREE/README_zh_CN.md" "$CLI_DIR/README_zh_CN.md"
    cp "$WORKTREE/CHANGELOG.md" "$CLI_DIR/CHANGELOG.md"

    (cd "$DIST" && tar czf "$CLI_NAME.tar.gz" "$CLI_NAME")
    (cd "$DIST" && sha256sum "$CLI_NAME.tar.gz" > "$CLI_NAME.sha256")
    rm -rf "$CLI_DIR"
done

printf 'build-release-assets: ok\n'
printf 'build-release-assets: outputs in %s\n' "$DIST"
ls -lh "$DIST"
