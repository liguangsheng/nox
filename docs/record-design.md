# Record 设计

本文记录 Nox 第一版静态 record 设计。record 是固定字段集合的值容器，不是动态对象。

## 目标

- 提供命名 `record` 声明。
- 字段有显式静态类型。
- record 字面量必须恰好初始化所有声明字段。
- 字段访问在静态阶段检查存在性和类型。
- record 值进入 heap，和字符串、数组、map、函数保持同一跟踪模型。

## 非目标

- 不支持动态新增、删除字段或反射。
- 不支持方法、继承、prototype、trait 或 interface。
- 不支持匿名结构 record 类型。
- 不在第一版中支持 record mutation。
- 不在 v0 C ABI 中暴露 record 参数、返回值或字段读取 API。

## 语法

record 声明是顶层声明：

```nox
record User {
    name: str,
    score: int,
}
```

record 字面量使用类型名作为构造前缀：

```nox
let user: User = User { name: "nox", score: 42 };
```

字段访问使用点语法：

```nox
user.name;
user.score;
```

## 类型规则

- record 声明会在当前模块定义一个命名类型。
- 同一个 record 内字段名必须唯一。
- record 字面量必须引用已存在的 record 类型。
- 字面量必须初始化每个字段，且每个字段只能出现一次。
- 字段值必须匹配声明类型。
- 多余字段会被拒绝。
- `value.field` 要求 `value` 是 record，且 `field` 在该 record 上声明过。
- 字段访问结果类型就是字段声明类型。
- record equality 暂不支持。

## 运行时规则

- record 字面量按源码顺序求值字段值。
- record 值保留声明类型名和字段值。
- 字段访问在运行时按字段名读取；静态检查已经保证字段存在。
- 如果运行时缺少字段，这是内部一致性错误，不是普通脚本行为。

## 已完成批次

1. parser/type checker 支持 record 声明、命名 record 类型、字面量和字段访问。
2. bytecode、heap 和 VM 支持 record 构造与字段读取。
3. 示例、负向 fixture、CLI 测试、语言文档和 C ABI value kind 报告已补齐。

## 后续方向

- record mutation、方法和接口都需要独立设计。
- C ABI 读取 record 字段需要明确 handle 所有权和释放规则。
- 如果未来支持模块命名空间，record 类型名冲突诊断需要同步强化。
