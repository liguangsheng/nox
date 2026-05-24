# 数组设计

本文记录 Nox 第一版静态数组设计。数组已经落地，文档保留为实现约束和后续扩展边界。
当前实现已在后续阶段加入数组 / map 显式 mutation helper 和 index assignment；本文中
关于“数组构造后不可变”的描述只保留为早期设计记录。当前用户可见行为见
[语言 v0](language-v0.md) 与 [运行时](runtime.md)。

## 目标

- 提供同质 `[T]` 类型。
- 提供数组字面量，并静态检查元素类型。
- 提供整数索引和 `len(array)`。
- 数组值进入引擎堆，和字符串、map、record、函数使用同一类跟踪边界。
- v0 嵌入边界先保持保守：C callback 不接收数组参数，C 宿主只能通过只读
  owning handle 读取脚本返回的数组。

## 非目标

- 不支持混合类型数组。
- 不在 `[int]` 和 `[float]` 之间做隐式转换。
- 不支持负数索引。
- 不支持切片、spread、comprehension、iterator protocol 或数组方法。
- 不在第一版 C ABI 中支持数组参数、构造或修改。

## 语法

数组类型写成 `[T]`：

```nox
let values: [int] = [1, 2, 3];
let names: [str] = ["nox", "core"];
```

空数组必须从上下文获得期望类型：

```nox
let empty: [int] = [];
```

索引读取元素：

```nox
let first: int = values[0];
```

长度使用核心 intrinsic，而不是方法语法：

```nox
let count: int = len(values);
```

## 类型规则

- `[T]` 中的 `T` 必须是当前 type checker 能表示的类型。
- 赋给 `[T]` 的数组字面量只能包含 `T` 类型元素。
- 非空字面量在没有期望类型时可以从第一个元素推导元素类型。
- 空字面量没有期望类型时拒绝，诊断为 `empty array literal needs an expected type`。
- `array[index]` 要求左侧是 `[T]`，`index` 是 `int`，结果类型为 `T`。
- `len(value)` 要求 `value` 是数组并返回 `int`。

## 运行时规则

- 字面量从左到右求值。
- 数组索引越界是运行时诊断。
- 数组构造后不可变；当前没有 `push`、元素赋值或切片。
- 数组相等性暂不支持，直到容器 equality 语义明确。
- 数组对象由 heap 跟踪，宿主可通过 `collect_garbage` 触发回收。

## 已完成批次

1. parser/type checker 支持 `[T]`、数组字面量、索引和 `len`。
2. bytecode/VM/heap 支持数组构造、读取和运行时边界检查。
3. CLI 示例、负向 fixture、语言文档和测试已覆盖。

## 后续方向

- v0.0.3 暂缓可变数组，不加入 `push`、元素赋值或切片；决策见
  [0010 - 暂缓可变数组](decisions/0010-defer-mutable-arrays.md)。
- 如果未来需要数组增长能力，优先评估返回新数组的 copy-on-write 风格 API，而不是原地
  修改 alias 可见的数组。
- C ABI 已有只读 owning handle 用于读取脚本返回的数组；未来如需从 C 端构造或传入数组，
  需要单独设计。
- 如果未来加入泛型或 iterator，需要重新审视 `[T]` 的格式化和错误消息。
