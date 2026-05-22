# 更新日志

本文件记录 Nox 的对外可见变更。正式发布前使用本地开发阶段版本号 `0.0.x`：
主版本号和次版本号都保持 0，只递增修订号。公共表面（语言、CLI、Rust API、C ABI）
允许在开发阶段调整，但变更必须在此文件留下记录。

## [未发布]

## [0.0.2] — 2026-05-22

本版本紧接 `v0.0.1` 基线，汇总当前 `main` 上已经完成且对外可见的能力与发布硬化。
此前内部计划中拆分过后续开发记录，但这些内部编号不作为已发布 tag。

### 稳定和兼容改进

- 诊断：runtime 权限未授予和文件系统 allowlist 拒绝现在使用稳定 code
  `permission.denied`，并保留 host function 包装后的原始诊断 code。
- 诊断：未细分的 host callback 错误现在使用稳定 code `host.callback`；宿主返回的
  更具体 code 会继续保留。
- 诊断：bytecode verifier 拒绝非法跳转、栈/作用域深度不一致和 malformed bytecode 时
  现在使用稳定 code `bytecode.verifier`，并覆盖 branch exit scope underflow 与嵌套函数体
  malformed bytecode 回归。
- 运行时：文件系统 write allowlist 对尚不存在的目标文件会解析最近的已存在父路径，
  防止通过 allowlist 内部 symlink 写入外部目录。
- 运行时：`env_list()` 读取进程环境时现在显式处理非 Unicode 名称或值，返回诊断而不是
  让底层环境迭代 panic。
- Runtime API：新增 `Runtime::set_instruction_budget`，并覆盖 instruction budget 耗尽时
  清理本次调用中新建 pending async task、host callback 返回后继续受预算约束的回归。
- Embedding：Rust host callback panic 现在会被隔离成 `host.callback` 诊断，`Engine`
  可在诊断后继续复用。
- Embedding：新增 C ABI option/result handle 生命周期回归，覆盖嵌套 heap 值保活和释放。
- Manifest：`modules.source_dirs` 和 `modules.test_dirs` 现在拒绝绝对路径、`..` 逃逸项目根
  和重复目录，CLI JSON 会以稳定 `manifest.invalid` 报告这些项目边界错误。
- CLI：`nox check --json` 在项目发现失败时仍输出 `nox.check.v1`，并用稳定 code
  `project.discovery` 标记缺少 manifest 或 manifest 展开路径不存在这类项目配置错误。
- CLI：scoreboard sample project 新增 manifest 默认发现与显式 path 列表的一致性回归，覆盖
  `check --json`、`test --json` 和 `project check --json`。
- LSP：打开文件所在项目的 manifest 无效时，现在优先发布 `manifest.invalid` diagnostic，避免
  将配置错误隐藏成 module resolution 失败。
- 诊断：manifest 解析和 schema 错误现在使用稳定 code `manifest.invalid`，并覆盖
  Rust manifest tests 与 CLI `check --json`。

### 工具和验证

- 工具：embedding regression 现在检查 `nox_core.h` 声明的 C ABI 函数是否由动态库实际导出。
- 稳定性：parser/type checker 新增生成式大输入负向回归，覆盖重复 malformed declaration
  和大量独立 type mismatch 不 panic 且诊断 code 稳定。
- 稳定性：新增模块返回容器值被 Rust 宿主持有再释放的 heap 压力回归，和既有 C ABI
  handle 释放回归一起覆盖复合值 GC 路径。
- 工具：benchmark smoke 现在断言每个 case 的预期输出片段，并在可用时给单个 case 加默认
  10 秒 timeout，避免卡死或跑错路径被误判为基线。
- 工具：CI 的 Rust toolchain 安装命令修正为分别声明 `rustfmt` 和 `clippy` component；
  `env_list` 相关测试共享环境变量锁，避免并发测试读取到非 Unicode 临时环境变量。

### 文档和发布流程

- 文档：`docs/embedding.md` 新增 Rust API 分层表，明确稳定入口、工具/实验表面和内部不承诺边界。
- 文档：`docs/embedding.md` 新增 C ABI 兼容矩阵，记录 enum 数值、handle ownership、
  error string 生命周期和 callback 边界，并用测试固定 enum 数值。
- 文档：embedding 指南明确 Rust/C host callback 的错误、panic/unwind、线程和重入边界。
- 文档：README 新增当前 `0.0.x` 本地 checkpoint 状态、生产边界说明和本地构建入口，并复核快速开始命令。
- 文档：同步 language/runtime 文档与当前实现，补全稳定 diagnostic code 列表，并修正文件系统
  allowlist 边界说明。
- 文档：release checklist 扩展回滚流程，覆盖保留 tag、标记撤回 release、hotfix、资产替换和下游升级路径。
- 版本：本地开发阶段版本号改为只推进修订号，例如 `v0.0.2`、`v0.0.3` 和
  `v0.0.3-alpha`；不再使用次版本号推进写法。

### 暂缓和边界

- 继续暂缓脚本级 async task status API、C ABI task status、源码级函数类型、高阶函数、
  mutable array、slice、package registry、watch mode 和 daemon。当前文档记录了重新启动条件。

### Breaking changes

- 本批未引入已知 breaking change。`0.0.x` 本地开发阶段仍允许公共表面调整；任何后续破坏性变更
  必须在本节、相关文档和 ADR 中明确标出。

## 0.0.6 内部开发记录 — 2026-05-22

### 改进

- 诊断：普通文件 import 读取失败现在使用稳定 code `module.not-found`，与 `std/*`
  缺失模块保持一致，并覆盖 CLI JSON 与 LSP diagnostics。
- CLI：新增 `nox project check --json`，输出 `nox.project-check.v1`，包含 manifest root、
  package 信息和 `check` / `test` / `fmt --check` 三个子步骤的状态与输出。
- 工具：release gate 和本地分发 smoke 覆盖 v0.0.6 新增的相对模块 `module.not-found`
  JSON diagnostic 和 `project check --json` 项目 summary。
- 文档：更新 embedding v0.0.6 兼容复审结论，确认当前不扩 Rust API / C ABI，并记录
  diagnostics、project JSON 和 async task status 暂缓对宿主边界的影响。
- 文档：新增 ADR 0016，明确 v0.0.6 暂缓脚本级 async task status API，并记录
  `TaskStatus`、id 生命周期、unknown id 和 C ABI 的重新启动条件。

## 0.0.5 内部开发记录 — 2026-05-21

### 新增

- 示例：扩展 scoreboard fixture 的 `runtime_info` 路径，覆盖 `std/fs.nox` 的
  `result` 读取、`option` match 和 LSP namespace completion，作为 v0.0.5 真实项目压力证据。
- 语言/runtime：新增 `map_get(map, key) -> option[T]`，为 map 缺失 key 提供可恢复读取；
  旧的 `contains(map, key)` + `map[key]` guard 和 map index 继续可用。
- LSP：普通 completion 现在会提示 `len`、`contains`、`map_get` 等语言/runtime
  内建函数，提升 v0.0.5 可恢复 API 的可发现性。
- 工具：release gate 和本地分发 smoke 增加 `map_get` 示例覆盖，release checklist
  切到 v0.0.5 流程。

## 0.0.4 内部开发记录 — 2026-05-21

### 新增

- 语言：新增 `option[T]` / `result[T, E]` 类型语法、静态验证地基和首批
  `some` / `none` / `ok` / `err` 构造值语义；`match` 支持 `some(name)` / `none` 和
  `ok(name)` / `err(name)` payload 解包与穷尽性检查。
- 标准库：`std/env.nox` 新增 `try_get(name: str) -> option[str]`，用于可恢复处理缺失
  环境变量；旧 `get(name: str) -> str` 保持 diagnostic 行为。
- 标准库：`std/fs.nox` 新增 `try_read_text(path: str) -> result[str, str]`，用于把普通
  读取失败表达为 `err(message)`；权限不足和 allowlist 越界仍保持 diagnostic。
- C ABI：为 option/result eval 返回值追加只读 owning handle 和 payload 读取函数。
- CLI：新增 `nox --version`，用于安装目录和本地分发 smoke 的版本自检。
- 工具：新增 `scripts/local-dist-smoke.sh`，构建 release CLI / `nox_core` 动态库并在
  临时安装目录运行版本、脚本和 C header smoke。
- 工具：`scripts/embedding-regression.sh` 现在覆盖 Rust embedding 示例，除 Rust API、
  runtime API 和 C ABI smoke 外，也验证长期宿主错误传播、权限边界和 async task 清理。
- 文档：新增 `docs/option-result-implementation-plan.md`，把 v0.0.4-dev
  `option[T]` / `result[T, E]` 实施拆成 parser、type checker、VM、Rust/C API、
  formatter、LSP 和 fixture 清单。
- 文档：`docs/embedding.md` 新增长期宿主 cookbook，说明 host callback 错误、
  `RuntimePermissions`、可恢复文件读取和 C ABI handle ownership。

## 0.0.3 内部开发记录 — 2026-05-21

### 新增

- Manifest：`nox.toml` 支持 `package.description`、命名 entrypoint、
  `modules.test_dirs` 和声明式 `runtime.permissions`。
- CLI：`nox run` 无显式路径时使用 manifest 的 `[entrypoints].main`；显式路径仍优先。
- CLI：`nox test` 无显式路径时优先使用 manifest 的 `modules.test_dirs`，再回退到
  `modules.source_dirs` 和项目根。
- CLI：`nox check` 无显式路径时按 manifest 展开 main、source dirs 和 test dirs。
- CLI：`nox fmt --check` / `nox fmt --write` 支持目录递归展开；无显式 path 时按
  manifest 展开 main、source dirs 和 test dirs。
- CLI：新增 `nox project check`，聚合项目级 `check`、`test` 和 `fmt --check`。
- 语言：支持 `import "path" as name;` 命名空间 import，`name.member` 在静态解析阶段
  绑定到模块导出成员，不引入运行时 object。
- 标准库：新增 `std/fs.nox`、`std/env.nox` 和 `std/time.nox` 静态模块，包装已有
  文件、环境和时间全局函数；旧全局函数作为兼容表面保持可用，当前不发 warning。
- LSP：diagnostics 复用 `Session` / `ModuleGraph`，被 import 的已打开文档变化后会刷新
  导入方 diagnostics。
- LSP：`name.` completion 会按命名空间 import 的模块可见成员补全。
- LSP：`std/*` 虚拟模块可用于 diagnostics 和 `name.` completion，不要求磁盘存在
  `std/fs.nox`。
- LSP：sample project 覆盖 manifest `modules.source_dirs` 下的 diagnostics、
  `name.` completion 和 formatting 行为。
- Rust API：新增 `Session` 和 `ModuleGraph`，为长期宿主提供 import 源码缓存和 overlay。
- Rust API：`RuntimePermissions` 支持文件系统 read/write root allowlist；`Runtime`
  新增 `pending_async_task_count()` 用于观察 pending async task。
- C ABI：新增 `nox_core_engine_set_userdata` / `nox_core_engine_userdata`，用于集中管理
  host callback 上下文；旧的 per-callback `ctx` 注册方式保持可用。
- 文档/工具：新增 `docs/diagnostics.md`、`scripts/bench-smoke.sh`、
  `scripts/robustness-smoke.sh` 和 malformed source corpus。
- 文档/工具：新增 `scripts/embedding-regression.sh`，聚合 Rust API、默认 runtime 和
  C embedding smoke。
- 示例：新增 `examples/projects/scoreboard/` 多模块项目 fixture，覆盖 manifest main、
  source/test dirs、namespace import、默认 runtime stdlib 和项目级 `run/check/test/fmt`。
### 设计决策

- v0.0.3 暂缓语言级 `option[T]` / `result[T, E]`，保留显式 guard + diagnostic 模式。
- v0.0.3 暂缓可变数组、源码级函数类型和高阶函数。
- 标准库命名策略改为：保留现有全局函数作为兼容表面，未来新增能力优先走静态
  namespace import 分层。
- 标准库模块加载策略采用 `std/*.nox` 虚拟内置模块，由 `nox` runtime 安装，不进入
  普通文件 import 搜索路径。
- Heap 模型复审结论：v0.0.3 继续使用 `Rc + Weak` 加弱引用追踪表，不引入 tracing GC、
  arena handle 或 cycle collector。

## 0.0.2 早期开发记录 — 2026-05-21

本版本汇总 v0.0.2 路径中已经在 `main` 上交付的对外可见能力。

### 新增

- 语言：`const` 顶层和块内常量，支持 `export`，赋值时给出静态错误。
- 语言：`while` 和 `for` 循环内的 `break` 和 `continue`，循环外使用是静态错误。
- 语言：语句式受限 `match`，支持 `int` / `str` 字面量 case 和 `_` 默认分支。
- 模块：`nox.toml` manifest 发现，`modules.source_dirs` 作为 import 备选根目录。
- 模块：平铺 import 表面增加 `module.name-conflict` 诊断，覆盖本地重复声明、
  import 与本地声明冲突、两个 import 暴露同名声明。
- 运行时/CLI：`args()` 读取 `nox run` 传入的位置参数，`env_list()` 返回环境字典。
- C ABI：array、map、record eval 结果暴露只读 handle 和读取函数。
- CLI：`check --json` 输出 schema 版本（`nox.check.v1`），新增 `files` 和 `summary`。
- CLI：`nox test [--json] [path...]`，运行 `*_test.nox` 中的 `test_*() -> bool`。
- CLI：`nox fmt --check` 和 `nox fmt --write`，支持多文件，默认 stdout 模式保留。
- LSP：`textDocument/formatting`、`textDocument/completion`、open-document import
  overlay；hover 沿用静态类型检查结果。
- 诊断：多个独立 top-level 类型错误一次报告；type checker 在不影响其他声明时不
  fail-fast。
- 诊断：parser/type checker/runtime 错误都带稳定 `code`，CLI JSON 和 LSP 看到的
  code 一致。
- 工具：GitHub Actions CI（`cargo fmt --check`、`cargo test`、
  `cargo clippy -D warnings`、C embedding smoke、Markdown 本地链接检查）。

### 变更

- 文档统一中文，并按"已实现"和"计划"两条线写明状态。
- `nox check --json` 现在总是带 `schema` 字段；旧的 `ok` / `diagnostics` 字段
  含义保持不变。

## v0.0.1 — 基线

`main` 上首次具备完整 v0 语言切片的状态。这一节描述 v0.0.2 工作开始时的对外能力，
便于回顾基线。

- 语言：变量、函数、块、`if`/`else if`、`while`、半开 `for` range、`return`、
  短路逻辑、字符串 escape、显式数值转换。
- 数据：`[T]` 数组、`map[str, T]`、命名 `record`、字段访问与静态字段检查。
- 模块：相对路径 `import`、重复导入只加载一次、循环导入诊断、`export` 可见性。
- 运行时：默认 stdlib（`sqrt`、`env_get`、`sleep_ms`、`tcp_connect`、
  `task_sleep_ms`、`task_ready`），文件系统/网络/环境/定时器/异步任务能力控制。
- VM：flat instruction stream、最小 verifier、instruction budget 取消。
- CLI：`nox run`、`nox check`、`nox check --json`、`nox fmt`、`nox lsp`、
  `nox inspect-bytecode`。
- Embedding：Rust host function API、C ABI host callback、版本查询、字符串返回、
  engine-owned error string、C embedding smoke。
- 文档：语言、运行时、CLI、embedding、目录结构、设计草案。
