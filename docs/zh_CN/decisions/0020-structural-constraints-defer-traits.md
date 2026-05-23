# 0020 - 暂缓 trait / interface，引入受限结构化约束

- 状态：已采纳
- 日期：2026-05-23
- 涉及：语言 / parser / typecheck / 工具链 / 诊断

## 背景

阶段 12 已经实现了泛型函数（unconstrained），脚本可以写 `fn pick<T>(xs: [T]) -> T`。
阶段 24（依赖 ADR 0019）将让函数值与闭包成为 first-class，typical scenario 是
`array.map_fn<T, U>(xs: [T], f: fn(T) -> U) -> [U]` 这种泛型 + 函数值的复合签名。

如果允许任意 `T`，调用方传入 `fn(int) -> int` 但实际数组是 `[str]` 时，typecheck 必须
拒绝；这部分目前已经覆盖（generic infer-failed code）。但是真实场景很快会要求
"`T` 必须可比较"、"`T` 必须可序列化"等约束，例如：

- `array.sort_fn<T>(xs: [T], cmp: fn(T, T) -> int) -> [T]`：cmp 显式传入即可。
- `array.dedupe<T>(xs: [T]) -> [T]`：需要 `T` 的相等性。

PLAN.md 阶段 25 把"trait / interface 或约束式泛型"列为依赖 P22.3 的实现批次。本 ADR
决定首批是否引入 trait 系统，还是用更轻量的结构化约束。

## 决策

v0.0.x 开发阶段**不引入 trait / interface 声明**，转而采用受限的结构化约束：

- 内建结构约束 set：`Equatable`、`Comparable`、`Stringify`、`Hashable`，作为 typecheck
  级别的固定 trait-like marker，不是用户可声明的语法。
- 泛型约束语法：`fn dedupe<T: Equatable>(xs: [T]) -> [T]`。约束只能出现在内建 marker
  集合中，不允许用户自定义 trait。
- 内建 marker 与现有类型的匹配规则在 typecheck 内硬编码：`int` / `float` / `str` / `bool`
  实现全部内建 marker；`record` 实现 `Equatable` / `Stringify` 当且仅当所有字段都实现；
  `enum` 实现 `Equatable` 当所有 variant payload 都实现。容器（array、map、option、result、
  tuple）按 element 推导。函数类型只实现 `Stringify`（输出 `"<function>"`）。
- stdlib 落点：`array.sort_fn`、`array.dedupe`、`array.contains_value`、`map.equals`
  使用上述约束；闭包 lambda 内对约束类型的 `==` / `<` 调用复用现有运算符。

显式排除：

- 用户声明的 `trait` / `interface` / `impl` 语法。
- 关联类型、类型类层级、孤儿规则、自动 derive 宏。
- HKT、高阶约束、bounded existential。
- 在 record 字段中嵌入约束（必须等 trait 系统决策再评估）。

兼容影响：

- 阶段 12 已经存在的 unconstrained 泛型继续可用；本 ADR 是 strict superset，不破坏既有
  脚本。
- ADR 0014（option/result 重启）与本 ADR 协同：`option<T>` / `result<T, E>` 现在可以
  在 stdlib 中用 `T: Equatable` 增加 `option.equals` 这类 helper，但本 ADR 不强行落地，
  实现在阶段 25。

权限边界：

- 不涉及 fs/env/net/timer/process capability。约束在编译期完成，runtime 行为不变。

embedding API 影响：

- Rust API：`Type::Generic` 上加 `constraints: Vec<ConstraintMarker>` 字段（additive）。
  现有 `Type::Generic("T")` 等价于无约束。所有 helper 函数和 host callback registration
  shape 不变。
- C ABI：完全不变。host function 注册不能引入约束泛型；host function 仍按现有 monomorphic
  签名注册。

诊断方案：

- `generic.constraint-unsatisfied`（稳定，新增 code）：调用泛型函数时实参类型不满足
  声明的约束。message 应解释具体缺哪一项 marker（例如 "type 'fn(int) -> int' does not
  implement Equatable"）。
- `generic.constraint-unknown`（稳定，新增 code）：源码使用了不在内建 set 中的约束名。
  message 应列出已知约束。
- 现有 `generic.infer-failed` 保持不变；约束推导失败优先用 `generic.infer-failed`，
  约束已知但不满足才用新 code。

测试矩阵（阶段 25 必须覆盖）：

- 单元：4 个内建 marker 在 int/float/str/bool/record/enum/array/map/tuple/option/result/function
  上的正负向覆盖。
- 单元：`array.sort_fn` / `array.dedupe` 真实使用；约束推导失败的稳定诊断。
- typecheck：约束在递归泛型函数中正确传播；嵌套容器（`[[int]]`）的约束传播。
- 反例：用户尝试声明 `trait Foo {}` 触发 parser 错误并指向"不支持用户 trait"诊断。
- LSP：泛型函数 hover 显示约束；signature help 显示约束。
- C ABI：host function 注册带约束触发 registration 错误（保持简单签名）。

放弃条件：

- 真实脚本压力证明内建 4 marker 不够，需要扩展到 ≥ 8 marker。届时应升级为正式 trait
  设计，废弃本 ADR。
- 约束传播在嵌套泛型 + 闭包混合场景下产生不可解释的 typecheck 报告。
- 与 0019 闭包决策的交互引发"约束在闭包内丢失"的回归。

## 后果

PLAN.md 阶段 25 可以进入实现批次，给阶段 24 的高阶 stdlib helper 提供必要的约束。
脚本与 stdlib 不需要为每种类型写 monomorphic 版本，同时避免引入完整 trait 系统的
工程代价（孤儿规则、关联类型、derive macro）。代价是约束 set 固定，未来需要扩展时
必须修改 typecheck 内置表，不能由用户在脚本侧扩展。

## 备选方案

- 完整 trait / interface 系统：表达力强，但需要新语法、impl 求解器、孤儿规则、错误
  诊断重写、LSP 跳转扩展。在当前没有真实多约束场景的情况下成本远大于收益。
- 完全不引入约束：靠运行时检查 + 隐式 `==` / `<` 行为。运行时失败比编译期失败更难
  定位，且与"从一开始静态类型"的设计原则冲突。
- 仅引入 `Equatable`：能让 `dedupe` / `contains_value` 工作，但 sort 需要传 cmp，
  且未来若新增 `Hashable` 仍要再写 ADR。一次性铺好 4 个 marker 更经济。
