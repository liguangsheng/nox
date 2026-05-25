# 0033 - 平台矩阵与分发第三轮路线

- 状态：已采纳
- 日期：2026-05-25
- 涉及：发布 / 分发 / CI / C ABI

## 背景

`v0.0.5` 已经形成最小生产分发矩阵：

- `x86_64-unknown-linux-gnu`：CLI 和 embedding SDK 都有 release asset、local dist smoke、
  embedding regression 和 release cutover 证据。
- `x86_64-unknown-linux-musl`：GitHub Actions 覆盖 CLI-only cross build 和 smoke；release
  asset manifest 也要求 CLI-only tarball，但还不承诺 embedding SDK。
- 其他 target：源码构建或 best-effort，直到 toolchain、CI、C ABI smoke、asset build、
  download smoke 和 rollback 证据齐备。

阶段 119 的问题不是“再加一个平台名字”，而是避免矩阵文档、脚本和 release checklist
在后续 release 中漂移。当前 `scripts/release-asset-manifest.sh` 只输出 asset basename，
人和脚本需要再从字符串中推断 target、kind 和承诺等级；这对正式 release 审计不够直观。

## 决策

第三轮继续保持当前平台承诺，不新增完整 SDK target：

- full SDK target 仍只有 `x86_64-unknown-linux-gnu`。
- CLI-only target 仍只有 `x86_64-unknown-linux-musl`。
- 不上传没有 target-specific C ABI smoke 证据的 embedding SDK。
- 不把 best-effort target 写成正式二进制承诺。

阶段 120 首选实现是让 release asset manifest 提供机器可读 JSON：

- `scripts/release-asset-manifest.sh --json` 输出 schema、version、tag 和 assets 数组。
- 每个 asset 明确包含 `name`、`kind`、`target`、`commitment` 和 `c_abi_smoke_required`。
- `commitment` 先使用 `full-sdk`、`cli-only` 和后续可扩展值；当前 manifest 只输出前两类。
- 现有纯文本输出保持不变，避免破坏 `release-upload-plan`、`release-asset-smoke`、
  `release-cutover-check` 和历史脚本。
- release toolchain status 可以继续从文本 manifest 工作；后续如有需要再迁移到 JSON。

## 非目标

- 不新增 `aarch64`、macOS、Windows 或 musl embedding SDK 承诺。
- 不让 build 脚本自动安装 toolchain、创建 tag、push、上传 GitHub Release 或修改 release notes。
- 不改变 release asset 命名格式。
- 不把内部 handoff 路线图纳入正式 release 文档。

## 后果

机器可读 manifest 能让 release evidence、upload plan、toolchain status 和人工审计更容易对齐，
同时不扩大平台支持承诺。代价是 release 脚本多维护一个 JSON 输出分支，但它是只读自描述能力，
不会影响已有文本管线。

阶段 120 的完成标准：

- `scripts/release-asset-manifest.sh --json` 输出可解析 JSON，并通过 self-test 覆盖三项当前资产。
- 正式 release checklist 说明 JSON manifest 的用途，并继续强调 full SDK / CLI-only 边界。
- CHANGELOG 记录 release tooling 变化。
- 运行 `scripts/release-toolchain-status.sh --self-test`、`scripts/release-asset-manifest.sh --self-test`、
  Markdown link check 和 `git diff --check`。

## 备选方案

- 直接新增 `aarch64-unknown-linux-gnu` 完整 SDK。未选择，因为当前缺少对应 CI、C ABI smoke、
  release asset smoke 和下载后验证证据。
- 承诺 musl embedding SDK。未选择，因为当前只覆盖 CLI cross smoke，C ABI 动态库和下游链接
  边界尚未形成 target-specific 证据。
- 只依赖文档表格。未选择，因为 release 工具已经依赖 asset manifest，继续让机器从文件名推断
  承诺等级会增加审计漂移风险。
