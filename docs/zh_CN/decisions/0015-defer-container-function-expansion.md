# 0015 - 暂缓容器和函数能力扩张

- 状态：被 0018 与 0019 取代
- 日期：2026-05-21
- 涉及：语言 / 堆 / ABI / 工具链

## 背景

阶段 27.3 在 v0.0.4 语言设计闸门里重新评估 0010 和 0011 的暂缓项。评估依据来自
scoreboard sample project、`std/*` 模块迁移、embedding regression、heap 压力测试和
benchmark/diagnostics 审计。

当前证据仍然偏向保守：

- sample project 使用固定 array/map/record 字面量、namespace import 和命名函数完成分层。
- `std/fs.nox`、`std/env.nox` 和 `std/time.nox` 通过静态模块成员表达能力，不需要脚本内
  动态构造大型数组。
- C ABI 的复合值暴露为只读 owning handle，array/map/record 的 host 读取路径不承诺
  mutation。
- heap 模型仍以 `Rc` + `Weak` 追踪长期值持有，没有引入 interior mutability、arena
  handle、mutation log 或 closure environment lifetime 的公共契约。

因此，本次复审只决定 v0.0.4 是否重启实现，不把暂缓项自动转成 parser/type/VM 工作。

## 决策

v0.0.4 不实现以下能力：

- 可变数组，包括 `push(values, x)`、`values[i] = x` 或任何会让 alias 观察到原地写入的操作。
- 数组切片或 range indexing，例如 `values[start..end]`。
- 源码级函数类型标注，例如 `fn(int) -> int` 出现在参数、返回值、数组或 map 类型位置。
- 高阶函数作为稳定语言表面，包括函数参数、函数返回值、函数数组和函数 map。
- 跨 C ABI 的脚本函数 handle 或 invoke API。

当前规则继续保持：

- 数组构造后不可变；`const` 只禁止重绑定，但因为没有容器 mutation，`let` 和 `const`
  都不能修改数组内容。
- C ABI array/map/record handle 继续只读。
- `Value::Array` 和 heap 追踪继续使用现有不可变表示，不引入 `RefCell<Array>`。
- 函数声明、静态调用、namespace import 和 host function 注册继续覆盖当前模块和嵌入需求。
- 内部函数签名类型只服务于声明、调用检查、host callback 和 LSP hover，不成为源码级类型语法承诺。

如果未来真实项目证明需要数组增长，优先重新评估“返回新数组”的 copy-on-write helper，
例如 `array_push(values, value) -> [T]`，而不是原地 mutation。如果未来需要函数组合，
必须先给出 parser/type/VM/Rust API/C ABI/LSP/formatter/test 的完整分阶段设计，再进入实现。

## 后果

v0.0.4 保持语言核心、heap 和 C ABI 的既有稳定边界。这样可以避免在同一轮里同时引入
aliasing、`const` 深浅可变性、闭包保活、跨 ABI 函数调用和 formatter/parser 新语法。

代价是 Nox 仍不适合作为通用集合变换语言：脚本内不能逐步构建大型数组，也不能用
高阶函数表达通用组合逻辑。当前推荐做法是把集合收集、过滤或回调组合留在宿主或
专用 std module 函数里，由脚本接收完整值并做静态调用。

## 备选方案

- 原地可变数组：提供 `push(values, x)` 和元素赋值。优点是表达直接；缺点是必须重新定义
  aliasing、`const` 是否冻结容器内容、bytecode assignment target、verifier 和 runtime
  bounds/type diagnostic，同时会破坏 C ABI 只读 handle 的直觉。
- Copy-on-write 数组增长 helper：`array_push(values, x) -> [T]` 返回新数组，旧 alias 不变。
  这是未来更安全的首选方向，但当前 sample project 和 stdlib 尚无足够压力，过早加入会扩展
  std 表面、负向测试和性能承诺。
- 数组切片：`values[start..end]` 可覆盖只读窗口需求，但需要新增 range 语法、bounds
  规则、返回值分配策略，以及是否共享底层数组的生命周期决策。当前没有真实用例支撑。
- 完整源码级函数类型：支持 `fn(T) -> U`、函数参数、返回值和容器元素。它能覆盖 map/filter
  类用法，但会立刻牵动 parser、formatter、type checker、VM、LSP、Rust `Value` 调用 API、
  closure environment lifetime 和 C ABI。
- 仅开放 C ABI function handle：让宿主读取并调用脚本函数。它对 embedding 很强，但必须先定义
  argument marshalling、错误传播、engine/session 生命周期和 re-entrancy；当前 host callback
  注册已经覆盖已有用例。
