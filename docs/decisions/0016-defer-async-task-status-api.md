# 0016 - 暂缓 async task 状态 API

- 状态：已采纳
- 日期：2026-05-22
- 涉及：运行时 / 语言 / 嵌入 / 工具链

## 背景

阶段 30.2 重新评估是否在 v0.0.6 暴露可恢复的 async task 状态 API。当前 runtime 已有
三个脚本级 helper：

- `task_sleep_ms(ms: int) -> int` 创建 sleep task 并返回 id。
- `task_ready(id: int) -> bool` 在 task 仍 pending 时返回 `false`，到达 deadline 时返回
  `true` 并释放 id。
- `task_cancel(id: int) -> null` 释放 pending task。

现有测试已经锁定这些生命周期事实：

- task id 在单个 `Runtime` 内单调递增，不复用。
- completed 和 cancelled task 都会立即从 task 表删除，之后该 id 变成 unknown。
- unknown id 当前是 diagnostic，用于区分“没有这个任务”和“任务尚未 ready”。
- 权限不足、负数 id、负数 duration 都是不可恢复 diagnostic。
- 顶层 eval/test 失败时，只清理本次调用中新建的 pending task；更早调用留下的 pending task
  继续由宿主 poll、cancel 或丢弃整个 `Runtime` 来处理。

v0.0.5 已用 `option[T]` / `result[T, E]` 和 `map_get(map, key) -> option[T]` 证明可恢复 API
有价值，但 async task 状态不同：它不是单个缺失值，也不只是普通 I/O 失败，而是会暴露 id
生命周期、状态形状和权限策略。

## 决策

v0.0.6 暂不实现新的脚本级 `task_status`、`task_poll` 或 `TaskStatus` record API。

当前稳定表面继续是：

- `task_ready(id: int) -> bool`：pending 返回 `false`，completed 返回 `true` 并消费 id。
- `task_cancel(id: int) -> null`：只取消 pending task，成功后消费 id。
- unknown id、权限不足、负数 id 和负数 duration 继续是 diagnostic。
- Rust embedding 继续只暴露 `Runtime::pending_async_task_count()` 作为宿主观察入口。
- C ABI 不新增 async task status handle 或 polling API。

重新启动实现必须先满足这些条件：

- 有稳定的状态表示，不能只用 `"pending"` / `"completed"` / `"cancelled"` 这类自由字符串作为
  长期契约。可接受方向包括语言级 enum/variant 设计，或受限命名 record 加固定字段并有
  完整 formatter/LSP/JSON 文档。
- 明确 completed task 是否能被非消费式观察。如果要让 `task_status(id)` 返回 completed，
  runtime 必须保留 completed task 一段时间，这会改变当前“ready 即释放”的内存和 id 语义。
- 明确 unknown id 是否可恢复。如果 unknown 变成 `none` 或 `err("unknown task id")`，
  必须说明它和权限不足、负数 id、已取消 id、已完成 id 的区别。
- 先定义该 API 是否只属于 `nox` runtime stdlib，还是也进入 Rust embedding 或 C ABI。
- 保持不引入语言级 `async` / `await`、promise、event loop、跨线程 callback 或 socket API。

## 后果

v0.0.6 可以继续把主要精力放在诊断契约、manifest/project 体验、embedding 兼容矩阵和 release
gate 上，不把 async task 状态 API 做成半成品公共表面。

代价是脚本仍然无法区分“已完成后又被消费的 id”和“从未存在的 id”；两者都会得到 unknown id
diagnostic。宿主如果需要更细的 task 生命周期，可以继续在 Rust 侧围绕 `Runtime` 管理自己的
task registry，或注册自定义 host function。

## 备选方案

- `task_status(id: int) -> result[TaskStatus, str]`：表达力最强，但需要先有稳定 `TaskStatus`
  形状。若 `TaskStatus.state` 是字符串，会把拼写和状态集合变成长期契约；若用 record 字段组合
  表示状态，容易出现无效组合。
- `task_poll(id: int) -> option[TaskStatus]`：可以把 unknown 表达为 `none`，但会混淆
  “从未存在”“已完成并被消费”“已取消”这些不同状态。
- `task_ready_result(id: int) -> result[bool, str]`：实现最小，但只是把 unknown diagnostic
  包装成字符串错误，不能解决状态形状和 completed/cancelled 观察问题。
- 保留 completed/cancelled tombstone：可以让 status 查询更完整，但会引入 tombstone 生命周期、
  清理策略、内存上限和跨 eval 调用保留规则，当前没有真实项目压力支撑。
