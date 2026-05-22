# 0009 - 暂缓语言级 option / result

- 状态：已采纳
- 日期：2026-05-21
- 涉及：语言 / 运行时 / ABI

> 后续：阶段 27.2 已通过
> [0014 - 重启 option / result 设计但暂不实现](0014-restart-option-result-design.md)
> 重新打开该议题。0009 仍记录 v0.0.3 的暂缓依据；0014 是 v0.0.4 设计闸门的当前结论。

## 背景

Nox 当前没有“可能缺失”或“可恢复错误”的语言级类型。真实触发点主要来自宿主边界：

- `env_get(name)`：环境变量缺失时返回 runtime diagnostic。
- `read_text(path)`：文件不存在、权限不足或读失败时返回 runtime diagnostic。
- map index：缺失 key 时返回 runtime diagnostic；脚本可先用 `contains(map, key)` guard。
- async task：未知 task id 返回 runtime diagnostic；脚本可保存 task id 并避免重复消费。
- host callback：宿主 callback 失败时通过 diagnostic / C ABI `last_error` 报告。

这些场景确实会受益于 `option[T]` 或 `result[T, E]`，但直接引入会影响 parser、
type checker、控制流收窄、VM value 表示、Rust `Value`、C ABI 和 formatter。

## 决策

v0.0.3 不实现语言级 `option[T]` / `result[T, E]`，也不引入隐式 nullability。

当前规则保持：

- `null` 仍是独立字面量和独立 `null` 类型，不自动成为任意类型的可空值。
- 可预判的缺失场景使用显式 guard，例如 `exists(path)`、`contains(map, key)`、
  `env_list()` 后再查 map。
- 不可恢复或宿主边界失败继续使用 diagnostic 中止当前 eval/check/test。
- host callback 失败继续通过 diagnostic 和 C ABI `nox_core_engine_last_error` 暴露。

如果未来重新启动该设计，必须先完成：

- 类型语法：例如 `option[T]` / `result[T, E]`，不使用隐式 `T?`。
- 构造与解包语法：如何创建、匹配、提前返回或默认值。
- 控制流收窄：`if` / `match` 如何证明某分支内值已解包。
- Rust API 表示：`Value` 是否增加变体，还是标准库层面编码。
- C ABI 表示：是否新增 tagged handle / scalar wrapper，且如何保持旧 ABI 可用。

## 后果

v0.0.3 可以继续保持小核心和稳定 ABI，避免在错误处理语法上过早锁定。脚本处理缺失值时
需要写显式 guard，部分 runtime 错误仍会中止执行。

这也让标准库设计更明确：新增 fallible API 时，优先提供一个可检查的 companion API
或返回 `bool` 的 probe，而不是悄悄返回 `null`。

## 备选方案

- 立即实现 `option[T]`：能覆盖 `env_get`、map lookup 和文件读取缺失，但需要控制流收窄
  和 ABI 表示，超出当前 v0.0.3 设计预算。
- 立即实现 `result[T, E]`：表达力更完整，但还需要错误类型、字符串错误还是 record 错误、
  传播语法等决策。
- 让现有 API 在失败时返回 `null`：实现看似简单，但会引入隐式 nullability，破坏当前
  精确静态类型模型。
