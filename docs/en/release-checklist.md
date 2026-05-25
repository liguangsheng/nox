# Release Checklist

Nox releases must keep version identity and evidence aligned:

- Cargo workspace version.
- `nox --version`.
- git tag.
- CHANGELOG section.
- release checklist state.
- GitHub Release notes and assets.
- local release gate and distribution smoke.
- remote CI evidence for the release commit.
- for projects that declare `[dependencies]`, a committed matching `nox.lock`;
  the release gate rejects tracked manifests with dependencies but no lockfile.
- module ecosystem regression evidence for project check lockfile JSON,
  `nox fetch` offline/cache behavior, external import cache/hash diagnostics,
  and integrated `nox lsp` external import diagnostics.
- compatibility golden evidence for parser AST shape, CLI diagnostic JSON, LSP
  diagnostic JSON, `nox doc` output, project lockfile JSON, host-metadata API
  JSON, C ABI enum values, and async Rust API task behavior.

## Current Checkpoint State

The latest production release is `v0.0.6`. The next release candidate starts at
the next patch version, but candidate audits on `main` keep `[workspace.package].version` at
the previous prepared version until the dedicated release-prep commit. In this
state, `CHANGELOG.md` keeps the next candidate changes under `[未发布]`;
`scripts/release-candidate-readiness.sh` verifies that this intentional
pre-release identity is still documented and that experimental/deferred surfaces
are not described as stable commitments.

Release assets are split by audience and platform commitment:

- `nox-cli-vX.Y.Z-<target-triple>.tar.gz` for CLI users.
- `nox-embed-vX.Y.Z-<target-triple>.tar.gz` for embedding hosts.

Each asset must have a matching `.sha256` file. `x86_64-unknown-linux-gnu` is
the supported binary target for both CLI and embedding SDK assets. The
`x86_64-unknown-linux-musl` target is a CI-smoked CLI-only target; do not upload
an embedding SDK for it until the C ABI has target-specific smoke evidence.
Other targets are source-build-only or best-effort until their toolchain,
artifact build, and smoke evidence are documented here.

To produce both tarballs and their `.sha256` sidecars in one step, run `scripts/build-release-assets.sh` after the release tag is pushed; it builds the release in an isolated git worktree on the tag and writes four files per full SDK target to `/tmp/nox-release-assets-<tag>/` ready for GitHub Release upload. Before that build, run `scripts/release-toolchain-status.sh` to confirm the local Rust targets required by the release asset manifest, especially the CLI-only `x86_64-unknown-linux-musl` target. Set `NOX_RELEASE_TAG=<tag>` when not passing the tag as an argument. Set `NOX_RELEASE_ASSET_DIR=/tmp/nox-release-assets-<tag>` when using a non-default output directory, and pass the same value to `scripts/release-upload-plan.sh` to print the exact `gh release upload <tag> <files...>` command after inspecting the assets. Run `scripts/release-asset-smoke.sh` against the same asset directory before upload, and again against downloaded GitHub Release assets if the release is repaired or mirrored. The smoke verifies each `.sha256`, extracts each tarball, runs host-compatible CLI assets against `examples/hello.nox`, and compiles the host-compatible embed C ABI package. By default `build-release-assets.sh` builds the current Rust host triple. Set `TARGET_TRIPLES="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu ..."` only after the target toolchains and C ABI smoke coverage are available. Set `CLI_ONLY_TARGET_TRIPLES="x86_64-unknown-linux-musl"` to additionally produce CLI-only assets for CI-smoked targets whose embedding SDK is not committed yet. Use `TARGET_TRIPLES=""` when intentionally producing only CLI-only assets for a verification run. This is a required release step — releases without binary assets force downstream users to build from source.
`scripts/release-asset-manifest.sh --json` prints the same required assets with
machine-readable `kind`, `target`, `commitment`, and `c_abi_smoke_required`
fields for evidence reports and manual audit; the default text output remains
the compatibility source for upload and cutover scripts.

Use `NOX_RELEASE_VERSION=<version> scripts/release-notes.sh` to generate GitHub
Release notes from the matching CHANGELOG section. Do not rewrite them by hand;
CHANGELOG remains the single source of release notes. Use
`scripts/release-command-plan.sh` to print the full Phase 77 command sequence
before running any authorized commit, tag, push, asset upload, or strict audit
step. Use `scripts/release-evidence-report.sh` after the release-prep commit
and again after assets exist to capture the cutover status JSON, toolchain
status JSON, required asset manifest, and command plan in one reviewable report.

For the dedicated release-prep commit, run
`scripts/prepare-release-version.sh <version> YYYY-MM-DD` to update the workspace
version, `nox_core` dependency version, CHANGELOG release heading, README
version identity, and `Cargo.lock`. The script does not commit, tag, push, build
assets, or upload a GitHub Release. Before making that diff, run
`scripts/prepare-release-version.sh --check-only <version> YYYY-MM-DD` to verify the
expected release-prep anchors without editing files. Run
`scripts/release-prep-dry-run.sh` before the real release-prep commit to apply
the same version switch in a temporary copy and verify cutover readiness plus
release notes extraction without changing the current checkout.

If a release must be withdrawn, keep the historical tag and release commit, mark the GitHub Release as withdrawn or deprecated, publish a hotfix version, and document the downstream upgrade path.

## crates.io Preflight

Publishing to crates.io is deferred for this release line. The module ecosystem
uses pinned GitHub/git URLs rather than a package registry, and the Rust crates
do not currently have available registry names under this project's ownership:
`nox` is occupied by another project, while crates.io resolves `nox_core` to the
existing `nox-core` crate. GitHub tag installs and release tarballs remain the
supported distribution paths.

If a future release reopens registry publishing, remember that crates.io
versions cannot be overwritten. Before publishing `nox_core` or `nox`:

- Confirm both crates have complete package metadata: description, repository,
  readme, keywords, categories, license, and version.
- Confirm registry name availability or ownership for both crates. The current
  `nox` and `nox_core` package names are not publishable commitments for this
  project.
- Confirm `nox` depends on `nox_core` with both a local `path` and the exact
  workspace version so local development and registry publishing resolve the
  same public API.
- Re-audit SemVer risk for Rust API, C ABI, CLI JSON, diagnostic codes,
  manifest behavior, and documented permissions. Breaking public changes must be
  called out in the CHANGELOG and release notes before the crate is published.
- Run `cargo publish --dry-run -p nox_core`, then
  `cargo publish --dry-run -p nox`. In an uncommitted working tree,
  `--allow-dirty` is only for preflight evidence. Treat `nox` dry-run failure
  on a missing registry `nox_core` dependency as expected until the core crate
  has a resolved registry name and published version.

Production release final review also covers a quantitative metrics appendix that maps the long-term production claims (small / fast / host-friendly / practical) to release-time measurable indicators, plus product-shape non-regression and deferred-item guards. The full mapping with measurement commands is maintained internally by the agent plan; a release-time summary is in the Chinese release checklist appendix below.

The detailed Chinese release checklist is available in [`../zh_CN/release-checklist.md`](../zh_CN/release-checklist.md).
