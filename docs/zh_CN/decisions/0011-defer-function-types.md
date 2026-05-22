# 0011 - 暂缓源码级函数类型

- 状态：已采纳
- 日期：2026-05-21
- 涉及：语言 / ABI / 工具链

## 背景

Nox 当前已经有函数声明、函数调用和内部函数签名类型：

- 源码用 `fn name(params) -> type { ... }` 声明函数。
- Type checker 为脚本函数和 Rust host function 都建立内部 `fn(...) -> ...` 类型，
  用于调用检查、返回值检查和 LSP hover。
- VM 运行时用 `Value::Function` 表示脚本函数和 host function。
- C ABI 只把函数报告为 `NOX_CORE_VALUE_FUNCTION` kind，不暴露读取或调用函数的 API。

阶段 18.3 要决定的是是否把这些内部能力提升为源码级一等函数表面，例如
`fn(int) -> int` 类型标注、函数参数、函数返回值、以及数组或 map 中保存函数。
阶段 27.3 在 v0.0.4 语言闸门中复审该结论后，继续暂缓函数能力扩张，见
[0015 - 暂缓容器和函数能力扩张](0015-defer-container-function-expansion.md)。

## 决策

v0.0.3 不实现源码级函数类型标注，不引入高阶函数作为稳定语言表面。

当前规则保持：

- 函数声明可以被静态调用，也可以作为模块或作用域里的命名声明参与解析。
- 内部 `Type::Function { params, return_type }` 继续存在，只服务于函数声明、host
  function、调用检查和 LSP hover。
- 源码类型位置不接受 `fn(int) -> int`，因此不能声明函数类型参数、函数类型返回值、
  `[fn(int) -> int]` 或 `map[str, fn(int) -> int]`。
- 类型等价在内部保持结构化：参数数量、参数类型顺序和返回类型全部相同才是同一函数
  签名；该规则暂不作为源码级类型语法承诺。
- 闭包环境生命周期不扩大为公共契约。脚本函数当前仍通过弱引用关联定义时环境；v0.0.3
  不承诺把捕获环境随函数值稳定跨模块、跨宿主或跨 ABI 持有。
- C ABI 继续只报告 function kind，不自动获得跨 ABI 函数调用能力。C host callback
  注册仍是独立入口，不能把脚本函数作为 callback handle 传给 C。

如果未来重启该设计，必须先同时设计源码类型语法、类型等价、闭包环境保活规则、Rust
`Value` 调用 API、C ABI function handle、formatter 和 LSP 行为。

## 后果

v0.0.3 保持函数模型简单：函数声明和 host function 仍能覆盖当前 stdlib、模块拆分和
嵌入回调需求，不把闭包逃逸、跨 ABI 调用和容器内函数值一起引入核心。

代价是 Nox 暂时不适合用高阶函数表达通用组合逻辑。需要标准库分层时，优先使用
命名空间 import 和模块成员访问，例如未来的 `std.fs.read_text`，而不是通过传递函数
值构造库表面。

## 备选方案

- 立即支持 `fn(T) -> U` 类型语法和函数参数。它能覆盖 map/filter 等高阶用法，但会立刻
  要求 parser、formatter、type checker、bytecode、LSP 和文档共同扩面。
- 只支持函数类型标注但不支持闭包捕获。这会制造一个看似一等、实则受限的表面，用户很
  难预测哪些函数值可以逃逸。
- 给 C ABI 增加 function handle 和 invoke API。这对嵌入宿主很强，但需要先定义
  argument marshalling、错误传播、engine/session 生命周期和 re-entrancy 边界。
