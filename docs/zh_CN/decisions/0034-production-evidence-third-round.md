# 0034 - 生产证据第三轮路线

- 状态：已采纳
- 日期：2026-05-25
- 涉及：发布 / 证据 / 回归 / 工具链

## 背景

Nox 的 release gate 已经覆盖 cargo test、clippy、compatibility golden、embedding regression、
robustness smoke、benchmark smoke、stdlib surface、health-check project、release tool self-test
和 release audit checkpoint mode。当前缺口不在于缺少更多功能测试，而在于 release operator
需要把同一轮候选的证据整理成稳定、可复核的报告。

`scripts/release-evidence-report.sh` 已经汇总 cutover status JSON、toolchain status JSON、
required assets 文本列表和 command plan。阶段 120 又让 `scripts/release-asset-manifest.sh --json`
能输出 asset kind、target、commitment 和 C ABI smoke 边界，但 evidence report 尚未包含这份
机器可读矩阵。

## 决策

第三轮生产证据不新增 runtime、语言或 stdlib 能力，只增强 release 证据聚合：

- 阶段 122 首选把 `scripts/release-asset-manifest.sh --json` 纳入 `release-evidence-report`。
- evidence report 同时保留文本 Required Assets 列表，保证人类 review 和旧脚本语境可读。
- JSON manifest 作为独立 fenced block 输出，schema 保持 `nox.release-asset-manifest.v1`。
- 不让 evidence report 构建资产、安装 toolchain、联网、tag、push、创建 GitHub Release 或上传文件。

## 非目标

- 不增加新的 smoke 类别、性能阈值或 runtime trace schema。
- 不改变 release gate 的 checkpoint blocker 语义。
- 不把远端 CI 查询、GitHub API 调用或 release upload 行为放入 evidence report。
- 不把 release evidence report 变成唯一真实来源；CHANGELOG、release checklist、tag、CI 和
  GitHub Release 仍各自是对应事实的来源。

## 后果

release 候选审阅时，operator 可以在同一份报告里看到 cutover 状态、toolchain 状态、资产矩阵、
必需资产文本列表和命令计划。代价是报告稍长，但它仍是只读脚本，失败模式只影响证据生成，不影响
runtime 或用户脚本执行。

阶段 122 的完成标准：

- `scripts/release-evidence-report.sh` 输出 `## Release Asset Manifest JSON` 段。
- self-test 覆盖 `nox.release-asset-manifest.v1` 和当前三项资产。
- release gate 继续通过。

## 备选方案

- 把 JSON manifest 只放在 toolchain status 中。未选择，因为 toolchain status 关注本地 Rust target
  是否安装，不应同时承担资产承诺矩阵的来源职责。
- 增加新的 robustness corpus。未选择，因为阶段 121 的目标是证据聚合，不是扩大 malformed 输入面。
- 让 evidence report 调 GitHub 检查 CI。未选择，因为当前 release 脚本保持本地只读，远端 CI evidence
  由 release operator 显式传入。
