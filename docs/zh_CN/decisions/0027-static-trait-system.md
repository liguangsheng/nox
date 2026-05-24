# 0027 - 静态 trait 系统路线

- 状态：已采纳
- 日期：2026-05-24
- 涉及：语言 / parser / typecheck / formatter / LSP / 文档 / 诊断

## 背景

ADR 0020 在 Nox 还缺函数值和高阶 helper 的阶段，选择暂缓用户可声明的 trait /
interface，先用内建 marker 约束 `Equatable`、`Comparable`、`Stringify`、`Hashable`
支撑泛型 stdlib。这个方案已经让受限结构化约束可用，但它不是长期库抽象模型：

- 用户不能给自己的 record / enum 定义可复用能力边界。
- 标准库 helper 只能依赖硬编码 marker，扩展新能力必须改 typechecker。
- LSP、`nox doc` 和 cookbook 难以把“能力”展示成用户可理解的 API。
- 后续 macro、async/await 和更完整 stdlib 会继续需要可命名、可诊断、可文档化的约束。

因此阶段 61 重新评估完整 trait/interface 系统，并决定后续实现切片。

## 决策

Nox 采用单一关键字 `trait`，不再同时引入 `interface` 作为语法别名。`interface` 保留为普通
identifier，除非未来有独立的 host/interface IDL 需求。选择 `trait` 的原因是它更贴近
Nox 已有的泛型 bound 语法、Rust embedding 语境和“静态约束”含义；避免两个关键字造成文档、
诊断和 formatter 表面分裂。

第一轮 trait 是纯静态能力系统：

```nox
trait Display {
    fn to_str(self: Self) -> str;
}

record User {
    name: str,
}

impl Display for User {
    fn to_str(self: User) -> str {
        return self.name;
    }
}

fn label<T: Display>(value: T) -> str {
    return value.to_str();
}
```

核心规则：

- `trait Name { ... }` 只声明 required methods。method 形态是签名，不包含函数体。
- `impl Trait for Type { ... }` 为一个具体类型提供 method 实现。
- `Self` 只在 trait declaration 和对应 impl method signature 中有效。
- `T: Trait` 扩展现有泛型 bound 语法；内建 marker bound 继续可用，并逐步映射到标准 trait。
- 调用 `value.method(args...)` 时，typechecker 先按现有 record method / namespace member
  规则解析；若 receiver 类型是泛型参数且 bound 中有唯一 trait method 匹配，再解析为 trait
  method 调用。
- 对 concrete record / enum 的 trait method 调用只有在能静态定位唯一 impl 时才允许。
- 编译产物不引入动态 vtable；调用在 typecheck/compile 阶段解析到具体函数或保守拒绝。

## 与现有语言表面的关系

record / enum：

- record 和 enum 可以实现 trait。
- trait 不引入继承，不改变 record 字段访问，也不改变 enum variant 构造和 match 规则。
- 如果 record 已有同名 method，普通 `value.method()` 优先按 record method 解析；trait method
  需要在冲突诊断中明确指出候选来源。第一轮不提供显式 fully-qualified call 语法，冲突时拒绝。

type alias：

- `impl Trait for Alias` 与 `impl Trait for UnderlyingType` 在第一轮不允许同时存在。
- type alias 不创建 nominal 类型；因此 impl 的 coherence 以展开后的实际类型为准。
- 若 alias 展开会让 impl 目标变成容器、函数类型或泛型参数，第一轮拒绝。

函数值和 lambda：

- 函数类型可以作为 trait method 参数或返回值。
- 第一轮不允许为函数类型写用户 impl，避免把 closure capture、函数 identity 和 ABI 边界混在一起。
- trait bound 可以约束泛型函数中的函数值参数，例如 `T: Display` 与 `fn(T) -> str` 共存。

namespace import：

- trait、impl method 和普通函数都属于 module surface。`export trait Name` 允许下游使用 bound；
  非 exported trait 不能跨模块作为 bound。
- `import "x.nox" as x;` 后，`x.Display` 可作为 bound 使用；具体语法由阶段 62 的 parser
  实现确认，但必须保持 namespace source 明确，不做隐式全局搜索。

stdlib helper：

- 现有内建 marker 是兼容层。阶段 63 可以先提供 `std/traits.nox` 或 core prelude 风格的
  `Display`、`Eq`、`Ord`、`Hash`，但是否暴露 prelude 需要单独实现确认。
- 阶段 63 第一批实际落点是 `std/array.nox` 导出的实验性 `Eq` trait、基础 primitive
  impl，以及 `contains_equal<T: Eq>` / `dedupe_equal<T: Eq>`；这不建立 prelude，也不移除
  旧 marker helper。
- 旧的 `T: Equatable`、`T: Comparable`、`T: Stringify`、`T: Hashable` 在 v0.0.x 保持兼容；
  如果后续迁移到标准 trait，必须给出 docs 和 diagnostics 过渡期。

## Typechecker 与 coherence

第一轮采用保守 coherence：

- 同一 module graph 中，一个具体 `(Trait, Type)` 只能有一个 impl。
- 只有定义 trait 的模块或定义 nominal type 的模块可以写 impl。由于 Nox 当前没有跨包发布稳定性，
  这个孤儿规则可以先按 project/module graph 执行；external dependency 进入后必须用 lockfile
  source identity 参与冲突诊断。
- 不支持 blanket impl，例如 `impl<T: Display> Display for [T]`。
- 不支持 negative impl、specialization、auto trait、derive macro、associated type、generic
  associated type、higher-kinded type。
- 不支持 trait object 或动态 dispatch，例如 `dyn Display`、`trait object`、interface value。

method lookup 必须可解释：

1. 解析 receiver expression 类型。
2. 查找内建/record/namespace method；若唯一命中，使用现有路径。
3. 查找 receiver concrete type 的 trait impl，或泛型参数 bound 中的 trait method。
4. 如果没有候选，报 `trait.method-not-found` 或沿用现有 `record.method-not-found`，以 receiver
   类型是否涉及 trait bound 为准。
5. 如果多个候选同名且签名无法唯一决定，报 `trait.method-ambiguous`，不靠返回类型猜测。

阶段 62 MVP 的字节码后端把 impl method 编译为内部 mangled 函数定义，并在 method call
指令里按 receiver nominal type 分派。因此不同类型可以实现同名 trait method。源码级顶层函数
仍是普通 record method 糖的入口，所以当前实现继续保守拒绝 impl method 与顶层函数同名，使用
`trait.method-ambiguous`。这不是长期语义目标；长期语义仍以 receiver type 和 trait impl
唯一性为准，后续可通过 typed AST rewrite 精确区分普通 record method 与 trait method。

## 诊断方案

新增或稳定以下 code：

- `trait.duplicate`：同一作用域重复声明 trait。
- `trait.not-found`：bound 或 impl 引用未知 trait。
- `trait.impl-duplicate`：重复 `(Trait, Type)` impl。
- `trait.impl-orphan`：违反孤儿规则。
- `trait.impl-incomplete`：impl 缺少 required method。
- `trait.method-signature-mismatch`：impl method 签名与 trait required method 不一致。
- `trait.bound-unsatisfied`：调用泛型函数或解析 trait method 时类型不满足 bound。
- `trait.method-not-found`：bound/impl 场景下找不到 required method。
- `trait.method-ambiguous`：多个 trait method 或 trait/record method 冲突，无法保守解析。

现有 `generic.constraint-unsatisfied` 和 `generic.constraint-unknown` 继续用于内建 marker。迁移到
用户 trait 后，新的 trait 诊断优先；旧 code 不重定义含义。

## 工具和文档要求

阶段 62 的 MVP 必须同步以下表面：

- Parser/AST：trait declaration、impl declaration、`Self`、trait bound。
- Formatter：稳定打印 trait / impl block 和 method signature。
- `nox doc`：输出 exported trait、required methods 和 exported impl summary。
- LSP：document symbol / workspace symbol 识别 trait 和 impl；hover 显示 trait bound；
  definition 能从 bound 或 impl method 跳到 trait declaration 或 impl block。rename 仍按
  Phase 52/60 的保守规则，不能证明安全时返回 `null`。
- CLI JSON diagnostics：新增 code 要与 LSP diagnostics 保持 parity。
- Docs：中英文 language docs、diagnostics docs、cookbook 和 CHANGELOG 同步。

## 后果

这个路线把 ADR 0020 的过渡性 marker 约束升级为可长期承诺的静态 trait 模型，同时避免直接承担
动态 dispatch、trait object、blanket impl 和关联类型的复杂度。Nox 可以先服务标准库和项目内
泛型抽象，再根据真实压力扩展。

代价是第一轮表达力有限：用户不能写 `dyn Trait`，不能给所有 `T` blanket impl，也不能用关联类型
表达 iterator item。它仍然比继续堆内建 marker 更可维护，因为 API、docs、LSP 和 diagnostics
都有可命名的语言表面。

## 实现顺序

1. Parser/AST 接受 trait / impl declaration，但先不改变 runtime dispatch。
2. Typechecker 建立 trait table 和 impl table，校验重复、完整性、签名一致性与孤儿规则。
3. 泛型 bound 接入 trait lookup，支持 trait method call 的静态解析。
4. Formatter、`nox doc`、LSP symbol/hover/definition 同步识别新声明。
5. 标准库小范围迁移，优先验证 display/equality/order/hash 类能力。
6. 只有当真实场景证明必要时，再评估 fully-qualified method call、blanket impl、associated
   type 或动态 dispatch。

## 备选方案

- 继续只用 ADR 0020 内建 marker。未选择，因为它会把标准库能力继续硬编码进 typechecker，
  用户项目无法扩展。
- 采用 `interface` 关键字。未选择，因为 Nox 第一轮没有动态 object 或 host IDL 语义，
  `interface` 容易暗示运行时对象协议。
- 同时支持 `trait` 和 `interface`。未选择，因为会增加 formatter、diagnostic、docs 和 LSP
  复杂度，却没有新的表达力。
- 直接实现 Rust 风格完整 trait。未选择，因为 blanket impl、关联类型、specialization 和 dyn
  dispatch 会显著扩大 typechecker、VM、C ABI 与 release gate 风险。
