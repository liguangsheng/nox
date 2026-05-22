# 0014 - 重启 option / result 设计但暂不实现

- 状态：已采纳
- 日期：2026-05-21
- 涉及：语言 / 运行时 / ABI / 工具链

## 背景

ADR 0009 在 v0.0.3 阶段暂缓了语言级 `option[T]` / `result[T, E]`。阶段 27.1
复盘 sample project、`std/*` 模块迁移和 embedding/runtime 边界后，确认可恢复错误
模型已经有真实压力，但这些压力还不足以跳过设计直接进入实现。

具体触发点：

- `std/fs.nox`：`exists(path)` + `read_text(path)` 能处理文件缺失，但无法表达
  权限不足、读取失败和成功读取之间的结构化差异。
- `std/env.nox`：脚本需要 `list()` 后 `contains(envs, name)` 再读 map，才能避免
  `get(name)` 缺失变量时产生 runtime diagnostic。
- map lookup：`contains(map, key)` + `map[key]` 是可用 guard，但重复写法明显，且
  不适合表达“缺失时给出原因”。
- async task：`task_ready(id)`、`task_cancel(id)` 对 unknown id 使用 diagnostic；
  这适合非法 id，但不适合未来 IO task 的“pending / completed / failed”状态。
- host callback：Rust callback 失败和 C callback 非 OK 状态只能通过 diagnostic /
  last_error 报告，脚本无法显式处理可恢复宿主错误。

这些场景都来自已实现 runtime、stdlib 和 embedding 表面，不是抽象语法完整性需求。

## 决策

v0.0.4 重新启动语言级 `option[T]` / `result[T, E]` 设计，但本 ADR 不批准立即实现。
下一步是产出完整设计和负向用例，再决定是否进入 parser/type/VM/API 改动。
阶段 28.1 已把该决定拆成实施计划，见
[Option / Result Implementation Plan](../option-result-implementation-plan.md)。

硬边界：

- 继续禁止隐式 nullable；`null` 仍是独立类型，不自动成为 `T` 的成员。
- 不引入异常、try/catch 或隐式错误传播。
- 不把现有 `env_get`、`read_text`、map index、host callback 语义在同一批里改掉。
- 不为 ABI 暂时便利引入只在 Rust 可见、C ABI 不可表达的稳定值语义。

候选方向：

- 类型语法优先使用显式泛型形态：`option[T]`、`result[T, E]`。
- 构造值需要显式名称，例如 `some(value)` / `none`、`ok(value)` / `err(error)`；具体命名
  需在后续设计中固定。
- 解包优先复用现有 `match` 或新增受限模式，而不是先加异常式传播。
- 错误值先从 `str` 或固定 record 形状开始评估，不直接设计复杂 error hierarchy。

如果后续决定进入实现，分阶段计划如下：

1. **类型和语法设计**：扩展 type parser 支持 `option[T]` / `result[T, E]`；给无效泛型形态
   加负向 parser/type tests；更新 formatter golden fixture 和 LSP hover/completion 预期。
2. **值表示和 VM**：决定 `Value::Option` / `Value::Result` 还是标准库编码；实现构造、
   equality 边界、display/debug 和 heap 追踪；明确 `null` 不参与 option。
3. **控制流与解包**：设计 `match` 分支如何绑定 payload，或新增最小 `is_some` /
   `unwrap` 风格 helper；先写负向测试证明未解包不能当作 `T`。
4. **Rust API 和 C ABI**：扩展 Rust `Value`、C `NoxCoreValueKind` / owning handle 或 tagged
   scalar wrapper；保持旧 ABI 函数行为不变，只末尾追加 enum/function。
5. **stdlib 迁移窗口**：新增可恢复 API，例如 `env.try_get`、`fs.try_read_text`、
   `map_get` 或 task 状态 API；旧 diagnostic API 至少保留一个 minor 阶段。
6. **工具和测试**：补 CLI JSON、LSP diagnostics、formatter、module/std wrapper、
   embedding 和 C smoke；文档说明 guard 模式与新 API 的迁移关系。

## 后果

这个决定承认 `option/result` 已经有足够真实用例，避免 27.x 后续继续把同一问题重复
作为开放项讨论。同时，它保留实现闸门：没有完整设计、负向用例和 ABI 计划前，不进入
语言实现。

短期内，runtime 文档继续推荐 guard + diagnostic 模式：

- 文件读取先 `exists(path)`，再 `read_text(path)`。
- 环境变量先 `env_list()`，再 `contains(envs, name)`。
- map 读取先 `contains(map, key)`，再 `map[key]`。
- async task 只对自己持有的 id poll/cancel，unknown id 仍是 diagnostic。
- host callback 可恢复错误由宿主拆成返回 `bool` / `str` / record 的显式 API。

代价是 v0.0.4 还不能立刻获得脚本内可恢复错误处理能力；但这比仓促加入隐式 nullable 或
只覆盖 Rust API 的半套 `Result` 更稳。

## 备选方案

- 继续完全暂缓：实现风险最低，但已无法解释 std/fs、std/env、map lookup 和 host callback
  中重复出现的 guard/diagnostic 压力。
- 立即实现 `option[T]`：能最快改善缺失值场景，但无法覆盖权限错误、IO 错误和 host callback
  失败原因，也会在没有控制流收窄方案时制造大量 `unwrap` 风格运行时错误。
- 立即实现 `result[T, E]`：表达力更完整，但需要同时确定错误类型、构造/解包语法、
  ABI 表示和 stdlib 迁移，范围过大。
- 让失败返回 `null`：实现最少，但破坏精确静态类型模型，并和 v0 已明确的 `null`
  独立类型冲突。
