# Nox 文档索引

这组文档面向两类读者：想运行 `.nox` 脚本的人，以及想嵌入或修改 Nox 的人。
当前文档以已实现的 v0 表面为准，设计文档会标明哪些内容只是后续预留。

## 先读这些

- [架构](architecture.md)：`nox_core` 和 `nox` 的边界、执行流水线和运行时责任。
- [语言 v0](language-v0.md)：已经实现的语法、类型、表达式、模块和限制。
- [CLI](cli.md)：`nox run/check/test/fmt/lsp/inspect-bytecode` 的行为、退出码和诊断格式。
- [诊断 code](diagnostics.md)：CLI JSON 和 LSP diagnostics 的机器可读 code 契约。
- [运行时](runtime.md)：默认标准库、权限模型、文件加载、异步任务和取消执行。
- [嵌入](embedding.md)：Rust API、C ABI、宿主函数、错误字符串和内存所有权。
- [开发](development.md)：验证命令、测试分布和日常修改规则。
- [Option / Result 实施计划](option-result-implementation-plan.md)：v0.0.4-dev
  错误处理模型的 parser/type/VM/API/formatter/LSP/test 执行清单。
- [发布 checklist](release-checklist.md)：版本号、tag、CHANGELOG、C ABI 兼容检查。
- [性能基线](benchmarks.md)：bench 示例、跑法和参考耗时。
- [目录结构](directory-structure.md)：仓库目录、源码模块、示例、文档和生成物归属。

## 设计记录

- [数组设计](array-design.md)：`[T]`、数组字面量、索引、`len(array)` 和 v0 边界。
- [Record 设计](record-design.md)：命名 `record`、字面量、字段访问和非目标。
- [模块系统设计](module-system-design.md)：当前 import/export 边界和后续模块方向。
- [包 Manifest 设计](package-manifest-design.md)：已实现的 `nox.toml` 形状和 import 解析规则。
- [堆与对象生命周期](heap-design.md)：当前 Rc/Weak 模型、约束、后续 GC 选项。
- [v0.0.2 设计草案集](v0.0.2-design-drafts.md)：match、可选值、命名空间、nox test、args()、Engine session、C ABI 复合值。
- [决策记录](decisions/README.md)：对外可见的语言、模块、ABI、工具链决策。

[CHANGELOG.md](../CHANGELOG.md) 记录对外可见变更。
[README.md](../README.md) 是项目快速入口，[examples/README.md](../examples/README.md)
列出可运行示例和负向 fixture。
