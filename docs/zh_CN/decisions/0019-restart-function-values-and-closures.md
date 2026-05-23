# 0019 - 重启函数值、闭包与高阶函数设计

- 状态：已采纳
- 日期：2026-05-23
- 涉及：语言 / parser / typecheck / VM / ABI / 工具链 / 诊断

## 背景

ADR 0011 和 0015 在 v0.0.3 / v0.0.4 暂缓了源码级函数类型和高阶函数。当时的依据是：

- 嵌入 API 还没稳定，闭包生命周期与 host callback 注册边界存在重复风险。
- typecheck、formatter、LSP、VM 同时落地会破坏阶段 3 完成的诊断契约和阶段 4
  的兼容矩阵。

阶段 16-19 的数据处理表面（数组、map、option、result、CSV、JSON）让脚本可以写大型
转换逻辑，但所有变换都必须命名静态函数或硬编码字面量。当前 `std/option.nox` 和
`std/result.nox` 提供的非闭包版 helper（`unwrap_or`、`map_err_to_str`）无法表达
"按字段排序"、"按谓词过滤"、"自定义 reducer" 这种场景，宿主侧只能为每个场景写专用
host function。

PLAN.md 阶段 24 把"函数值 + 闭包 + 高阶函数"列为依赖 P22.2 的实现批次。本 ADR 决定
是否启动。

## 决策

v0.0.x 开发阶段重启函数值与闭包设计，但严格限定首批能力：

- 函数类型语法：`fn(T1, T2) -> R` 允许出现在参数、返回值、`let` 注解、数组 / map
  元素类型中。不允许直接出现在 record 字段中（在阶段 25 trait 决策之前避免与方法槽
  冲突）。
- 函数值：以现有命名 `fn` 声明为基础，新增 lambda 字面量 `fn(x: int) -> int { ... }`
  和 `|x: int| -> int x + 1` 中**只接受前一种语法**；reject 短 lambda 以避免和位运算
  `|` 冲突。
- 闭包捕获：按 by-value 捕获，捕获时拍快照；mutation（在 0018 引入后）通过 alias 共享，
  因此闭包内对捕获 `array` / `map` 的写入对外部可见，对 `int` / `str` 等不可变值的
  写入仅影响闭包内副本。捕获 list 静态分析得出，不允许显式 capture clause。
- 高阶 stdlib helper：在 `std/array.nox` 和 `std/map.nox` 中新增 `map`、`filter`、
  `reduce`、`for_each`。命名与既有 helper 区分（`array.map_fn` / `array.filter_fn`），
  避免和 `std/map.nox` 模块同名引起 import 误读。

显式排除：

- partial application、currying、operator section。
- 闭包返回闭包嵌套捕获优化；首版允许但不承诺性能。
- C ABI 暴露脚本闭包 handle。host callback 注册继续是 host→VM 单向；脚本函数不能从
  C 端调用回去（与 0015 一致）。
- 跨模块闭包传递（闭包跨模块边界传递是合法的，但 module loader 不为此添加新承诺）。

兼容影响：

- ADR 0011、0015 关于 "源码级函数类型" 与 "高阶函数" 的暂缓决策被本 ADR 取代。
- parser 新增 `fn(...) -> T` 类型语法。这是兼容扩展（之前是 syntax error）。
- typecheck 新增 `Type::Function`（已有内部表示）暴露到源码级；现有 namespace import
  导出函数仍然按现有 binding 形态注册，不会立刻变成 first-class 函数值，除非显式以
  `let f: fn(...) -> ... = name;` 绑定。
- bytecode 新增 `MAKE_CLOSURE`、`LOAD_UPVALUE`、`CALL_VALUE` 指令；bytecode verifier
  扩展覆盖。这是 internal change，C ABI 无影响。

权限边界：

- 闭包内调用 host function、stdlib、fs/net/env/timer/process 全部沿用现有 capability
  模型。闭包不能"提权"也不会"降权"。
- 取消执行 (cancellation) 同样作用于闭包调用栈帧；现有 instruction budget 直接覆盖。

embedding API 影响：

- Rust API：`Value` 新增 `Closure(Rc<Closure>)` 变体；现有 `Value::Function` 仅用于
  顶层命名函数，保持兼容。`Engine::register_host_function` 不变。
- C ABI：维持 0015 决策，不引入 script-function handle。脚本闭包对 C 端不可见；
  host callback 仍只能接收原始 `NoxCoreValue`，调用 closure 由脚本侧完成。

诊断方案：

- `function.type-mismatch`（稳定，新增 code）：函数值参数 / 返回类型与 expected
  `fn(...)` 类型不一致。
- `closure.capture-mutability`（稳定，新增 code）：在 0018 mutation 启用前，闭包内
  对非容器 capture 的赋值产生该 diagnostic（避免 silent shadow）。
- `function.arity-mismatch`（复用现有 arity 检查 code）：闭包调用参数数量不匹配。
- 现有 `enum.variant-not-found`、`record.method-not-found`、`generic.infer-failed`
  保持不变。

测试矩阵（阶段 24 必须覆盖）：

- 单元：lambda 字面量解析、类型检查、调用、嵌套捕获、递归（命名 `fn` 内引用自身）。
- 单元：`std/array.nox map/filter/reduce/for_each` 正负向；闭包内异常通过 `?` 传播。
- VM：闭包跨模块返回；闭包从 host callback 返回（应被禁止并报 diagnostic）。
- bytecode verifier：MAKE_CLOSURE / LOAD_UPVALUE 在循环、try 边界内的栈深度不变量。
- LSP：lambda hover 显示函数类型；signature help 在闭包调用处显示参数。
- 性能：1000 个闭包嵌套调用不爆 instruction budget；heap 压力测试覆盖闭包环境释放。

放弃条件：

- 实现期间发现 closure environment lifetime 与 0018 mutation 决策耦合到无法独立测试。
- LSP 类型推断在 lambda 推断处无法保持 < 100ms 响应（current performance budget）。
- bytecode 变更破坏 fuzz corpus 中既有 case 的 verifier 不变量。

任一触发即将本 ADR 状态改为"已废弃"，并写后续 ADR 说明退回原因。

## 后果

PLAN.md 阶段 24 可以进入实现批次。脚本能用 `array.map_fn(arr, fn(x: int) -> int x * 2)`
表达通用变换，stdlib 不再需要为每种 reducer 写专用版本；与 0018 配合可实现高效就地
更新。代价是 VM 复杂度上升一档，闭包 capture / upvalue 引入新一类 GC 压力源；阶段 33
需要把闭包加入 heap pressure 基线。

## 备选方案

- 仅允许把命名函数作为值：用 `fn` 声明 + `let f: fn(...) -> ... = name;`。
  优点是不需要 MAKE_CLOSURE 指令；缺点是无法捕获局部 state，map/filter 等场景仍要新
  顶层函数才能写，与初衷不符。
- 引入完整 trait+closure 一起做：等 0020 trait/约束式泛型决策后统一启动。代价是
  阶段 24 工作量翻倍，且会延迟阶段 16-19 数据处理脚本的实际使用。
- 闭包默认 by-reference 捕获：靠近 JavaScript 直觉，但与现有不可变 binding + 即将
  推出的 0018 mutation 语义混合时容易产生 "看似拷贝但实际共享" 的歧义。明确 by-value
  + alias-for-containers 更易解释。
