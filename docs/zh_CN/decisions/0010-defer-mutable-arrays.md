# 0010 - 暂缓可变数组

- 状态：已采纳
- 日期：2026-05-21
- 涉及：语言 / 堆 / ABI

## 背景

Nox 当前数组是构造后不可变的 `[T]` 值：

- 语言支持数组字面量、整数索引和 `len(array)`。
- `Value::Array` 持有 `Rc<Array>`，heap 只追踪 weak 引用。
- C ABI 通过只读 owning handle 读取脚本返回的数组。
- `const` 只禁止重新赋值绑定；容器本身目前没有可变操作。

阶段 18.2 要先明确数组可变性，再决定是否加入 `push`、元素赋值或切片。
阶段 27.3 在 v0.0.4 语言闸门中复审该结论后，继续暂缓容器能力扩张，见
[0015 - 暂缓容器和函数能力扩张](0015-defer-container-function-expansion.md)。

## 决策

v0.0.3 不实现可变数组，不添加 `push`、元素赋值或切片。

当前规则保持：

- 数组构造后不可变。
- alias 后没有可观察的写入行为。
- `const values: [int] = [1, 2];` 与 `let values: [int] = [1, 2];` 都不能修改数组内容。
- C ABI array handle 继续只读，只提供 len/get/free。
- heap 表示继续使用 `Rc<Array>`，不引入 `RefCell<Array>`、arena handle 或 mutation log。

如果未来需要数组增长能力，优先设计“返回新数组”的 copy-on-write 风格 API，例如
`array_push(values, value) -> [T]`。这种模型让 alias 不会观察到旧数组被修改，也让 `const`
语义保持简单：`const` 禁止重绑定，因此不能把返回的新数组赋回同名绑定。

## 后果

v0.0.3 保持现有 value、heap 和 C ABI 稳定，避免把 mutation、alias 和 const 语义一次性
引入核心。脚本需要构造新数组时，短期仍只能通过字面量或宿主函数返回完整数组。

代价是 Nox 暂时不适合需要逐步构建大型数组的脚本。相关场景应先由宿主提供更高层的
专用函数，等真实需求稳定后再重启数组增长设计。

## 备选方案

- 原地可变数组：`push(values, x)` 修改所有 alias 可见的同一数组。实现直观，但需要
  把 `Rc<Array>` 改成 interior mutability，并重新定义 `const` 是否冻结容器内容。
- 元素赋值：`values[i] = x` 更接近通用语言，但会扩展 assignment target、bytecode 和
  verifier，同时带来 bounds/type/runtime 诊断组合。
- C ABI 可变 handle：允许 C 端 push/set。它会把宿主 mutation 与 VM value 生命周期耦合，
  与当前只读 owning handle 决策冲突。
