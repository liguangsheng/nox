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
