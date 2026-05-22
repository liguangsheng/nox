# Diagnostics

Nox diagnostics carry human-readable messages and machine-readable `code` values. The same codes are used across CLI JSON output and LSP diagnostics where applicable.

Important code families include:

- `parse.*` and `type.*` for frontend errors.
- `module.*` for import and module visibility failures.
- `manifest.invalid` and `project.discovery` for project configuration problems.
- `permission.denied` for runtime capability denials.
- `host.callback` for host callback failures without a more specific host-provided code.
- `bytecode.verifier` for malformed bytecode rejection.

Tooling should prefer diagnostic codes over matching message text.

The detailed Chinese diagnostics reference is available in [`../zh_CN/diagnostics.md`](../zh_CN/diagnostics.md).
