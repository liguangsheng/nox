# 模块系统设计

本文记录当前 v0 import/export 边界，以及后续模块系统演进方向。

## 当前模型

`nox_core` 负责 import 语义，但不直接读文件。宿主通过 module loader 根据 import
specifier 返回源码；默认 `nox` 运行时把相对路径解析到入口文件目录。

当前行为：

- import 在类型检查和字节码编译前完成。
- 同一次 `Engine` 操作内，同一 specifier 的源码只通过 module loader 加载一次：
  resolver 维护 `loaded` 集合，包括直接重复 import 和 diamond 形 import（多个
  模块共同 import 同一第三方模块）。这是单次操作内的缓存，不跨 `Engine` 调用。
- 循环 import 会产生诊断。
- 没有任何 `export` 的模块保留早期 v0 行为：顶层声明形成导入表面。
- 使用 `export` 的模块只向导入者暴露导出的顶层 `let`、`const`、`fn` 和 `record`。
- 私有声明仍保留在定义模块内部，导出的函数可以调用同模块私有 helper。
- 平铺导入表面必须没有重名。当前文件顶层声明、两个 import 暴露的同名声明、
  `record` 与 value/function 同名，都会产生 `module.name-conflict` 诊断。
- 命名空间 import 使用 `import "math.nox" as math;`。`math.member` 是静态模块成员访问，
  不创建运行时 object。
- 重复 import 同一 specifier 会去重，不算作命名冲突。

## 缓存边界

- 缓存只覆盖一次 `check` / `eval` / `inspect_bytecode` / `hover_type` 内部的多次
  import 解析，避免同一文件被重复读盘或重复 parse。
- 缓存不跨 `Engine` 方法调用；下一次 `eval` 仍然会从头加载模块。这避免了"在
  整个 Engine 生命周期内不可见地累积状态"，让 LSP 等宿主拿到的总是最新内容。
- LSP overlay（见 [embedding.md](embedding.md) 和 `nox` runtime 的
  `check_source_diagnostics_with_overlay`）在 module loader 层提供编辑器中
  打开文档的内容，可以与缓存共存：overlay 决定"从哪里读"，缓存决定"是否再读"。

## 选择的方向

Nox 选择显式 `export` 标记，而不是默认命名空间或通配导出：

```nox
export fn double(value: int) -> int {
    return helper(value);
}

fn helper(value: int) -> int {
    return value * 2;
}
```

导入者只能看到 `double`；`helper` 仍可被 `double` 使用。

## 命名空间 import

命名空间 import 避免把导出声明平铺到当前作用域：

```nox
import "math.nox" as math;

math.double(21);
```

resolver 在 import 解析阶段把 `math.double` 改写为导入模块内部的静态声明名。后续
type checker、compiler 和 VM 不持有模块对象，也不会把 `math` 当作 record 或 map。

命名空间暴露的成员与平铺 import 的可见表面一致：

- 有 `export` 的模块只暴露导出顶层声明。
- 没有 `export` 的模块暴露全部顶层声明，保持早期 v0 兼容。
- 缺失成员产生 `module.member-not-found`。

alias 不能和当前模块顶层声明重名，也不能和同模块内另一个 namespace alias 重名；
冲突产生 `module.name-conflict`。平铺 import 仍保留一个 minor 阶段，用于兼容已有脚本。
新代码优先使用命名空间 import，只有很小的单文件式 helper 模块才建议继续平铺导入。

## 非目标

- 当前不实现 wildcard import。
- 当前不实现 re-export。
- 当前不实现包 registry、依赖求解或 Node.js 兼容层。
- 当前不实现 selective import。

## 实现形状

解析 import 后，resolver 会把每个源文件作为 module unit 处理。内部仍编译成当前 flat
bytecode，但 resolver 会在有 `export` 的模块边界上限制导入表面，并在需要时重写私有
导入声明名，避免导入者直接访问。

type checker 在 records/functions 预声明前检查顶层名字唯一性。诊断 code 为
`module.name-conflict`，供 CLI JSON 和 LSP 消费。这个检查覆盖普通顶层重复声明，
也覆盖 import 展平后的可见声明冲突。

命名空间 import 的成员访问在 resolver 中完成：入口模块中符合 `alias.member` 的 field
access 会被改写为导入模块的内部声明名。record field access 继续由 type checker 处理；
只要 receiver 不是已知 namespace alias，就不会走模块成员解析。

## 已完成批次

1. 引入 module unit，同时保持旧导入行为。
2. parser 支持 `export` 标记。
3. import 时限制 exported surface，并测试私有 helper 对导出函数仍可见。
4. 增加示例和文档。
5. 为平铺 import 表面增加 `module.name-conflict` 诊断，覆盖本地重复声明、import 与
   本地声明冲突、两个 import 暴露同名声明，以及 record/value 同名。
6. 增加命名空间 import，补 parser、formatter、LSP completion 和成员/冲突负向测试。

后续模块计划见 [package-manifest-design.md](package-manifest-design.md)。
