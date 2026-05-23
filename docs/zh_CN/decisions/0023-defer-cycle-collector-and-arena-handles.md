# 0023 - 暂缓 cycle collector 与 arena handle

- 状态：已采纳
- 日期：2026-05-23
- 涉及：堆 / 性能 / 运行时

## 背景

阶段 33 性能工程要求评估 GC/heap 策略：是否需要 cycle collector、arena handle 或
compacting GC。当前 heap 实现：

- `Value::Array` / `Value::Map` / `Value::Record` 等通过 `Rc<...>` 引用，weak
  references 用于 lambda 环境捕获（`env.downgrade()`）和函数闭包。
- `GcHeap` 内部维护 strong owning 的 `Vec` 跟踪长寿命容器，并周期性 collect 通过
  `Rc::strong_count == 1` 释放不再引用的容器。
- 阶段 23 引入的可变集合通过 `Rc<RefCell<...>>` 而非纯 `Rc`，没有引入新的 cycle 风险，
  因为 mutation 只发生在元素层而非 owner 层。
- 阶段 24 lambda 字面量通过 `Vm Function` 指令 + `env.downgrade()` 弱引用 outer env，
  避免闭包 capture 形成 strong cycle。

阶段 33 同时落地了 `Engine::set_max_call_stack_depth(Option<usize>)` 配置，让宿主限制
脚本调用栈深度，避免无穷递归 / 误用闭包导致的资源耗尽。

## 决策

v0.0.x 开发阶段**不引入 cycle collector / arena handle / compacting GC**。继续依赖
`Rc` 引用计数 + 当前 `GcHeap` 周期性 strong-count==1 释放策略；闭包 capture 通过
`Weak<Env>` 显式避免 cycle。

理由：

1. **当前 cycle 风险已被 Weak 隔离**：所有 closure 与命名 fn 的 outer-env 引用都是
   weak。除非脚本能在用户级别构造 Rc cycle（v0.0.x 无此能力），否则 cycle collector
   带来的收益为零。
2. **Arena 收益不显著**：当前 heap 压力测试（`heap pressure` / `loop pressure`）
   表明常态对象 churn 在毫秒级；arena/compacting 需要重做 `Rc` 到自定义 handle
   类型，破坏 C ABI 只读 handle 决策（ADR 0005）。
3. **set_max_call_stack_depth 已经覆盖最常见 DoS 路径**：脚本 OOM 大多由无穷递归
   或无界容器增长触发；阶段 33 加的 call-stack 限制对前者直接拦截，instruction
   budget（已存在）对后者拦截。

显式不做的事：

- 引入 `tracing_gc` / `cycle_collector` crate。
- 把 `Rc<Array>` 换成 arena handle index。
- 在 release-gate 加 heap object count 上限作为强制门槛（保留为可选 budget）。

允许保留 / 已经存在：

- `set_instruction_budget(Option<usize>)`：限制 VM 总指令数。
- `set_max_call_stack_depth(Option<usize>)`：限制 script function 调用栈深度。
  超出时返回稳定 code `runtime.call-stack-overflow`。
- `GcHeap::collect()`：宿主可手动触发释放。
- benchmark suite：已经在阶段 8 / 15 验证；阶段 33 不强制扩展。

兼容影响：

- `Engine` 公共 API 新增 `set_max_call_stack_depth`；现有调用未设置时行为不变（
  无限制 native stack 限制兜底）。
- `runtime.call-stack-overflow` 加入 `docs/{en,zh_CN}/diagnostics.md`。

权限边界：

- call-stack depth 限制不涉及任何 capability。所有脚本默认无上限（保留向后兼容），
  宿主显式调用 `set_max_call_stack_depth` 启用。

embedding API 影响：

- Rust API 加 `Engine::set_max_call_stack_depth(Option<usize>)`。
- C ABI 暂不暴露；宿主可通过 Rust 调用配置。
- 不破坏现有 `Value` / handle 接口。

测试矩阵：

- `rejects_deep_recursion_when_max_call_depth_is_set`：8 层上限 + 100 层递归触发
  稳定 code。
- `allows_recursion_within_max_call_depth_limit`：64 层上限 + 10 层递归正常完成。
- 既有 `runs_recursive_function_pressure` 不受影响（无上限）。

放弃条件：

- 真实脚本压力证明 `Rc` 周期 leak 不可忽视（如长跑 host process 中 heap 持续增长）。
- 嵌入方需要 arena handle 跨 ABI 暴露（与 ADR 0005 冲突，需新 ADR）。
- 性能 benchmark 显示 `Rc::clone` / `RefCell::borrow` 在 hot path 占比 >10%
  且 arena 能减少一档。

## 后果

v0.0.x 保持现有 heap 策略；call-stack depth 给嵌入方一个明确的资源边界 hook。
代价是无法回收用户构造的 Rc cycle —— 但当前语言不允许构造（Weak 已隔离），所以无
实际影响。

## 备选方案

- 立即引入 tracing GC：破坏 C ABI、显著增加运行时复杂度；当前没有用户级 cycle
  构造能力，收益为零。
- 引入 arena handle：要求重做 `Rc<Array>` → 索引 + 全局表；阻碍 C ABI 只读 handle
  决策。
- 不引入 call-stack depth：依赖 native stack 抛 stack overflow signal；这会让脚本
  错误转成 SIGSEGV 而非 diagnostic，破坏宿主隔离能力。
