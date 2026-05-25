# Stability and Compatibility

This page defines the public-surface status for the `v0.0.x` release line. Nox
is still pre-1.0, but production releases must not leave downstream users
guessing which behavior is stable, experimental, deferred, or internal.

## Status Tags

- **Stable**: backwards-compatible changes are expected through the `v0.0.x`
  line. Additive fields, new helpers, and new diagnostics are allowed when old
  consumers can ignore them.
- **Stable, permissioned**: stable only when the caller explicitly grants the
  required runtime capability. The default behavior remains deny-by-default.
- **Experimental**: available for real use, but the signature, semantics, or
  tooling surface may change in a later `v0.0.x` release. Experimental entries
  must be labelled in docs or ADRs.
- **Deferred**: intentionally not part of the current release line. Restarting a
  deferred item requires a design note or ADR, tests, documentation, and release
  gate updates.
- **Internal**: implementation detail. Users and embedders must not depend on it.

## Stability Matrix

| Surface | Status | Compatibility rule | Evidence |
| --- | --- | --- | --- |
| `.nox` core syntax documented in `language-v0.md` | Stable | Existing accepted programs should keep parsing and typechecking unless a CHANGELOG entry marks a pre-1.0 compatibility break. | Language tests, fixture tests, formatter golden, release gate. |
| Parser/typechecker/VM implementation details | Internal | AST internals, bytecode instruction layout, verifier internals, and heap layout are not public contracts unless a CLI/API exposes them. | Unit tests only; no downstream contract. |
| `nox run`, `check`, `test`, `fmt`, `project check`, `lsp`, `inspect-bytecode` | Stable | Subcommands, exit-code meanings, and documented flags require release-note coverage for behavior changes. | CLI tests, product-shape guardrail, release gate. |
| CLI JSON schemas `nox.check.v1`, `nox.test.v1`, `nox.project-check.v1` | Stable | Additive fields are compatible; removing or changing field meaning is a breaking change requiring migration notes. | CLI JSON tests, compatibility golden. |
| Coverage/profile/trace JSON or NDJSON schemas | Stable where documented | Additive event fields are compatible; event renames or required-field removals need migration notes. | CLI tests and docs. |
| Diagnostic `code` values documented in `diagnostics.md` | Stable | Tools should match `code`, not message text. Code removal or semantic reuse is breaking. Message wording may improve. | Diagnostics tests, LSP parity tests. |
| LSP diagnostics | Stable for diagnostic parity | Diagnostic codes and ranges must stay aligned with CLI checks where the same source is analyzed. Capability additions are additive. | LSP integration tests, compatibility golden. |
| LSP IDE features beyond diagnostics | Experimental unless documented otherwise | Completion, hover, signature help, rename, semantic tokens, and code actions may improve conservatively; schema or capability changes need docs and tests. | LSP tests. |
| Rust `nox_core` API | Stable for documented embedding paths | Public documented types, host registration, ownership behavior, and error reporting require compatibility notes for changes. | Rust API tests, embedding regression. |
| Rust `nox` runtime API | Stable where documented | `Runtime`, `RuntimePermissions`, mocks, and project helpers require migration notes for behavior changes. | Runtime tests, embedding examples. |
| C ABI in `crates/nox_core/include/nox_core.h` | Stable | Enum values, function signatures, handle ownership, string lifetime, callback threading, and last-error rules are compatibility contracts. | C ABI tests, enum stability tests, C embedding smoke. |
| Standard library entries tagged stable in `stdlib-index.md` | Stable | Signatures, return types, permission requirements, and error models should remain compatible. | Stdlib surface fixture, runtime tests. |
| Standard library entries tagged stable, permissioned | Stable, permissioned | Stable only with explicit capability. Missing capability must remain a deterministic denial. | Permission tests, runtime docs. |
| Standard library entries tagged experimental | Experimental | Use is allowed, but changes can occur in `v0.0.x` with CHANGELOG coverage. | Stdlib docs and tests. |
| GitHub/git URL modules and `nox.lock` | Stable enough for `v0.0.7` hardening | Existing lockfile behavior should be preserved while the schema and drift diagnostics are formalized. | Fetch/project-check tests. |
| Release tarball names, `.sha256` sidecars, and asset manifest JSON | Stable for published releases | Published assets must remain downloadable; repair or withdrawal requires release notes. | Asset smoke, cutover check. |

## Deferred Surfaces

These are not stable public commitments in `v0.0.7`: self-hosted registry,
crates.io publish, full multi-platform SDK matrix, TLS/HTTPS, database drivers,
trait objects, dynamic dispatch, associated types, blanket impls, built-in macro
system, import-time codegen, IO reactor, multi-threaded async runtime, top-level
await, async traits, C ABI task handles, full YAML/XML/protobuf, large streaming
writers, installers, Docker images, CI action, SBOM/signing, and performance
trend dashboards.

Deferred items may appear in ADRs as future options. They must not be described
as supported behavior until their design, tests, docs, and release gate evidence
land.

## Change Rules

- Any stable public-surface change must update `CHANGELOG.md`.
- Any new CLI JSON field or diagnostic code must include tests and docs.
- Any C ABI change must preserve enum values and handle ownership rules or be
  documented as a breaking pre-1.0 change with migration steps.
- Any new permissioned runtime helper must default to denied, document the
  capability, and include positive and negative tests.
- Experimental features must be labelled in the relevant docs before release.
- Internal refactors do not need user documentation unless they change a public
  surface.

## Release Audit Expectations

Before a production release, maintainers must run the release gate, local
distribution smoke, strict cutover check, and strict release audit described in
the release checklist. `v0.0.7` additionally prioritizes making this matrix
machine-checkable in compatibility and release guardrails.
