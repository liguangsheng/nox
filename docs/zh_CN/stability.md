# 稳定性与兼容承诺

本文定义 `v0.0.x` release 线的公开表面状态。Nox 仍是 pre-1.0 项目，但 production release 不能让
下游猜测哪些行为是稳定、实验、暂缓或内部实现。

## 状态标签

- **稳定**：`v0.0.x` release 线内应保持向后兼容。新增字段、新 helper、新 diagnostic 允许作为兼容扩展，
  前提是旧消费者可以忽略。
- **稳定，需权限**：行为稳定，但调用方必须显式授予对应 runtime capability；默认仍然拒绝。
- **实验**：可以真实使用，但签名、语义或工具表面仍可能在后续 `v0.0.x` release 中调整。实验项必须在
  文档或 ADR 中标注。
- **暂缓**：当前 release 线不承诺。重启暂缓项必须先有设计记录或 ADR、测试、文档和 release gate 更新。
- **内部**：实现细节。用户和 embedder 不应依赖。

## 稳定性矩阵

| 表面 | 状态 | 兼容规则 | 证据 |
| --- | --- | --- | --- |
| `language-v0.md` 记录的 `.nox` 核心语法 | 稳定 | 已接受的程序应继续 parse/typecheck，除非 CHANGELOG 标记 pre-1.0 兼容破坏。 | language tests、fixture tests、formatter golden、release gate。 |
| parser/typechecker/VM 实现细节 | 内部 | AST 内部结构、bytecode 指令布局、verifier 内部和 heap 布局不是公开契约，除非被 CLI/API 暴露。 | 仅 unit tests，不作为下游契约。 |
| `nox run`、`check`、`test`、`fmt`、`project check`、`lsp`、`inspect-bytecode` | 稳定 | 子命令、退出码含义和已记录 flag 的行为变化必须写 release notes。 | CLI tests、product-shape guardrail、release gate。 |
| CLI JSON schema：`nox.check.v1`、`nox.test.v1`、`nox.project-check.v1` | 稳定 | 新增字段是兼容扩展；删除字段或改变字段含义是 breaking change，需要迁移说明。 | CLI JSON tests、compatibility golden。 |
| coverage/profile/trace JSON 或 NDJSON schema | 已记录部分稳定 | 新增事件字段是兼容扩展；事件重命名或删除必需字段需要迁移说明。 | CLI tests 和文档。 |
| `diagnostics.md` 记录的 diagnostic `code` | 稳定 | 工具应匹配 `code`，不要匹配 message 文本。删除 code 或复用 code 表示不同语义是 breaking change。message 可改进。 | diagnostics tests、LSP parity tests。 |
| LSP diagnostics | 稳定诊断对齐 | 同一源码分析下，diagnostic code 和 range 必须与 CLI check 保持一致。新增 capability 是兼容扩展。 | LSP integration tests、compatibility golden。 |
| LSP diagnostics 之外的 IDE 能力 | 未另行说明时为实验 | completion、hover、signature help、rename、semantic tokens、code actions 可保守增强；schema 或 capability 变化需要文档和测试。 | LSP tests。 |
| Rust `nox_core` API | 已记录嵌入路径稳定 | 已记录的 public type、host registration、ownership 行为和错误报告变化需要兼容说明。 | Rust API tests、embedding regression。 |
| Rust `nox` runtime API | 已记录部分稳定 | `Runtime`、`RuntimePermissions`、mock 和 project helper 的行为变化需要迁移说明。 | Runtime tests、embedding examples。 |
| `crates/nox_core/include/nox_core.h` C ABI | 稳定 | enum 数值、函数签名、handle ownership、字符串生命周期、callback 线程/重入边界和 last-error 规则是兼容契约。 | C ABI tests、enum stability tests、C embedding smoke。 |
| `stdlib-index.md` 中标为 stable 的标准库项 | 稳定 | 签名、返回类型、权限要求和错误模型应保持兼容。 | stdlib surface fixture、runtime tests。 |
| 标为 stable, permissioned 的标准库项 | 稳定，需权限 | 只有显式 capability 下稳定；缺少 capability 必须保持确定性拒绝。 | Permission tests、runtime docs。 |
| 标为 experimental 的标准库项 | 实验 | 可在 `v0.0.x` 内调整，但必须有 CHANGELOG 记录。 | Stdlib docs 和 tests。 |
| GitHub/git URL module 与 `nox.lock` | `v0.0.7` 稳定化对象 | 在 schema 和 drift diagnostic 正式化过程中，应保持既有 lockfile 行为。 | fetch/project-check tests。 |
| Release tarball 名称、`.sha256` sidecar、asset manifest JSON | 已发布 release 稳定 | 已发布资产必须可下载；修复或撤回必须写 release notes。 | Asset smoke、cutover check。 |

## 暂缓表面

以下不是 `v0.0.7` 的稳定公开承诺：自建 registry、crates.io publish、完整多平台 SDK matrix、TLS/HTTPS、
数据库驱动、trait object、dynamic dispatch、associated type、blanket impl、内建宏系统、import-time
codegen、IO reactor、多线程 async runtime、top-level await、async trait、C ABI task handle、完整
YAML/XML/protobuf、大型 streaming writer、installer、Docker image、CI action、SBOM/signing、性能趋势
dashboard。

暂缓项可以出现在 ADR 中作为未来选项。在设计、测试、文档和 release gate 证据落地前，不得把它们描述为
已支持行为。

## 变更规则

- 任何稳定公开表面的变化必须更新 `CHANGELOG.md`。
- 任何新增 CLI JSON 字段或 diagnostic code 必须有测试和文档。
- 任何 C ABI 变化必须保留 enum 数值和 handle ownership 规则；否则必须作为 pre-1.0 breaking change
  记录，并提供迁移步骤。
- 任何新增 permissioned runtime helper 必须默认拒绝、记录 capability，并有正向与负向测试。
- 实验功能必须在相关文档中标注后才能发布。
- 内部重构不需要用户文档，除非改变公开表面。

## Release audit 期望

production release 前，维护者必须运行 release checklist 中定义的 release gate、local distribution smoke、
strict cutover check 和 strict release audit。`v0.0.7` 额外优先把本文矩阵转成 compatibility 与 release
guardrail 中的机器检查。
