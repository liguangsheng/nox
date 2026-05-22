# 0003 - 不在 v0.0.2 引入第三方依赖

- 状态：已采纳
- 日期：2026-05-20
- 涉及：发布 / 工具链

## 背景

`nox_core` 和 `nox` 这两个 crate 在 v0.0.1 完全没有外部 crate 依赖，构建快、审计
面小、嵌入到宿主时不带额外二进制。v0.0.2 的工作（manifest、LSP、CI、stdlib 扩展）
有几次都可以靠引入 `toml`、`serde_json`、`tower-lsp` 等 crate 直接搞定。

## 决策

- v0.0.2 保持零第三方 runtime 依赖。manifest、JSON 输出、LSP 协议、TOML 解析都
  自己写小子集；只支持当前项目需要的字段。
- 依赖只能进 `dev-dependencies`，且要在 ADR 里说明动机。
- 这条原则只覆盖 v0.0.2 路径。v0.0.3 之后如果发现某个领域必须引入大 crate，再写
  新的 ADR 说明取舍。

## 后果

- 编译时间和构建产物保持小。
- 嵌入宿主仍然是单一 `nox_core` 库 + C ABI。
- 自写解析器有功能上限：例如 manifest 不支持嵌套 inline table，JSON 输出不
  解析回来；遇到具体需求时再补。
- 学习成本和审计成本都低。

## 备选方案

- 现在就引入 `toml` + `serde_json`：被拒绝，会让仓库立刻多出大量传递依赖，
  当前 v0.0.2 不需要这种灵活性。
- 引入 `tower-lsp` 处理 LSP：被拒绝，stdio LSP 已经能用，第三方 LSP 框架体量
  远超当前需求。
