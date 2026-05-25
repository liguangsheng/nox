# 支持与安全政策

本文定义 `v0.0.x` production release 线的维护流程。它补充 release checklist：checklist
说明怎样发布，本文说明版本支持窗口、hotfix、撤回 release 和漏洞响应如何处理。

## 支持版本

Nox 支持 `v0.0.x` 线上的最新 production release。新的 production release 发布并通过 release
audit 后，上一个 production release 进入一个 patch 周期的 security-fix-only 支持窗口。更早的
`v0.0.x` release 默认 EOL，除非 release notes 明确延长支持。

alpha、beta、release candidate、本地 checkpoint 和未发布的 `main` 构建不是受支持的 production
版本。它们可以在 `main` 上获得修复，但不承诺 hotfix 分支或资产修复。

## 漏洞响应

安全敏感报告应先走私有渠道，不要直接公开利用细节。优先使用仓库的 GitHub Security Advisories。
如果该渠道不可用，可以开一个最小公开 issue 请求维护者提供私有联系方式，但不要包含 exploit 细节、
凭据或可直接复现入侵的材料。

维护者 triage 时按以下边界判断风险：

- Runtime capability 必须保持默认拒绝。
- 文件、网络、环境、定时器、异步任务、进程和 host callback 行为不得越过文档化权限。
- C ABI ownership、字符串生命周期、callback 重入和 last-error 行为同时是兼容边界和安全边界。
- CLI JSON、diagnostic code 和 release asset 是用户可见契约；安全修复如果改变这些表面，需要迁移说明。

确认的高危漏洞会阻断下一次 production release，直到修复或从 release scope 明确撤回。如果最新
production release 受影响，修复后发布 hotfix patch release，并重新通过本地 release gate、本地
分发 smoke、远端 CI、asset smoke 和 strict release audit。

## Hotfix

Hotfix release 必须：

- 保留受影响历史 tag 和 release commit。
- 使用下一个 patch 版本。
- 在 `CHANGELOG.md` 和 GitHub Release notes 中记录受影响版本、修复摘要、兼容影响和下游升级路径。
- 重新构建并 smoke release assets，不原地修改已发布 tarball。
- 重新运行 `scripts/release-gate.sh`、`scripts/local-dist-smoke.sh` 和
  `NOX_RELEASE_CI_EVIDENCE=<CI run URL or id> scripts/release-audit.sh`。

Hotfix 应保持范围收敛。功能开发、平台扩展和实验表面变化应留到正常 release train，除非它们是移除漏洞
所必需的改动。

## 撤回 Release

如果某个 production release 不安全或不适合继续使用：

- 保留 git tag 和 release commit；不要 force-push 或替换已发布 tag。
- 在 GitHub Release 中标记 withdrawn 或 deprecated。
- 在 CHANGELOG 中增加说明，写清撤回版本、影响范围和替代版本。
- 如果代码或资产发生变化，发布 hotfix release。
- 告知下游是否需要清理 module cache、重新生成 `nox.lock`、重建 C bindings 或替换已下载资产。

删除 GitHub Release 只用于法律要求或泄露凭据等情况。普通正确性、兼容性或打包问题应使用撤回加 hotfix。

## Release Train

正常 release 经过这些状态：

1. `main` 上进行候选工作，`[workspace.package].version` 仍保持最新 production 版本，新变更写在
   `[未发布]`。
2. 由 `scripts/prepare-release-version.sh` 生成 release-prep commit。
3. 通过本地 gate、本地分发 smoke、远端 CI、release assets 和 strict audit 证据。
4. 创建 git tag、GitHub Release、来自 CHANGELOG 的 release notes、assets 和 sha256 sidecar。
5. 发布后 audit 确认 tag、assets、CI evidence 和 rollback 说明。

正常本地 release gate 中的脚本不会 push、tag、创建 GitHub Release 或上传 assets。
