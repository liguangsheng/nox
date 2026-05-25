# 更新日志

本文件记录 Nox 的对外可见变更。正式发布前使用本地开发阶段版本号 `0.0.x`：
主版本号和次版本号都保持 0，只递增修订号。公共表面（语言、CLI、Rust API、C ABI）
允许在开发阶段调整，但变更必须在此文件留下记录。

## [未发布]

- 文档：新增中英文稳定性与兼容承诺矩阵，明确 `v0.0.x` release 线中 Rust API、C ABI、
  CLI JSON、diagnostic code、LSP diagnostics、语言语义、stdlib、release assets 和暂缓项的
  稳定 / 实验 / 暂缓 / 内部边界，作为 `v0.0.7` 稳定化路线的第一步。
- 工具：新增 `scripts/stability-guardrail.sh` 并接入 release gate / release audit /
  release candidate readiness，检查稳定性矩阵、支持政策、文档索引和正式文档边界仍在位。
- 工具：`scripts/release-candidate-readiness.sh` 不再硬编码旧的 `v0.0.4 -> v0.0.5`
  候选版本流程；candidate 模式从当前 workspace patch 推导下一候选版本，cutover 模式仍可由
  `NOX_RELEASE_CUTOVER_VERSION` 显式指定。
- 工具：`scripts/compatibility-golden.sh` 新增 release asset manifest JSON golden，固定
  `nox.release-asset-manifest.v1` 的资产名称、target、commitment 和 C ABI smoke 要求。
- 文档：新增中英文支持与安全政策，写清 supported versions、EOL、hotfix、withdrawn release、
  漏洞响应和 release train；release checklist 与文档索引同步链接该政策。

## [0.0.6] — 2026-05-25

- 文档：ADR 0031 增加阶段 117 集成式 LSP 第六轮复评结论，继续不拆独立 LSP
  二进制、不做跨文件 rename、后台 daemon 或 generated/codegen source-map 穿透；阶段 118
  首选把已审计的 `source_map` / `source_map_hash` 元数据加入 generated-source hover 标注。
- LSP：generated source hover 标注现在会显示 manifest `[codegen]` artifact 中可选的
  `source_map` 与 `source_map_hash` 元数据；LSP 仍不读取或解释 source map，也不把
  diagnostics、definition、rename、formatting 或 semantic tokens 重映射到模板。
- 文档：新增 ADR 0033 记录平台矩阵与分发第三轮路线；当前继续只承诺
  `x86_64-unknown-linux-gnu` full SDK 和 `x86_64-unknown-linux-musl` CLI-only，不新增
  无 CI / C ABI smoke / asset smoke 证据的平台资产。
- 发布：`scripts/release-asset-manifest.sh --json` 现在输出机器可读资产矩阵，包含每个
  release asset 的 `kind`、`target`、`commitment` 和 `c_abi_smoke_required`；默认文本输出
  保持兼容。
- 文档：新增 ADR 0034 记录生产证据第三轮路线；阶段 122 首选把机器可读 release asset
  manifest 纳入 `scripts/release-evidence-report.sh`，不新增 runtime、语言或 stdlib 能力。
- 发布：`scripts/release-evidence-report.sh` 现在输出 `Release Asset Manifest JSON` 段，
  把 `nox.release-asset-manifest.v1` 资产矩阵与 cutover status、toolchain status 和命令计划
  汇总到同一份只读 release 证据报告。
- 发布：`scripts/release-candidate-readiness.sh` 现在守护阶段 117-122 的收敛结果，包括
  LSP generated-source source-map 元数据边界、平台矩阵 ADR、asset manifest JSON 和 evidence
  report JSON 段。
- 文档：ADR 0032 增加阶段 115 数据脚本标准库第六轮设计结论，继续稳态化现有
  YAML/XML 纯 helper；阶段 116 首选 `std/xml.nox` 安全 XML comment helper，不引入
  YAML 完整化、压缩/归档、protobuf、SQLite/database driver、TLS/HTTPS 或 streaming writer。
- 标准库：`std/xml.nox` 新增 `comment(value) -> result[str, str]`，用于生成安全 XML
  comment 片段；内容包含 `--` 或以 `-` 结尾时返回 `err`。该 helper 仍是纯字符串能力，
  不解析 XML、不提供 CDATA、processing instruction、schema validation 或 streaming writer。
- 文档：ADR 0029 增加阶段 113 宏/codegen 复评结论，继续暂缓内建宏系统；阶段 114
  首选 codegen source-map 元数据的只读审计，不引入 `macro` 语法、attribute、
  procedural macro、compile-time execution 或 import-time codegen。
- 工具：`[codegen]` artifact 新增可选 `source_map` 和 `source_map_hash` 元数据；
  `nox project check --json` 只读报告 source-map 路径、存在性和 hash，并在声明的
  source-map 文件缺失时返回 `manifest.invalid`。Nox 仍不执行生成器、不解释 source-map
  内容，也不把 diagnostics、definition、rename、formatting 或 `nox doc` 穿透到模板。
- 工具：ADR 0025 将 release CLI size cap 继续校准到 3.078125 MiB（3,227,648 bytes），
  覆盖阶段 114 codegen source-map 元数据只读审计带来的约 3,220,344 bytes release CLI
  实测体积；`nox_core` 上限和零第三方 runtime dependency 约束不变。
- 文档：ADR 0030 增加阶段 111 async/await 第四轮复评结论，继续保持单 runtime、单线程、
  显式权限、无 IO reactor 的模型；阶段 112 优先补 awaitable task 清理/取消传播、
  async diagnostic parity 或 stdlib/doc surface 守卫，不引入 top-level await、async trait、
  C ABI task handle、多线程 runtime 或真正非阻塞 IO。
- Runtime：`run_test_file` 现在会在测试用例失败时清理本次 test run 创建的 async task，
  同时保留运行前由宿主创建的 pending task，避免失败测试泄漏 awaitable sleep task。
- 文档：ADR 0026 增加 GitHub/git module 生态第二轮设计结论，继续不做自建 registry、
  publish 命令、中心索引或版本范围求解；阶段 106 首选 `nox fetch --check` / `--locked`
  这类只验证 lockfile/cache、不改写项目状态的能力，`project check` 继续不联网。
- CLI：`nox fetch` 新增 `--check` 和 `--locked` 只读模式，用于验证 manifest、`nox.lock`
  和 module cache 是否仍一致而不改写 lockfile；二者可与 `--offline` 组合，cache miss、
  corrupt cache、缺少 lockfile 或 lockfile drift 都会返回非零。
- 文档：ADR 0027 增加 trait/interface 第五轮设计结论，继续采用单一静态 `trait`，
  不引入 `interface` 别名、associated type、blanket/generic impl、trait object、dynamic dispatch
  或 async trait；阶段 108 优先做标准库 trait helper、诊断/source identity、LSP/doc parity
  这类小型静态强化。
- 标准库：`std/traits.nox` 新增 `not_equal<T: Eq>` 和 `display_label<T: Display>`，
  继续作为显式导入的小型静态 trait helper；不建立 prelude，不改变旧 `Equatable` marker helper。
- 文档：ADR 0028 增加阶段 109 错误/异常模型复评结论，继续不实现 `try {}`、`throw`、
  `catch`、`finally`、VM unwind 或 catchable runtime diagnostic；阶段 110 只接受 result/option
  helper、诊断文案或 CLI/LSP parity 这类低风险 ergonomics。
- 标准库：`std/option.nox` 新增 `filter<T>`，`std/result.nox` 新增 `map_or<T, U, E>`，
  都是源码级纯 helper；不引入 `try {}`、异常、VM unwind 或 catchable runtime diagnostic。
- 标准库：新增实验性纯计算 `std/yaml.nox` 和 `std/xml.nox`。YAML helper 提供最小配置
  子集 `parse(source) -> result[json, str]`，XML helper 提供 name 校验、文本/属性转义和
  安全标签拼接；压缩/归档、protobuf、SQLite/database driver 和 HTTPS/TLS 继续暂缓。
- 标准库：`std/xml.nox` 新增 `attrs`、`empty_element` 和 `text_element_attrs`，用于批量
  校验/转义属性并生成带属性的安全 XML 文本；仍不提供 XML parser、schema validation、
  namespace resolver 或 streaming writer。
- 标准库：`std/task.nox` 新增 `delay<T>`、`join2<T, U>` 和 `join3<T, U, V>`，
  在不引入 IO reactor、多线程 runtime、top-level await 或 C ABI task handle 的前提下，
  提供最小 task 组合 helper。
- 标准库：`std/task.nox` 新增 `map<T, U>` 和 `and_then<T, U>`，用于组合调用方已经创建的
  task；helper 本身不新增 runtime task kind、pending task 计数、隐式权限、取消语义或
  后台 scheduler。
- 文档：ADR 0028 增加阶段 95 复评结论，继续不重启 `try {}` block 或异常机制；阶段 96
  只规划源码级 `std/option.nox` / `std/result.nox` ergonomics helper，不改变 parser、
  VM unwind、CLI JSON、LSP diagnostic schema 或权限边界。
- 标准库：`std/option.nox` 新增 `unwrap_or_else<T>` 和 `ok_or<T, E>`；`std/result.nox`
  新增 `unwrap_or_else<T, E>` 和 `or_else<T, E>`。这些 helper 是纯源码级组合函数，
  不捕获 runtime diagnostic，不新增权限、parser 语法或 CLI/LSP schema。
- 文档：ADR 0027 增加 trait/interface 第四轮设计结论，继续选择单一静态 `trait` 路线；
  阶段 98 首选实现是 typed method selection 和 method lookup 完整化，不引入 `interface`
  语法别名、associated type、blanket/generic impl、dynamic dispatch、trait object 或 async trait。
- 语言能力：trait impl method 现在可以与顶层 record-style function 同名；record-style function
  的第一个参数匹配 receiver 时保持优先，否则 concrete receiver 可分派到唯一 trait impl method。
  冲突仍使用 `trait.method-ambiguous`，不引入返回类型重载、dynamic dispatch 或 trait object。
- 工具：ADR 0025 将 release CLI size cap 继续校准到 3.046875 MiB（3,194,880 bytes），
  覆盖阶段 98 trait method lookup 完整化带来的约 3,188,880 bytes release CLI 实测体积；
  `nox_core` 上限和零第三方 runtime dependency 约束不变。
- 文档：ADR 0029 增加阶段 99 宏/codegen 复评结论，继续暂缓内建宏系统；阶段 100
  首选方向收敛为外部 codegen source map / manifest 的显式只读工具支持，不引入 `macro`
  语法、attribute、procedural macro、compile-time execution 或 import-time codegen。
- 工具：`nox.toml` 新增可选 `[codegen]` 元数据 section，`nox project check --json`
  只读报告 generated `.nox` 文件、generator/template/input hash/command，并拒绝缺失的
  generated 文件；不会执行生成器、改变 import/typecheck/runtime 或把 LSP/doc 穿透到模板。
- 工具：ADR 0025 将 release CLI size cap 继续校准到 3.0625 MiB（3,211,264 bytes），
  覆盖阶段 100 codegen metadata 只读工具支持带来的约 3,205,504 bytes release CLI 实测体积；
  `nox_core` 上限和零第三方 runtime dependency 约束不变。
- 文档：ADR 0031 增加 LSP/IDE 第五轮设计结论，继续只交付集成式 `nox lsp`；阶段 102
  首选实现收敛为 generated source 的只读 IDE 标注，不做独立 LSP 二进制、跨文件 rename、
  后台 daemon、跨 invocation index 或真正 source-map 穿透。
- LSP：当打开文件匹配 manifest `[codegen]` artifact 的 `generated` 路径时，hover 会追加
  generated source 只读标注，包含 artifact 名称和可选 generator/template/input hash/command；
  不执行生成器，也不改变 diagnostics、definition、rename、formatting 或 semantic tokens。
- 文档：ADR 0032 增加数据脚本标准库第五轮设计结论，阶段 104 首选 `std/xml.nox`
  namespace 文本生成 helper；YAML 完整实现、XML parser/namespace resolver、压缩/归档、
  protobuf、SQLite/database driver 和 TLS/HTTPS 继续暂缓。
- 标准库：`std/xml.nox` 新增 `qname`、`xmlns`、`xmlns_default`、`empty_element_ns`
  和 `text_element_ns`，用于安全生成 namespace-qualified XML 文本；仍不解析 XML、不解析
  namespace scope、不做 schema validation、XPath 或 streaming writer。
- 工具：ADR 0025 将 release CLI size cap 继续校准到 3.0703125 MiB（3,219,456 bytes），
  覆盖阶段 104 XML namespace helper 带来的约 3,211,904 bytes release CLI 实测体积；
  `nox_core` 上限和零第三方 runtime dependency 约束不变。
- 标准库：新增实验性 `std/traits.nox` 小核心，导出 `Eq`、`Display`、`equal<T: Eq>` 和
  `display<T: Display>`，用于 trait/interface 第三轮的显式标准库抽象迁移；不建立 prelude，
  不移除 `std/array.nox` 既有 `Eq` helper 或旧 marker helper。
- 语言：泛型函数调用现在可以从 `task[T]` 参数推断 payload 类型，例如
  `fn identity<T>(value: task[T]) -> task[T]` 能接受 `task[int]`。
- LSP：hover 和 signature help 现在能展示泛型函数的 trait bound，例如
  `fn label<T: Display>(value: T) -> str`，避免静态 trait 调用点只显示返回类型而丢失
  bound 信息。
- LSP：`nox lsp` 新增 `semanticTokensProvider` 和 `textDocument/semanticTokens/full`，
  对已打开文档返回词法级 semantic token stream；继续不拆独立 LSP 二进制、不做
  generated/codegen source map 或 external dependency source map。
- LSP：`initialize` 现在为 code action 声明 `quickfix`、`source.fixAll.nox` 和
  `source.format.nox`；`textDocument/codeAction` 返回 `nox.check`、`nox.format` source
  action，并对可见 `TODO` marker 返回精确范围 quickfix edit。LSP 仍只通过集成式
  `nox lsp` 子命令交付，不拆独立二进制或 package。
- 语言设计：ADR 0027 增补 trait/interface 第三轮路线，下一步优先在 method lookup 收敛和
  `std/traits.nox` 小核心迁移之间选择最小实现；associated type、blanket impl、dynamic
  dispatch、trait object、async trait 和 `interface` 别名继续暂缓。
- 工具：ADR 0025 将 release CLI size cap 从 3.0 MiB（3,145,728 bytes）小幅校准到
  3.0078125 MiB（3,153,920 bytes），覆盖阶段 79 trait IDE hover/signature 增量；第三方
  runtime dependency 仍保持 0。
- 工具：ADR 0025 将 release CLI size cap 继续校准到 3.03125 MiB（3,178,496 bytes），
  覆盖阶段 83 YAML/XML 纯计算标准库增量；第三方 runtime dependency 和新增 runtime
  capability 仍保持 0。
- 工具：ADR 0025 将 release CLI size cap 继续校准到 3.0390625 MiB（3,186,688 bytes），
  覆盖阶段 90 `std/traits.nox` 小核心增量；第三方 runtime dependency、新增 runtime
  capability、prelude 和默认授权仍保持 0。
- 发布：新增 `scripts/release-asset-smoke.sh`，对 GitHub Release tarball 目录执行只读
  download/repair smoke：校验 `.sha256`、解包资产、运行 host-compatible CLI tarball，并
  编译 host-compatible embed C ABI 包；release gate 仅运行该脚本的 self-test。
- 发布：`scripts/release-prep-dry-run.sh` 在工作区已经处于目标候选版本时改为只读验证
  candidate readiness 和 release notes，避免把“已准备版本”误报为 dry-run 失败。

## [0.0.5] — 2026-05-24

### 工具和发布维护

- 发布：新增 `scripts/cross-cli-smoke.sh` 和 GitHub Actions `Cross CLI smoke (x86_64 musl)`，
  为 `x86_64-unknown-linux-musl` 建立 CLI-only 交叉构建与 hello smoke；该目标暂不扩大
  嵌入式 SDK / C ABI 资产承诺。
- 发布：`scripts/build-release-assets.sh` 新增 `CLI_ONLY_TARGET_TRIPLES`，可为已 smoke
  的 CLI-only 目标生成 `nox-cli-*` tarball；`TARGET_TRIPLES=""` 可用于只验证 CLI-only
  产物路径。
- 发布：明确本 release 线暂缓 crates.io 发布；`nox` 已被其他项目占用，`nox_core`
  会解析到已有的 `nox-core` crate。当前继续以 GitHub tag install、release tarball 和源码
  checkout 作为支持的分发路径。
- 工具：新增 `scripts/compatibility-golden.sh` 并接入 release gate，显式固定
  parser/formatter 表面、CLI diagnostic JSON、LSP diagnostic JSON、`nox doc` 输出、
  project lockfile JSON 和 host-metadata API JSON；release gate 同时新增 parser AST
  golden、C ABI enum 数值和 async Rust API task focused tests，避免语言扩展后机器可读
  表面静默漂移。
- 发布：新增 `scripts/release-candidate-readiness.sh` 并接入 release gate / release audit，
  在正式 release-prep commit 前固定 `v0.0.4` 当前生产版本、`v0.0.5` 下一候选流程、
  `[未发布]` CHANGELOG、gnu + musl CLI-only 资产口径、实验/暂缓功能标注和 crates.io
  暂缓决策，避免候选收敛阶段把 checkpoint、RC 和 production release 承诺混在一起。
- CLI：新增 `nox new <name> [--dir <path>] [--force]`，生成包含 `nox.toml`、
  `src/main.nox`、`tests/main_test.nox` 和 `README.md` 的最小项目。默认拒绝覆盖非空
  目标目录；`--force` 只覆盖脚手架文件，不删除未知用户文件。
- 标准库：`std/http.nox` 新增 `request` / `request_binary` 通用入口，支持自定义请求
  headers，并返回 response headers；response header name 统一小写，重复 header 用
  `", "` 折叠。仍仅支持明文 HTTP/1.1，继续要求 `network` capability。
- 标准库：新增纯计算 `std/jsonl.nox` 和 `std/hash.nox`，提供 JSON Lines
  `parse_lines` / `format_lines` 以及 SHA-256 `sha256_hex` / `sha256_text`；CSV/TSV
  模块新增 eager 多行 `parse_rows` / `format_rows`，错误信息包含 1-based 行号。
- 标准库：`std/hash.nox` 新增纯计算 HMAC-SHA256 helper：
  `hmac_sha256_hex(key: [int], bytes: [int]) -> str` 和
  `hmac_sha256_text(key: str, value: str) -> str`，保持无权限、零第三方依赖。
- LSP：`initialize` 新增 `workspaceSymbolProvider`，`workspace/symbol` 返回项目内顶层
  `fn` / `record` / `enum` / `trait` / `type` 声明；发现 manifest 时按 `modules.source_dirs`
  扫描，并合并已打开文档 overlay。
- LSP：`textDocument/definition` 支持跨文件跳转到 imported module 的 exported 顶层声明，
  覆盖 `import "path" as alias; alias.member` 与直接 `import "path"; Symbol`；查找遵循
  manifest `modules.source_dirs` 和 open-document overlay，虚拟 stdlib module 保守返回
  `null`。
- LSP：新增 `renameProvider` 与 `textDocument/prepareRename` / `textDocument/rename`
  当前文件保守 rename。当前只重命名顶层 symbol；如果存在同名局部声明或参数，prepare/rename
  返回 `null`，避免跨作用域误改。跨文件 rename 仍未开放。
- LSP：`textDocument/completion` 在 `import "..."` 字符串内会提示 `std/*` 虚拟模块和
  manifest `modules.source_dirs` 下的项目模块路径。
- LSP：普通 `textDocument/completion` 现在会提示 manifest `modules.source_dirs` 下项目
  `fn` / `record` / `enum` / `trait` / `type` 顶层声明，并继续排除项目级 `let` / `const`
  以降低噪声。
- LSP：`value.` completion 新增保守的当前文档 method 建议：当 receiver 有显式
  `let value: Type` 注解时，会提示第一个参数为 `Type` 的 record-style 函数和
  `impl Trait for Type` 中的方法；无法确定 receiver 类型时仍返回空结果。
- LSP：hover 与 signature help 现在对 `async fn` 展示源码返回类型和调用侧
  `task[T]` 返回信息；`std/fs.nox` / `std/http.nox` 新增的 `_async` helper 也会出现在
  namespace completion 中。
- LSP：hover 在 namespace import alias 上显示 module source 和 exported surface，例如
  `module std/fs.nox` 及其导出成员列表；项目模块同样只展示导出表面。
- LSP：workspace symbol 和项目顶层 completion 现在复用进程内 symbol graph cache；
  `didOpen` / `didChange` 会失效并重建缓存，避免编辑器看到陈旧顶层声明。
- LSP：publishDiagnostics 增加进程内诊断缓存；缓存绑定当前 LSP 文档 revision 和 source
  hash，`didOpen` / `didChange` 会让旧诊断失效，避免 imported open document 改动后沿用旧结果。
- 语言：新增静态 trait MVP：parser/AST 接受 `trait` 和 `impl Trait for Type`，typechecker
  校验 required method、impl 完整性、签名匹配、重复 impl 和 `T: Trait` bound；`nox fmt`、
  `nox doc` 和 LSP document symbol 已识别 trait 声明。该能力仍不包含动态 dispatch、trait
  object、blanket impl 或 associated type。impl method 会编译为内部 mangled 函数名，并按
  receiver nominal type 分派，因此不同类型可以实现同名 trait method；当前 MVP 仍保守拒绝
  impl method 与顶层函数同名。
- 标准库：`std/array.nox` 新增实验性 `Eq` trait、基础 primitive impl，以及 trait-bound
  helper `contains_equal<T: Eq>` / `dedupe_equal<T: Eq>`；旧
  `contains_value<T: Equatable>` / `dedupe<T: Equatable>` 保持兼容。
- 文档：新增 ADR 0026，确定 Nox 包生态第一阶段不做自建 registry，改走 GitHub /
  git URL module、版本 pin、lockfile、cache 和离线复现路线。
- 文档：新增 ADR 0027，确定后续语言抽象采用单一 `trait` 关键字和纯静态 trait 系统；
  旧 ADR 0020 的内建 marker 约束保留为 v0.0.x 兼容层。
- 文档：新增 ADR 0028，确认 Nox 继续以 `result` / `option` / `?` / diagnostic 为错误模型；
  不引入 `throw` / `catch` / `finally`，并暂缓 Rust 风格 `try {}` block。阶段 65 改为
  result/option helper、文档和边界测试收敛。
- 标准库：`std/option.nox` 新增 `map` / `and_then`，`std/result.nox` 新增 `map` /
  `map_err` / `and_then`，用于在不引入 `try {}` block 或异常机制的前提下组合可恢复值。
- 文档：新增 ADR 0029，暂缓内建宏系统；当前推荐用函数、trait、标准库 helper 或显式外部
  codegen 处理重复样板，不引入 `macro`、attribute、procedural macro 或编译期执行。
- 文档：新增 ADR 0030，确定 async/await 采用分阶段路线：先 awaitable task runtime，再
  `async fn` / `await` 语法；第一轮保持单线程、显式 capability、无 IO reactor，也不隐式
  授权文件、网络、环境、计时器或进程能力。
- 语言：新增 `task[T]`、`async fn` 和 `await` 的最小语法闭环；`await` 只能在
  `async fn` 内消费 `task[T]`，top-level 未消费 task 以 `async.top-level-task` 诊断拒绝。
  `async fn -> result[...]` / `option[...]` 内的后缀 `?` 按声明 payload 返回类型传播，
  继续不捕获 permission denied、resource cap 或其它 runtime diagnostic。
  `task_sleep(ms) -> task[null]` 和 `std/task.nox` 的 `sleep(ms) -> task[null]` 已接入
  与 `task_sleep_ms` 相同的 runtime task 表、`async task` capability、pending 上限和清理规则。
  async 函数失败后会清理本次 eval 新建的 awaitable sleep task，并保留 awaitable 边界上的
  host/script stack frame。
  `nox doc` 已识别 `async fn`，并在保留源码签名的同时展示调用侧 `task[T]` 返回类型；
  新增 `examples/async.nox` 展示最小 async/await 用法。
  当前仍不提供 IO reactor、top-level await、async trait 或跨 C ABI task handle。
- 标准库：`std/fs.nox` 和 `std/http.nox` 新增 `_async` 后缀 wrapper，供 `async fn`
  内 await 文件系统和 HTTP helper 的结果。wrapper 只包裹现有同步 helper，不引入 IO
  reactor、后台调度或隐式授权；await 后仍复用原有 filesystem / filesystem_write /
  network capability、allowlist、mock、timeout、响应上限和 diagnostic。
- Rust API：`nox::Runtime` 新增 `spawn_sleep_task`、`poll_async_task`、
  `cancel_async_task` 和 `AsyncTaskPoll`，让嵌入宿主可以用与 `std/task.nox` 相同的权限、
  pending 上限、unknown-id 诊断和清理规则驱动单 runtime task 表。C ABI 当前仍不暴露
  runtime task handle。
- Manifest：`nox.toml` 新增 `[dependencies]` schema skeleton，支持 pinned GitHub /
  git URL dependency 声明；`project check` 现在要求有依赖的项目提供匹配的 `nox.lock`，
  并在 `project check --json` 中报告 lockfile 状态。当前不会下载依赖或接入 import resolution。
- CLI：新增 `nox fetch [--offline] [--cache-dir <dir>]`，显式下载 pinned GitHub/git
  dependency 到 module cache，解析 tag/rev 到完整 commit，计算 `sha256:<hex>` content hash，
  并写入 `nox.lock`。`--offline` 只消费已有 cache，cache miss 或 corrupt cache 会失败；
  下载动作不授予脚本运行阶段的 runtime permissions。
- 模块：`run`、`check`、`test` 和 LSP diagnostics 开始支持
  `import "<dependency>/<path>.nox"`，从 `nox.lock` 指向的 module cache 读取 pinned external
  module，并校验 cache archive hash。缺 lockfile、cache miss、cache corrupt 或 hash mismatch
  都会诊断失败；普通命令不会静默联网。
- CLI：`nox doc` 改为结构化顶层声明扫描，支持跨行函数签名的 doc 输出，并避免把函数体内
  nested declaration 误作为顶层 API。stdlib index 测试现在同时校验 runtime registry 中的
  std module 是否都列入中英文索引。
- 工具：ADR 0025 将 release-gate CLI 二进制大小上限从 2.75 MiB 小幅校准到
  2.8125 MiB（2,949,120 bytes），吸收 LSP 跨文件 definition 与当前文件 rename 的明确工具面
  增量；`nox_core` 1.5 MiB 上限和零第三方运行时依赖约束不变。
- 工具：阶段 56 的 manifest dependency / `nox.lock` 校验和 release gate lockfile guardrail
  让 release CLI 实测增至约 2,958,736 bytes；ADR 0025 将默认 CLI size cap 继续小幅校准到
  2.84375 MiB（2,981,888 bytes），`nox_core` 1.5 MiB 上限和零第三方运行时依赖约束不变。
- 工具：阶段 57-58 的 `nox fetch`、module cache、external import resolution 和 cache hash
  校验让 release CLI 实测约 2,982,960 bytes；已移除 CLI 内重复 SHA-256 实现并复用库内 helper，
  ADR 0025 将默认 CLI size cap 小幅校准到 2.8515625 MiB（2,990,080 bytes）。`nox_core`
  1.5 MiB 上限和零第三方运行时依赖约束不变。
- 工具：阶段 60 的 LSP import path completion、项目顶层 symbol completion、module alias
  hover 和 diagnostic cache 让 release CLI 实测约 3,013,504 bytes；ADR 0025 将默认 CLI
  size cap 小幅校准到 2.875 MiB（3,014,656 bytes）。`nox_core` 1.5 MiB 上限和零第三方
  运行时依赖约束不变。
- 工具：阶段 62 静态 trait MVP 和 impl method receiver dispatch 让 release CLI 实测约
  3,108,032 bytes；ADR 0025 将默认 CLI size cap 小幅校准到 2.96875 MiB
  （3,112,960 bytes）。`nox_core` 1.5 MiB 上限和零第三方运行时依赖约束不变。
- 工具：阶段 68-70 async/await MVP、awaitable runtime task 桥接、async diagnostics 和
  `nox doc` async 展示让 release CLI 实测约 3,135,568 bytes；ADR 0025 将默认 CLI size
  cap 小幅校准到 3.0 MiB（3,145,728 bytes）。`nox_core` 1.5 MiB 上限和零第三方运行时依赖
  约束不变。
- 发布：`scripts/release-gate.sh` 新增 module ecosystem regression 门禁，显式运行
  project check lockfile JSON、`nox fetch` offline/cache、external import cache/hash mismatch
  和集成式 `nox lsp` external import 回归测试。
- 文档：补充 module cache 定位、清理和锁网 CI 复现说明；external import 的 cache missing /
  corrupt / hash mismatch 诊断已有 CLI JSON 回归覆盖。
- 发布：`scripts/build-release-assets.sh` 支持 `TARGET_TRIPLES="..."` 目标矩阵，默认仍只构建
  当前 Rust host triple；README 和 release checklist 明确非 host 目标在 CI/smoke 覆盖前
  属于 best-effort。
- 发布：补齐 `nox` / `nox_core` crates.io package metadata 和 crate README；`nox`
  对 `nox_core` 同时使用本地 `path` 与精确版本约束。README 区分 crates.io install、
  GitHub tag install 和本地 checkout install，并记录当前 `nox` package name 已被 crates.io
  上其他项目占用；release checklist 新增 crates.io dry-run、registry name 预检与 Rust API /
  C ABI / CLI JSON / diagnostic code 的 SemVer 风险审计。
- 文档：新增中英文 cookbook，按项目创建、CLI stdin/stdout/stderr/exit、JSON/TOML
  配置、文件权限、HTTP、CSV/TSV/JSONL 类数据、测试 helper 和 embedding host function
  组织现有示例与任务配方。
- CI：GitHub Actions workflow 升级到 `actions/checkout@v6` 与 `actions/cache@v5`，
  消除 Node.js 20 action runtime deprecation 注解，并提前适配 GitHub-hosted runner
  默认切换到 Node.js 24 的时间线。

## [0.0.4] — 2026-05-24

### 稳定和兼容改进

- 集合 mutation：`Value::Array` 与 `Value::Map` 内部 `elements`/`entries` 字段改为
  `RefCell` 包裹，所有持有同一 `Rc<Array>` / `Rc<Map>` 的 alias 共享底层存储和可观察的
  mutation。Rust API 新增 `Array::push`、`Array::pop`、`Array::set`、`Map::set`、
  `Map::delete` 等方法；`Array::elements()` 与 `Map::entries()` 维持原签名但现在返回
  owned snapshot（之前返回引用），下游 embedder 升级时需要在 snapshot 之外保留自己的
  借用。C ABI handle 仍只读，不暴露 mutable 入口。
- 语言能力：parser 接受 `fn(T1, T2, ...) -> R` 作为类型语法（之前仅作为内部签名存在）。
  类型可出现在参数注解、返回类型、`let` 类型注解和容器元素类型中；命名函数可以通过
  `let f: fn(int) -> int = double;` 绑定为 first-class value，并在调用、函数参数、
  函数数组中使用。
- 语言能力：新增 lambda 字面量 `fn(x: int) -> int { ... }`；可绑定到变量、作为参数
  传递、放入容器，闭包通过 lexical env 自动捕获外部 binding（不引入显式 capture
  clause，by-value-for-scalars + alias-for-containers）。typecheck 的泛型推导
  `unify_call_type` 加入 `Type::Function` 分支以支持 `fn<T,U>(values: [T], f: fn(T) -> U)`
  这类签名。
- 标准库：`std/array.nox` 新增高阶 helper `map_fn<T,U>(values, f) -> [U]`、
  `filter_fn<T>(values, predicate) -> [T]`、`reduce<T,A>(values, init, f) -> A`、
  `for_each<T>(values, f) -> null`。这些 helper 用纯 nox 实现，调用 lambda/命名 fn
  通过现有 Value::Function 调用路径，不引入新指令；与现有 alias-shared mutation
  helper 协同。
- 语言能力：泛型函数声明支持受限结构化约束 `fn name<T: Marker, U: M1 + M2>(...)`。
  内建 marker 集合：`Equatable`、`Comparable`、`Stringify`、`Hashable`，typecheck
  内置匹配规则（int/float/str/bool 全部支持；null/json 支持 Stringify + Equatable；
  容器按元素递归；record/enum 视为不透明仅满足 Stringify；function 类型仅满足
  Stringify）。约束违反在调用点报稳定 code `generic.constraint-unsatisfied`，未知
  marker 报 `generic.constraint-unknown`。当前不开放用户自定义 trait/marker。
- 标准库：`std/array.nox` 新增 `dedupe<T: Equatable>(values) -> [T]` 和
  `contains_value<T: Equatable>(values, target) -> bool`，作为受限约束的真实使用点。
- 语言能力：lexer 把 `try`、`catch`、`panic`、`defer`、`finally` 列为保留字，按
  ADR 0021 暂留供未来异常机制评估。源码使用任一保留字作为标识符返回新稳定 code
  `parse.reserved-keyword`。这是兼容收紧（之前可作普通 identifier）。
- Rust API：`Runtime::set_mock_clock_unix(Option<i64>)` 让宿主注入固定 Unix 秒时间。
  设置后 `time.now_unix()` 返回固定值，`time.now_unix_ms()` 返回该值 × 1000。设为
  `None` 即恢复真实时钟。专为测试框架与确定性 snapshot 测试场景设计；不影响
  `sleep_ms` 等真实计时行为。
- Rust API：`Runtime::set_mock_env(Option<BTreeMap<String, String>>)` 让宿主注入
  确定性环境变量集。设置为 `Some(map)` 时 `env.get(name)` / `env.try_get(name)` /
  `env.list()` 全部读取 mock map（未命中 key 时 `env.get` 返回带 "not present in
  mock env" 的诊断、`env.try_get` 返回 `none`）；设为 `None` 即恢复读取真实进程
  环境。仍受 `environment` capability 控制，未授权时与真实环境一致返回 capability
  诊断。
- Rust API：`Runtime::set_mock_stdin(Option<String>)`、`set_mock_stdout(bool)` 和
  `take_stdout()` 让嵌入宿主在单个 runtime 上替换 `std/process.nox` 的
  `read_stdin()` 输入，并捕获脚本 `print(...)` 输出；`set_mock_stdin(None)` 恢复
  真实 stdin，`set_mock_stdout(false)` 停止 stdout 捕获。`take_stderr()` 继续读取
  `print_err(...)` 写入的 stderr 缓冲。
- Rust API：新增 `MockFilesystem` 与
  `Runtime::set_mock_filesystem(Option<MockFilesystem>)`。启用后
  `std/fs.nox` 的 `read_text`、`try_read_text`、`exists`、`is_file`、
  `is_dir`、`list_dir`、`read_binary`、`write_text`、`write_binary` 和
  `canonicalize` 从 mock 文件集读写；每个入口仍先检查对应 capability 与
  allowlist，mock 不会授予或扩大文件权限。启用 mock 后写入 helper 写入 mock
  存储，不触碰真实文件系统。
- Rust API：新增 `MockNetwork` / `MockHttpResponse` 与
  `Runtime::set_mock_network(Option<MockNetwork>)`。启用后 `tcp_connect` 和
  `std/http.nox` 的 `get` / `post` / `get_binary` / `post_binary` 读取 mock
  网络响应；每个入口仍先检查 `network` capability，mock 不会授予网络能力。
  未配置的 mock HTTP 请求返回 `result.err`，不会回落到真实网络。Criterion
  `runtime_capabilities` 增加 `runtime/http-get-mock` case。
- 标准库：`std/json.nox` 新增 `require_field(value, path, expected_kind)` 和
  `validate_schema(value, required_fields)`。`require_field` 支持 `server.port` /
  `tags[1]` 形式路径，类型不匹配或路径不存在返回带路径的 err；`validate_schema`
  非递归检查 JSON object 必填字段，缺失字段拼接到 message。
- 标准库：`std/json.nox` 新增 `validate_object(value, required_fields,
  allowed_fields)`，在 `validate_schema` 的必填字段检查基础上额外报告 unknown
  object keys。错误 message 会同时包含缺失字段和未知字段，便于配置 / schema
  检查脚本一次性展示所有顶层 object 问题。
- 标准库：`std/json.nox` 新增 `apply_defaults(value, defaults) -> result[json,
  str]`。两个参数都必须是 JSON object；返回值会把 `defaults` 中缺失的顶层 key
  注入到 `value` 的副本中，已有 key 不覆盖，用于配置 schema 的默认值补全。
- 标准库：`std/json.nox` 新增 `apply_defaults_deep(value, defaults) -> result[
  json, str]`。在 `apply_defaults` 的顶层规则基础上递归合并嵌套 JSON object，
  只补缺失字段，不覆盖用户已有字段，满足配置 schema 的嵌套默认值补全。
- Manifest：`nox.toml` schema 改为封闭集合，未知 section 或 `[package]` /
  `[modules]` / `[runtime]` 内未知 key 现在返回稳定 code `manifest.invalid`。
  `nox project check --json` 的 `nox.project-check.v1` 兼容新增
  `schema_validation` 顶层对象，报告有效 manifest 已按封闭 schema 校验。
- 标准库：`std/term.nox` 新增 `is_tty_stderr() -> bool`，对称于现有
  `is_tty_stdout`；Unix 上通过 `isatty(stderr_fd)` 实现，Windows 上通过
  `GetConsoleMode` 实现，其他平台返回 false。
- 标准库：`std/term.nox` 新增 `progress(current, total, width) -> str`，纯计算
  ASCII 进度条字符串（`[####-----] 4/10 (40%)`）。current 截到 `[0, total]`，
  total = 0 时百分比显示为 0%，width 必须非负。返回字符串而不打印，非 TTY 安全。
- 标准库：`std/term.nox` 新增 `prompt_password(message) -> result[str, str]`，
  Linux 平台通过 termios (`tcgetattr` / `tcsetattr`) 直接关闭 stdin 回显并在读取后
  恢复原始终端模式；回显控制不可用时返回
  `term.prompt-password.echo-disable-failed` 而不静默退化为回显输入。EOF 返回
  `term.prompt-password.eof`，I/O 错误返回 `term.prompt-password.read-failed`。
  message 写入 stderr。
- 标准库：`std/term.nox` 新增 `select(message, items, default_index) -> result[
  int, str]`。把 `items` 渲染为带星标 default 的数字菜单到 stderr，从 stdin 读取
  1-based 整数；空输入或 EOF 配合合法 default 时返回 default index，越界 / 非数字
  返回 err。password input / progress bar / TUI 仍留作后续 session。
- 标准库：新增 `std/random.nox` 提供 seeded PRNG（xorshift64）：
  `next_int(seed, min, max) -> (int, int)`、`next_bool(seed) -> (int, bool)`、
  `next_float_unit(seed) -> (int, float)`。所有 helper 返回 `(下一个 seed, 取值)`，
  纯计算无 capability 依赖。`min > max` 报 runtime 诊断。让脚本可以写可复现的
  property test 循环（自行用 next_int(seed, 0, len(arr)-1) 抽取数组）。
- 测试框架：`std/test.nox` 新增 deterministic property helper：
  `gen_int` / `gen_bool` / `gen_string` / `gen_int_array` / `gen_int_map` 以及
  `assert_property_int(label, seed, cases, min, max, property)`。失败时会把 int
  counterexample shrink 到更小值，并在 `test.assertion-failed` 诊断中写入
  seed、case index、原始 value、minimized value 和 replay metadata。
- 测试框架：新增
  `assert_property_int_array(label, seed, cases, len, min, max, property)`，
  对 `[int]` property case 执行结构化 shrink：先尝试缩短 failing prefix，再把
  元素向 0 shrink；失败诊断写入原始长度、最小化长度、首元素和 replay metadata。
- 测试框架：新增
  `assert_property_int_map(label, seed, cases, len, min, max, property)`，
  对 deterministic `k0..` `map[str, int]` property case 执行结构化 shrink：先缩短
  key prefix，再把 value 向 0 shrink；失败诊断写入原始长度、最小化长度、首元素和
  replay metadata。
- 测试框架：`std/test.nox` 新增显式构造器版 record/enum property helper：
  `gen_record3<T>(seed, min, max, build)`、`gen_enum3<T>(seed, min, max, max_len,
  build_int, build_str, build_bool)`、`assert_property_record3<T>(...)` 和
  `assert_property_enum3<T>(...)`。由于 Nox 当前没有反射/宏，record helper 通过
  `fn(int, str, bool) -> T` builder 生成三字段值，enum helper 通过三个 variant
  builder 生成三路 variant；失败时会结构化 shrink 字段 / payload / variant，并写入
  replay metadata。
- CLI：`nox test` 新增 `--export-failures <dir>` opt-in fuzz bridge。失败诊断包含
  property replay metadata 时，会把原始 `.nox` 测试源码和 source/test/diagnostic
  注释导出成 corpus 文件，可指向 `fuzz/corpus/...` 或 `tests/malformed/...`。
- CLI：`nox test` 新增 `--export-failures-classified <dir>`，在保留
  `--export-failures` 扁平导出兼容行为的同时，把 property replay 和模块级 malformed
  失败按 diagnostic code 自动写入 `property` / `parser` / `typecheck` / `verifier` /
  `runtime` 子目录，便于把导出 case 分流到 fuzz corpus 或坏输入回归。
- 工具：`scripts/release-gate.sh` 新增 opt-in quality layers：
  `NOX_RELEASE_GATE_PROPERTY=1` 跑 property failure export smoke，
  `NOX_RELEASE_GATE_COVERAGE=1` 跑 coverage JSON / NDJSON smoke。默认 release gate
  不跑这两层，保持 fast path 稳定。
- 标准库：`std/json.nox` 新增类型化标量提取 helper：`as_int(value: json) ->
  result[int, str]`、`as_float`、`as_str`、`as_bool`、`as_array(value) ->
  result[[json], str]`、`as_object(value) -> result[map[str, json], str]`。
  JSON kind 不匹配时返回 `result.err`（如 `expected JSON number, got string`）；
  `as_int` 额外要求数字为有限整数。配合 `object_get` / `require_field`，脚本
  可以手工把 JSON 映射为 record / enum；后续 `from_json<T>` 自动反序列化复用同一
  类型提取语义。
- 标准库：`std/json.nox` 新增 `to_json<T>(value: T) -> json` 一向序列化 helper。
  支持把任意 Nox value 转成 `json`：record→object（按字段名）、enum 带 payload→
  `{"_variant", "payload"}` / 无 payload→`"VariantName"` 字符串、tuple→array、
  map→object、option→payload 或 null、result→`{"_variant": "ok"|"err",
  "payload"}`、scalar/null 透传。function 值返回 runtime 诊断。Rust API：`Record`
  公开 `name()`/`fields()` accessor，`EnumValue` 公开 `name()`/`variant()`/
  `payload()` accessor。tagged enum 的默认 JSON 表示已采纳 adjacent 形状并由
  ADR 0024 固定；`from_json<T>` 反向自动映射使用同一 adjacent 契约。
- 标准库：`std/json.nox` 新增 `variant_name(value: json) -> result[str, str]`
  与 `variant_payload(value: json) -> result[json, str]`，用于手工解析 adjacent
  tagged enum JSON。无 payload enum 字符串可通过 `variant_name` 读出名称；带 payload
  object 可读取 `_variant` 和 `payload`，为后续自动 enum 反序列化保留同一契约。
- 标准库：`std/json.nox` 新增显式 builder 反解析 helper：
  `decode_record3<T>(...) -> result[T, str]` 对三个 path-aware 字段执行 kind 校验后
  调用 `fn(json, json, json) -> result[T, str]` builder；
  `decode_adjacent_enum3<T>(...) -> result[T, str]` 按 adjacent enum variant 名称分派到
  三个 `fn(json) -> result[T, str]` builder。该层作为 `from_json<T>` 之外的显式
  可控扩展点保留。
- 标准库 / VM：`std/json.nox` 新增 `from_json<T>(value: json) -> result[T, str]`
  自动反序列化入口。调用点必须提供 expected `result[T, str]` 类型，编译器把目标
  type 写入 `JsonDecode` bytecode，VM 按 record 字段、adjacent enum variant、scalar、
  array、map、option 和 result 规则构造目标值；缺失字段、未知字段、类型不匹配和
  unknown variant 返回 path-aware `result.err`，不抛 runtime diagnostic。
- CLI：新增 `nox doc <file.nox>` 子命令，扫描文件中的顶层 `fn` 声明并输出 Markdown
  文档；紧邻 fn 上方的连续 `///` doc comment 作为函数描述（吞掉单个前导空格以匹配
  Markdown 风格）。当前为 text-based 扫描，富 AST 抽取、LSP hover 复用、stdlib
  自动校验留作后续 ADR 评估。`scripts/release-gate.sh` product-shape guardrail 同步
  加入 `nox doc`。
- 测试框架：`TestCaseResult` 新增 `duration_us: u128` 字段记录每个 test 的真实
  执行时间（包含 before_each / test body / after_each）。CLI JSON 输出
  `nox.test.v1` 每条 test 加 `"duration_us": int` 字段（兼容扩展，旧消费者忽略）。
- 测试框架：`nox.test.v1` 每条 test 记录兼容新增 `"stdout"` / `"stderr"` 字段。
  `nox test --json` 期间脚本 `print(...)` 与 `std/process.nox` `print_err(...)`
  按测试 case 捕获，避免污染外层 JSON stdout/stderr。
- 测试框架：`std/test.nox` `assert_snapshot` 失败时，`nox.test.v1` 对应 test
  记录兼容新增 `"snapshot_diff": {"label", "actual", "expected"}`；非 snapshot
  失败或通过用 `null`。
- 测试框架：`nox.test.v1` 兼容新增顶层 `"suites"` 数组，按测试文件输出
  `{file, cases}`，让编辑器和 CI 工具可以直接消费 suite/case hierarchy，而不必从
  平铺 `tests` 数组推导。
- LSP：新增 Nox 扩展请求 `nox/testDiscovery`，按当前文档顶层 `test_*` 函数返回
  `{uri, name, range}` 列表，供编辑器 test explorer 直接发现测试；现有
  `textDocument/codeLens` 继续使用同一规则生成 `nox.runTest` 命令。
- 测试框架：`std/test.nox` 新增 `assert_snapshot(label, actual, expected)`：失败时
  返回 `test.assertion-failed` 并打印截断的 actual / expected 文本对比（160 字符以上
  自动截断并标注剩余长度）。
- 测试框架：`std/test.nox` 新增 `assert_table_row<T: Equatable>(label, index, actual,
  expected)`：用于 table-driven 测试，失败时附带 row index 信息。脚本可在
  `test_*_table() -> null` 中用 while + 数组遍历组合多个 case，不引入新语法。
- 测试框架：`nox test` runner 识别 `before_each() -> null` 和 `after_each() -> null`
  顶层函数，按约定在每个 `test_*` 函数前后调用。before_each 失败时跳过测试体并把
  失败原因记到测试结果；after_each 失败把原本通过的测试转成失败。两个 hook 都可
  返回 `null` 或 `bool`（与 test 函数同），多次声明同一 hook 报稳定 code
  `test.signature`。
- 标准库：新增 `std/bytes.nox` 提供轻量字节数组 helper：`encode_utf8(text) -> [int]`、
  `decode_utf8(values) -> result[str, str]`、`len`、`get`、`slice_copy`、`equal`、
  `base64_encode/decode`、`hex_encode/decode`。字节用 `[int]`（0..=255）表示，不引入新的
  `Type::Bytes`（核心 parser/typecheck/VM 改造留作未来 ADR）。非 UTF-8 字节、
  out-of-range int 都走 result.err / runtime 诊断；显示 / 线格式使用 hex/base64 helper。
- 标准库：`std/fs.nox` 新增 `read_binary(path) -> result[[int], str]` 与
  `write_binary(path, bytes) -> result[null, str]`，同样用 `[int]` 字节数组而不引入
  `Type::Bytes`。`read_binary` 复用 `filesystem` capability 与 read allowlist；
  `write_binary` 复用 `filesystem_write` capability 与 write allowlist；out-of-range
  字节 (超过 0..=255) 返回 `result.err`。这条路径让脚本可以直接读写二进制文件
  （图片、归档、二进制协议）而不必走文本编码桥接。
- 标准库：`std/http.nox` 新增二进制 body 变体：
  `get_binary(url, timeout_ms) -> result[(int, [int]), str]` 与
  `post_binary(url, body, timeout_ms) -> result[(int, [int]), str]`。两者复用
  既有 `network` capability、1 MiB 响应上限、30s 默认超时；body 与响应为 `[int]`
  字节数组，不做 UTF-8 lossy 转换，适合图片 / 归档 / 二进制协议。底层 `http_request`
  现在 wrap 新的 `http_request_bytes`，避免文本/二进制路径分叉。
- 标准库：`std/fs.nox` 新增 `canonicalize(path) -> result[str, str]`，使用
  `filesystem` read capability + read allowlist 校验输入路径；返回符号链接解析后
  的绝对路径，底层 `fs::canonicalize` 失败时返回 `result.err`。
- 测试框架：`nox test` 新增 `--retry <N>` 选项（0..=10）。失败 test 会被最多重跑
  N 次；最后一次的结果作为最终结果。CLI JSON 输出每条 test 加 `attempts: int`
  与 `retried: bool` 字段（兼容扩展，旧消费者忽略即可）。flaky test 必须显式开启
  retry，默认行为不变。完整 property testing / coverage 强制门槛 / fuzz bridge
  留作后续 ADR。
- Rust API：`Runtime::engine_mut()` 公开 mutable Engine 访问，让宿主在创建 Runtime
  后追加注册 host 函数。配合新示例 `crates/nox/examples/rust_embedding_namespace.rs`
  演示 host namespace 约定（`<namespace>__<function>` 双下划线分隔），把 host 内部
  helper 名字与脚本 surface 分离。
- Rust API：`Engine::set_max_string_length(Option<usize>)` 给嵌入宿主提供
  字符串长度上限配置。脚本拼接 (`+`) 超过 cap 时返回稳定诊断 code
  `runtime.string-length-cap`，message 含实际长度与 cap。未配置 cap 时不触发
  该诊断。用于限制脚本内存使用的细粒度控件。
- Rust API：`Engine::set_max_array_length(Option<usize>)` 与
  `Engine::set_max_map_entries(Option<usize>)` 给嵌入宿主提供数组 / map 大小
  上限配置。超出 cap 时返回稳定诊断 code `runtime.array-length-cap` /
  `runtime.map-size-cap`，message 含实际长度与 cap。覆盖字面量构造、`array.append`、
  `map.set` 与 map 索引赋值增长；更新既有 map key 不增加 entry 数。
- Rust API：`Engine::set_max_heap_objects(Option<usize>)` 给嵌入宿主提供跨
  string / json / container / option / result / enum / record / function 的 heap
  object 总数上限。VM 分配和 host callback 返回值都会登记到 engine heap；
  超出 cap 返回稳定诊断 code `runtime.heap-object-cap`。
- 基准测试：criterion `core_paths` 加 `core/eval-lambda` case；新增
  `tests/benchmarks/bench-lambda.nox` 覆盖 lambda + 闭包 capture + 高阶函数
  调用热路径。`scripts/bench-smoke.sh` 加 `lambda` 行（budget 0.5s，当前实测
  ~5ms）。
- 基准测试：新增 `tests/benchmarks/bench-host-capabilities.nox` 覆盖 permissioned
  filesystem host helper 路径，`scripts/bench-smoke.sh` 加 `host-capabilities` 行。
  新增 `crates/nox/benches/runtime_capabilities.rs`，Criterion 覆盖
  `runtime/fs-read-text` 与 `runtime/async-task-ready`；`NOX_BENCH_CRITERION=1`
  会同时运行 `nox_core` 与 `nox` runtime benchmark。
- Rust API：`HostFunctionBuilder` 新增 `.docstring(text)` 和 `.capability(name)`
  链式方法，让宿主在注册 host 函数时同步声明文档字符串与所需 capability（结构化
  存储为 metadata，不直接强制运行时检查）。新增 `Engine::host_function_names()` /
  `Engine::host_function_signature(name)` 公开 API，返回 `HostFunctionSignature`
  (含 name / type_params / params / return_type / docstring / capabilities)。
  CLI/LSP 基于这套 metadata 提供 host 函数 completion detail、hover、
  signature help 和 capability 审计入口。
- CLI：新增 `nox host-metadata --json`，输出 `nox.host-metadata.v1`，列出本地
  进程内注册的 host function 签名、docstring 与 capability metadata；文本模式
  输出同一内容的人类可读摘要。
- C ABI：新增 `nox_core_engine_register_host_function_ex`，在现有同步 host callback
  注册基础上追加可选 docstring 与 capability metadata。所有字符串在注册时复制；
  callback、`ctx`、engine userdata、last_error、线程与非重入规则沿用旧入口。
  `examples/embed/c_embedding.c` 改用该入口注册 `math__add_offset`，覆盖 host
  namespace + metadata smoke。
- 验证：`scripts/embedding-regression.sh` 在 release-gate 中跑新示例，保证 host
  namespace 示例不被静默破坏。
- CLI：`nox profile` 和 `nox coverage` 新增 `--json` 标志，输出 `nox.profile.v1` 与
  `nox.coverage.v1` schema：`{schema, functions: [{name, call_count, total_us}],
  operations: [{name, count, total_us}]}`。operation profile 覆盖 host callback、
  array / tuple / map literal、index/index assignment、match pattern、map_get /
  map_keys / map_values 等 VM 热路径。机器消费者可基于此构建 trace / 调用图分析。
  人类可读 profile 输出在函数表后追加 operation 表。
- Core / CLI：`nox coverage` 现在复用 VM span profile 数据，输出 statement
  execution count 与 branch true/false count。`nox.coverage.v1` 兼容新增
  `statements` / `branches` 数组，`nox.coverage.event.v1` 新增
  `kind:"statement"` / `kind:"branch"` 事件；每条 coverage 记录包含 byte span 与
  1-based source location。
- CLI：新增 `nox trace [--ndjson] <file.nox>`，输出 `nox.trace.event.v1`
  NDJSON 事件流。事件覆盖 run_start、静态 capability summary、捕获的 stdout/stderr、
  permission_check、function_profile、operation_profile、host_callback、diagnostic
  和 run_finish；每行都有稳定 `trace_id` 与递增 `seq`。
- CLI：`nox trace` 的 diagnostic 事件现在携带 `span`、`source` 和可用的
  `stack_frames`，让机器消费者能在同一条 `trace_id` / `seq` 事件里关联运行时错误、
  源码位置和脚本调用栈。
- Runtime / CLI：`nox trace` 启用 opt-in runtime trace buffer，并输出实际 host
  边界事件：`io`（stdout/stderr write）、`timer`（sleep 尝试）和 `task`（spawn /
  poll / cancel / join / pending_count）。事件同样使用 `nox.trace.event.v1`、
  `trace_id` 与递增 `seq`，并记录 capability 拒绝时的 `allowed:false`。
- Runtime / CLI：runtime `io` trace 事件扩展到 stdin read，以及顶层
  `read_text` / `write_text` 与 `std/fs.nox` filesystem 操作；事件包含 path、bytes、
  status、entries 和 capability 拒绝时的 `allowed:false`，便于排查脚本 I/O 边界。
- Core / CLI：profiled VM 执行现在记录逐调用 host callback trace 事件，`nox trace`
  输出 `host_callback_call` enter / exit 行，包含 callback name、span、elapsed_us
  和 exit status。原有聚合 `host_callback` operation summary 保持不变。
- DAP：`nox dap` 的 `initialize` 现在声明 `supportsConditionalBreakpoints` 和
  exception breakpoint filter；`setBreakpoints` 会保留并回显 breakpoint
  `condition` metadata，新增 `setExceptionBreakpoints` 响应并在 variables 中暴露
  exception filter 数量。`configurationDone` 会在 launch 后对简单
  `result == value` / `result != value` 条件做最小求值；匹配时发布
  `reason:"breakpoint"` stopped event，不匹配时直接 terminated。开启 `raised`
  exception filter 后，launch 阶段 runtime error 会发布 `reason:"exception"` stopped
  event，并在 variables 中暴露 `exceptionMessage`。
- DAP：`variables` request 支持可选 `depth` / `maxDepth` 参数。`0` 会抑制可展开
  child reference；更大值会暴露受深度限制的 `debugState` 子变量，避免编辑器在最小
  adapter 上无限展开变量。
- LSP：`textDocument/publishDiagnostics` 的 error 和 warning 现在都会在
  `data.trace_id` 中携带确定性关联 id；运行时栈帧仍放在同一个 `data` 对象中，方便
  编辑器或外部工具把 LSP 诊断与 trace/log 记录对应起来。
- Rust API：`Engine::set_max_call_stack_depth(Option<usize>)` 限制脚本函数调用栈
  深度。超出上限返回新稳定 code `runtime.call-stack-overflow`，未设置时使用 OS
  native stack 兜底（无 diagnostic）。`Vm` 内部新增引用计数式 `CallGuard` 在 drop
  时减少 depth，避免 panic / err 路径泄露。ADR 0023 记录了暂缓 cycle collector
  和 arena handle 的决策，明确依赖 `Rc + Weak<Env>` + 显式 budget 作为当前 v0.0.x
  内存治理基线。
- 文档：新增 `docs/{en,zh_CN}/stdlib-index.md` 按主题归类当前 stdlib 模块并标注
  稳定性（stable / stable, permissioned / experimental）。维护规则文档化：新 helper
  必须更新 stdlib-surface guardrail、新 capability 必须扩展 manifest / RuntimePermissions
  / docs。
- CLI：新增 `nox lint [--json] <file.nox> ...` 子命令，报告非阻断质量警告：
  `lint.unused-variable`、`lint.unused-function`、`lint.unused-import`，针对顶层
  声明 + AST 引用集合对比。`_`-前缀变量跳过 unused-variable 检查。退出码始终为 0
  即使有 warning；`--json` 输出 `nox.lint.v1` schema。release-gate product-shape
  guardrail 同步加入 `nox lint` 防止子命令被静默删除。
- CLI：`nox lint` 扩展加入 `lint.unreachable-code`，检测函数体、`if` / `else`
  分支、`while` / `for` body、lambda body、`match` case body 以及顶层 `Block`
  中 `return` / `break` / `continue` 之后的不可达语句。仅在每个 block 的第一条
  不可达语句处报告，避免噪音。
- CLI：`nox lint` 扩展加入 `lint.shadowed-variable`，检测内层 `let` 声明遮蔽
  外层同名 binding（函数参数、外层 `let`）；下划线前缀名豁免；同 scope 内的
  reassignment（`name = value`）不报。新增 4 个 nox_core 单元测试 + CLI 集成
  测试。tuple/record 解构 shadowing 仍留作后续 session。
- CLI：`nox lint` 扩展加入 `lint.constant-condition`，检测 `if (true)` /
  `if (false)`（条件总是 / 从不成立）以及 `while (false)`（body 永不执行）。
  `while (true)` 作为 forever-loop idiom 豁免（脚本通过 `break` 或 `return`
  退出）。新增 4 个 nox_core 单元测试 + CLI 集成测试。capability summary 留
  作后续 session。
- CLI：`nox lint` 扩展加入 `lint.duplicate-match-arm`，检测 `match` 语句中
  两条 arm 的模式完全相同（int / float / str / range / enum-variant / some /
  none / ok / err 递归对比）。后一条 arm 永远不可达，提示用户清理。新增 3 个
  nox_core 单元测试 + CLI 集成测试。
- CLI：`nox profile` / `nox coverage` 新增 `--ndjson` 选项输出每个函数一行
  JSON 事件，schema 分别为 `nox.profile.event.v1` 与 `nox.coverage.event.v1`，
  字段同 `--json` 聚合形式但去掉 `functions: [...]` 外层数组。便于工具用流式
  方式消费。`--json` 与 `--ndjson` 互斥；同时指定返回 exit code 2。
- LSP：`textDocument/publishDiagnostics` 现在同时透出 lint warning，使用
  `severity: 2`（Warning），与 typecheck/runtime error 的 `severity: 1` 区分；
  只在没有 error 时才查询 lint 以避免冗余。`lint.unused-function` 同时豁免
  `test_*` / `before_each` / `after_each` 函数（test runner discovery convention），
  避免在 nox test 项目中产生大量误报。
- LSP：`textDocument/codeLens` 给 `test_*` 命名的顶层函数返回 "Run \<name>"
  code lens（命令 `nox.runTest`，参数 `[uri, function_name]`）。
  `initialize` 响应新增 `codeLensProvider: {resolveProvider: false}` capability。
  让 VS Code / 其他 LSP 编辑器可以提供"运行测试"按钮，配合 `nox test --filter`
  逐个执行测试函数。
- 测试与维护：新增单元测试 `stdlib_index_documents_every_exported_helper`
  与 `english_stdlib_index_documents_every_exported_helper`，自动校验 21 个内置
  `std/*` 模块的所有 `export fn` 名称都在 `docs/zh_CN/runtime.md` /
  `docs/zh_CN/stdlib-index.md` 与 `docs/en/runtime.md` / `docs/en/stdlib-index.md`
  中各自被提及。新增 helper 必须同步进入两边文档；漂移会在 cargo test 阶段立即
  失败。同时补全 en stdlib-index 之前未跟进的 helper（read_binary / write_binary
  / canonicalize / run_with / select / is_tty_stderr / std/random / std/bytes 等）。
- CLI：`nox doc` 扩展覆盖顶层 `record`、`enum` 与 `type` alias 声明（之前仅
  `fn`），每个章节加 `Kind: ... Visibility: ...` 标签。`///` doc comment 关联
  规则不变。新增 CLI 集成测试 `doc_emits_markdown_for_records_enums_and_type_aliases`。
- LSP：`textDocument/hover` 在类型信息基础上追加紧邻顶层 `fn` / `record` /
  `enum` / `type` / `let` / `const` 声明上方的 `///` doc comment 文本，让
  编辑器把 doc comment 透给开发者。当前用 text-scan 实现（不引入 lexer / parser
  改动），多行注释按空格 trim 后拼接。新增 CLI 集成测试
  `lsp_hover_includes_doc_comment_for_top_level_function`。
- CLI：`nox lint --json` 输出 `summary` 新增 `capabilities` 字段，按 `import
  "std/X.nox"` 加上常见调用模式（`write_text(` / `write_binary(` / `sleep_ms(`
  / `process.run(` 等）推断脚本所需 runtime capability 集合。文本模式追加
  `capabilities: ...` 行。当前覆盖 `filesystem` / `filesystem_write` / `environment`
  / `timers` / `async_tasks` / `network` / `process_run`。`summary.file_count` /
  `summary.warning_count` 字段不变；旧消费者只需忽略新字段即可。
- 标准库：新增 `std/term.nox` 提供交互式 CLI helper：`is_tty_stdout()`、
  `color_enabled()`（`NO_COLOR` env 或非 TTY 关闭）、`style_color(value, color)` /
  `style_bold(value)`（支持 red/green/yellow/blue/magenta/cyan/bold；颜色未启用时
  透传原文）、`pad_column(value, width)`、`prompt(message)`、
  `confirm(message, default_yes)`。password input、select 菜单、TUI 框架显式不实现。
- 标准库：`std/process.nox` 新增 `run(program, args, stdin, timeout_ms) -> result[(int,
  str, str), str]`，返回 `(exit_code, stdout, stderr)`。新增 `process_run` capability
  （默认拒绝），manifest 中通过 `process_run` 字符串声明。可选 `process_run_allowlist`
  限制可执行程序名（空白表示不限制）；输出硬上限 4 MiB（超出 kill 子进程）；
  timeout_ms > 0 时到时间会 kill 进程。shell expansion / 管道 / PTY / 后台服务显式
  不支持。
- 标准库：`std/process.nox` 增加 `run_with(program, args, stdin, timeout_ms, cwd,
  env_pairs) -> result[(int, str, str), str]`。`cwd` 为空字符串时继承当前工作目录，
  非空作为 `Command::current_dir`；`env_pairs` 是 `[(str, str)]` 列表，在继承父
  进程环境基础上叠加 key/value（空列表 = 完全继承），value 为 `"<unset>"`
  时删除子进程环境中的该变量，空字符串继续表示设置为空值。共享 `run` 的
  capability、allowlist、输出上限、timeout 规则。Rust API：`Tuple` 新增公开
  `elements()` 与 `element_types()` 访问器，供宿主代码从 `Value::Tuple` 中取元素。
- 标准库：`std/process.run` / `std/process.run_with` 的 `result.err` 消息现在以
  稳定 code 前缀打头：`process_run.spawn-failed` / `process_run.timeout` /
  `process_run.allowlist-denied` / `process_run.output-cap-stdout` /
  `process_run.output-cap-stderr` / `process_run.stdin-write-failed` /
  `process_run.wait-failed`。冒号后是人类可读详细信息，消费者按 `: ` 拆分。
  这是 message 格式收紧，依赖旧消息字面值的代码需要更新（更新已落地的内部
  allowlist 单元测试）。
- Rust API：`RuntimePermissions::process_run_max_concurrent` 控制单个 Runtime
  内同时运行的 `std/process.nox` 子进程数量，默认 `Some(8)`。达到上限时
  `process.run` / `process.run_with` 返回 `process_run.concurrent-limit` 前缀的
  `result.err`，已结束或失败的子进程会释放计数槽。
- 标准库：`std/time.nox` 新增 UTC 日期算术：`add_days(unix_seconds, days)` /
  `add_months(unix_seconds, months)`（月末日 clamp 到目标月最长日，如 Jan 31 +
  1 month → Feb 28/29）、`year_of` / `month_of` / `day_of`（UTC 年/月/日，1-12 /
  1-31）、`weekday_of`（ISO weekday，0=Mon..6=Sun）。所有 helper 纯计算，复用
  既有 civil-day 算法。locale 格式化与非 UTC 时区仍留作后续 ADR。
- 标准库：扩展 `std/time.nox` 加 duration helper（`from_seconds`/`from_minutes`/
  `from_hours`/`to_seconds`/`to_minutes`/`to_hours`）、ISO-8601 编解码
  （`iso8601_format(unix_seconds) -> str`、`iso8601_parse(value) -> result[int, str]`，
  仅支持 UTC，`Z` 或 `+00:00` 后缀）、deadline helper（`deadline_ms`、
  `is_past_deadline_ms`）。所有 helper 都是纯计算，不需要 capability。非 UTC 时区
  parse 返回 `result.err`。
- 标准库：新增 `std/encoding.nox` 提供 `base64_encode`、`base64_decode`、`hex_encode`、
  `hex_decode`，纯计算；decode 失败返回 `result.err(message)`，非 UTF-8 字节也走 err
  路径。新增 `std/dotenv.nox` 提供 `parse(source) -> result[map[str, str], str]`，
  支持 `#` 注释、双引号、单引号、`export` 前缀。新增 `std/ini.nox`
  `parse(source) -> result[map[str, map[str, str]], str]`，支持简单 `[section]`
  分节、`=` / `:` key/value、`#` / `;` 注释，顶层 key 放在空字符串 section 下。
  新增 `std/toml.nox` `parse(source) -> result[json, str]` 最小 reader，支持 table、
  dotted key、字符串、bool、数字和数组，输出 JSON object；datetime / array-of-tables
  等完整 TOML 特性返回 `result.err`。按 ADR 0003 不引入第三方依赖。YAML 显式不实现，
  推荐使用 JSON / TOML / INI / dotenv。
- 测试框架：`nox test` 加 `--filter <substr>` 选项，按 test 函数名子串过滤。
- 测试框架：test 函数返回类型放宽：允许 `bool` 或 `null`（null 配合 assertion helper
  raise diagnostic 表示失败）；`test.signature` message 同步更新为 "must return bool
  or null"。
- 标准库：新增 `std/test.nox` 提供 `assert_eq<T: Equatable>`、`assert_ne<T: Equatable>`、
  `assert_true`、`assert_false`、`assert_contains`、`fail`。assertion 失败返回新稳定
  code `test.assertion-failed`。assert_eq/ne 复用 ADR 0020 结构化约束 `Equatable`。
- 标准库：新增 `std/task.nox`，包装现有 sleep-based 异步原语：`sleep_ms(ms) -> int`
  返回 task id，`is_ready` 非阻塞 poll，`cancel(id)` 取消，`wait(id) -> bool` 阻塞
  直到完成（unknown id 永远阻塞），`wait_or_timeout(id, timeout_ms) -> bool` 阻塞
  最多 timeout 然后自动 cancel 并返回 false，`pending_count() -> int` 公开当前
  pending 任务数。全部需要 `async task` capability。同时在 host 层新增 `task_join`
  与 `task_pending_count` 内建以支持这些 helper；LSP stub 注册同签名。
- Rust API：`RuntimePermissions::async_task_max_pending` 控制单个 Runtime 内 pending
  sleep task 数上限，默认 `Some(1024)`。达到上限时 `task_sleep_ms` 返回稳定
  diagnostic code `runtime.task-pending-cap`，且不创建 task id。
- 标准库：新增 `std/url.nox` 与 `std/http.nox`。`std/url.nox` 提供 `parse(url)`
  返回 `(scheme, host, port, path, query)` tuple、`build` 反向构造、`query_encode` /
  `query_decode`（百分号编码，`+` 在 decode 时映射为空格，invalid percent 返回
  `result.err`）。`std/http.nox` 提供 `get(url, timeout_ms)` 和 `post(url, body,
  timeout_ms)` 返回 `result[(status: int, body: str), str]`；当前实现仅支持 HTTP（明文），
  HTTPS / chunked transfer / keep-alive 不在范围内（HTTPS 需新 ADR 引入 TLS 依赖）。
  HTTP client 复用现有 `network` capability，未授权返回 `network capability is required`
  诊断；响应体硬上限 1 MiB；timeout_ms ≤ 0 时使用 30 秒默认超时；HTTP/1.1 请求强制
  `Connection: close`。
- CLI：新增 `nox watch [--interval-ms <ms>] (check|test|run) [args...]` 子命令，按
  ADR 0022 实现 stat-poll 前台 watch 模式：监视 manifest `source_dirs` / `test_dirs`
  下的 `.nox` 文件（没有 manifest 则用当前目录），默认 500ms 间隔，文件 mtime/size
  变化时通过 `current_exe()` 自调子进程重新执行被包装命令；CTRL-C 退出。监视路径
  不存在返回新稳定 code `watch.path-not-found`。daemon、增量 typecheck cache、hot
  reload 显式不实现。`scripts/release-gate.sh` 的 product-shape guardrail 已加入
  `nox watch` 防止 CLI 表面被静默删除。
- 语言能力：新增 `arr[i] = value` 和 `map[key] = value` 索引赋值语法。语法糖编译为
  新的 `IndexAssign` AST 节点和 bytecode 指令，运行时直接修改底层 `Value::Array` /
  `Value::Map` 存储，等价于 `array.set` / `map.set` 但带 alias 共享语义；越界写入返回
  稳定诊断 code `runtime.index-out-of-range`，对非数组/非 map LHS 报 `type.assign-target`。
  formatter、parser 越深处接受同样语法。
- 标准库：`std/array.nox` 新增 `set<T>(values: [T], index: int, value: T) -> result[null, str]`、
  `append<T>(values: [T], value: T) -> null`、`pop<T>(values: [T]) -> option[T]`；
  `std/map.nox` 新增 `set<T>(values: map[str, T], key: str, value: T) -> null` 与
  `delete<T>(values: map[str, T], key: str) -> bool`。所有 mutation 都通过显式 helper
  呈现，当前阶段不引入 `arr[i] = value` 或 `m[k] = value` 语法糖。
- 诊断体验：typecheck 在 `undefined variable`、`record '...' has no field '...'`、
  `enum '...' has no variant '...'` 和 `unknown type '...'` 报错时，会基于
  Levenshtein 距离在当前作用域、record schema、enum schema、record/enum/type alias
  名字集合中查找相近候选，并在原 message 末尾追加 `, did you mean 'X'?` 建议；
  没有近似候选时 message 保持兼容旧形态。
- 诊断体验：`Diagnostic.stack_frames` 新增 `kind` 字段，区分
  `script`（用户脚本函数）和 `host`（host callback / 注册的 host function）；
  CLI 文本输出新增 `[kind]` 标签（例如 `  at divide [script] (...)`），
  CLI 和 LSP JSON 的 `stack_frames` 元素新增 `"kind"` 字符串字段。
- 健壮性 corpus：`tests/malformed/` 新增 `stdlib-string-bad-call.nox`、
  `stdlib-array-bad-call.nox`、`stdlib-process-exit-bad-type.nox`、
  `stdlib-path-misspelled.nox`，覆盖阶段 16-19 stdlib 表面的类型错误、
  错误使用和拼写错误；`scripts/robustness-smoke.sh` 同步把它们纳入
  check/fmt 退出码矩阵。
- 语言能力：默认 `nox` 运行时新增 `print(value: str) -> null` 与
  `to_str_int` / `to_str_float` / `to_str_bool` / `to_str_null` / `to_str_str`
  输出辅助；`nox run` 对最终 `null` 不再额外打印 `null`，脚本可以直接用
  `print(to_str_int(42))` 输出一行文本。
- 标准库：新增 `std/string.nox` 纯计算模块，提供 `split`、`substring`、`trim`、
  `replace`、`starts_with`、`ends_with`、`index_of`、`to_upper` 和 `to_lower`；
  新函数不需要 runtime capability，并已纳入 stdlib surface guardrail。
- 标准库：扩展 `std/string.nox`，新增 `join`、`contains`、`last_index_of`、
  `repeat`、`pad_left`、`pad_right`、`parse_int`、`parse_float` 和 `lines`；
  文本解析失败返回 `result.err(message)`，不需要 runtime capability。
- 语言能力：字符串字面量支持 `${expr}` 插值，占位表达式可自动 stringify
  `null`、`bool`、`int`、`float` 和 `str`；`\\$` 可用于输出字面量 `$`。
- 语言能力：新增 `?` 后缀错误传播运算符，支持在函数内从 `result[T, E]`
  和 `option[T]` 解包成功值，并在 `err` / `none` 时按当前函数返回类型提前返回；
  不兼容上下文使用稳定诊断 code `result.question-mark.mismatch`。
- 语言能力：新增 record method 调用糖，`record_value.method(args)` 会按
  `method(record_value, args)` 类型检查和执行；找不到匹配函数或 receiver 类型不匹配时
  使用稳定诊断 code `record.method-not-found`。
- 语言能力：扩展 `match` pattern，支持 `float` 字面量、`int` 半开范围
  `start..end`，以及 `some(some(value))`、`some(none)`、`ok(some(value))`
  等嵌套 option/result destructuring；非穷尽 match 使用稳定诊断 code
  `match.non-exhaustive`。
- 标准库：扩展默认运行时数学内建，新增 `abs`、`min`、`max`、`pow`、
  `floor`、`ceil`、`round`、`log`、`log2`、`sin`、`cos`、`tan`、`pi`
  和 `e`，并将完整 math surface 纳入 stdlib surface guardrail；`sqrt` 负数和
  `log` / `log2` 非正数会返回 runtime diagnostic。
- 标准库：扩展 `std/time.nox`，新增 `now_unix`、`now_unix_ms`、`duration_ms`、
  `format_unix` 和 `parse_unix`；Unix 时间格式化/解析使用 UTC，支持 `%Y/%m/%d/%H/%M/%S`
  和 `%%`，解析错误通过 `result.err(message)` 返回。
- 语言能力：新增 map ergonomics 内建 `map_keys`、`map_values`、`map_has`
  和 `map_size`；当前 map key 固定为 `str`，`map_keys` / `map_values` 按 key 字典序返回。
- 标准库：新增不透明 `json` 值类型与 `std/json.nox` 纯计算模块，提供
  `parse(value: str) -> result[json, str]` 和 `stringify(value: json) -> str`，覆盖
  number/string/bool/null/array/object 基础 JSON 形态；malformed JSON 返回 `err(message)`。
- 标准库：扩展 `std/json.nox`，新增 `kind`、`array_len`、`array_get`、
  `object_has` 和 `object_get` helper；错误 kind、越界 index 和缺失 key 都返回
  `result.err(message)`。
- 标准库：新增 `std/csv.nox` 与 `std/tsv.nox` 单行文本数据 helper，提供
  `parse_line(value: str) -> result[[str], str]` 和格式化行输出；CSV 支持双引号字段和
  `""` 转义，TSV 格式化会拒绝包含 tab 的字段。
- 语言能力：容器类型标注允许 tuple、map、option 和 result 作为 array/map 元素类型，例如
  `[(str, int)]` 和 `map[str, option[int]]`，以支撑数据结构 helper 的公开签名。
- 标准库：新增 `std/array.nox`、`std/map.nox`、`std/option.nox` 和 `std/result.nox`
  纯计算 helper；覆盖 array copy/sort/slice、map entries/merge/remove/get_or，以及
  option/result 状态判断和 fallback，不引入原地 mutation、闭包或高阶函数。
- 标准库：新增 `std/process.nox`，提供 `argv() -> [str]`、`read_stdin() -> str`、
  `print_err(value: str) -> null` 和 `exit(code: int) -> null`；`argv()` 与既有 `args()` 一致，
  不包含脚本路径，`exit` 接受 0-255 并作为 `nox run` 成功执行后的进程退出码。
- 标准库：新增 `std/path.nox` 纯计算路径 helper（`join`、`basename`、`dirname`、
  `extension`、`normalize`），并扩展 `std/fs.nox` 的 `is_file`、`is_dir` 和
  `list_dir(path) -> result[[str], str]`；新增 fs helper 沿用 filesystem capability 与 read
  allowlist，`list_dir` 按名称排序返回目录项。
- 语言能力：整数字面量新增 `0xff` 十六进制、`0b1010` 二进制、`0o17` 八进制和
  `_` 分隔符；malformed 整数字面量使用稳定诊断 code `lex.invalid-integer`。
- 语言能力：字符串字面量新增 `"""..."""` 多行字符串和 `r"..."` raw 字符串；
  多行字符串保留内部换行，raw 字符串不解析 escape 或 `${...}` 插值标记。
- 语言能力：新增单引号字符字面量，如 `'A'`、`'界'` 和 `'\n'`；该语法降为单字符
  `str` 值，暂不引入独立 `char` 或 `bytes` 类型。
- 语言能力：新增 tuple 类型和值，如 `(int, str)` 与 `(42, "nox")`，并支持
  `let (a, b) = pair;` tuple 解构和 `let { x, y } = point;` record 解构；tuple
  数量和元素类型错误分别使用稳定诊断 code `tuple.arity-mismatch` 与
  `tuple.element-type-mismatch`。
- 语言能力：新增顶层类型别名，如 `type UserId = int;` 和
  `type Pair = (UserId, str);`；alias 在类型检查时透明展开，循环别名使用稳定诊断
  code `type-alias.cyclic`。
- 语言能力：新增用户自定义 `enum` / sum type，支持 `EnumName.Variant` 与
  `EnumName.Variant(value)` 构造，并通过 `match` 解包；用户 enum match 必须覆盖所有
  variant，缺失分支使用稳定诊断 code `match.non-exhaustive`，未知 variant 使用
  `enum.variant-not-found`。
- 语言能力：新增函数级泛型参数，如 `fn id<T>(value: T) -> T`；调用点从实参和 expected
  return type 推导类型参数，推导冲突或缺少上下文时使用稳定诊断 code
  `generic.infer-failed`。泛型 record、泛型 trait、显式 type argument、一等函数值和高阶函数仍暂缓。
- 诊断：运行时错误现在携带脚本函数调用栈；人类 CLI 输出会打印 `at <function>` 栈帧，
  `nox.test.v1` diagnostic 和 LSP diagnostic data 会在有栈时包含兼容新增字段
  `stack_frames`，顺序为最近调用帧在前。
- 语言能力：新增 `int` 位运算符 `&`、`|`、`^`、`<<`、`>>` 和一元 `~`；非 `int`
  操作数使用稳定诊断 code `type.bitwise-non-int`。`>>` 是算术右移，移位计数必须在
  `0..64` 内。
- 语言能力：新增 `if let`、`let ... else` 和 `while let` 控制流语法，复用 `match`
  pattern 解包 `option`、`result` 与用户 `enum`；`let ... else` 的 `else` 分支必须提前
  `return`，否则使用稳定诊断 code `control-flow.let-else-fallthrough`。
- 语言能力：新增 array / map literal spread，支持 `[...arr, value]` 与
  `{...defaults, "k": value}` 创建新容器；map 合并按书写顺序让后面的 key 覆盖前面的 key，
  spread 源类型不匹配使用稳定诊断 code `type.spread-mismatch`。
- 文档：README 与 README_zh_CN 新增 `cargo install` 安装路径，覆盖 `cargo install --git https://github.com/liguangsheng/nox --tag vX.Y.Z --locked nox` 与本地 `cargo install --path crates/nox --locked` 两种用法，并说明 `cargo install` 不产出 `nox_core` 的 C ABI 动态库——嵌入式宿主需要 `nox-embed` release 包或 `cargo build --release -p nox_core`。
- 文档：README 与 README_zh_CN 中的 release 下载示例从 `v0.0.2` 资产名同步到 `v0.0.3`。

### 工具和验证

- 工具链：新增阶段 14 工具面，包含 `nox repl`、TextMate grammar、VS Code `.vsix`
  扩展打包、LSP signature help / code action、`nox dap` Debug Adapter Protocol 最小子集、
  以及 `nox profile` / `nox coverage`。profile 通过 VM 调用路径记录脚本函数调用次数和累计时间；
  VS Code 扩展同时贡献高亮、LSP 与 Nox debug 配置。
- 工具：新增 `fuzz/` cargo-fuzz workspace，包含 `parser`、`typecheck` 和 `verifier` 三个
  fuzz target 及 seed corpus；`nox_core` 仅在 `--cfg fuzzing` 下暴露 fuzz harness，正常 Rust API
  和 C ABI 不变。`scripts/release-gate.sh` 新增 opt-in fuzz 段：设置
  `NOX_RELEASE_GATE_FUZZ=1` 后按 `NOX_FUZZ_TIME` 对三项目标运行 `cargo +nightly fuzz run`。
- 工具：新增 `scripts/sanitizer-smoke.sh`，使用 nightly ASan 运行 heap/C ABI ownership 回归、
  TSan 运行 host callback 回归，并用 Valgrind leak check 跑 C embedding smoke；
  `scripts/release-gate.sh` 新增 opt-in sanitizer 段，设置 `NOX_RELEASE_GATE_SANITIZER=1` 时接入同一门禁。
- 工具：`nox_core` 新增 Criterion benchmark `core_paths`，覆盖递归 check/compile、loop eval 和
  container eval 三条关键路径；`scripts/bench-smoke.sh` 保留原有快速 smoke，同时支持
  `NOX_BENCH_CRITERION=1` 运行 `cargo bench -p nox_core --bench core_paths` 输出统计置信区间。
- CLI：`nox project check --json` 的 `nox.project-check.v1` 兼容新增 `entrypoints`、
  `capabilities.declared` 和 `module_graph` 顶层字段，用于报告 manifest 入口、声明的 runtime
  capability 以及 `modules.source_dirs` 下发现的 `.nox` 文件。
- LSP：`initialize` 现在声明 `documentSymbolProvider` 和 `definitionProvider`；新增
  `textDocument/documentSymbol` 当前文档顶层声明列表，以及 `textDocument/definition` 当前文档顶层
  声明跳转的保守子集。跨文件定义、workspace symbol 和 rename 仍未声明为能力。
- 工具：v0.0.3 release 后收紧 release-gate 阈值（PLAN 完成定义第 10-12 项的护栏数值），让"小型"和"快速"门槛对回归更敏感：
  - `NOX_SIZE_CAP_CLI` 由 4 MiB 收紧到 2.5 MiB（v0.0.3 实测 1.67 MiB，留 ~1.5x 缓冲）。
  - `NOX_SIZE_CAP_CORE` 由 2.5 MiB 收紧到 1.5 MiB（v0.0.3 实测 1.03 MiB，留 ~1.45x 缓冲）。
  - `NOX_BENCH_BUDGET_FIB` 2.0s → 1.0s、`LOOP` 3.0s → 1.5s、`CONTAINERS` 1.0s → 0.3s、`MODULES` 1.0s → 0.3s、`NOX_TEST` 2.0s → 1.0s（每项相对实测最大值留 7-15x 缓冲）。
  - `NOX_EMBEDDING_TIME_BUDGET` 180s → 60s（warm cache 实测 ~1s，留 60x 缓冲覆盖 CI cold-rebuild 场景）。
  阈值变更通过环境变量仍可在调试时临时上调；后续 release-prep 阶段不允许临时上调来掩盖回归。
- 工具：ADR 0025 重新校准 release-gate CLI 二进制大小上限：`NOX_SIZE_CAP_CLI`
  从 2.5 MiB 调整为 2.75 MiB。当前 release 构建实测约 2.55 MiB；增长来自
  profile/trace/coverage、DAP/LSP 和 JSON/schema 标准库等生产可观测性与工具能力。
  `NOX_SIZE_CAP_CORE` 仍保持 1.5 MiB，零第三方运行时依赖约束不变。

## [0.0.3] — 2026-05-22

本版本紧接 `v0.0.2` 基线，把 v0.0.2 之后累积的文档基础设施改动（中英文档拆分、英文 README、test
fixture 与示例分离）收口，并在 release-gate / release-audit 中接入 PLAN 完成定义第 9-13 项的
release-time 护栏与 GOAL 实现判定，让 `scripts/release-audit.sh` 能在无人推理的情况下输出
"GOAL implementation: ACHIEVED" / "NOT ACHIEVED"。

### 文档基础设施

- 文档：中英文档分别迁入 `docs/zh_CN/` 与 `docs/en/` 目录；新增完整英文文档树（README、language-v0、cli、runtime、embedding、diagnostics、benchmarks、release-checklist、architecture、development、directory-structure）和英文 README。
- 测试：将示例 `.nox` 脚本目录中的回归 fixture 与 benchmark 文件迁出到 `tests/fixtures/`、`tests/malformed/`、`tests/benchmarks/`；`examples/` 只保留对外展示的示例脚本与示例项目。release-gate、release-checklist 和相关脚本同步更新路径引用。

### 工具和验证

- 工具：release-gate 新增 product-shape guardrail（PLAN 完成定义第 9 项）：对 `nox --help` 输出 grep 七个稳定 CLI 子命令（`run`、`check`、`test`、`fmt`、`project check`、`lsp`、`inspect-bytecode`），任一缺失立即失败。
- 工具：release-gate 新增 small-footprint guardrail（PLAN 完成定义第 10 项）：release CLI 二进制大小 ≤ 4 MiB、`libnox_core` 动态库 ≤ 2.5 MiB、第三方运行时依赖数 = 0、LOC 趋势记录。当前基线分别为 1,673,912 / 1,030,240 bytes、0 第三方依赖、19,489 行 Rust 源码。阈值上调需独立 commit + CHANGELOG + ADR，不允许在 release-prep 阶段临时上调。
- 工具：`scripts/bench-smoke.sh` 新增 per-case e2e budget（PLAN 完成定义第 11 项）：bench-fib ≤ 2.0s、bench-loop ≤ 3.0s、bench-containers ≤ 1.0s、bench-modules ≤ 1.0s、nox-test ≤ 2.0s。budget 在 release-gate `benchmark smoke` 段强制执行，超 budget 立即 fail。当前 release 实测分别为 ~0.04 / 0.05 / 0.016 / 0.008 / 0.002 秒，budget 留 6-1000x 缓冲应对 CI 共享核与机器负载波动。budget 上调需独立 commit + CHANGELOG + ADR。
- 工具：release-gate 在 `embedding regression` 段加 wall-time budget（PLAN 完成定义第 12 项前半）：默认 `NOX_EMBEDDING_TIME_BUDGET=180` 秒，覆盖 Rust API/runtime test、Rust embedding 示例、`nox_core` 动态库 build、C ABI header↔library symbol parity 与 C 嵌入 smoke 编译/链接/运行；当前 warm cache 下 ~1 秒。
- 工具：release-gate 新增 stdlib surface guardrail（PLAN 完成定义第 12 项后半）：`tests/fixtures/stdlib-surface.nox` 集中类型检查 fs/env/time/net/async/math/string/json/csv/tsv/array/map/option/result 公开入口，任一签名缺失或被删除立即失败。

### 示例项目

- 新增 `examples/projects/health-check` 示例项目（PLAN 完成定义第 12 项后半"真实生产场景"）：用 `std/fs.nox`、`std/env.nox` 与 `option[str]` 检查文件与环境变量是否齐全，把 capability-bound 调用与纯决策函数分层以便不授予 capability 即可跑单元测试；release-gate 通过 `project check`（含 check/test/fmt --check）与 `project check --json` 双路 smoke。

### 持续维护门槛

- 工具：`scripts/release-audit.sh` 新增 PLAN 完成定义第 9-13 项的综合断言（PLAN 完成定义"持续维护门槛"），检查 product-shape / small-footprint / bench budget / embedding budget / stdlib surface guardrail / 非 scoreboard 示例项目 / 暂缓项关键词共 7 项是否在 release-gate 与仓库中仍然在位；任一缺失立即作为 blocker 计入。脚本同时输出 `GOAL implementation: ACHIEVED` / `NOT ACHIEVED` 判定，让 release operator 不需要再做二次推理。

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
