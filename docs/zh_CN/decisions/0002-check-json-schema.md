# 0002 - check --json schema 稳定化

- 状态：已采纳
- 日期：2026-05-20
- 涉及：工具链 / 对外 ABI

## 背景

v0.0.1 的 `nox check --json` 只输出 `{ok, diagnostics}`，工具消费者只能靠 substring
匹配判断结构。随着 LSP、CI、外部脚本接入，需要一份稳定可演进的输出结构。

## 决策

- 输出顶层加入 `schema` 字段，当前固定为 `"nox.check.v1"`。
- 在保留 `ok` / `diagnostics` 的基础上新增 `files` 数组（按入口文件顺序，包含
  `path`、`ok`、`diagnostic_count`）和 `summary`（`checked`、`passed`、`failed`、
  `diagnostic_count`）。
- 后续 minor 阶段保证已有字段含义不变；破坏性变更需要升级 schema 标签
  （例如 `nox.check.v2`），并在文档与 CHANGELOG 中明确兼容窗口。

## 后果

- 工具可以通过 `schema` 字段精确判断协议版本。
- 多文件 / CI 场景下不再需要解析人类输出来获得汇总信息。
- 任何新增字段是兼容性扩展，旧消费者可以安全忽略。

## 备选方案

- 不加 schema 字段，靠语义版本：被拒绝，CLI JSON 没有版本号载体。
- 用 NDJSON（每条诊断一行）：被拒绝，无法承载 `summary` 等聚合字段，且和现有
  消费方式差别大。
