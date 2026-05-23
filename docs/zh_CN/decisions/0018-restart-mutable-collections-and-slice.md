# 0018 - 重启可变集合与 slice 设计

- 状态：已采纳
- 日期：2026-05-23
- 涉及：语言 / 堆 / 运行时 / ABI / 工具链 / 诊断

## 背景

ADR 0010、0015 在 v0.0.3 与 v0.0.4 都暂缓了可变数组与 slice。当时的依据是：

- 真实脚本（scoreboard、health-check、std/* helper）用字面量加 `push_copy`、`concat`
  足以表达。
- C ABI 复合值只读 handle 已经稳定，引入 aliasing 会同时扰动 `const` 深浅语义、heap
  追踪、bytecode assignment target 和 verifier。

阶段 16-19 又把数据处理表面扩到了 csv、map、option/result、process、path、fs。这一轮
新增 helper 全部采用 copy-on-write 风格（`push_copy`、`reverse_copy`、`merge`、
`remove_copy`），需要逐步构建大数组或分组聚合的脚本会触发 O(n²) 拷贝路径，CLI 数据
处理脚本和测试 fixture 都开始遇到这个边界。

PLAN.md 阶段 23 把"可变集合 + slice"列为依赖 P22.1 的实现批次，但是否启动需要在本
ADR 给出明确门槛。

## 决策

v0.0.x 开发阶段重启可变集合与 slice 设计，但限定能力面：

- 数组元素更新：`arr[i] = value`，越界返回稳定 runtime 诊断 `runtime.index-out-of-range`。
  类型检查复用现有 array element type。
- 数组增长：仅提供 `array.push(arr, value) -> null` 与 `array.pop(arr) -> option[T]`，
  不引入 splice / insert / sort-in-place / clear。
- map 写入：`map[key] = value`、`map.remove(m, key) -> null`，重复 key 直接覆盖；
  没有的 key 写入即创建。
- slice：只做 `array.slice(arr, start, end) -> [T]` 拷贝语义，不引入语法糖
  `arr[start..end]`，不暴露 view / borrow。字符串 slice 沿用既有 `string.substring`。
- 默认值语义：可变数组与可变 map 仍按 alias 共享底层 storage；`let` / `const` 都允许
  容器内部 mutation，但 `const` 继续禁止重新绑定。

显式排除：

- 数组/字符串的语法级 range slice、迭代器协议、引用类型、view。
- C ABI 暴露 mutable handle。host 侧仍只能通过 host callback 接受脚本传入数组 / map
  的快照拷贝；mutation 不跨 ABI 边界。
- 任何在 mutation 路径暴露内部指针、unsafe buffer 或 borrow lifetime 的 API。

兼容影响：

- ADR 0010、0015 标记为被本 ADR 取代；本 ADR 在采纳后将在它们的状态行追加
  "被 0018 取代" 注脚（先在本文档登记，实现 PR 提交时再回填，避免双向 dangling
  链接）。
- `Value::Array` / `Value::Map` 需要由 `Rc<...>` 演进为 `Rc<RefCell<...>>`。这是
  Rust API 兼容破坏，必须在 CHANGELOG 标记为开发阶段调整，并保留旧字段访问 helper
  期间至少一轮过渡。
- LSP / formatter / project check：assignment target 解析 + 诊断、formatter 输出
  `arr[i] = x` 风格、project check JSON 仍向后兼容（无新顶层字段）。

权限边界：

- mutation 完全发生在 VM 内部值上，不接触 fs/env/net/timer/process 任何 capability。
- host callback 返回数组 / map 时按当前规则拷贝进 VM；mutation 不回流到宿主，
  embedding 端无新 capability。

embedding API 影响：

- Rust API：`Value::Array` / `Value::Map` 内部结构变更将通过新方法（`array_set`、
  `array_push`、`map_set` 等）公开，旧 `.as_array()` / `.as_map()` 行为保留为
  快照拷贝，不返回内部 `RefCell` guard。
- C ABI：维持只读 handle，不新增 mutable ABI 入口；C 端读到的仍是稳定快照。

诊断方案：

- `runtime.index-out-of-range`（稳定，新增 code）：数组写入越界。
- `type.assign-target`（稳定，新增 code）：`x = y` 中左侧不是合法 assignment target
  （例如对非容器调用 `[i] =`，或对 `const` 绑定试图 `=`）。
- `map.key-type-mismatch`（稳定，已存在的 map key 类型检查复用）：`map[key]` 写入
  时 key 类型不一致。
- `array.pop` 在空数组上返回 `option.none`，不产生 diagnostic。
- 现有 type alias、tuple、record 路径不引入新 code。

测试矩阵（阶段 23 必须覆盖）：

- 单元：array set / push / pop 正常路径、越界写、空 pop。
- 单元：map set / remove 覆盖、不存在 remove。
- 单元：alias 写入后另一引用观察到 mutation；clone-into-host 不观察到后续 mutation。
- bytecode verifier：assignment target span / stack 深度 / scope 不变量。
- LSP：mutation 表达式 hover 显示返回 null；formatter 保留 `arr[i] = x` 格式。
- C ABI：host callback 接收可变数组的快照后继续做 mutation，VM 端不受影响。
- 压力：N=10000 step 的 array push / map set 不爆 instruction budget；GC pressure 测试
  追加 mutation 路径。

放弃条件：

- 实现期间发现 `Rc<RefCell<...>>` 方案与现有 heap 压力测试或 cycle 检测冲突，无法在
  阶段 23 范围内得出稳定结论。
- C ABI 决策被推翻，必须暴露 mutable handle。
- aliasing 引发不可解释的 cycle GC 问题，超出阶段 33 性能工程预算。

一旦触发任一放弃条件，本 ADR 状态改为"已废弃"，并新写一份 ADR 解释退回原因。

## 后果

PLAN.md 阶段 23 可以进入实现批次。脚本不再需要为常见集合更新走 O(n²) copy
helper；同时 C ABI 与 embedding 表面保持向后兼容。代价是 `Value::Array` /
`Value::Map` 内部需要承担 `RefCell` 借用规则的运行时检查；这会引入一个新的运行时
失败模式（嵌套借用），但仅会在 host callback 错误使用 `Value::as_array` guard 时
触发，PLAN.md 阶段 33 内可以加 fuzz 覆盖。

## 备选方案

- 维持 copy-on-write：拒绝 mutation，把 `push_copy` 之类的命名升格为推荐方式。
  优点是 heap 与 ABI 完全不动；缺点是数据处理脚本性能不可接受，且与 PLAN.md 阶段
  16-19 引入的 helper 集合明显冲突。
- 完整借用 / view 体系：参考 Rust 的 `&mut [T]`、`Vec::drain`。完全超出 Nox 的脚本
  定位，会迫使 typecheck 引入借用检查器，violates 小核心原则。
- 仅 slice，不引入 mutation：能缓解 O(n²) 路径但仍要 copy，且无法解决 map 与 push。
  推迟到再下一个 ADR 仍要做同样规模的改动，没有节省工作量的好处。
