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

- 现有内建 marker 是兼容层，不再作为新标准库抽象的首选扩展点。
- 已落地的第一批 trait 表面是 `std/array.nox` 导出的实验性 `Eq` trait、基础 primitive
  impl，以及 `contains_equal<T: Eq>` / `dedupe_equal<T: Eq>`；这不建立 prelude，也不移除
  旧 marker helper。
- v0.0.x 内，`contains_value<T: Equatable>` / `dedupe<T: Equatable>` / `std/test.nox`
  的 `assert_eq<T: Equatable>` / `assert_ne<T: Equatable>` 继续使用内建 marker，避免破坏
  已有脚本和诊断 code。
- 新增 equality helper 优先使用 `Eq`；新增 display/order/hash 类 helper 必须先设计对应
  `Display` / `Ord` / `Hash` trait 表面，再决定是否暴露在 `std/traits.nox`、具体模块
  或未来 prelude 中。
- 旧的 `T: Equatable`、`T: Comparable`、`T: Stringify`、`T: Hashable` 在 v0.0.x 保持兼容；
  如果后续迁移到标准 trait，必须给出 docs、diagnostics 和 CHANGELOG 过渡期。

## Typechecker 与 coherence

第一轮采用保守 coherence：

- 同一 module graph 中，一个具体 `(Trait, Type)` 只能有一个 impl。
- 只有定义 trait 的模块或定义 nominal type 的模块可以写 impl。由于 Nox 当前没有跨包发布稳定性，
  这个孤儿规则可以先按 project/module graph 执行；external dependency 进入后必须用 lockfile
  source identity 参与冲突诊断。
- 不支持 blanket impl，例如 `impl<T: Display> Display for [T]`。
- 不支持 generic impl，例如 `impl<T: Eq> Eq for Box<T>` 或 `impl<T> Display for T`；Nox
  当前也没有泛型 nominal record/enum 作为 impl target。
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

## 第三轮路线

阶段 89 复评后，trait/interface 第三轮继续沿用单一 `trait` 路线，不引入 `interface` 别名。
下一次实现切片应优先解决 method lookup 的可解释性，而不是直接扩大到 Rust 完整 trait：

- method lookup 的长期顺序保持为 record-style function / namespace member 优先，trait method
  只在 receiver 类型、imported trait / impl 和 generic bound 能唯一确定时参与解析。
- 如果顶层 record-style function 与 trait impl method 同名，第三轮实现可以放宽当前
  `trait.method-ambiguous` 限制，但前提是 typechecker 已在 typed AST 或等价结构中记录被选中的
  method 来源，VM 不再只靠源码级普通函数名判断。
- imported trait / impl 只能来自当前 module graph、manifest source dirs 或 lockfile 证明的
  GitHub/git URL dependency；external dependency 的冲突诊断必须包含 source identity，不能只显示
  裸模块名。
- shadowing 仍保守处理：局部 binding、参数、namespace alias 和顶层 symbol 同名时，diagnostic
  必须指出被拒绝的候选集合；不能通过返回类型猜测。

标准库抽象迁移采用“小核心、显式导入、兼容旧 marker”的路线：

- 新建 `std/traits.nox` 可以作为第三轮实现候选，但首批只允许放入 `Eq`、`Display`、`Ord`、
  `Hash` 这类无 runtime 权限、无全局状态、无隐式 prelude 的静态 trait。
- `std/array.nox` 现有实验性 `Eq` helper 不在第三轮直接删除；如果引入 `std/traits.nox`，
  必须保留兼容导出或给出明确过渡期。
- `std/test.nox` 的 `assert_eq<T: Equatable>` / `assert_ne<T: Equatable>` 和
  `std/array.nox` 的 `contains_value<T: Equatable>` / `dedupe<T: Equatable>` 继续保持可用；
  新 helper 可以优先使用 `Eq` / `Display`，但旧 marker 不在 v0.0.x 内静默失效。
- 不建立 prelude，不隐式导入标准 trait；用户脚本必须通过 namespace import 或直接 import
  明确使用标准 trait。

第三轮仍暂缓以下能力：

- associated type 和 generic associated type。它们需要新的 type projection、diagnostic、LSP hover
  和 formatter 规则，且会影响 async trait 与 iterator 设计。
- blanket impl、generic impl、specialization、negative impl 和 auto trait。它们会显著扩大
  coherence、orphan rule、dependency identity 和冲突诊断复杂度。
- trait object、`dyn Trait`、interface value 和动态 dispatch。当前 VM、C ABI、host callback
  与 resource cleanup 尚未定义 vtable/value ownership 边界。
- async trait method。它依赖 associated type 或 effect model 设计，不能在 async runtime MVP
  之上直接承诺。

因此阶段 90 的最小实现应在以下两类中二选一：

1. method lookup 收敛：让 concrete receiver 的 trait impl method 与同名顶层函数能通过 typed
   selection 区分，并补冲突诊断、formatter、LSP hover/signature 和 CLI JSON parity。
2. 标准库 trait 迁移：新增或整理 `std/traits.nox` 的最小 `Eq` / `Display` 表面，并让
   `std/array.nox`、`std/test.nox` 通过兼容导出或 wrapper 使用它。

两类实现都必须保持当前 `std/array.nox` 实验性 `Eq` helper 可用，并且不得把 dynamic dispatch、
blanket impl 或 associated type 当作附带实现。

## 第四轮路线

阶段 97 复评后，trait/interface 第四轮继续选择“静态 trait、单一 `trait` 关键字、无
`interface` 语法别名”的路线。当前真实缺口不在于表达力不足，而在于 method lookup 的长期语义
还没有和实现完全对齐：ADR 已经要求 receiver type 和 trait impl 能唯一决定 method 来源，但当前
MVP 为了避免误分派，仍保守拒绝 impl method 与顶层 record-style function 同名。

第四轮设计结论如下：

- 下一次实现切片优先做 typed method selection：typechecker 在解析 `value.method(...)` 时记录
  选中候选的来源，例如 built-in / record-style function / namespace member / concrete trait impl /
  generic trait bound。后端和 LSP 读取这个 typed selection，而不是重新靠源码级函数名猜测。
- lookup 顺序保持稳定：局部值解析 receiver 表达式后，先匹配 built-in / record-style function /
  namespace member 的唯一候选；再匹配 concrete receiver 的唯一 trait impl method；最后匹配泛型
  receiver bound 中的唯一 trait method。仍不靠返回类型做重载选择。
- impl method 与顶层函数同名只有在 typed selection 能证明 receiver 类型唯一时才允许；多个 trait
  impl、trait/record 候选或 imported candidate 仍用 `trait.method-ambiguous` 保守拒绝，并在
  diagnostic 中列出候选来源。
- imported trait / impl 的可见性继续绑定 module graph 和 dependency source identity。跨 GitHub/git
  URL dependency 的冲突诊断必须包含 module/dependency 来源，不能只显示 trait/type 名称。
- 标准库迁移继续“小核心、显式导入、兼容旧 marker”：`std/traits.nox` 的 `Eq` / `Display` /
  `equal` / `display` 保持实验性；`std/array.nox` 的 `Eq` helper 和旧 `Equatable` helper 都不在
  第四轮移除。

第四轮仍不接受以下扩面：

- associated type、generic associated type、type projection 或 iterator-style trait family。
- generic impl、blanket impl、negative impl、specialization、auto trait 或 orphan/coherence 放宽。
- `dyn Trait`、trait object、interface value、dynamic dispatch、vtable 或跨 C ABI trait handle。
- async trait method。该能力依赖 associated type 或 effect model，并且会扩大 async runtime 承诺。
- `interface` 关键字别名。若未来需要 host/interface IDL，应另开 ADR，不复用静态 trait 语法。

因此阶段 98 的首选实现是 method lookup 完整化，而不是新增 trait 表达力。最小完成标准：

1. 新增或调整 typechecker 内部结构，记录 method call 的来源和 resolved callee。
2. 允许 concrete receiver 上唯一 trait impl method 与同名顶层 record-style function 共存，并正确
   选择 trait impl method 或普通 record-style function。
3. 保持所有 ambiguous/import/shadowing 情况的稳定诊断，不引入返回类型重载。
4. 同步 formatter 不变性、VM dispatch、LSP hover/signature/completion、CLI JSON parity 和
   `nox doc` 证据。

## 第五轮路线

阶段 107 复评后，trait/interface 路线继续保持保守静态模型。阶段 98 已经补齐 typed method
selection：顶层 record-style function 与 trait impl method 可以安全同名，record-style function
在 receiver 匹配时保持优先，concrete receiver 可分派到唯一 trait impl method。阶段 90 也已引入
实验性 `std/traits.nox` 小核心，导出 `Eq`、`Display`、`equal` 和 `display`。

当前缺口不再是 MVP 可用性，而是“完整 trait/interface 系统”中哪些能力值得进入生产承诺。第五轮
结论如下：

- 继续只使用 `trait`，不增加 `interface` 关键字别名。`interface` 如果未来用于 host IDL 或 C ABI
  描述，应另开 ADR，不能与静态 trait 共享语义。
- 下一轮实现优先做标准库 trait 迁移和诊断/source identity 硬化，而不是直接增加高级类型系统能力。
  合适切片包括：为 `std/traits.nox` 补小型纯 helper、让更多新 stdlib helper 使用 `Eq` /
  `Display`、改善 `trait.bound-unsatisfied` / `trait.method-ambiguous` 文案、在 external dependency
  参与时把 lockfile/cache source identity 写入冲突诊断。
- `std/array.nox` 的旧 `Equatable` helper、`std/test.nox` 的旧 assertion helper 和 built-in marker
  约束继续保留。新 helper 可以优先使用标准 trait，但不得让旧 helper 在 v0.0.x 内静默失效。
- LSP 和 `nox doc` 必须继续跟随静态 trait 表面：hover/signature 保留 trait bound，completion 只在
  receiver type 能唯一证明时建议 trait impl method，doc 输出 exported trait 和 required methods。
- GitHub/git external dependency 进入 trait 解析时必须遵守 lockfile/cache/offline 边界；不能为了 trait
  lookup 让 `check`、`test`、LSP 或 `project check` 静默联网。

第五轮继续暂缓以下能力：

- associated type、generic associated type、type projection 和 iterator-style trait family。
- generic impl、blanket impl、negative impl、specialization、auto trait 和 orphan/coherence 放宽。
- `dyn Trait`、trait object、interface value、dynamic dispatch、vtable、C ABI trait handle 或 host
  callback trait object。
- async trait method。该能力依赖 associated type、effect model 或 async return desugaring，不能作为
  `async fn` / `await` MVP 的附带能力。
- derive macro 或自动 impl。该方向必须等宏/codegen ADR 明确 source span、hygiene 和安全边界。

因此阶段 108 的首选实现是一个小型静态 trait 强化批次：在不引入新语法、不改变 VM dispatch、
不建立 prelude 的前提下，补 `std/traits.nox` / 标准库 helper / diagnostics / LSP/doc 中的一个
明确缺口，并用 tests 和 docs 证明旧 marker 兼容表面仍然可用。

## 诊断方案

新增或稳定以下 code：

- `trait.duplicate`：同一 trait 内重复声明 required method。重复顶层 trait 名称继续按
  `module.name-conflict` 处理。
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
