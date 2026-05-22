# 堆与对象生命周期设计

本文记录 Nox v0.0.2 的堆模型选择、当前实现状态以及后续要解决的问题。这是 PLAN.md
阶段 12.1 的设计文档：先写清楚约束，再决定要不要替换实现。

## v0.0.3 复审结论

2026-05-21 基于阶段 17.3 和 18.2 重新评估后，v0.0.3 继续采用 `Rc + Weak` 加弱引用
追踪表，不引入 arena handle、tracing GC、cycle collector 或 interior mutability。

复审依据：

- 长期宿主持有 Rust `Value` 已由 `host_held_rust_values_keep_heap_objects_until_dropped`
  覆盖：宿主持有期间对象保持存活，drop 后 `collect_garbage()` 能把弱引用表清到 0。
- C ABI owning handle 已由 `c_abi_handles_keep_heap_objects_until_freed` 覆盖：array/map/record
  handle free 前保持对象存活，free 后可清理。
- v0.0.4-beta 嵌入压力已补 `repeated_eval_and_check_collect_transient_heap_values`、
  `repeated_host_callback_returns_do_not_accumulate_heap_values` 和
  `repeated_c_abi_handle_free_collects_nested_heap_values`：反复 `check` 不留下 heap 值，
  反复 `eval` 的嵌套 record/array/map/string 在结果 drop 后可回收，host callback 返回值
  不在 engine heap 里累积，C record handle free 后嵌套值也能清到 0。
- 阶段 18.2 已决定 v0.0.3 暂缓可变数组，array/map/record 继续构造后不可变，不需要
  `RefCell<Array>`、arena handle 或 mutation log。
- 阶段 18.3 已决定 v0.0.3 暂缓源码级函数类型和高阶函数，不把闭包逃逸或跨 ABI 函数调用
  作为稳定表面。

因此 v0.0.3 接受下面限制：

- 循环引用不会被回收；当前语言表面没有可变共享容器，真实脚本自然构造循环的路径有限。
- 宿主长期持有 `Value` 或 C handle 时由 Rust ownership / C free 函数控制生命周期；宿主
  需要在合适时机 drop/free，并可调用 `collect_garbage()` 清理弱引用表。
- `heap_object_count()` 是观测和测试指标，不是精确 GC telemetry。

## 当前模型

`crates/nox_core/src/heap.rs` 中的 `GcHeap` 是一个"弱引用追踪表"：

- 每次分配 `Rc<str>`、`Rc<Array>`、`Rc<Map>`、`Rc<Record>`、`Rc<Function>` 时，
  `GcHeap` 把 `Weak` 引用挂进对应的 `Vec`，方便观察对象数量。
- 真正的所有权仍然是 `Rc`：`Value::Array(Rc<Array>)` 等持有引用，VM 栈、env、
  函数闭包都通过 clone `Rc` 共享。
- `collect()` 只是定期把已经 `strong_count == 0` 的弱引用从表里清掉，**不是**
  tracing GC，也不会回收"实际不可达但相互持有强引用"的循环。
- `Engine::heap_object_count()` 返回当前活跃 `Rc` 数量，用于宿主/测试观察泄漏。

所以"堆"在 v0.0.2 阶段更像 Rust 标准库提供的 reference counting + 一个统计表，
而不是独立的 garbage collector。

## 已知约束

- **循环引用会真泄漏**：`Array<Array>` 自包含、record 字段间接互指、闭包持有
  自身闭包，全都没有 cycle collector，会让对象永远不被释放。
- **函数对定义环境是 `Weak`**：`FunctionKind::Script { env: Weak<RefCell<EnvData>> }`。
  函数定义时把所属 env 降级为 `Weak`，调用时升级；如果 env 已经被 drop，调用
  会失败。这避免了"函数反向延长 env 生命周期"。v0.0.3 暂缓源码级函数类型和高阶函数，
  因此不把闭包逃逸或长期跨宿主调用承诺为稳定表面。
- **string 是 `Rc<str>`**：字符串 immutable，逻辑相等可以但每次创建新 Rc 不会
  自动 intern。`alloc_string` 也不会去重。
- **host-held `Value`**：宿主拿到 `Value::Array(Rc<Array>)` 后，Rust 端的 `Rc`
  会保持对象活着，直到宿主 drop。不需要额外注册到 heap。
- **跨 Engine 复用**：`Value` 只在产生它的 `Engine` 内部使用是安全的；跨
  `Engine` 共享没有定义行为，相关接口也没暴露。

## 压力测试覆盖

`crates/nox_core/src/language_tests.rs` 已经覆盖：

- 递归 / 深栈：`runs_recursive_function_pressure`、`runs_loop_pressure`、
  `runs_deep_scope_pressure`。
- 函数值：`heap_tracks_and_collects_script_function_values` 覆盖脚本函数值的 heap 追踪。
- 容器嵌套：`heap_keeps_nested_record_container_fields_alive` 等。
- host-held 值：`api_tests::*` 多次通过 `Engine::eval` 拿回 `Value` 比较结构。
- 反复 eval/check：`repeated_eval_and_check_collect_transient_heap_values` 覆盖 250 次
  嵌套 record/array/map/string 脚本；每次 `check` 后 heap count 为 0，每次 `eval`
  返回值 drop 并 `collect_garbage()` 后 heap count 也为 0。
- host callback 返回值：`repeated_host_callback_returns_do_not_accumulate_heap_values`
  覆盖 200 次 Rust host function 返回 string/array/map 后被脚本消费；每轮显式收集后
  heap count 回到 0，证明 callback 返回值不会在 engine heap 观测表里累积。
- 长期宿主持有值：`host_held_rust_values_keep_heap_objects_until_dropped` 会批量持有
  string / array / map / record `Value`，确认宿主 drop 前对象保持存活、drop 后
  `collect_garbage()` 能把弱引用表清到 0。
- C owning handle：`c_abi_handles_keep_heap_objects_until_freed` 会批量持有
  `NoxCoreArrayHandle` / `NoxCoreMapHandle` / `NoxCoreRecordHandle`，确认 free 前对象保持
  存活、对应 free 函数释放后可被清理。`repeated_c_abi_handle_free_collects_nested_heap_values`
  进一步覆盖 120 次嵌套 record handle 创建/free，确认 record 持有的 string、array 和 map
  会随 top-level handle 释放。

`Engine::collect_garbage()` 在被显式调用时清理已经 dropped 的 Weak 引用。
没有 generation / mark phase。

## 选项对比

下面记录三种潜在的"真正堆模型"，便于将来再做权衡：

### A. 继续 Rc + Weak

- 优势：实现最简单，借用 Rust 标准库；和 Rust API、C ABI 行为一致；零外部依赖。
- 代价：循环引用会泄漏。
- 适用：脚本里几乎都是树形数据时（v0.0.2 的 array/map/record 大多如此）。

### B. Arena + handle

- 优势：对象生命周期由 arena 控制；脚本结束时一次性释放；handle 在 Rust/C 边界
  上稳定。
- 代价：需要为每种值类型设计 handle、引入显式 `Engine::drop_handle` 类的 API；
  和现有 `Value` 接口变化大。
- 适用：宿主需要持有大量长生命周期对象、并且要避免循环引用泄漏时。

### C. Tracing GC

- 优势：可以回收循环引用；将来支持 weak ref、finalization。
- 代价：需要可达性根集合（VM 栈、env、host roots）、对象遍历元数据、暂停点；
  和现有 `Rc` 大量交互需要重写值表示。
- 适用：脚本运行时间很长、堆里大量含循环的数据结构。

## v0.0.3 暂不做

- 不改 `Value` 表示：保留 `enum Value { ... Rc<...> ... }`。
- 不把 array/map/record 改成 interior mutability：v0.0.3 容器表面保持构造后不可变。
- 不引入 cycle collector：v0.0.2 不承诺回收循环引用，宿主应避免在共享对象间制造
  环；脚本端没有显式可变共享，所以构造环需要刻意为之。
- 不引入 finalization / drop trampoline：宿主用 `Drop` 兜底即可。
- 不引入 `Engine::drop_value` 或 C handle registry：当前 Rust `Value` 由 Rust ownership
  管理，C 复合值由 owning handle + 对应 free 函数管理，测试覆盖了长生命周期释放路径。
- 不引入 generational GC。
- 不暴露 `GcHeap` 到对外 API：它仍是 `pub(crate)`。

## 下一步触发条件

当下面任一条件出现，再回来重新评估堆模型：

1. 出现真实脚本场景（不是 fuzz 构造）必须依赖循环引用：例如递归 record 互相引用、
   "节点 + 父指针" 的图结构。届时优先考虑选项 B（arena + handle），因为它最接近
   现有 `Rc` 的语义且不必引入 tracing GC。
2. 宿主嵌入需要长期持有 `Value`（例如几小时的 LSP session）并出现内存增长：
   评估是否引入 `Engine::drop_value(value)` 类 API、或要求宿主主动调用
   `collect_garbage()`，给出基线数字（见 [benchmarks.md](benchmarks.md)）。
3. C ABI 要暴露 array / map / record 内部数据：决定是按 handle 模型还是
   按 borrow 模型设计（见 PLAN.md 阶段 13.2）。
4. 未来重新启动可变容器、函数值逃逸或跨 ABI 函数调用时，先回到本文更新生命周期和
   root 管理方案，再改 `Value` 表示。

只要这些条件没出现，就维持选项 A，把精力投入到语言表面和工具链上。
