# Support and Security Policy

This page defines the maintenance process for the `v0.0.x` production release
line. It complements the release checklist: the checklist says how to cut a
release, while this policy says how long releases are supported and how fixes,
withdrawals, and vulnerability response are handled.

## Supported Versions

Nox supports the latest production release in the `v0.0.x` line. After a new
production release is published and its release audit passes, the previous
production release enters security-fix-only support for one patch cycle. Older
`v0.0.x` releases are end-of-life (EOL) unless a release note explicitly extends
support.

Alpha, beta, release-candidate, local checkpoint, and unreleased `main` builds
are not supported production versions. They may receive fixes on `main`, but
they do not get hotfix branches or asset repair promises.

## Security Response

Security-sensitive reports should use a private channel before public issue
discussion. Prefer GitHub Security Advisories for the repository when available.
If that channel is unavailable, open a minimal public issue that asks for a
private maintainer contact without including exploit details, secrets, or a
working proof of compromise.

Maintainers should triage reports against these boundaries:

- Runtime capabilities must remain deny-by-default.
- File, network, environment, timer, async task, process, and host callback
  behavior must not exceed documented permissions.
- C ABI ownership, string lifetime, callback re-entry, and last-error behavior
  are compatibility and safety boundaries.
- CLI JSON, diagnostic codes, and release assets are user-facing contracts; a
  security fix that changes them needs a migration note.

Confirmed high-severity vulnerabilities should block the next production
release until fixed or explicitly withdrawn from the release scope. If the
latest production release is affected, publish a hotfix patch release after the
local release gate, local distribution smoke, remote CI, asset smoke, and
strict release audit pass.

## Hotfixes

A hotfix release must:

- Keep the affected historical tag and release commit intact.
- Use the next patch version.
- Document the affected versions, fix summary, compatibility impact, and
  downstream upgrade path in `CHANGELOG.md` and GitHub Release notes.
- Rebuild and smoke release assets instead of modifying existing tarballs in
  place.
- Re-run `scripts/release-gate.sh`, `scripts/local-dist-smoke.sh`, and
  `NOX_RELEASE_CI_EVIDENCE=<CI run URL or id> scripts/release-audit.sh`.

Hotfixes should be narrow. Feature work, platform expansion, and experimental
surface changes should wait for the normal release train unless they are
required to remove the vulnerability.

## Withdrawn Releases

If a production release is unsafe to use:

- Keep the git tag and release commit; do not force-push or replace a published
  tag.
- Mark the GitHub Release as withdrawn or deprecated.
- Add a CHANGELOG note that names the withdrawn version, impact, and replacement
  version.
- Publish a hotfix release when code or assets changed.
- Tell downstream users whether they need to clear module cache, regenerate
  `nox.lock`, rebuild C bindings, or replace downloaded assets.

Deleting a GitHub Release is reserved for legal requirements or exposed
secrets. Normal correctness, compatibility, or packaging failures should use
withdrawal plus hotfix.

## Release Train

Normal releases move through these states:

1. `main` candidate work with `[workspace.package].version` still at the latest
   production version and new changes under `[未发布]`.
2. Release-prep commit produced by `scripts/prepare-release-version.sh`.
3. Local gate, local distribution smoke, remote CI, release assets, and strict
   audit evidence.
4. Git tag, GitHub Release, release notes from CHANGELOG, assets, and sha256
   sidecars.
5. Post-release audit confirming tag, assets, CI evidence, and rollback notes.

No script in the normal local release gate pushes, tags, creates a GitHub
Release, or uploads assets.

The detailed Chinese support policy is available in
[`../zh_CN/support-policy.md`](../zh_CN/support-policy.md).
