# 0028 - result / option 错误模型与 try block 暂缓

- 状态：已采纳
- 日期：2026-05-24
- 涉及：语言 / runtime / typecheck / VM / 诊断 / 文档

## 背景

ADR 0021 在 Nox 只有早期 `result` / `option` 传播能力时，决定暂缓异常模型、`panic` /
`recover`、`try` / `catch` / `finally` 和运算符重载。后续阶段已经完成了更多错误处理表面：

- 语言级 `option[T]` / `result[T, E]`、构造、`match`、`if let`、`while let`、
  `let ... else` 和后缀 `?`。
- stdlib 中大量可恢复 API 返回 `result.err(message)` 或 `option.none`，例如 JSON/TOML
  parse、filesystem `try_read_text`、environment `try_get`、HTTP helper、bytes/encoding
  decoder 和 `std/term.nox` 交互 helper。
- runtime permission denied、resource cap、host callback error、host panic、parser /
  typechecker diagnostic 和 stack trace 已经形成一条不可恢复诊断通道。
- `try`、`catch`、`panic`、`defer`、`finally` 已是保留字，但尚未赋予用户可见语义。

阶段 64 重新评估是否需要加入 Rust 风格 `try { ... }` block，或者继续只完善现有
result/option ergonomics。

## 决策

Nox 继续以 `result[T, E]`、`option[T]`、后缀 `?`、显式 pattern matching 和 runtime
diagnostic 作为错误模型。阶段 64 不引入用户可见的通用异常机制，也暂缓 Rust 风格
`try { ... }` block。

稳定边界如下：

- 可恢复业务失败用 `result.err(value)` 或 `option.none` 表达。
- 函数内的早返回传播继续使用 `expr?`，外层函数必须返回兼容的 `result` 或 `option`。
- 需要分支处理时使用 `match`、`if let`、`while let` 或 `let ... else`。
- runtime diagnostic 是不可捕获通道：权限不足、allowlist 越界、资源上限、除零、
  越界索引、host callback panic、host callback 返回值类型错误、parser/typechecker 失败
  都不会被脚本侧捕获或包装成 `err`。
- `try` / `catch` / `panic` / `defer` / `finally` 保持保留字。它们不能作为 identifier，
  但也不具备运行时语义。

本 ADR 细化并延续 ADR 0021。ADR 0021 的“无 try/catch/finally/unwind”结论仍有效；
本 ADR 额外确认：即使考虑 Rust 风格 `try {}` block，当前也不采用。

## 为什么暂缓 `try {}`

Rust 风格 `try {}` block 可以把一段 `?` 链包成局部 `result` 或 `option` 表达式，理论上
有助于减少一层 helper 函数。但 Nox 当前收益不足：

- 现有函数级 `?` 已经覆盖主要脚本路径；需要局部组合时可以提取小函数。
- `match` / `if let` / `let ... else` 已覆盖需要保留局部上下文的分支处理。
- 引入 block expression 会影响 parser、formatter、typechecker、LSP hover、diagnostic span
  和 `nox doc`，而不增加新的错误语义。
- 如果用户把 `try {}` 与 `catch` 类异常混淆，文档和诊断成本会上升。
- Nox 仍没有稳定的 block expression 表面；只为错误处理单独加入一种 block expression 会让
  语言形状不均衡。

因此阶段 65 不实现 `try {}`。阶段 65 的实现方向改为：收敛 result/option helper、补文档、
补 cookbook 示例和补边界测试。

## 与 runtime diagnostic 的关系

runtime diagnostic 不是普通值，也不是可恢复业务错误。诊断代表脚本、宿主边界或运行时资源约束
违反了执行前提，应终止当前 eval / test case / CLI 命令：

- permission denied 说明宿主没有授予 capability，不能由脚本用 `catch` 绕过。
- resource cap 说明宿主设定的安全边界被触达，不能降级为业务错误继续运行。
- host callback panic 和类型不匹配说明宿主边界失效，脚本侧不拥有恢复语义。
- parser/typechecker diagnostic 发生在执行前，不存在运行时 catch 目标。

需要可恢复语义时，stdlib 或宿主应提供显式 `try_` / `parse` / `request` / `get` helper，并
把普通失败返回为 `result` 或 `option`。权限和安全边界仍必须在 helper 内先检查，失败时走
diagnostic。

## 工具和诊断要求

- `parse.reserved-keyword` 继续覆盖 `try`、`catch`、`panic`、`defer`、`finally`。
- `result.question-mark.mismatch` 继续表示 `?` 所在函数返回类型不兼容。
- CLI JSON、LSP diagnostics 和 project check JSON 不新增异常相关 schema。
- Formatter 不需要支持 `try` block；遇到这些保留字作为 identifier 仍由 parser 报错。
- Runtime stack trace 不新增 catch frame、finally frame 或 unwind marker。

## 后果

这个决策保持 Nox 的错误模型单一、静态、可诊断：可恢复失败是值，不可恢复边界是诊断。实现面
集中在 stdlib helper、文档和测试，而不是引入 VM unwind、catch scope 或新的 block expression。

代价是脚本不能在局部表达式里直接写 Rust 风格 `try { ... }`。短期通过小函数、`match` 和
`if let` 解决。若未来真实代码大量出现“只为使用 `?` 被迫提取一次性函数”的模式，可以重启
try-block ADR，但仍不得把 runtime diagnostic 变成可捕获异常。

## 重新启动条件

满足以下条件之一时，可以重启 `try {}` 评估：

- 三个以上真实项目出现同类局部 result/option 组合问题，并且小函数或 pattern matching 明显
  降低可读性。
- Nox 已经拥有通用 block expression 语义，`try {}` 可以作为普通 block expression 的受限形式
  加入，而不是单独开一条语法路径。
- stdlib helper 收敛后仍出现大量重复 `match ok/err` 样板，且新增 helper 不能消除。

即使重启，仍保持以下硬边界：不做 `throw` / `catch` / `finally`，不做 VM unwind，不捕获
runtime diagnostic，不让权限或资源限制变成用户可吞掉的值。

## 备选方案

- 引入通用 `try/catch/finally`。未选择，因为它会建立第二条错误通道，破坏 capability、
  resource cap、host panic 和 stack trace 的边界。
- 引入 Rust 风格 `try {}`。未选择，因为当前收益低于 parser/typechecker/formatter/LSP 成本，
  且容易被误读成异常捕获。
- 只增加更多 `unwrap` 风格 helper。未选择作为主要路线，因为 `unwrap` 会把可恢复错误重新变成
  diagnostic；可以保留少量测试或边界明确的 helper，但不能成为推荐错误处理方式。
- 继续只靠文档，不更新 ADR。未选择，因为 `try` 等关键字已经保留，后续阶段需要一个明确的
  重启条件和不实现依据。
