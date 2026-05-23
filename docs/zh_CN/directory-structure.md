# 目录结构

Nox 是一个 Rust workspace。仓库结构按“核心引擎、默认运行时、示例、文档、生成物”
分层，避免把系统能力和语言核心混在一起。

## 顶层

```text
.
├── Cargo.toml
├── Cargo.lock
├── README.md
├── README_zh_CN.md
├── crates/
├── docs/
├── examples/
├── tests/
└── target/              # Cargo 生成，忽略
```

顶层文件职责：

- `Cargo.toml`：workspace 定义和共享 package metadata。
- `Cargo.lock`：workspace 依赖锁定文件。
- `README.md`：英文快速入口。
- `README_zh_CN.md`：中文快速入口。
- `.gitignore`：忽略 Cargo 输出、本地工具和临时文件。

## Crate 布局

```text
crates/
├── nox_core/
└── nox/
```

### `crates/nox_core`

`nox_core` 是可嵌入引擎。它拥有语言核心、诊断、字节码、VM、堆、Rust 宿主 API 和 C ABI。

```text
crates/nox_core/
├── Cargo.toml
├── include/
│   └── nox_core.h
└── src/
    ├── api_tests.rs
    ├── bytecode.rs
    ├── compiler_tests.rs
    ├── ffi.rs
    ├── heap.rs
    ├── language_tests.rs
    ├── lexer.rs
    ├── lib.rs
    ├── parser.rs
    ├── typecheck.rs
    └── vm.rs
```

文件职责：

- `Cargo.toml`：`nox_core` crate 配置和 crate-type 设置。
- `include/nox_core.h`：C ABI 头文件，宿主从这里获取 C 类型和函数声明。
- `src/lib.rs`：公开 Rust API、核心类型、AST、module resolver、engine pipeline 和 formatter。
- `src/lexer.rs`：词法分析、token 和 span。
- `src/parser.rs`：递归下降 parser，从 token 构造 AST。
- `src/typecheck.rs`：静态语义检查、类型环境、record schema 和 hover 类型收集。
- `src/bytecode.rs`：bytecode 指令、compiler、verifier 和 compact inspect 输出。
- `src/vm.rs`：运行时环境、控制流、操作符、容器访问和 VM 执行。
- `src/heap.rs`：脚本 heap object 跟踪和显式 collection。
- `src/ffi.rs`：C ABI 类型转换、engine wrapper、host callback 和 exported C functions。
- `src/compiler_tests.rs`：lexer/parser/compiler/verifier/inspect 单元测试。
- `src/language_tests.rs`：语言语义、容器、诊断和运行时错误测试。
- `src/api_tests.rs`：Rust API、C ABI、import、budget 和 heap 测试。

放置规则：

- 新语言语法、类型、字节码和 VM 行为放这里。
- 系统 I/O、文件路径策略、网络、环境变量、定时器不要放这里。
- 对外公开 Rust API 或 C ABI 时，同步 [embedding.md](embedding.md) 和测试。

### `crates/nox`

`nox` 是默认运行时、CLI 和 LSP 所在 crate。它建立在 `nox_core` 上，负责系统能力和用户工具面。

```text
crates/nox/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── lsp.rs
│   └── main.rs
└── tests/
    └── cli.rs
```

文件职责：

- `Cargo.toml`：默认运行时和 CLI crate 配置。
- `src/lib.rs`：`Runtime`、`RuntimePermissions`、文件加载、标准库宿主函数和运行时测试。
- `src/main.rs`：命令行参数解析、命令分发、人类/JSON 诊断输出。
- `src/lsp.rs`：stdio LSP loop、消息处理、diagnostics、hover 和 JSON escaping。
- `tests/cli.rs`：黑盒 CLI 集成测试，运行编译出的 `nox` 二进制。

放置规则：

- 文件系统、权限、标准库、CLI、LSP 和项目发现放这里。
- 只通过 `nox_core` 公开 API 使用引擎，不依赖 crate-private compiler 结构。

## 文档

```text
docs/
├── en/
│   ├── README.md
│   ├── architecture.md
│   ├── cli.md
│   ├── development.md
│   ├── diagnostics.md
│   ├── directory-structure.md
│   ├── embedding.md
│   ├── language-v0.md
│   ├── release-checklist.md
│   └── runtime.md
└── zh_CN/
    ├── README.md
    ├── architecture.md
    ├── array-design.md
    ├── cli.md
    ├── decisions/
    ├── development.md
    ├── directory-structure.md
    ├── embedding.md
    ├── language-v0.md
    ├── module-system-design.md
    ├── package-manifest-design.md
    ├── record-design.md
    └── runtime.md
```

文档职责：

- `docs/en/README.md`：英文文档索引。
- `docs/zh_CN/README.md`：中文文档索引。
- `architecture.md`：架构和执行流水线。
- `language-v0.md`：已实现语言表面。
- `cli.md`：命令行为、退出码和诊断格式。
- `runtime.md`：运行时标准库和权限模型。
- `embedding.md`：Rust/C 嵌入 API。
- `development.md`：验证命令、测试分布和修改规则。
- `directory-structure.md`：本文，目录和文件归属。
- 中文 `array-design.md`、`record-design.md`、`module-system-design.md`、
  `package-manifest-design.md` 和 `decisions/`：设计记录和后续边界。

修改公共行为时同步对应文档：语法改 `language-v0.md`，CLI 改 `cli.md`，权限改
`runtime.md`，嵌入边界改 `embedding.md`，目录变化改本文。

## 示例

```text
examples/
├── README.md
├── arrays.nox
├── control-flow.nox
├── conversions.nox
├── else-if.nox
├── embed/
│   └── c_embedding.c
├── export-main.nox
├── export-math.nox
├── for-range.nox
├── hello.nox
├── logical.nox
├── maps.nox
├── math.nox
├── numeric-boundaries.nox
├── records.nox
├── recursion.nox
├── scopes.nox
├── stdlib.nox
├── string-escapes.nox
└── strings.nox
```

示例文件分三类：

- 正向示例：应能通过 `cargo run -p nox -- run <file>`。
- 示例项目：`examples/projects/scoreboard/`，用于真实项目工作流 smoke。
- 嵌入 smoke：`examples/embed/c_embedding.c`，用于 C ABI 编译/运行验证。

## 测试输入

```text
tests/
├── README.md
├── benchmarks/
├── fixtures/
└── malformed/
```

测试输入分三类：

- `tests/fixtures/`：CLI、parser、type checker、runtime、formatter 和 `nox test` 使用的固定输入。
- `tests/malformed/`：panic-free robustness smoke corpus，覆盖 lexer、parser、formatter、type checker、module resolver、manifest 和 LSP 的坏输入边界。
- `tests/benchmarks/`：benchmark smoke 输入，覆盖递归、循环、容器、模块、lambda、
  permissioned host helper 和 test runner。

## 生成目录和本地工具

- `target/`：Cargo build 输出，不手写、不提交。
- `.git/`：Git 元数据。
- `.codex/`：如本地出现，属于个人工具配置，不是项目源码。

## 新文件放置规则

- 语言核心、AST、类型、bytecode、VM、heap 和 C ABI：放 `crates/nox_core`。
- 默认运行时、CLI、LSP、文件加载、权限和标准库：放 `crates/nox`。
- 可执行脚本样例和示例项目：放 `examples/`。
- 只服务断言、负向诊断、鲁棒性或性能 smoke 的输入：放 `tests/`。
- malformed robustness corpus：放 `tests/malformed/`。
- 本地开发脚本和可复制 smoke：放 `scripts/`。
- 面向用户或维护者的英文说明：放 `docs/en/`，并从 `docs/en/README.md` 链接。
- 面向用户或维护者的中文说明：放 `docs/zh_CN/`，并从 `docs/zh_CN/README.md` 链接。
- 设计尚未实现时写入设计文档，不要混进实现文档里假装已完成。
- 生成物和本地工具文件不放进源码目录。
