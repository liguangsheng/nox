# 0029 - 暂缓宏系统，优先使用函数、trait 与外部 codegen

- 状态：已采纳
- 日期：2026-05-24
- 涉及：语言 / parser / typecheck / formatter / LSP / 文档 / 安全

## 背景

阶段 66 评估 Nox 是否需要宏系统。当前 Nox 已经具备函数值、lambda、静态 trait、标准库
helper、`nox doc`、LSP symbol/hover/definition、稳定 diagnostic code 和 release gate。宏可以
减少重复样板，但也会把语言从“静态、显式、可诊断”的模型推向更复杂的编译期展开模型。

需要评估的候选包括：

- declarative macro：类似 pattern/template 的编译期展开。
- syntax template：受限模板，只生成表达式或声明。
- attribute-like helper：在 record / enum / fn 上附加生成行为。
- codegen CLI：在 Nox 编译前由外部工具生成 `.nox` 文件。

当前真实压力主要来自测试 helper、JSON/enum/record 样板和未来 trait/async ergonomics。这些压力
已经部分由 `std/test.nox`、`std/json.nox`、trait MVP 和 result/option helper 缓解；还不足以
证明宏系统的长期成本。

## 决策

Nox 暂缓内建宏系统。阶段 67 的 declarative macro MVP 不进入实现；后续如果继续推进，应先重写
阶段计划，而不是默认实现 parser/expander。

明确不做：

- 不引入 `macro` declaration 或 macro invocation 语法。
- 不引入 attribute 语法，例如 `#[derive(...)]`、`@test` 或 `@json`.
- 不引入 procedural macro，不执行宿主程序，不读文件、网络或环境。
- 不在 VM 或 runtime 中加入 `eval`、compile-time script execution 或动态代码加载。
- 不让 formatter、LSP、`nox doc` 解释 macro expansion 结果。

推荐路线：

- 首选普通函数、lambda、高阶 stdlib helper 和静态 trait。
- 重复 JSON/record/enum 样板先通过 `std/json.nox` helper 或更明确的 stdlib 函数收敛。
- 项目若确实需要生成代码，可以在 Nox 编译前运行外部 codegen 工具，把生成的 `.nox` 文件作为
  普通源码提交或纳入项目目录；Nox 编译器本身不执行该工具。
- `nox doc`、LSP 和 diagnostics 只对最终 `.nox` 源码负责，不追踪外部生成器内部模板。

## 为什么暂缓

宏系统需要一次性回答多组长期问题：

- Hygiene：生成的 identifier 是否能捕获调用点变量，如何避免 shadowing。
- Source span：diagnostic 应指向 macro 定义、调用点，还是展开后的代码。
- Formatter：格式化 macro 定义还是格式化展开结果。
- LSP：hover、definition、rename、completion 如何穿过 expansion。
- `nox doc`：导出的 API 是 macro 本身、展开结果，还是两者都展示。
- Security：宏是否能读取文件、环境、网络或执行外部程序；如果不能，procedural macro 的收益会
  大幅下降。
- Release audit：生成代码是否进入分发产物、如何复现、如何审计。

Nox 当前还在收敛 trait、result/option、async 和包生态。此时加入宏会扩大 parser、typechecker、
formatter、LSP 和 release gate 的共同承诺面，但真实用户代码还没有证明不可替代需求。

## 外部 codegen 边界

外部 codegen 是项目构建流程的一部分，不是 Nox 语言特性：

- 生成器由项目自己运行，Nox CLI 不隐式执行。
- 生成结果是普通 `.nox` 文件，进入 `nox check`、`nox test`、LSP 和 release gate 的常规路径。
- 生成器需要网络、文件系统或环境访问时，由项目自己的构建脚本负责，不继承 Nox runtime
  capability 模型。
- 推荐把生成源码提交进仓库，或在 CI 中固定生成器版本并让生成结果可 diff。

如果未来 Nox 提供 `nox generate`，它也应是显式工具命令，不是 import、typecheck 或 runtime
隐式副作用。

## 工具要求

由于当前不采用宏：

- Parser 不新增保留字；`macro` 仍是普通 identifier，避免无收益地收紧兼容性。
- Formatter、LSP、`nox doc`、CLI JSON schema 和 runtime 不需要宏相关改动。
- Diagnostics 不新增 macro code。
- 文档应继续把 macro 系统列为 v0 暂不支持，并说明重启条件。

## 阶段 82 复审

阶段 82 重新扫描标准库源码、examples、tests/fixtures 和当前文档中的重复模式。当前仓库内没有
外部用户脚本语料；本轮结论只基于项目自带样本和 release gate 覆盖面：

- JSON / record / enum 样板主要集中在 `std/json.nox` 的 `to_json` / `from_json`、
  `decode_record3` 和 `decode_adjacent_enum3`。这些 helper 仍显式、可 typecheck、可文档化，
  没有证明需要 `derive` 或 attribute macro。
- property test 的 record / enum 生成样板由 `std/test.nox` 的 `gen_record3`、`gen_enum3`、
  `assert_property_record3` 和 `assert_property_enum3` 覆盖。builder 参数虽然啰嗦，但边界清晰，
  不需要 reflection 或 compile-time execution。
- 静态 trait 第二轮已经用 `Eq` helper、trait bound 和 method lookup 解决标准库抽象迁移压力；
  当前不需要 `derive Eq`。
- async 第二阶段用 `std/task.nox` 的 `delay`、`join2` 和 `join3` 解决最小 task 组合压力；
  不需要 `select!`、语法模板或宏展开。
- examples/projects 目前只有小型 record/enum 和显式 builder，重复度不足以抵消 hygiene、
  source span、formatter、LSP 和 release audit 成本。

因此阶段 82 继续维持“暂缓内建宏系统”的结论。后续如果真实项目开始提交生成源码，推荐先按外部
codegen 边界处理：生成 `.nox` 文件进入普通 `nox check` / `nox test` / LSP / release gate，
而不是让 import、typecheck 或 runtime 隐式执行生成器。

## 阶段 99 复审

阶段 99 在 trait/interface 第三、四轮，result/option ergonomics，async task helper 和
YAML/XML 纯 helper 落地后重新评估宏压力。结论仍是不重启内建宏系统，但阶段 100 可以先补外部
codegen 的显式、只读 tooling 元数据。

当前新增证据：

- trait method lookup 已允许 impl method 与顶层 record-style function 安全同名，并保持
  record-style receiver 优先。这个变化降低了 `derive Display` / `derive Eq` 一类 attribute
  macro 的迫切性。
- `std/traits.nox`、`std/option.nox`、`std/result.nox` 和 `std/task.nox` 的组合 helper 已覆盖
  多数重复调用形态，且保持普通源码、普通类型检查和普通 LSP/doc 路径。
- `std/json.nox`、`std/test.nox`、YAML/XML helper 仍有 builder 参数和三字段/三分支 helper 的样板，
  但这些样板集中在库 API 边界，尚未证明需要 parser、formatter、typechecker 和 LSP 共同承担宏展开。
- 目前没有三个以上真实外部项目提交相同 codegen/derive 压力；仓库内样本不足以启动宏语言设计。

因此阶段 99 决策为：

- 继续不引入 `macro`、attribute、procedural macro、compile-time execution 或 import-time codegen。
- `macro` 仍不是保留字；CLI JSON、diagnostic code、VM 和 runtime 不增加 macro 表面。
- 阶段 100 的首选实现不是宏，而是外部 codegen source map / manifest 的最小只读工具支持。

阶段 100 允许探索的最小 tooling 形态：

- 生成器仍由项目显式运行；Nox CLI 不自动执行生成器，不继承 runtime capability，也不访问网络、
  环境或额外文件。
- 生成后的 `.nox` 文件仍作为普通源码进入 `check`、`test`、LSP、`doc` 和 release gate。
- 可接受一个显式元数据文件，描述 generated file 与 generator/template 的关系、生成命令文本、
  generator 版本或输入 hash；该文件只用于审计、诊断附加说明、`project check` 或 `nox doc`
  标注，不改变 parser/typechecker 的 source span。
- LSP 可以先只展示“此文件由外部生成”的只读信息；definition、rename、formatting 和 semantic
  tokens 默认仍以生成后的 `.nox` 文件为准，不穿透到模板。
- release gate 只能验证元数据存在性、路径合法性、hash 一致性或文档 parity；不能隐式重新生成源码。

不在阶段 100 做的事项：

- 不设计宏 hygiene、展开 AST、模板语法或 attribute 语义。
- 不让 diagnostics 指向模板内部位置，除非后续已有独立 source-map 设计和测试矩阵。
- 不让 formatter 修改模板或生成器输入。
- 不把 external codegen 集成进 import resolver、module cache 或 runtime。

这个复审让 codegen 路线先获得可审计性，而不是直接扩大语言核心。若后续外部项目证明只读
source-map tooling 仍不足，再按“重新启动条件”重开宏 ADR。

## 阶段 113 复审

阶段 113 在 `[codegen]` manifest 元数据、`project check --json` 只读审计和 LSP generated-source
hover 标注落地后重新评估宏压力。结论仍是不重启内建宏系统；阶段 114 应继续加强外部 codegen
tooling 的可审计性，而不是引入 `macro` 语法或编译期执行。

当前新增证据：

- `project check` 已能报告 generated 文件是否存在，以及 generator、template、input hash 和 command
  等元数据；这解决了“生成物是否纳入项目审计”的第一层问题。
- LSP hover 已能对 manifest 声明的 generated source 给出只读标注；IDE 可以提示用户该文件来自外部
  codegen，同时 definition、rename、formatting 和 diagnostics 仍按生成后的 `.nox` 文件工作。
- trait、result/option、async 和数据 stdlib 的新一轮扩展继续通过普通函数、静态 trait 和源码级
  helper 完成，没有出现必须依赖 hygienic expansion、attribute 或 procedural macro 的公共 API。
- 仍没有真实外部项目证明同类样板已经超过函数、trait、stdlib helper 或外部 codegen 的可维护边界。

因此阶段 113 决策为：

- 继续不引入 `macro`、attribute、procedural macro、compile-time execution 或 import-time codegen。
- `macro` 仍不是保留字；parser、formatter、typechecker、VM、runtime、CLI JSON 和 diagnostic code
  不增加 macro 表面。
- 阶段 114 首选实现是 codegen source-map 元数据的只读审计：允许在 `[codegen]` artifact 中声明
  显式 source-map 文件或 digest，并让 `project check` 验证路径、存在性和 hash 格式。

阶段 114 可以接受的最小能力：

- 只读解析 manifest 中的 source-map 元数据，不执行生成器，不读取模板内部语义，不改变 import、
  typecheck、runtime 或 module cache。
- source-map 文件如果声明，必须是项目内相对路径；存在性与可选 hash 由 `project check` 报告并进入
  JSON，缺失时返回非零。
- LSP、`nox doc` 和 diagnostics 第一轮仍只展示 generated source 标注；不把诊断、definition、
  rename 或 formatting 穿透回模板。
- release gate 可以把 source-map 元数据检查纳入 manifest/project check 回归，但不能隐式重新生成源码。

阶段 114 不做：

- 不设计 macro hygiene、AST expansion、模板语法、attribute 语义或 procedural macro API。
- 不让 source-map 改写诊断 span、LSP range、formatter 输入或 doc declaration source。
- 不执行 generator command，不比较生成结果和模板重新生成结果。
- 不把 codegen 集成进 import resolver、module cache、package fetch 或 runtime permission 模型。

这个复审把“生成源码可审计”继续作为外部 tooling 问题处理。只有当只读 source-map 元数据仍无法满足
真实项目，且重启条件被实际证据满足时，才重新打开宏语言设计。

## 后果

暂缓宏保持语言核心小、诊断路径直接，也避免把 source-map / hygiene / LSP expansion 边界提前固定。
代价是 record/enum/JSON/test 相关样板仍要通过 helper 或外部生成解决。

该决策不会阻止未来引入宏；它要求未来先用真实代码证明宏比函数、trait、stdlib helper 或外部
codegen 更合适。

## 重新启动条件

满足以下条件之一时，可以重启宏 ADR：

- 三个以上真实项目出现相同样板，且函数、trait、stdlib helper 或外部 codegen 都显著降低可读性
  或可维护性。
- `std/json.nox` / test helper / async helper 继续增长，证明没有宏会导致 stdlib API 成倍膨胀。
- LSP 已经具备稳定 source-map 基础设施，可以把 expansion diagnostic、definition 和 rename
  边界解释清楚。
- release gate 已经能验证生成源码的可复现性和 docs/diagnostics parity。

重启时第一候选仍应是受限 declarative macro 或 syntax template；procedural macro、任意外部程序、
文件系统、网络和环境访问继续作为非目标。

## 备选方案

- 立即实现 declarative macro MVP。未选择，因为没有足够真实样板压力，且 source span、hygiene、
  formatter 和 LSP 边界会成为新长期承诺。
- 引入 attribute-like derive。未选择，因为它需要先确定 record/enum metadata、生成 API 的
  docs 表面和冲突规则。
- 引入 procedural macro。未选择，因为它会直接打开编译期执行、安全、复现和分发审计问题。
- 把 codegen 集成进 import。未选择，因为 import 应保持静态、可复现、无隐式网络/文件副作用。
