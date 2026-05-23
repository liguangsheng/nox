# 0021 - 暂缓运算符重载与异常模型，保持 result/option 单一错误路径

- 状态：已采纳
- 日期：2026-05-23
- 涉及：语言 / parser / typecheck / VM / 诊断 / 工具链

## 背景

PLAN.md 阶段 26 计划评估"运算符重载、defer / finally、panic / recover、异常机制"。
当前 Nox 的错误处理是单一路径：`result[T, E]` + `option[T]` + `?` 运算符。所有 stdlib
helper、host callback、runtime intrinsic 都使用这一模型；CLI JSON、LSP 诊断、退出码
都围绕该模型构建。

运算符重载和异常机制是脚本语言常见的扩展方向，但都会显著扰动现有契约：

- 运算符重载会让 typecheck 必须解析 user-defined methods，破坏当前"运算符基于内建类型"
  的简单规则。
- 异常机制（即使是 panic / recover 风格）会引入第二条控制流：runtime 错误既能通过 `?`
  传播，也能通过 unwind 跳过若干栈帧。这会让 stack trace、profile span、host callback
  错误隔离全部需要重新验证。

## 决策

v0.0.x 开发阶段**不引入运算符重载**，**不引入 panic / recover / try / catch / finally**。
保持 result / option 是唯一错误传播路径。

显式不做的能力：

- 用户为 record / enum 定义 `==`、`<`、`+`、`-`、`*`、`[]`、`()` 等运算符。
- `try { ... } catch (e) { ... }` 或 `try { ... } except E { ... }`。
- `panic("...")` 内建函数（runtime panic 仍存在但仅来自 VM bug 和 host callback panic
  保护，不暴露为脚本语法）。
- `recover` / `catch_unwind` 风格的脚本侧 unwind 拦截。
- `defer` / `finally` 块。

允许保留 / 已经存在：

- `?` 后缀运算符，按 ADR 0014 / 阶段 9 决策传播 `result.err` / `option.none`。
- host callback panic 由 VM 捕获并转换为 diagnostic（code `host.callback`）；脚本侧
  无法拦截，行为与 0021 兼容。
- runtime 错误（除零、索引越界、容器借用冲突）始终以 diagnostic 结束 VM 执行；
  脚本侧无 try/catch，等价于"立即终止"。
- 资源清理：脚本不需要 `defer` / `finally`，因为没有用户可见的资源类型。host
  callback 的资源生命周期由宿主管理，与脚本控制流解耦。

兼容影响：

- 现有 result / option / `?` 行为不变；所有阶段 9-21 的相关测试和 example 不动。
- parser：保留 `try` / `catch` / `panic` / `defer` 作为保留字（reserved keyword）以便
  未来重启时不破坏现有脚本。这是兼容**收紧**：原本可以作为 identifier，现在不行。
  实现 PR 必须把这一收紧写进 CHANGELOG。
- 运算符重载相关：record method（阶段 9 已落地）继续可用，但 `record_value.add(other)`
  不会被自动绑定到 `+`。这保持当前"运算符是基于类型类别的固定行为"的诊断不变性。

权限边界：

- 不涉及 fs/env/net/timer/process capability。

embedding API 影响：

- Rust API 和 C ABI 完全不变。
- `Diagnostic.stack_frames` 不需要因 unwind 添加 catch-frame；ADR 0021 与 ADR 0019
  之后的 `kind` 字段（Script / Host）兼容。

诊断方案：

- `parse.reserved-keyword`（稳定，新增 code）：脚本中把 `try` / `catch` / `panic` /
  `defer` / `finally` 作为 identifier 时报告。message 解释这些词被保留以供未来评估。
- 现有 `runtime.*` code 不变。

测试矩阵（实现 PR 必须覆盖）：

- 单元：试图把保留字用作变量、字段、函数名，被 parser 拒绝。
- 单元：现有 result / option / `?` 路径回归不变。
- LSP：保留字 hover 显示"reserved for future use"。
- formatter：保留字保持原样输出，不被识别为标识符。
- 文档：`docs/zh_CN/diagnostics.md` 增加 `parse.reserved-keyword` 行。

放弃条件：

- 三个独立真实用户脚本提出"用 `?` 表达不出"的具体场景，并且无法用 record method +
  result 组合解决。
- host 侧出现需要脚本拦截 runtime 错误的合理用例（例如插件加载失败后继续运行 CLI 主流程）。
  当前 `Engine::eval` 返回 `Result<Value, Diagnostic>`，host 已经能在 Rust 侧处理。

任一触发后，本 ADR 状态改为"已废弃"，并写一份重启 ADR 说明边界与放弃条件。

## 后果

阶段 26 的工作面被显式收窄：只允许在不引入用户可见 unwind 的前提下做控制流改进。
保持 result / option 单一路径降低诊断、stack trace、profile span 和 LSP 的实现复杂度，
并让 PLAN.md 阶段 30 的测试框架围绕 result-based 错误模型设计。

代价是脚本无法表达"局部 try"或"运算符让位于 record method 风格"。当前 stdlib 已经
覆盖错误传播；这类需求短期由 result chain + record method 顶住。

## 备选方案

- 引入运算符重载但不引入异常：可以让 record 当数学对象用，但会让 typecheck 进入
  ad-hoc polymorphism；阶段 25 已经决定不做用户级 trait，运算符重载会绕过该决策。
- 引入 panic / recover：能模拟异常处理，但同时会让 stack trace、profile span 和
  host callback 错误隔离一起复杂化，violates"小核心 + 明确诊断"原则。
- 把 `?` 扩展到 record method 链：保留单一错误路径，等于复述现状；本 ADR 把这一点
  显式记下来，不是另一个备选。
