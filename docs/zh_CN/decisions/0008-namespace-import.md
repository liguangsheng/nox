# 0008 - 命名空间 import

- 状态：已采纳
- 日期：2026-05-21
- 涉及：语言 / 模块 / 工具链

## 背景

v0.0.2 的 `import "math.nox";` 会把可见声明平铺到当前模块。这个模型适合小脚本，
但模块数量增加后容易出现导出名冲突，也会让导入方看不出一个名字来自哪个模块。

阶段 16.2 需要给模块表面一个长期方向，同时保持已有平铺 import 在一个 minor 阶段内
可用，避免已有脚本立即迁移。

## 决策

Nox 支持命名空间 import：

```nox
import "math.nox" as math;

math.double(21);
```

命名空间不是运行时 object。resolver 在 import 解析阶段把 `math.double` 静态改写为
被导入模块的内部声明名，之后 type checker 和 bytecode compiler 仍看到普通函数、
常量或 record 声明。

导出规则与平铺 import 一致：

- 被导入模块有任意 `export` 时，命名空间只暴露导出的顶层 `let`、`const`、`fn` 和
  `record`。
- 被导入模块没有 `export` 时，保留早期 v0 行为，顶层声明都属于模块表面。
- 私有声明仍可被同模块导出函数使用，但不能通过命名空间访问。

冲突规则：

- namespace alias 不能与当前模块顶层声明重名。
- 同一模块内不能重复使用同一个 namespace alias。
- 缺失成员诊断为 `module.member-not-found`。
- alias 冲突诊断为 `module.name-conflict`。

formatter 固定输出 `import "path" as alias;`。LSP completion 在 `alias.` 后只返回该模块
可见成员。

## 后果

模块使用方可以逐步从平铺 import 迁移到命名空间 import，降低导出名冲突和来源不清的
问题。实现仍保持 flat bytecode，不引入动态模块对象，也不改变 C ABI value 模型。

代价是 `as` 成为 import 语法中的关键字，不能再在该位置作为普通标识符使用。当前类型
语法仍不支持 `math.Point` 这样的命名空间类型名；record 类型名仍使用 resolver 改写后
的静态声明名。

## 备选方案

- 继续增强平铺 import：实现简单，但冲突会随着项目规模增长变成默认问题。
- 默认 namespace import、废弃平铺 import：长期更干净，但对已有 v0.0.2 脚本过于突然。
- 把 namespace 做成运行时 object：表达力更强，但会把模块系统、record field 和 object
  模型耦合，超出当前嵌入式语言目标。
