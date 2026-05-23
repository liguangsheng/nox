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

Release assets are split by audience:

- `nox-cli-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` for CLI users.
- `nox-embed-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` for embedding hosts.

Each asset must have a matching `.sha256` file. Current release assets only commit to `x86_64-unknown-linux-gnu`.

To produce both tarballs and their `.sha256` sidecars in one step, run `scripts/build-release-assets.sh` after the release tag is pushed; it builds the release in an isolated git worktree on the tag and writes four files to `/tmp/nox-release-assets-<tag>/` ready for `gh release upload <tag> <files...>`. This is a required release step — releases without binary assets force downstream users to build from source.

If a release must be withdrawn, keep the historical tag and release commit, mark the GitHub Release as withdrawn or deprecated, publish a hotfix version, and document the downstream upgrade path.

Production release final review also covers a quantitative metrics appendix that maps the long-term production claims (small / fast / host-friendly / practical) to release-time measurable indicators, plus product-shape non-regression and deferred-item guards. The full mapping with measurement commands is maintained internally by the agent plan; a release-time summary is in the Chinese release checklist appendix below.

The detailed Chinese release checklist is available in [`../zh_CN/release-checklist.md`](../zh_CN/release-checklist.md).
