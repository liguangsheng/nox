# 从 v0.0.6 迁移到 v0.0.7

`v0.0.7` 是稳定化 release，不扩展新的语言语法或 runtime 能力。主要变化是明确兼容承诺、支持政策、
多平台 CLI smoke 证据，以及更严格的 GitHub/git module lockfile 预期。

## 应继续工作的内容

- 已在 `v0.0.6` 可运行的 `.nox` 脚本应继续 parse、typecheck 和运行，除非 CHANGELOG 明确标记
  pre-1.0 兼容破坏。
- `nox.check.v1`、`nox.test.v1` 和 `nox.project-check.v1` 等 CLI JSON schema 保持兼容扩展。
- `diagnostics.md` 中记录的 diagnostic `code` 是稳定机器可读契约；message 文本可以改进。
- `x86_64-unknown-linux-gnu` 仍是 full SDK release asset 目标，`x86_64-unknown-linux-musl` 仍是
  CLI-only 目标。

## 运维变化

- 稳定边界见 [稳定性与兼容承诺](stability.md)。
- supported versions、EOL、hotfix、withdrawn release 和漏洞响应见 [支持政策](support-policy.md)。
- CI 新增 Linux、macOS 和 Windows host CLI smoke。这只是 CLI-only 证据，不新增 macOS 或 Windows
  release asset。
- GitHub/git package 路线把 `nox.lock` schema version `1`、content hash、cache key 和 offline 行为
  作为 `v0.0.7` 稳定化表面。

## 需要做的事

- 声明 `[dependencies]` 的项目继续提交匹配的 `nox.lock`。
- CI 中需要证明 lockfile/cache 未漂移时，使用 `nox fetch --check` 或 `nox fetch --locked`，避免改写
  项目状态。
- 如果项目用非默认 cache 目录 fetch，后续命令设置 `NOX_MODULE_CACHE` 指向同一目录，尤其是锁网 CI。
- production release 不依赖 branch 或默认分支 dependency；使用完整 `rev` pin，或用 tag 解析后写入
  `nox.lock`。
- macOS 和 Windows CLI smoke 只代表构建证据。在 release asset manifest 明确新增这些平台前，仍按
  source build / best-effort CLI 运行处理。

## 无需迁移的情况

不使用外部 GitHub/git dependency 的 `v0.0.6` 项目不需要改源码。使用 dependency 的项目，在进入
production release 前用 `v0.0.7` CLI 重新生成或检查 `nox.lock`。
