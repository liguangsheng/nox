# Migrating from v0.0.6 to v0.0.7

`v0.0.7` is a stabilization release. It does not add new language syntax or
runtime capabilities. The main changes are clearer compatibility promises,
support policy, multi-platform CLI smoke evidence, and stricter GitHub/git
module lockfile expectations.

## What Should Keep Working

- Existing `.nox` scripts that run on `v0.0.6` should keep parsing,
  typechecking, and running unless the CHANGELOG calls out a pre-1.0
  compatibility break.
- CLI JSON schemas such as `nox.check.v1`, `nox.test.v1`, and
  `nox.project-check.v1` remain additive-compatible.
- Diagnostic `code` values documented in `diagnostics.md` remain the stable
  machine-readable contract; message text may improve.
- `x86_64-unknown-linux-gnu` remains the full SDK release asset target, and
  `x86_64-unknown-linux-musl` remains CLI-only.

## What Changed Operationally

- Stability boundaries now live in [Stability and compatibility](stability.md).
- Support, EOL, hotfix, withdrawn release, and security response rules now live
  in [Support policy](support-policy.md).
- CI now includes Linux, macOS, and Windows host CLI smoke. This is CLI-only
  evidence and does not create macOS or Windows release assets.
- The GitHub/git package route treats `nox.lock` schema version `1`, content
  hashes, cache keys, and offline behavior as a `v0.0.7` hardening surface.

## Action Items

- Keep committed `nox.lock` files for projects that declare `[dependencies]`.
- Use `nox fetch --check` or `nox fetch --locked` in CI when you need to prove
  the lockfile and cache still match without rewriting project state.
- Set `NOX_MODULE_CACHE` when a project was fetched with a non-default cache
  directory, especially in locked-down CI.
- Do not rely on branch or default-branch dependencies for production releases;
  use full `rev` pins or tags resolved into `nox.lock`.
- Treat macOS and Windows CLI smoke as build evidence only. Build from source on
  those platforms until release assets are explicitly added to the manifest.

## No Migration Needed

No source rewrite is required for `v0.0.6` projects that do not use external
GitHub/git dependencies. For projects that do use dependencies, regenerate or
check `nox.lock` with the `v0.0.7` CLI before cutting a production release.
