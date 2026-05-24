# 0030 - 分阶段引入 async/await

- 状态：已采纳
- 日期：2026-05-24
- 涉及：语言 / runtime / typecheck / VM / stdlib / embedding / C ABI / LSP / 诊断

## 背景

Nox 当前已有 sleep-based async task 基础：

- `task_sleep_ms(ms) -> int` 创建 runtime 内部 sleep task。
- `task_ready(id) -> bool` 非阻塞检查，完成时消费 id。
- `task_cancel(id) -> null` 取消 pending task。
- `std/task.nox` 提供 `sleep_ms`、`is_ready`、`cancel`、`wait`、`wait_or_timeout` 和
  `pending_count`。
- `RuntimePermissions::async_task_max_pending` 限制单个 runtime 内 pending task 数，默认
  `Some(1024)`。
- 顶层 eval/test 失败会清理本次调用中新建的 pending task；更早调用留下的 task 由宿主继续
  poll/cancel 或丢弃 runtime。

ADR 0016 暂缓脚本级 `task_status` / `task_poll`，因为当时还没有稳定 task 状态表示，也不希望
半途引入 event loop、promise 或 async/await。现在阶段 68 重新设计升级路径：不是一次性引入
完整 async runtime，而是把当前 task/timer 能力逐步提升为可等待的静态语言表面。

## 决策

Nox 采用分阶段 async/await 路线：

1. 先实现 awaitable task runtime MVP。
2. 再引入 `async fn` / `await` 的最小语法闭环。
3. 最后扩展 stdlib async helper、LSP 和 release compatibility。

第一轮 async/await 仍是单线程、显式权限、无 IO reactor 的模型：

- 不引入多线程 runtime。
- 不引入 epoll/kqueue/io_uring 或 socket/file async IO reactor。
- 不让 `await` 隐式授予 filesystem、network、environment、timer、process 或 async task 权限。
- 不让 host callback 在 VM 内 reentrant 执行脚本。
- 不把 runtime diagnostic 变成可捕获异常；错误模型继续遵循 ADR 0028。

## Awaitable 类型

新增语言级 awaitable 类型建议命名为 `task[T]`，表示将来产出 `T` 的单个 runtime task。

首批规则：

- `task[T]` 是普通静态类型，可以作为函数返回值、变量类型、数组元素和 record 字段类型。
- `await expr` 要求 `expr: task[T]`，求值为 `T`。
- `await` 只能出现在 async context：`async fn` 或未来明确引入的 async block。
- `async fn f(...) -> T` 的调用结果类型是 `task[T]`；函数体内部 `return value;` 的 `value`
  类型是 `T`。
- `async fn` 不是隐式并发权限；真正创建 sleep/network/process 等 runtime task 仍需要对应
  capability。
- `task[T]` 不进入第一轮 C ABI 复合值读取表面；C ABI 只暴露最小 poll/cancel handle 或继续
  只允许宿主在 Rust API 层驱动，具体由阶段 69 实现确认。

不做：

- 不支持 `task` cancellation token 作为语言内值。
- 不支持 `select` / `race` / structured concurrency 语法。
- 不支持 async trait method，直到 trait system 有更完整的 associated type / effect 设计。
- 不支持跨 runtime await。

## Runtime 与 scheduling

阶段 69 的 runtime MVP 使用单线程 scheduler：

- task 状态属于单个 `Runtime`，不跨 runtime 共享。
- timer task 可以在 deadline 后 ready；第一轮不为 filesystem/network/process 提供真正非阻塞 IO。
- `await` 在 CLI `run` / `test` 中可以驱动当前 runtime 直到 awaited task ready 或诊断失败。
- embedding API 必须允许宿主选择：阻塞等待、非阻塞 poll，或取消 task。
- pending-task 上限继续生效；超过上限返回 `runtime.task-pending-cap`。
- 顶层 eval/test 失败仍清理本次调用中新建的 pending task；await 内部失败也遵循同一清理规则。
- `task_cancel` 只取消 pending task；已 completed/cancelled 的 id 或 task handle 不复活。

如果阶段 69 需要保留 completed task 结果以支持 `await task`，必须定义结果生命周期：

- completed result 至少要保留到第一次 await/poll 消费。
- 消费后释放 payload 和 task entry。
- 未消费 completed task 受 pending/completed 总量上限约束，避免泄漏。

## Typechecker 与错误传播

`async fn` 与 `?` 的关系保持简单：

- `async fn f() -> result[T, E]` 内部可以对 `result[U, E]` 使用 `?`；失败时完成为
  `result.err(E)`，不是 runtime diagnostic。
- `async fn f() -> option[T]` 内部可以对 `option[U]` 使用 `?`；失败时完成为 `none`。
- `await` 一个完成为 `result[T, E]` 的 task 不自动 unwrap；调用方仍显式写
  `let value: T = (await task)?;`。
- runtime diagnostic、permission denied、resource cap、host panic 仍终止当前 eval/test，
  不进入 `task[result[T, E]]` 的 `err`。

sync/async 边界：

- sync `fn` 不能使用 `await`。
- sync `fn` 可以创建或传递 `task[T]`，但不能等待它，除非调用显式 blocking helper。
- async `fn` 可以调用 sync `fn`。
- async `fn` 调用 async `fn` 得到 `task[T]`，需要 `await` 才取得 `T`。

## CLI、embedding、C ABI 和 LSP

CLI：

- `nox run` 对 final value 是 `task[T]` 的脚本应保守诊断，除非 ADR 后续明确允许 top-level await。
- `nox test` 的测试函数第一轮仍要求返回 `bool`；是否允许 `async fn test_*() -> bool` 留到阶段
  70/71 由测试框架单独设计。
- JSON diagnostics 不新增 schema；新增 code 必须与 LSP diagnostics parity 测试同步。

Rust embedding：

- `Runtime` 应提供最小 task poll/cancel API，或通过 `eval` / `run_test_file` 的 await 行为隐藏
  scheduler 细节。阶段 69 必须选定其一并写入 docs。
- 任何阻塞 wait 都必须由宿主显式调用；host callback 不应被 VM 自动重入。

C ABI：

- 第一轮不要求 C ABI 暴露 `task[T]` payload 读取。
- 如果暴露 task handle，必须定义 ownership、poll result、cancel、free 和 thread/reentrancy 边界；
  否则 C ABI 文档应明确 task 值暂不跨 ABI。

LSP：

- Hover 显示 `async fn` 的调用类型为 `task[T]`。
- Diagnostics 覆盖 sync context 中的 `await`、await 非 task、async return mismatch。
- Definition/rename 不穿透 scheduler；只按源码 symbol 工作。

## 诊断计划

阶段 70 起新增稳定 code：

- `async.await-outside-async`：sync context 使用 `await`。
- `async.await-non-task`：await 的表达式不是 `task[T]`。
- `async.return-mismatch`：async function body 返回值与声明 `T` 不一致。
- `async.top-level-task`：top-level final value 是未 await 的 `task[T]` 且当前 CLI 不支持 top-level await。
- `runtime.task-pending-cap`：继续复用现有 pending task 上限 code。

## 后果

该路线让 Nox 可以逐步进入 async/await，而不一次性承担完整 runtime、IO reactor、多线程和权限传播
复杂度。阶段 69 可以专注 task 值和 scheduler 生命周期；阶段 70 再接 parser/typechecker/VM
语法；阶段 71 再决定哪些 stdlib helper 需要 async variant。

代价是第一轮 async/await 表达力有限：没有 async file/network IO、没有 `select`、没有 async
trait、没有 top-level await 承诺。这个限制是刻意的，用于保护权限模型、embedding 边界和 release
gate。

## 实现顺序

1. 阶段 69：实现 awaitable task runtime MVP，定义 task result 生命周期、poll/cancel API 和
   Rust/C ABI 边界。
2. 阶段 70：实现 `task[T]` 类型、`async fn`、`await`、formatter、typechecker 和 VM 执行。
3. 阶段 71：给 `std/task.nox` 和少量 timer/helper 提供 async-friendly wrapper；评估 HTTP/fs 是否
   只做 blocking wrapper 还是继续暂缓。
4. 阶段 72：补 LSP hover/completion/diagnostics、docs、examples 和 release gate 回归。

## 备选方案

- 继续只保留 `std/task.nox` id API。未选择，因为它无法表达类型化结果、组合和语言级等待边界，
  也难以支撑未来 async stdlib。
- 直接实现完整 async runtime 和 IO reactor。未选择，因为权限、取消、resource cleanup、C ABI
  和 release audit 成本过高。
- 采用 JavaScript Promise 风格。未选择，因为 Nox 是静态类型语言，`task[T]` 更直接表达结果类型。
- 顶层 await 先行。未选择，因为 CLI/test/embedding 的阻塞语义和取消边界必须先设计清楚。
