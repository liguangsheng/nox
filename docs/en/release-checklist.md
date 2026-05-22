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

If a release must be withdrawn, keep the historical tag and release commit, mark the GitHub Release as withdrawn or deprecated, publish a hotfix version, and document the downstream upgrade path.

Production release final review also covers a quantitative metrics appendix that maps the long-term production claims (small / fast / host-friendly / practical) to release-time measurable indicators, plus product-shape non-regression and deferred-item guards. The full mapping with measurement commands is maintained internally by the agent plan; a release-time summary is in the Chinese release checklist appendix below.

The detailed Chinese release checklist is available in [`../zh_CN/release-checklist.md`](../zh_CN/release-checklist.md).
