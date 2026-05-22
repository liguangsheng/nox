# Nox 下一阶段长期计划

更新时间：2026-05-22。

本计划记录从本地 `v0.0.6` tag 推进到 `v0.0.7` 的下一轮长期路线。当前基线已经具备静态类型语言核心、
数组、`map[str, T]`、命名 `record`、模块 import/export、namespace import、
manifest 项目边界、session/module graph、flat bytecode VM、verifier、运行时能力控制、
Rust API、C ABI、`option[T]` / `result[T, E]`、`std/*` 静态模块可恢复错误试点、
`map_get(map, key) -> option[T]`、`nox run/check/test/fmt/lsp/inspect-bytecode`、Rust/C embedding smoke、ADR、
性能/robustness smoke、release gate、本地分发 smoke 和基础文档。

`v0.0.6` 已在本地完成 release-prep commit 和 git tag；当前没有配置远端，因此不把发布到
GitHub Release 作为阻塞项。`v0.0.7` 的目标不是启动 package manager、watch daemon 或大语言
扩张，而是继续把已经可用的诊断契约、manifest 项目边界、项目级 JSON、LSP 复用和本地分发证据
打磨到更适合日常开发和宿主集成的状态。

## 路线图总览

下一阶段按“先扩第二批稳定诊断、再补项目检查细节、再复审宿主边界、最后做 release 收口”的顺序推进。
每个里程碑都必须有可审查产物，不能只停留在口头设计。

- v0.0.7-dev 诊断契约第二批：优先处理 manifest schema / project discovery、permission denied
  或 bytecode verifier 中能被 CLI JSON、LSP 或 release gate 稳定覆盖的错误族。
- v0.0.7-dev 项目检查 JSON 第二轮：在 `nox.project-check.v1` 不破坏兼容的前提下，补充失败摘要、
  manifest 路径、子步骤状态和 CI 友好的断言；不把它升级成包管理器。
- v0.0.7-dev LSP / CLI 项目一致性：继续复用 manifest/session/module graph，优先消除 CLI 和 LSP
  对项目 root、source_dirs、test_dirs 或 manifest 错误的分歧。
- v0.0.7-dev embedding 稳定性：只在真实宿主用例需要时扩 Rust API / C ABI；默认先补文档、
  regression 和兼容矩阵，不引入半稳定 task status ABI。
- v0.0.7-dev release 自动化复审：保持无 remote 的本地 release 模型，补足 tag 后 smoke、
  changelog 流程和本地 dist 可复制证据。
- v0.0.7 继续暂缓：package registry、dependency resolver、watch daemon、可变数组、slice、
  源码级函数类型、高阶函数、动态 std object 继续不作为硬前置。

## 里程碑边界

### v0.0.7-alpha 必须完成

- 当前 `v0.0.6` release gate、local dist smoke、embedding regression、benchmark smoke 和
  robustness smoke 继续通过。
- 第二批至少一个诊断族从通用 `error` 收敛为稳定 code，并有可复制 CLI JSON、LSP 或 release
  gate 证据；优先选择 manifest / permission / verifier 中能形成稳定契约的路径。
- `nox project check --json` 的失败路径或项目边界输出有一个可测试增强，并保持
  `nox.project-check.v1` 兼容。
- `PLAN.md`、`docs/diagnostics.md`、`docs/cli.md`、`docs/runtime.md` 和 `docs/embedding.md` 对
  v0.0.7 要做与继续暂缓的能力有一致说明。
- 不把系统能力放进 `nox_core`，不让 manifest 声明自动变成 runtime 授权。

### v0.0.7-beta 候选

- 诊断 code 表覆盖 manifest、permission 或 bytecode verifier 中至少一个新的高价值错误族；如果
  同批只完成设计，必须说明为何暂不稳定。
- 项目体验至少完成一个高价值收紧项：manifest diagnostic 更具体、project check JSON failure
  detail 更适合 CI，或 LSP project boundary 更稳定。
- async task 状态 API 继续默认暂缓；除非状态 record、tombstone 生命周期和权限边界同时稳定，
  否则不得进入脚本 API 或 C ABI。
- release gate 覆盖新增路径，且 `scripts/local-dist-smoke.sh` 继续验证 tag 后本地可复制产物。
- 旧 guard + diagnostic 表面仍有兼容窗口；新增推荐 API 不破坏 v0.0.6 脚本。

### 仍可暂缓

- package registry、dependency resolver、lockfile。
- watch mode、后台 daemon、增量 typecheck。
- tracing GC、cycle collector、arena handle。
- 可变数组、完整函数类型、闭包和高阶函数。

这些项目只能在设计闸门通过后进入实现，不作为 v0.0.7 发布硬前置。

## 历史里程碑边界

以下边界是阶段 15-21 的历史约束，保留用于审计已经完成的 v0.0.2/v0.0.3 路线。

### v0.0.2 已完成

- 当前已实现能力不再扩面，只修正文档漂移、版本号、发布命令和 release checklist。
- C ABI 只允许兼容性修正；新增函数也要进入 changelog 和 header 检查。
- `cargo fmt --all --check`、`cargo test --all`、`cargo clippy --all-targets -- -D warnings`、
  CLI smoke、C embedding smoke 和 CI 必须通过。

### v0.0.3-alpha 已完成

- `nox.toml` 从“模块搜索辅助”升级为项目入口和工具边界的来源。
- `nox check` / `nox test` 可以在无显式 path 时按 manifest 工作。
- Rust 高级嵌入 API 有明确 session/module graph 生命周期，但 `Engine::eval/check` 保持可用。
- LSP 使用同一套项目/模块解析语义，不再和 CLI 行为分裂。

### v0.0.3-beta 已完成

- 命名空间 import 或其替代方案完成，并有迁移策略。
- runtime 权限、async task 和 stdlib 命名策略进入可长期维护状态。
- 诊断 code、formatter golden fixture 和项目级 JSON 输出稳定。

### v0.0.3 已暂缓

- `option[T]` / `result[T, E]`。
- 可变数组、`push`、元素赋值、切片。
- 完整函数类型、闭包和高阶函数。
- watch mode、后台 daemon、package registry。

## 总原则

- 静态类型优先。新增语言能力必须先写清类型规则、运行时表示、错误边界和 API
  影响，再进入 parser/type checker/bytecode/VM。
- `nox_core` 保持小核心：语言前端、静态检查、字节码、VM、值、诊断、Rust/C
  嵌入边界属于核心；文件、网络、环境、计时器、LSP、项目发现属于 `nox`。
- 嵌入优先。宿主必须能控制权限、内存、取消执行、host function、错误传播和
  值生命周期。
- 不引入动态 object/prototype，不为了兼容历史语法保留包袱。
- 每个阶段都要能独立验证、独立提交。语言设计、运行时能力、工具链和文档不要
  混成不可审查的大改。
- 对外可见行为必须同步更新文档、示例、负向 fixture 或集成测试。
- C ABI 和 Rust API 一旦暴露给宿主，就按兼容契约维护；内部 AST/bytecode 默认
  不稳定。
- `PLAN.md` 是长期路线和阶段状态，不记录碎片化日 TODO；完成阶段性批次后更新状态、
  验证记录和下一批推荐顺序。

## 执行节奏

推荐每批次控制在一个可独立 review 的主题内：设计文档、语言/运行时实现、工具链行为、
验证补齐不要混成一个提交。默认节奏如下：

1. 先写或更新 ADR / design doc，明确语义和拒绝方案。
2. 再补 parser/type checker/runtime/API 的最小实现。
3. 同批补正向测试、负向 fixture、CLI/LSP/JSON 行为测试。
4. 最后更新 README、docs、examples、CHANGELOG 和本计划状态。
5. 每个阶段完成后跑固定验证；发布相关阶段额外跑 release checklist dry run。

阶段状态标记使用：`待启动`、`设计中`、`实现中`、`验证中`、`已完成`、`暂缓`。

## 当前基线

已完成并已提交：

- Release：本地已有 `v0.0.2`、`v0.0.3`、`v0.0.4`、`v0.0.5` 和 `v0.0.6` tag；这些 tag
  是本地开发 checkpoint，当前未配置 git remote，暂不 push、不创建 GitHub Release。
- 语言：变量、`const`、函数、块、`if`/`else if`、`while`、半开 `for` range、
  `break`、`continue`、`return`、短路逻辑、受限语句式 `match`、字符串 escape、
  显式数值转换。
- 数据：`[T]`、`map[str, T]`、命名 `record`、字段访问、静态字段检查、
  `len(array)`、`len(str)`、`contains(map, key)`。
- 模块：相对 import、`nox.toml`、`modules.source_dirs`、重复导入缓存、
  循环导入诊断、`export` 可见性、平铺导入冲突诊断。
- VM：flat instruction stream、控制流 verifier、instruction budget 取消、
  递归/循环/容器压力测试。
- Runtime：默认 stdlib、文件系统/网络/环境/定时器/异步任务能力控制、脚本参数。
- Tooling：`nox run`、多文件 `nox check`、`check --json`、`nox test`、
  `fmt --check/--write`、LSP diagnostics/formatting/completion/hover、
  `inspect-bytecode --compact`。
- Embedding：Rust host function API、C host callback、版本查询、字符串返回、
  array/map/record 只读 owning handle、engine-owned error string、C smoke。
- Docs：语言、CLI、runtime、embedding、目录结构、开发验证、release checklist、
  ADR 和 changelog。

## 阶段 28：v0.0.4 本地开发启动

目标：在不依赖远端发布的前提下，启动 v0.0.4 本地开发，把已经通过设计闸门的错误处理
模型推进到最小实现，并为本地分发和宿主集成留下可复制证据。

### 28.1 `option[T]` / `result[T, E]` 最小语义落地计划

状态：已完成。

设计方向：

- 以 `docs/decisions/0014-restart-option-result-design.md` 为边界，不引入隐式 nullable、
  异常或 try/catch。
- 先实现最小类型语法、构造、判别和解包，不急于语法糖。
- 类型收窄、formatter golden fixture、负向测试和 LSP hover/completion 必须同批进入。
- Rust `Value` 和 C ABI 表示先写兼容计划，再决定是否在同批暴露。

验收标准：

- ADR 0014 被拆成 parser/type checker/VM/API/formatter/LSP/test 的执行清单。
- 至少有一个正向 fixture 和三个负向 fixture 计划覆盖 option/result 类型错误。
- 在实现前不得把 `docs/language-v0.md` 或 `docs/runtime.md` 改写成 option/result 已可用；
  `CHANGELOG.md` 只记录本实施计划文档。

建议首批任务：

- 已新增 `docs/option-result-implementation-plan.md`，明确 `option[T]`、`result[T, E]`、
  `some(value)`、`none`、`ok(value)`、`err(value)` 和受限 `match` payload binding 的
  首批语义。
- 已决定首批实现使用 `Type::Option/Result` 和 `Value::Option/Result`，并要求同批追加
  C ABI 只读 owning handle；如果 C ABI 来不及同批完成，则不能把 option/result 放入
  可 eval 的稳定表面。
- 已列出 parser、type checker、VM、Rust/C API、formatter、LSP、CLI JSON 和 fixture
  清单；正向 fixture 与四个负向 fixture 以代码片段写入实施计划。
- 已把 28.2 的 stdlib 试点顺序定为 `std/env.nox` `try_get` 优先，其次是 `std/fs.nox`
  `try_read_text`；旧 API 至少保留一个 minor 阶段。
- 已更新 ADR 0014、docs 索引和 CHANGELOG；未更新 `docs/language-v0.md` / `docs/runtime.md`
  为已实现状态，避免文档先于代码。
- 验证：`cargo fmt --all --check`、`cargo test --all`、
  `cargo clippy --all-targets -- -D warnings`、Markdown 链接检查、`git diff --check HEAD`。

### 28.2 stdlib 错误模型迁移试点

状态：已完成。

设计方向：

- 只迁移最能证明价值的函数：`std/fs.nox` 的读取、`std/env.nox` 的查询、map lookup 或
  async task poll。
- 旧全局函数和 guard + diagnostic 模式保留，避免 v0.0.4-dev 破坏 v0.0.3 脚本。
- sample project 必须用迁移后的推荐表面跑通，不能只改库函数不改真实用例。

验收标准：

- 至少一个 `std/*` 模块用 option/result 表达可恢复失败。
- sample project 或 examples 有真实调用路径。
- CLI JSON/LSP diagnostic 对不可恢复错误仍保持稳定。

建议首批任务：

- 已完成前置类型语法地基：`Type::Option`、`Type::Result`、`option[T]` /
  `result[T, E]` parser、递归类型验证、类型显示和负向 arity/type mismatch 测试。
- 已完成首批构造和值表示：`some(value)`、`none`、`ok(value)`、`err(value)` 进入
  type checker、bytecode、VM、heap tracking、Rust `Value` helper 和 C ABI 只读
  owning handle。
- 已完成受限 `match` payload 解包：`some(name)` / `none` 和 `ok(name)` / `err(name)`
  case、穷尽性检查、payload 分支局部绑定、formatter golden fixture 和文档同步。
- 已完成 `std/env.nox` `try_get(name: str) -> option[str]` 试点：缺失环境变量返回 `none`，
  存在时返回 `some(value)`，权限不足和非 UTF-8 值仍保留 diagnostic；sample project
  已加入真实静态调用路径，CLI JSON 和 LSP diagnostic 回归覆盖 `option[str]` 类型表面。
- 已完成 `std/fs.nox` `try_read_text(path: str) -> result[str, str]` 试点：成功读取返回
  `ok(contents)`，普通读取失败返回 `err(message)`；权限不足、allowlist 越界和无效路径仍保留
  diagnostic；旧 `read_text(path: str) -> str` diagnostic 行为不变。sample project 已加入
  真实静态调用路径，CLI JSON 和 LSP diagnostic 回归覆盖 `result[str, str]` 类型表面。
- 验证：`cargo test -p nox_core match`、`cargo test -p nox_core option`、
  `cargo test -p nox_core result`、`cargo test -p nox std_env_try_get`、
  `cargo test -p nox --test cli check_json_reports_std_env_try_get_option_type`、
  `cargo test -p nox --test cli lsp_reports_std_env_try_get_option_type`、
  `cargo test -p nox try_read_text`、
  `cargo test -p nox --test cli try_read_text`、
  `cargo fmt --all --check`、`cargo test --all`、
  `cargo clippy --all-targets -- -D warnings`、Markdown 链接检查、
  `git diff --check HEAD`。

### 28.3 本地分发和安装 smoke

状态：已完成。

设计方向：

- 当前没有 remote；本阶段只做本地可复制产物，不 push、不发布 GitHub Release。
- 在 `scripts/release-gate.sh` 之外增加本地 package/install smoke，验证 tag 后产物能在
  临时目录运行。
- 不引入包管理器、registry、lockfile 或 daemon。

验收标准：

- 有一个脚本或文档化命令可以从当前 checkout 构建本地 release 产物。
- 临时目录中能运行 `nox --version` 或等价版本 smoke、`examples/hello.nox`、C header
  编译 smoke。
- 文档明确该流程只是本地分发，不等同远端 release。

建议首批任务：

- 已新增 `nox --version`，输出 `nox X.Y.Z`，用于安装目录版本自检。
- 已新增 `scripts/local-dist-smoke.sh`：构建 release CLI 和 `nox_core` 动态库，复制
  `nox`、`nox_core.h`、`libnox_core`、`examples/hello.nox` 和 `examples/math.nox` 到临时
  安装目录，再从该目录运行版本、脚本和 C header smoke。
- 已更新 `docs/cli.md`、`docs/development.md`、`docs/release-checklist.md`、CHANGELOG 和
  release gate；该流程只写临时目录或 `NOX_LOCAL_DIST_DIR`，不 push、不打 tag、不创建
  GitHub Release，也不提交构建产物。
- 验证：`scripts/local-dist-smoke.sh`、`cargo test -p nox version_prints_package_version`、
  `cargo fmt --all --check`、`cargo test --all`、
  `cargo clippy --all-targets -- -D warnings`、Markdown 链接检查、`git diff --check HEAD`。

### 28.4 Embedding 文档和示例二次打磨

状态：已完成。

设计方向：

- 基于现有 C/Rust embedding regression，把长期宿主关注点写成更直接的 cookbook。
- 聚焦 value ownership、callback error、userdata、runtime permissions、取消和 async task。
- 不扩 C ABI 表面，除非 28.1/28.2 证明 option/result 需要新的跨 ABI 表达。

验收标准：

- `docs/embedding.md` 有一节可直接照抄的长期宿主初始化和清理流程。
- C 示例和 Rust 示例覆盖错误传播和权限边界。
- `scripts/embedding-regression.sh` 继续通过。

建议首批任务：

- 已审计 `docs/embedding.md` 与 `examples/embed/c_embedding.c` 的 embedding API 覆盖。
- 已新增长期宿主 cookbook，覆盖 `Engine` host callback 错误、`RuntimePermissions`
  文件 allowlist、`std/fs.nox try_read_text` 可恢复读取、权限 diagnostic 和 async task
  失败清理。
- 已新增 `crates/nox/examples/rust_embedding.rs`，并把它接入
  `scripts/embedding-regression.sh`；C smoke 继续覆盖 version、userdata fallback、
  callback error、last_error/clear_error、string free 和 array/map/record/option/result
  handle free。
- 验证：`cargo fmt --all --check`、`cargo run -p nox --example rust_embedding`、
  `scripts/embedding-regression.sh`。

## 阶段 29：v0.0.5 规划和开发启动

目标：在 `v0.0.4` 已可本地发布的基础上，推进下一轮真实项目体验和可恢复错误模型。
v0.0.5 不追求大而全的语言扩张，而是把 v0.0.4 已落地的 `option` / `result`、`std/*`、项目
工具链和 embedding 证据串成更接近日常开发的闭环。

### 29.1 v0.0.5 真实项目压力设计

状态：已完成。

设计方向：

- 从现有 `examples/projects/scoreboard` 出发，决定是扩展该 fixture，还是新增一个更偏
  runtime/std library 的小型项目。
- 真实项目必须同时覆盖 namespace import、manifest project check/test/fmt、`std/fs.nox`、
  `std/env.nox`、`option` / `result` match 和 LSP diagnostics/completion。
- 不为了展示能力引入动态 object、package registry 或数组 mutation。

验收标准：

- 有一个项目 fixture 能在 `nox project check`、`nox test`、`nox fmt --check` 和 LSP smoke
  中复用。
- fixture 中至少一条路径使用 `result` 处理文件或解析类失败，至少一条路径使用 `option`
  处理缺失值。
- README 或 docs 说明该 fixture 是 v0.0.5 真实用例证据，不是语言功能展示清单。

完成记录：

- 选择继续扩展 `examples/projects/scoreboard`，避免新增一个只有语言展示价值的小项目。
- 已新增 `tests/runtime_info_test.nox`，通过 `std/fs.nox` 读取 manifest 的 `result[str, str]`
  路径和 `option[str]` match 覆盖真实项目中的可恢复返回值。
- CLI 集成测试已覆盖 scoreboard 的 6 个 project check 文件、5 个项目测试和 runtime_info
  测试名；LSP smoke 已覆盖 `tests/runtime_info_test.nox` 的 diagnostics 与
  `runtime_info.` namespace completion。
- `examples/README.md` 和 CHANGELOG 已说明 scoreboard 是 v0.0.5 真实项目压力证据。

### 29.2 可恢复 API 第二批

状态：已完成。

设计方向：

- 第一候选是 map lookup：当前 `contains(map, key)` + `map[key]` 可用但重复，目标是提供
  `option` 形态的可恢复读取。
- 第二候选是 async task 状态：当前 unknown id 是 diagnostic；如果要返回 `result` 或 record，
  必须先稳定 task 状态形状和 id 生命周期。
- 如果当前类型系统无法表达泛型 map lookup，先写 ADR/设计结论，不用临时 `map_get_str`
  这类窄 API 污染长期表面。

验收标准：

- 至少一个第二批 API 进入实现，或有明确 ADR 说明为什么 v0.0.5 继续暂缓实现。
- 新 API 有 runtime tests、CLI JSON type mismatch、LSP diagnostic 和 docs/runtime.md 覆盖。
- 旧 API 保留兼容窗口，不把 v0.0.4 脚本变成 warning 或 error。

完成记录：

- 已选择 map lookup 作为第二批可恢复 API；`map_get(map, key) -> option[T]` 用引擎内置
  特殊规则从 `map[str, T]` 推导返回类型，不引入公开泛型函数机制或 `map_get_str` 窄 API。
- 已补 runtime tests、CLI JSON type mismatch、LSP diagnostic、`docs/runtime.md` 和示例覆盖。
- async task 状态 API 暂不进入本批实现，后续需要先稳定 `TaskStatus` record 形状和 id 生命周期。

### 29.3 项目和 LSP 体验收紧

状态：已完成。

设计方向：

- 重点是已有能力的开发体验，不是 watch daemon 或后台增量编译。
- 优先修正会影响真实项目的诊断：manifest 错误、module member 缺失、std module completion、
  project root 与相对路径展示。
- LSP 行为继续以 stdio、open document overlay 和显式 request 为边界，不承诺文件系统 watch。

验收标准：

- 至少一个项目体验痛点被改成可测试行为。
- CLI JSON 和 LSP diagnostics 对同类错误使用一致 code。
- 文档明确仍不支持 workspace symbol、rename、go-to-definition、watch mode 或 daemon。

完成记录：

- 已选择 LSP completion 可发现性作为本批痛点：v0.0.5 新增 `map_get` 后，普通 completion
  需要提示 `len`、`contains`、`map_get` 等语言/runtime 内建函数。
- 已补 `crates/nox/tests/cli.rs` 的 LSP completion 回归和 `docs/cli.md` 文档。
- 继续不承诺 workspace symbol、rename、go-to-definition、watch mode 或 daemon。

### 29.4 v0.0.5 release gate 和本地分发复审

状态：已完成。

设计方向：

- 保持无 remote 的本地 release 模型：release gate、local dist smoke、tag 后验证。
- 如果 29.1-29.3 新增项目或 API，必须接入 release gate 的最小稳定 smoke。
- 不引入 registry、安装器或 GitHub Release 自动化作为 v0.0.5 硬前置。

验收标准：

- `scripts/release-gate.sh` 覆盖 v0.0.5 新增的真实项目或 API。
- `scripts/local-dist-smoke.sh` 继续能在临时目录验证 CLI、C header 和动态库。
- `docs/release-checklist.md` 切到 v0.0.5，并且版本号、CHANGELOG、tag 流程一致。

完成记录：

- 已把 `map_get` 示例运行和 bytecode smoke 接入 `scripts/release-gate.sh`。
- 已把 `map_get` 示例接入 `scripts/local-dist-smoke.sh`。
- 已把 `docs/release-checklist.md` 切到 v0.0.5 release candidate / release-prep 流程。
- v0.0.5 release-prep 仍保持独立 commit，只做版本号、Cargo.lock、CHANGELOG 和 PLAN 基线切换。

### 29.5 v0.0.5 发布收口

状态：已完成。

设计方向：

- 发布前只允许 release 阻断修复和文档一致性修正，不再混入新语言能力。
- CHANGELOG 必须从 `[未发布]` 切成 `[0.0.5]`，并重新开新的 `[未发布]`。
- 如果仍无 remote，只完成本地 release commit、tag 和 local dist smoke。

验收标准：

- `scripts/release-gate.sh` 和 `scripts/local-dist-smoke.sh` 在 release-prep commit 后通过。
- `git tag v0.0.5` 指向 release commit。
- `target/release/nox --version` 和 C smoke 的 `nox_core_version()` 都显示 `0.0.5`。

完成记录：

- 已切 `Cargo.toml` / `Cargo.lock` 到 `0.0.5`。
- 已把 CHANGELOG 从 `[未发布]` 切出 `[0.0.5] — 2026-05-21`，并重新打开 `[未发布]`。
- 已完成对应 release-prep commit，并创建本地 `v0.0.5` tag。
- 已在 release-prep commit 后通过 `scripts/release-gate.sh` 和 `scripts/local-dist-smoke.sh`。

## 阶段 30：v0.0.6 规划和开发启动

目标：在 `v0.0.5` 已可本地发布的基础上，收紧对外契约和项目体验。v0.0.6 不以新增大语言能力为
主线，而是优先减少“能用但不够稳定”的边界：诊断 code、async task 状态、manifest/project 输出、
embedding 兼容矩阵和 release 复现流程。

### 30.1 诊断契约硬化第一批

状态：已完成。

设计方向：

- 从 `docs/diagnostics.md` 中仍使用通用 `error` 的高价值错误族开始，而不是一次性重命名所有错误。
- 首批候选：parser expected-token/recovery、manifest schema、module member missing、
  permission denied、bytecode verifier failure。
- 稳定 code 必须同时经过 Rust API、CLI JSON 和 LSP diagnostics，不能只改 message 或 docs。
- 保留 message 可优化空间；`code`、span/source 填充和 JSON/LSP 映射才是契约。

验收标准：

- 至少一个错误族从通用 `error` 收敛为稳定 code，并写入 `docs/diagnostics.md`。
- `cargo test -p nox --test cli` 有 CLI JSON 和 LSP 双路径断言。
- 若涉及 parser/type checker/core，`cargo test -p nox_core` 有对应单元或语言测试。
- 文档明确哪些错误族继续暂用 `error`，避免误以为 v0.0.6 已完成全部诊断分层。

建议首批任务：

完成记录：

- 已审计现有稳定 code，确认 `parse.expected-token`、`module.member-not-found` 和 `std/*`
  `module.not-found` 已有 CLI/LSP 证据；manifest schema 当前主要是 CLI/project discovery
  错误，不作为首批双路径目标。
- 已把普通文件 module loader 读取失败从通用 `error` 提升为稳定 `module.not-found`，与
  `std/*` 缺失模块保持同一 code。
- 已补 CLI JSON 和 LSP diagnostics 双路径测试
  `check_json_and_lsp_report_relative_module_not_found_code`。
- 已更新 `docs/diagnostics.md` 和 CHANGELOG；manifest、host callback、bytecode verifier、
  权限、文件系统和网络错误仍按错误族继续暂用 `error`。

### 30.2 Async task 可恢复状态 API 评估

状态：已完成。

设计方向：

- v0.0.5 已暂缓 async task 状态 API，因为 unknown id、completed、cancelled 和 pending 的状态形状
  还没有稳定 record 契约。
- v0.0.6 可以评估 `task_status(id: int) -> result[TaskStatus, str]` 或
  `task_poll(id: int) -> option[TaskStatus]`，但必须先定义 `TaskStatus` 的字段、状态枚举表达和
  completed task 是否消费 id。
- 权限不足仍是 diagnostic，不包装成 `result`；unknown id 是否可恢复必须由 API 语义明确。
- 不引入语言级 async/await、event loop、promise 或跨线程 callback。

验收标准：

- 有 ADR 或 runtime 设计文档说明 `TaskStatus`、id 生命周期、取消语义和权限边界。
- 如果实现，必须有 runtime tests、CLI JSON type mismatch、LSP diagnostic、docs/runtime.md、
  embedding cookbook 和 example 覆盖。
- 如果暂缓，必须在 `docs/runtime.md` 和本计划中写明重新启动条件。

建议首批任务：

完成记录：

- 已用现有 async task lifecycle tests 反推状态机：pending 可重复 poll，completed 和 cancelled
  都会立即释放并变成 unknown；unknown id、权限不足、负数 id 和负数 duration 仍是 diagnostic。
- 已新增 `docs/decisions/0016-defer-async-task-status-api.md`，比较 `task_status`、
  `task_poll`、`task_ready_result` 和 tombstone 方案。
- 结论是 v0.0.6 暂不实现脚本级 `TaskStatus` / `task_status` API，也不扩 C ABI；重新启动条件是
  先有稳定状态表示、completed/cancelled tombstone 生命周期和 unknown id 可恢复语义。
- 已同步 `docs/runtime.md`、`docs/embedding.md`、ADR 索引和 CHANGELOG。

### 30.3 Manifest 和项目体验收紧

状态：已完成。

设计方向：

- 目标是让 `nox check/test/fmt` 在 manifest 项目中更适合日常开发和 CI，而不是实现 package manager。
- 优先改进错误定位、项目 root 展示、source_dirs/test_dirs summary、JSON 输出一致性。
- LSP 继续复用 manifest/session/module graph，不引入 filesystem watch 或 workspace daemon。

验收标准：

- 至少一个 manifest/project 痛点被改成可测试行为。
- CLI human output 和 JSON output 都能解释项目发现边界；LSP 对同类错误使用同一 code。
- `examples/projects/scoreboard` 继续作为项目回归 fixture，并覆盖新增输出或诊断。

建议首批任务：

完成记录：

- 已审计 `project check` 输出，确认人类可读模式可用，但缺少 CI 可解析的项目边界和步骤 summary。
- 已新增 `nox project check --json`，输出 `nox.project-check.v1`，包含 manifest root、
  package name/version、`check` / `test` / `fmt` 三个子步骤的退出码和捕获 stdout/stderr。
- JSON 模式在 manifest root 下运行子步骤，因此从项目根或子目录执行都围绕同一个 manifest
  main、`modules.source_dirs` 和 `modules.test_dirs`。
- 已用 scoreboard fixture 覆盖 `project check --json`，并更新 `docs/cli.md`、
  `docs/package-manifest-design.md` 和 CHANGELOG。

### 30.4 Embedding 兼容矩阵复审

状态：已完成。

设计方向：

- v0.0.6 默认不扩 C ABI；先确认 Rust API / C ABI 当前文档、header、examples 和 regression 是否一致。
- 重点复审错误字符串生命周期、host callback error、userdata fallback、option/result handle、
  host-held value、permission model 和 version reporting。
- 如果 30.2 引入 async task 状态 API，必须明确它是否进入 Rust embedding、C ABI 或仅 runtime stdlib。

验收标准：

- `docs/embedding.md` 有 v0.0.6 兼容矩阵更新，说明允许的 minor 扩展和禁止的破坏。
- `scripts/embedding-regression.sh` 覆盖任何新增或重新定义的宿主边界。
- C header 改动必须只末尾追加，并有 C smoke 证明。

建议首批任务：

完成记录：

- 已对比 `docs/embedding.md`、`crates/nox_core/include/nox_core.h`、`examples/embed/c_embedding.c`、
  `crates/nox/examples/rust_embedding.rs` 和 `scripts/embedding-regression.sh`。
- 已确认 v0.0.6 到 30.4 为止没有新增 Rust embedding API、C enum、C struct 字段或 exported C
  function；30.1 的 `module.not-found` 是 `Diagnostic.code` 兼容扩展，30.3 的
  `nox.project-check.v1` 是 CLI schema，30.2 明确暂缓 task status ABI。
- 已在 `docs/embedding.md` 增加 v0.0.6 兼容复审结论，明确不追加 C ABI symbol、不改变
  `NoxCoreValue` layout、userdata/last_error/只读 handle/async task id 生命周期保持现状。
- 已运行 `scripts/embedding-regression.sh` 覆盖 Rust API、runtime API、Rust embedding example
  和 C ABI smoke；无需扩 header。

### 30.5 v0.0.6 Release gate 和本地分发复审

状态：已完成。

设计方向：

- 保持无 remote 的本地 release 模型：release gate、local dist smoke、tag 后验证。
- 如果 30.1-30.4 新增稳定诊断、API 或项目输出，必须接入 release gate 的最小 smoke。
- 不引入 registry、安装器、GitHub Release 自动化或跨平台打包矩阵作为 v0.0.6 硬前置。

验收标准：

- `scripts/release-gate.sh` 覆盖 v0.0.6 新增的稳定诊断、async task API 或项目体验路径。
- `scripts/local-dist-smoke.sh` 继续能在临时目录验证 CLI、C header、动态库和至少一个 v0.0.6 示例。
- `docs/release-checklist.md` 切到 v0.0.6 release candidate / release-prep 流程。

建议首批任务：

完成记录：

- 已把 v0.0.6 新增的相对模块 `module.not-found` JSON diagnostic 接入 `scripts/release-gate.sh`。
- 已把 scoreboard `project check --json` 接入 `scripts/release-gate.sh` 和
  `scripts/local-dist-smoke.sh`，覆盖 `nox.project-check.v1` 项目 summary。
- 已把 `docs/release-checklist.md` 切到 v0.0.6 release candidate / release-prep 流程。
- v0.0.6 release-prep 仍保持独立 commit，只做版本号、Cargo.lock、CHANGELOG 和 PLAN 基线切换。

### 30.6 v0.0.6 发布收口

状态：已完成。

设计方向：

- 发布前只允许 release 阻断修复和文档一致性修正，不再混入新语言能力。
- CHANGELOG 必须从 `[未发布]` 切成 `[0.0.6]`，并重新开新的 `[未发布]`。
- 如果仍无 remote，只完成本地 release commit、tag 和 local dist smoke。

验收标准：

- `scripts/release-gate.sh` 和 `scripts/local-dist-smoke.sh` 在 release-prep commit 后通过。
- `git tag v0.0.6` 指向 release commit。
- `target/release/nox --version` 和 C smoke 的 `nox_core_version()` 都显示 `0.0.6`。

完成记录：

- 已切 `Cargo.toml` / `Cargo.lock` 到 `0.0.6`。
- 已把 CHANGELOG 从 `[未发布]` 切出 `[0.0.6] — 2026-05-22`，并重新打开 `[未发布]`。
- 已完成对应 release-prep commit，并创建本地 `v0.0.6` tag。
- 已在 release-prep commit 后通过 `scripts/release-gate.sh` 和 `scripts/local-dist-smoke.sh`。

## 阶段 31：v0.0.7 规划和开发启动

目标：在 `v0.0.6` 已可本地发布的基础上，继续收紧对外可依赖的契约。v0.0.7 不以新增大语言能力为
主线，而是优先把 manifest/project 诊断、项目级 JSON、CLI/LSP 项目边界、embedding 文档证据和
release gate 串成下一轮稳定闭环。

### 31.1 诊断契约硬化第二批

状态：已完成。

设计方向：

- 从 manifest schema / project discovery、permission denied、bytecode verifier failure 中选择一个
  能形成稳定 code 和稳定测试证据的错误族。
- 优先选择 CLI JSON 和 LSP 都能覆盖的路径；如果错误族天然只在 CLI/project 层出现，必须接入
  release gate 或项目级 JSON 测试作为契约证据。
- 不一次性重命名所有 `error`；每批只收敛一个错误族，避免破坏现有诊断 message 和 fixture。
- `docs/diagnostics.md` 必须同步写清新增稳定 code、仍暂用 `error` 的错误族和兼容窗口。

验收标准：

- 至少一个新的错误族从通用 `error` 收敛为稳定 diagnostic code。
- Rust API、CLI JSON、LSP diagnostics 或 release gate 中至少两条证据覆盖新增 code；如果只适用于
  CLI/project 层，必须解释为什么没有 LSP 对等路径。
- CHANGELOG、`docs/diagnostics.md` 和相关负向 fixture 与实际行为一致。

建议首批任务：

完成记录：

- 已选择 manifest 解析/schema 错误作为 v0.0.7 第二批稳定诊断族；该路径主要发生在 CLI/project
  manifest discovery 阶段，不天然对应已打开源码文档的 LSP diagnostic。
- 已把 `Manifest::parse` 产生的 manifest 内容/结构错误提升为稳定 code `manifest.invalid`；
  `nox.toml` 文件读取失败仍暂用通用 `error`，避免把 IO/权限问题混入 schema code。
- 已补 Rust manifest 单元测试断言缺失必需 key、未知 runtime permission 都返回
  `manifest.invalid`。
- 已补 CLI JSON 测试 `check_json_reports_invalid_manifest_code`，覆盖显式文件检查时发现无效
  `nox.toml` 并在 `nox.check.v1` 中输出 `manifest.invalid`。
- 已更新 `docs/diagnostics.md` 和 CHANGELOG；permission、bytecode verifier、文件系统、网络和
  host callback 错误仍按后续批次继续暂用 `error`。

### 31.2 Project check JSON 失败细节增强

状态：待启动。

设计方向：

- 保持 `schema: "nox.project-check.v1"` 兼容，不删除或重命名现有字段。
- 优先补充 CI 真正需要的失败摘要：失败子步骤、退出码、manifest root、package name/version、
  stdout/stderr 截断策略和从子目录执行时的 root 归一化。
- 不做 dependency resolver、lockfile、workspace package graph 或 watch daemon。

验收标准：

- scoreboard fixture 至少覆盖一个失败路径，断言 JSON summary 中的失败子步骤和项目边界。
- human output 不被 JSON 增强破坏；`project check --json` 仍能从项目根和子目录稳定运行。
- `docs/cli.md` 和 `docs/package-manifest-design.md` 记录 JSON 兼容约束和字段语义。

建议首批任务：

- 新增一个最小失败 fixture 或临时修改型测试，覆盖 `check`、`test` 或 `fmt` 子步骤失败时的
  `nox.project-check.v1` 输出。
- 如果发现字段形状需要 v2，先写设计结论，不能在 v1 上做破坏式迁移。

### 31.3 CLI / LSP 项目边界一致性复审

状态：待启动。

设计方向：

- 复审 CLI、LSP、manifest discovery 和 session/module graph 是否对项目 root、relative import、
  `modules.source_dirs`、`modules.test_dirs` 使用同一套语义。
- 优先修复真实不一致：同一错误在 CLI JSON 和 LSP 中 code 不同、span/source 不同或 root 解析不同。
- 不引入 filesystem watcher、后台 daemon 或增量 typecheck；只复用现有项目发现和缓存边界。

验收标准：

- 至少一个 CLI/LSP 项目边界场景有双路径测试，或复审证明当前一致并补上缺失文档/fixture。
- `examples/projects/scoreboard` 继续作为主项目 fixture，不新增展示型样板项目。
- 不改变单文件脚本优先级；显式 path 仍优先于 manifest 默认入口。

建议首批任务：

- 从 scoreboard 子目录执行、缺失相对模块、manifest source_dirs 配置错误三类场景中选一个做
  CLI JSON / LSP 对照测试。

### 31.4 Embedding 和权限边界复审

状态：待启动。

设计方向：

- 默认不扩 C ABI；先复审 v0.0.6 后新增诊断、project JSON 或权限行为是否影响宿主文档。
- 重点检查 `docs/embedding.md`、C header、Rust embedding example、C smoke、runtime permissions
  和 async task 暂缓结论是否仍一致。
- 如果 31.1 选择 permission diagnostic，必须明确宿主如何看到该 code、是否影响 last_error 和
  runtime permissions cookbook。

验收标准：

- `scripts/embedding-regression.sh` 继续通过，并覆盖本阶段任何宿主可见变化。
- `docs/embedding.md` 说明 v0.0.7 是否新增 Rust API / C ABI；默认结论应是“不扩 ABI”。
- 如果新增 C ABI，必须只末尾追加 header、补 C smoke 和 compatibility note。

建议首批任务：

- 在 31.1-31.3 完成后复审 embedding 文档；除非出现真实宿主缺口，不单独追加 C symbol。

### 31.5 v0.0.7 Release gate 和本地分发复审

状态：待启动。

设计方向：

- 把 31.1-31.4 已稳定的新诊断、项目 JSON 或宿主边界接入 `scripts/release-gate.sh`。
- 保持 `scripts/local-dist-smoke.sh` 验证 release CLI、C header、动态库、示例脚本和项目 fixture。
- `docs/release-checklist.md` 只在进入 v0.0.7 release candidate 时切到 `0.0.7` 流程。

验收标准：

- release gate 覆盖 v0.0.7 新增路径，不只依赖单元测试。
- local dist smoke 在临时目录能验证 `nox --version`、C header smoke 和项目级 JSON smoke。
- CHANGELOG、PLAN 和 release checklist 对 v0.0.7 新增能力与暂缓能力描述一致。

建议首批任务：

- 在 31.1-31.4 收口后补 release gate；不要在规划阶段提前把 checklist 切到 v0.0.7。

### 31.6 v0.0.7 发布收口

状态：待启动。

设计方向：

- 发布前只允许 release 阻断修复、版本号切换、CHANGELOG、PLAN 基线和 checklist 一致性修正。
- 如果仍无 remote，只完成本地 release commit、tag、release gate 和 local dist smoke。
- 不在 release-prep commit 中混入新语言能力、项目 JSON schema 破坏或 C ABI 扩面。

验收标准：

- `scripts/release-gate.sh` 和 `scripts/local-dist-smoke.sh` 在 release-prep commit 后通过。
- `git tag v0.0.7` 指向 release commit。
- `target/release/nox --version` 和 C smoke 的 `nox_core_version()` 都显示 `0.0.7`。

## 阶段 15：v0.0.2 发布收口

目标：把当前 `main` 做成可复现、可审计、可打 tag 的 v0.0.2 发布候选。

### 15.1 Release candidate 审计

状态：已完成。

设计方向：

- 按 `docs/release-checklist.md` 跑一次完整 dry run。
- 对照 `CHANGELOG.md`、`README.md`、`docs/language-v0.md`、`docs/cli.md`、
  `docs/embedding.md`，确认对外能力没有文档漂移。
- 用 `git grep` 检查旧状态词：`v0.0.2 路径`、`尚未`、`待实现`、`第一版` 等是否
  仍然准确。

验收标准：

- release checklist dry run 有记录。
- `CHANGELOG.md` 可以直接整理为 v0.0.2 release section。
- 文档链接检查、C smoke、CLI smoke 全部通过。

建议首批任务：

- 从 `docs/release-checklist.md` 复制准备 checklist 到本地记录或 PR 描述。
- 跑完整验证，把失败项只按发布阻断级别修正，不在本阶段扩语言功能。
- 检查 `CHANGELOG.md` 的 v0.0.2 路径是否完整覆盖 `README.md` 宣称的能力。

验证记录（2026-05-21）：

- 已跑 `cargo fmt --all --check`、`cargo test --all`、`cargo clippy --all-targets -- -D warnings`。
- 已跑 CLI smoke：`run`、`check`、`check --json`、`test`、`test --json`、`fmt`、
  `fmt --check`、`inspect-bytecode --compact`。
- 已跑 C embedding smoke，`nox_core_version()` 对外返回 `0.0.2`。
- 已跑本地 Markdown 链接检查和 `git diff --check HEAD`。
- 审计中发现并修复 formatter 会把 `42.0` 格式化为 `42` 的语义破坏；已补回归测试。

### 15.2 版本号与 tag 策略

状态：已完成。

设计方向：

- 明确 Cargo crate 版本、C header 版本字符串、CHANGELOG 标题和 git tag 的关系。
- v0.0.2 发布前不引入自动发布；先保持手动 tag。
- 如果 C ABI 后续有兼容破坏，必须在 header 注释和 changelog 中写清。

验收标准：

- `docs/release-checklist.md` 包含 v0.0.2 实际命令。
- `nox_core_version()`、Cargo 版本、CHANGELOG 版本一致。
- tag 前最后一次验证命令列表可复制执行。

建议首批任务：

- 确认 `[workspace.package].version` 是唯一版本来源。
- 确认 `nox_core_version()` 不硬编码漂移，或把硬编码更新规则写进 checklist。
- 准备 `Prepare v0.0.2 release` 的最小 release commit 范围。

验证记录（2026-05-21）：

- `[workspace.package].version` 已更新为 `0.0.2`，`Cargo.lock` 中 `nox` / `nox_core`
  均同步为 `0.0.2`。
- `nox_core_version()` 使用 `CARGO_PKG_VERSION`，C smoke 已确认输出 `nox_core 0.0.2`。
- `CHANGELOG.md` 已切出新的 `[未发布]` 节，并把 v0.0.2 路径整理为 `[0.0.2] — 2026-05-21`。
- 下一步实际发布只需要单独提交 release-prep 改动并按 `docs/release-checklist.md` 打
  `v0.0.2` tag；本阶段不自动 push 或发布 GitHub Release。

## 阶段 16：项目化和模块系统 v0.0.3

目标：让 Nox 项目从“多个文件能跑”提升到“项目结构可维护、工具能理解边界”。

### 16.1 Manifest 语义升级

状态：已完成。

设计方向：

- 扩展 `nox.toml` 的保守子集，但继续避免引入完整 TOML 依赖，除非维护成本超过收益。
- 候选字段：
  - `[package]`：`name`、`version`、`description`
  - `[entrypoints]`：`main`、可选命名入口
  - `[modules]`：`source_dirs`、可选 `test_dirs`
  - `[runtime]`：默认权限声明只做文档/检查，不自动授予危险能力
- 明确“显式 CLI 参数优先于 manifest main”的规则。

验收标准：

- Manifest parser 有正负测试。
- CLI 对缺失 main、错误字段类型、重复 section 的诊断稳定。
- `docs/package-manifest-design.md` 更新为 v0.0.3 设计。

建议首批任务：

- 先扩设计文档和 manifest fixture，不急于改运行时入口。
- 明确 manifest 解析失败时 `run/check/test` 的退出码和 JSON 诊断形状。
- 把当前 `modules.source_dirs` 行为写成兼容性基线，避免升级时破坏现有项目。

验证记录（2026-05-21）：

- Manifest parser 已支持 `package.description`、命名 entrypoint、`modules.test_dirs` 和
  `runtime.permissions`；未知 runtime permission、错误字段类型、重复 section 仍有负向测试。
- `nox run` 无显式 path 时使用 `[entrypoints].main`；显式 path 仍优先于 manifest main。
- `nox test` 无显式 path 时优先使用 `modules.test_dirs`，未配置时保留 `source_dirs` / 项目根回退。
- `docs/package-manifest-design.md` 和 `docs/cli.md` 已更新为 v0.0.3 manifest 语义。
- 已跑 `cargo fmt --all --check`、`cargo test --all`、`cargo clippy --all-targets -- -D warnings`、
  `git diff --check HEAD`。

### 16.2 命名空间 import 设计

状态：已完成。

设计方向：

- 评估 `import "math.nox" as math;`，避免所有导出声明平铺进入当前作用域。
- 命名空间值先只作为静态模块表面，不引入动态 object。
- `math.sqrt` 的字段访问需要区分 record field 和 module member。
- 保留现有平铺 import 一个 minor 阶段，给出冲突时的迁移建议。

验收标准：

- ADR 写清语法、类型规则、冲突规则和格式化规则。
- parser/type checker/formatter/LSP completion 覆盖命名空间 import。
- 负向测试覆盖命名空间不存在成员、命名空间与本地声明冲突。

建议首批任务：

- 已写 ADR 0008，采用 `import "x.nox" as x;`，并保留平铺 import 的兼容路径。
- `x.member` 在 import resolver 中静态改写为导入模块声明，不引入运行时 object。
- LSP completion 已在 `alias.` 后按 module member 表面补全，并复用打开文档 overlay。

验证记录（2026-05-21）：

- parser/lexer/formatter 支持 `import "math.nox" as math;`。
- resolver 支持 namespace 成员访问，缺失成员返回 `module.member-not-found`，alias 与顶层
  声明冲突返回 `module.name-conflict`。
- namespace import 不把导出成员平铺到当前作用域；平铺 import 保持兼容。
- LSP completion 在 `math.` 后返回导入模块导出成员，不返回私有 helper。
- 已跑 `cargo fmt --all`、`cargo test -p nox_core namespace_import`、
  `cargo test -p nox fmt_prints_namespace_imports --test cli`、
  `cargo test -p nox lsp_completion_includes_namespace_members_from_open_import --test cli`。

### 16.3 项目级检查和测试

状态：已完成。

设计方向：

- `nox check` 无 path 时从 manifest 查找 main 和 source/test dirs。
- `nox test` 优先使用 manifest 的 `test_dirs`，没有时保留当前发现策略。
- 不做 watch mode；先保证一次性项目检查可靠。

验收标准：

- CLI 集成测试覆盖 manifest 项目根、显式 path 覆盖 manifest。
- JSON 输出能表达项目级文件列表和失败摘要。
- 文档说明当前不支持 registry/package install。

建议首批任务：

- 已定义项目根发现：无显式 path 时从当前目录向上查找 `nox.toml`。
- `nox check` 会检查 `[entrypoints].main`、`modules.source_dirs` 和 `modules.test_dirs`
  展开的 `.nox` 文件，并去重。
- `check --json` 的 `files` 字段输出 manifest 展开后的实际文件路径。
- `nox test` 的 manifest `test_dirs` 优先级沿用 16.1 已完成实现。

验证记录（2026-05-21）：

- 新增 CLI 集成测试覆盖 `check --json` manifest 展开 source/test dirs、无 dirs 时只
  检查 manifest main、显式 path 覆盖 manifest 展开。
- 没有 manifest 且没有 path 时仍是用法错误，返回 `2`。
- 已跑 `cargo fmt --all`、`cargo test -p nox check_without_paths --test cli`。

## 阶段 17：嵌入 API 和长期会话

目标：让宿主能更自然地把 Nox 嵌入到长期进程，而不是每次操作都像一次性脚本。

### 17.1 Rust API session / module graph 设计

状态：已完成。

设计方向：

- 重新评估 v0.0.2 草案中暂缓的 `Session` / `ModuleGraph`。
- 保持 `Engine::eval/check` 简单 API；新增高级 API 用于复用 module cache、
  open document overlay、host state 和 diagnostics。
- 明确 session 与 runtime permission 的关系：权限属于 runtime/host，不属于源码模块。

验收标准：

- ADR 列出不拆、拆 `Session`、拆 `ModuleGraph` 三种方案的取舍。
- API tests 覆盖简单 API 兼容和 session API 的模块复用。
- LSP 后续可以直接复用该设计，不另建一套缓存语义。

建议首批任务：

- 从当前 `Engine::eval/check` 使用点出发，列出 CLI、LSP、宿主 API 的重复状态。
- 设计 `Session` 时先不承诺 watch mode，只承诺 open document overlay 和 module cache。
- 新 API 必须能和 runtime permission 分离，避免源码模块隐式获得宿主能力。

验证记录（2026-05-21）：

- 新增 ADR `docs/decisions/0007-rust-session-module-graph.md`，比较“不拆”、只拆
  `ModuleGraph`、暴露完整 AST/typechecked graph 等方案，并采纳 `Session + ModuleGraph`。
- 新增 `nox_core::Session` 和源码级 `ModuleGraph`，支持跨调用 import 源码缓存、overlay、
  cache 清理，并通过 `engine_mut()` 保持 host function / budget 等简单 API 兼容。
- Runtime permission 未进入 `nox_core::Session`；权限仍由宿主或 `nox` runtime 管理。
- `docs/embedding.md` 已补长期 Session 用法和 overlay 语义。
- 已跑 `cargo test -p nox_core session_`；全量固定验证见本批次提交前记录。

### 17.2 C ABI 错误与 userdata 细化

状态：已完成。

设计方向：

- 为 C callback 错误增加更稳定的错误来源和错误码表达。
- 明确 `ctx` 生命周期、线程假设、reentrancy 边界。
- 评估是否需要 `nox_core_engine_set_userdata`，避免每个 callback 自行包 ctx。

验收标准：

- Header 注释覆盖 ctx 生命周期和线程假设。
- C smoke 覆盖 host callback 返回错误、last_error、userdata 生命周期。
- `docs/embedding.md` 给出 C 宿主的错误传播示例。

建议首批任务：

- 已审 `nox_core.h` 中 callback、ctx、engine、value/handle ownership 说明，并补齐
  callback 线程、reentrancy、userdata 优先级。
- 新增 `nox_core_engine_set_userdata` / `nox_core_engine_userdata`。注册 host function 时
  传入非 null `ctx` 仍优先；传入 null `ctx` 时 callback 使用 engine 当前 userdata。
- callback 返回非 OK 状态会写入包含 callback 名和状态的 `last_error`。
- C smoke 已覆盖 engine userdata 驱动的 host callback。

验证记录（2026-05-21）：

- Rust API tests 覆盖 engine userdata、callback 错误 last_error、旧 C callback 注册、
  C ABI string/array/map/record 读取。
- C smoke 重新编译并运行：`cc -Icrates/nox_core/include examples/embed/c_embedding.c
  -Ltarget/debug -lnox_core -Wl,-rpath,$PWD/target/debug -o target/debug/c_embedding_smoke &&
  target/debug/c_embedding_smoke`。
- 已跑 `cargo fmt --all`、`cargo test -p nox_core c_abi_`、`cargo build -p nox_core`。

### 17.3 Host-held Value 生命周期

状态：已完成。

设计方向：

- 现有 Rust `Value` 可被宿主持有；C ABI 通过 owning handle 持有复合值。
- 评估是否需要显式 `Engine::drop_value` / handle registry，还是继续 Rust `Drop`
  与 C free 函数。
- 对长生命周期宿主场景给出内存增长基线，而不是先重写 heap。

验收标准：

- 压力测试覆盖宿主持有大量 string/array/map/record 后释放。
- 文档说明跨 Engine 共享 `Value` 不被支持。
- 如果暂不改 API，必须写清触发条件。

建议首批任务：

- 已补长生命周期宿主持有值压力测试，覆盖 Rust `Value` 批量持有 string/array/map/record。
- 已补 C owning handle 压力测试，覆盖 array/map/record handle free 后可清理。
- 当前结论是不新增 `Engine::drop_value` 或 C handle registry；继续使用 Rust `Drop` 与
  C owning handle free 函数。
- 跨 Engine 共享 `Value` 仍不支持，见 `docs/heap-design.md`。

验证记录（2026-05-21）：

- `host_held_rust_values_keep_heap_objects_until_dropped`：宿主持有前对象保持存活，drop 后
  `collect_garbage()` 归零。
- `c_abi_handles_keep_heap_objects_until_freed`：C handles free 前对象保持存活，free 后
  `collect_garbage()` 归零。
- 已跑 `cargo fmt --all`、`cargo test -p nox_core host_held --lib`、
  `cargo test -p nox_core c_abi_handles --lib`。

## 阶段 18：语言表面 v0.0.3 设计

目标：补齐真实脚本会频繁需要的语言能力，但不破坏 Nox 的静态、小核心边界。

### 18.1 可选值 / Result 再评估

状态：已完成。

设计方向：

- 从宿主边界和文件/runtime 错误出发评估，而不是为了语法完整性添加。
- 候选：
  - `option[T]`：表达缺失值
  - `result[T, E]`：表达可恢复错误
  - 继续使用 runtime diagnostic：保持 v0.0.2 行为
- 不引入隐式 nullability。

验收标准：

- ADR 明确是否进入 v0.0.3。
- 如果实现，必须覆盖构造、解包、控制流收窄、C/Rust API 表示。
- 如果暂缓，runtime 错误处理文档要说明替代方式。

建议首批任务：

- 已从 `read_text`、`env_get`、map lookup、async task、host callback 错误倒推需求。
- 已写 ADR 0009，决定 v0.0.3 暂缓语言级 `option[T]` / `result[T, E]`，不引入隐式
  nullability。
- 替代模式固定为显式 guard + diagnostic：`exists`、`contains`、`env_list`、
  `tcp_connect -> bool`。
- 如果未来重启该设计，必须先补类型语法、构造/解包、控制流收窄、Rust `Value` 和
  C ABI 表示。

验证记录（2026-05-21）：

- 新增 `docs/decisions/0009-defer-option-result.md`。
- `docs/runtime.md` 已记录 guard 模式和 diagnostic 边界。
- `docs/language-v0.md` 已说明 `null` 是独立类型，不是隐式 nullable。

### 18.2 可变数组前置设计

状态：已完成。

设计方向：

- 先设计数组可变性，再决定 `push(array, value)`、元素赋值、切片。
- 必须回答：
  - 数组是原地可变还是返回新数组？
  - alias 后修改是否可见？
  - `const` 绑定是否禁止修改容器内容？
  - C ABI 只读 handle 是否继续只读？
  - heap ownership 是否需要从 `Rc<Array>` 改成 interior mutability 或 arena handle？
- v0.0.3 可以只完成设计，不急于实现。

验收标准：

- ADR 写清至少两种设计方案和拒绝理由。
- `docs/array-design.md` 更新未来边界。
- 不允许在语义未定时添加 `push` 的半成品实现。

建议首批任务：

- 已只写设计，不实现 `push`。
- 已写 ADR 0010，决定 v0.0.3 暂缓可变数组；数组继续构造后不可变。
- alias 后没有可观察的写入行为；`const` 与 `let` 都不能修改容器内容。
- C ABI array handle 继续只读，heap 继续使用 `Rc<Array>`，不引入 interior mutability。
- 如果未来需要增长能力，优先评估返回新数组的 copy-on-write 风格 API。

验证记录（2026-05-21）：

- 新增 `docs/decisions/0010-defer-mutable-arrays.md`。
- `docs/array-design.md` 已更新可变数组暂缓和未来 copy-on-write 方向。
- `docs/heap-design.md` 已记录 v0.0.3 不把容器改成 interior mutability。
- `docs/language-v0.md` 已说明数组构造后不可变。

### 18.3 函数类型与高阶函数边界

状态：已完成。

设计方向：

- 当前函数可以声明和调用，但源码级函数类型标注仍不是完整表面。
- 评估是否支持 `fn(int) -> int` 类型、函数值传参、数组/map 中保存函数。
- 和闭包 env 生命周期、C ABI function kind、LSP hover 一起设计。

验收标准：

- 设计文档说明函数类型语法、类型等价、闭包生命周期。
- 如果实现，正向测试覆盖函数传参和返回；负向测试覆盖签名不匹配。
- C ABI 不自动获得跨 ABI 调用 function 的能力，除非另开设计。

建议首批任务：

- 已先区分“函数声明可调用”和“一等函数值”两件事。
- 已写 ADR 0011，决定 v0.0.3 暂缓源码级函数类型和高阶函数。
- 内部 `fn(...) -> ...` 类型继续用于声明、host function、调用检查和 LSP hover；源码
  类型位置不接受 `fn(int) -> int`。
- 闭包环境生命周期不扩大为公共契约；C ABI 继续只报告 function kind，不提供跨 ABI
  调用脚本函数 API。
- stdlib 分层优先走命名空间 import，不用高阶函数绕路。

验证记录（2026-05-21）：

- 新增 `docs/decisions/0011-defer-function-types.md`。
- `docs/language-v0.md` 已记录源码级函数类型暂缓、内部类型等价和 C ABI function kind
  边界。
- `docs/embedding.md` 已明确 C callback 注册不是脚本函数 handle。
- `docs/heap-design.md` 已同步函数环境弱引用的 v0.0.3 边界。

## 阶段 19：LSP 与开发者体验

目标：让编辑器反馈更稳定，项目越大时越需要复用检查结果和清晰诊断。

### 19.1 LSP project awareness

状态：已完成。

设计方向：

- LSP 初始化时发现 manifest root。
- open document overlay 与 module resolver 共用 session/module graph。
- diagnostics 需要覆盖被 import 文件，而不只当前打开文件。

验收标准：

- stdio LSP 集成测试覆盖跨文件 import 修改后的 diagnostics 刷新。
- 文档说明当前 LSP 支持和不支持的 request。
- 不引入后台 watch；先由 didOpen/didChange/didSave 驱动。

建议首批任务：

- 等 17.1 的 session/module graph 方案定稿后再实现，避免 LSP 自建缓存。
- 先覆盖 didOpen/didChange 后被 import 文件的诊断刷新。
- 对未打开文件只做项目检查级别诊断，不承诺后台实时 watch。

验证记录（2026-05-21）：

- LSP diagnostics/hover 已使用长期 `Session` / `ModuleGraph` loader，open document overlay
  与 module resolver 共用同一套源码缓存和覆盖语义。
- didOpen/didChange 后会重新发布所有已打开文档 diagnostics；被 import 的打开文档变化会刷新
  导入方 diagnostics。
- 保持无后台 watch：未打开文件只在打开文档触发检查时按 manifest/import 规则读取。
- `docs/cli.md` 已更新 LSP 支持边界；已补 stdio LSP 集成测试覆盖跨文件 import 刷新。
- 已跑 `cargo test -p nox lsp_ --test cli`；全量固定验证见本批次提交前记录。

### 19.2 Formatter 稳定性

状态：已完成。

设计方向：

- Formatter 已支持当前语法；下一步保证 idempotence 和项目级批量格式化。
- 增加 golden fixture，而不是只测单个字符串片段。
- 明确注释保留策略；如果 parser 仍丢注释，文档必须写清。

验收标准：

- `nox fmt --check` 对一组 fixtures 稳定。
- 格式化两次输出不变。
- 新语法进入语言前必须先补 formatter。

建议首批任务：

- 已把 examples 中代表性语法整理为 `examples/formatter-golden.nox` golden fixture。
- 已增加“格式化两次不变”的测试覆盖。
- `fmt --check` / `fmt --write` 已支持目录递归展开；无显式 path 时按 manifest main、
  `source_dirs` 和 `test_dirs` 展开项目文件。
- formatter 已稳定打印 `else if` 链，不再退化成嵌套 `else { if ... }`。
- 注释仍不保留，`docs/cli.md` 已明确 `fmt --write` 的当前限制。

验证记录（2026-05-21）：

- 新增 `fmt_golden_fixture_is_idempotent`，覆盖代表性语法 fixture 和二次格式化稳定性。
- 新增目录批量格式化测试和 manifest 项目格式化测试。
- 已跑 `cargo test -p nox fmt_ --test cli`。

### 19.3 诊断文案和 code 稳定性

状态：已完成。

设计方向：

- 为已存在 code 建一个文档表，说明 code 是否稳定、何时可变。
- 减少测试只依赖英文 message substring；对 JSON/LSP 优先断言 code 和 span。
- 人类 CLI 输出继续保持可读。

验收标准：

- `docs/diagnostics.md` 或 `docs/cli.md` 增加诊断 code 表。
- 核心负向测试覆盖 code。
- 改 message 不应大面积破坏结构性测试。

建议首批任务：

- 已盘点 parser/type checker/runtime/module/test 当前 code，并分成稳定 code 与通用
  `error` 兜底。
- 已新增 `docs/diagnostics.md`，记录稳定 code、`error` 边界和兼容规则。
- JSON/LSP 测试已覆盖 `parse.expected-token`、`type.mismatch`、`runtime.division-by-zero`、
  `module.name-conflict`、`module.member-not-found` 和 `test.signature`。
- 已补 `test --json` 的 invalid signature code/span/source 回归。
- CLI 人类输出继续允许 message 优化；结构化输出以 schema/code/span/source 为契约。

验证记录（2026-05-21）：

- 新增 `docs/diagnostics.md` 并从 docs index、CLI、语言和开发文档链接。
- 新增 `test_json_reports_invalid_signature_code`。
- 已跑 `cargo test -p nox test_json_reports_invalid_signature_code --test cli`。

## 阶段 20：运行时能力和标准库边界

目标：让默认 runtime 更实用，但继续保持能力显式、宿主可控。

### 20.1 文件系统能力细分

状态：已完成。

设计方向：

- 当前已有 read/write 区分；下一步评估 path allowlist、工作目录边界、manifest
  声明和 CLI 授权之间的关系。
- 不默认授予写能力。
- 错误需要区分 permission denied、not found、invalid path。

验收标准：

- Runtime tests 覆盖 allowlist 和拒绝路径。
- CLI 文档说明入口读取权限不等于任意文件读写。
- Host API 可以构造最小权限 runtime。

建议首批任务：

- 已定义 read/write allowlist：root 为空时保持旧的授权不限制路径；root 非空时，
  `read_text` / `exists` / `write_text` 的规范化路径必须位于对应 root 下。
- 路径规范化规则：空路径是 invalid；已存在路径使用 `canonicalize` 解析符号链接和
  `..`；缺失路径转成绝对路径并做 lexical normalization，用于权限判断。
- 已明确 manifest `runtime.permissions` 只声明期望能力，不自动授予权限或配置 allowlist。
- CLI 文档已说明入口/import 读取权限不等于任意脚本文件读写。
- Host API 可用 `RuntimePermissions::none().allow_filesystem_read_under(path)` /
  `allow_filesystem_write_under(path)` 构造最小文件权限 runtime。

验证记录（2026-05-21）：

- Runtime tests 覆盖 read allowlist 内部读取、`..` 越界拒绝、缺失路径 `exists=false`、
  空路径 invalid、write allowlist 内部写入和越界拒绝。
- 已跑 `cargo test -p nox filesystem_ --lib`。

### 20.2 Async task 生命周期

状态：已完成。

设计方向：

- 当前 async task 是最小模型；下一步明确 task id、取消、完成后释放、重复 poll
  的长期语义。
- 评估是否需要 event loop 抽象，或者继续保持 runtime host function。
- 不引入 promise/async 语法，除非另开语言设计。

验收标准：

- Runtime 文档有状态机。
- 测试覆盖取消后 poll、完成后重复 poll、任务泄漏压力。
- CLI 行为和嵌入 API 行为一致。

建议首批任务：

- 已在 `docs/runtime.md` 写清 pending、completed、cancelled、rejected、unknown 状态机。
- task id 在单个 `Runtime` 内单调递增，不复用；completed 和 cancelled 都会释放任务并让
  id 进入 unknown 状态。
- 新增 `Runtime::pending_async_task_count()`，让宿主和测试能观察 pending task 释放。
- 不新增 `async/await` 语法，不引入 event loop；继续保持 runtime host function 模型。
- CLI 和嵌入 API 共用同一个 `Runtime` 实现，因此生命周期语义一致。

验证记录（2026-05-21）：

- Runtime tests 覆盖 completed 后重复 poll、cancel 后 poll、unknown cancel、pending
  重复 poll 保留任务，以及批量 completed/cancelled 任务释放到 0。
- 已跑 `cargo test -p nox async_task_ --lib`。

### 20.3 标准库命名和分层

状态：已完成。

设计方向：

- 整理当前全局 host function：`sqrt`、`read_text`、`env_get`、`args` 等。
- 评估未来是否需要模块化 stdlib，例如 `std.fs.read_text`，并与命名空间 import
  设计保持一致。
- 不为了命名整洁立刻破坏现有脚本；先写迁移策略。

验收标准：

- ADR 写清 stdlib 全局函数是否继续增长。
- 如果改名，旧名保留窗口和 warning 策略明确。
- 示例和 README 使用推荐命名。

建议首批任务：

- 已列出现有全局函数，并按核心纯函数、运行时纯函数、脚本参数、文件系统、环境变量、
  计时器、网络和 async task 分类。
- 已写 ADR 0012：v0.0.3 保留现有全局函数作为兼容表面，新增标准库能力默认不继续扩大全局
  命名空间。
- 未来标准库分层优先走 ADR 0008 的静态 namespace import，例如 `std/fs.nox` as `fs`、
  `std/env.nox` as `env`，不引入动态 `std` object。
- 旧全局函数至少保留一个 minor 阶段；警告、formatter/LSP 推荐和实际 std module loader
  另开设计。
- 阶段 24.2 已实现第一批 `std/*` 模块；旧全局函数继续作为兼容表面保留。

验证记录（2026-05-21）：

- 新增 `docs/decisions/0012-stdlib-namespace-strategy.md`。
- `docs/runtime.md` 已标注现有全局函数是 v0.0.3 兼容表面，并链接 ADR 0012。

## 阶段 21：性能、内存和可靠性

目标：有基线、有压力测试，再做结构性优化。

### 21.1 性能基线自动化

状态：已完成。

设计方向：

- 把当前 benchmarks smoke 整理成可重复脚本。
- 记录 parse/typecheck/compile/eval 分段耗时。
- 不把性能数字作为 CI 硬门禁，先用于趋势观察。

验收标准：

- `docs/benchmarks.md` 给出一条可复制命令。
- 至少覆盖递归、循环、容器、模块加载、`nox test`。
- 输出格式稳定，方便后续对比。

建议首批任务：

- 已新增 `scripts/bench-smoke.sh`，串联 release CLI benchmark smoke。
- 覆盖递归、循环、容器、模块加载和 `nox test examples/example_test.nox`。
- 输出稳定为 tab-separated：`case`、`command`、`status`、`real_seconds`、`output`。
- 当前记录端到端 CLI real time，不做 parse/typecheck/compile/eval 分段，不作为 CI 硬门禁。
- `docs/benchmarks.md` 和 `docs/development.md` 已记录可复制命令和数字解释边界。

验证记录（2026-05-21）：

- 已跑 `scripts/bench-smoke.sh`，所有 case 返回 `ok`。

### 21.2 Fuzz / parser robustness

状态：已完成。

设计方向：

- 先做轻量 corpus 和 panic-free 检查，不急于引入复杂 fuzz 基础设施。
- parser/type checker/formatter 是优先目标。
- 所有 fuzz 发现都要转成普通回归测试。

验收标准：

- 有本地脚本可跑一组 malformed source。
- parser 不 panic，formatter 不对非法 AST 产生未定义行为。
- docs/development.md 记录命令。

建议首批任务：

- 已新增 `examples/malformed/` 固定 corpus，覆盖未闭合字符串、深嵌套、非法 token、
  半截 import、错误 record 和静态类型错误。
- 已新增 `scripts/robustness-smoke.sh`：语法/词法坏输入要求 `check` 与 `fmt` 稳定返回
  `1`；静态类型错误要求 `check` 返回 `1` 且 `fmt` 返回 `0`。
- 其它退出码视为回归，尤其用于捕捉 panic。
- 暂不接入随机 fuzz 或 CI 硬门禁；先保持本地固定 corpus 可复制。

验证记录（2026-05-21）：

- 已跑 `scripts/robustness-smoke.sh`，所有 case 返回 `ok`。

### 21.3 Heap 模型触发条件复审

状态：已完成。

设计方向：

- 基于 17.3 和 18.2 的结果复审 `Rc + Weak` 是否继续足够。
- 若可变容器或长期 host-held values 成为主线，再评估 arena + handle。
- 不在没有真实需求和基线数据时引入 tracing GC。

验收标准：

- `docs/heap-design.md` 更新一次复审结论。
- 若继续 `Rc + Weak`，写清 v0.0.3 仍接受的限制。
- 若改实现，必须先有迁移计划和 API 兼容说明。

建议首批任务：

- 已基于 17.3 和 18.2 复审，不启动 heap 重写。
- 长期 host-held Rust `Value` 和 C owning handle 已分别由压力测试覆盖；可变容器已在
  ADR 0010 中暂缓，因此不需要引入 interior mutability 或 arena handle。
- v0.0.3 继续采用 `Rc + Weak` 加弱引用追踪表，不引入 tracing GC、cycle collector、
  arena handle、`Engine::drop_value` 或 C handle registry。
- `docs/heap-design.md` 已写清 v0.0.3 接受的限制和未来触发条件。

验证记录（2026-05-21）：

- 已跑 `cargo test -p nox_core host_held --lib`。
- 已跑 `cargo test -p nox_core c_abi_handles_keep_heap_objects_until_freed --lib`。

## 阶段 22：v0.0.3 发布冻结和兼容审计

目标：把阶段 15-21 形成的 v0.0.3 能力冻结成可发布候选，避免“已完成但不可复现”。

### 22.1 v0.0.3 Release candidate 审计

状态：已完成。

设计方向：

- 对照阶段 16-21 的完成记录，确认 README、CHANGELOG、docs index、CLI 文档、
  runtime 文档、embedding 文档和 ADR 没有漂移。
- 按 release checklist 跑 v0.0.3 dry run，但不在本阶段扩语言功能。
- 明确 v0.0.3 tag 前是否需要单独 release-prep commit。

验收标准：

- 有一份可复制的 v0.0.3 dry run 命令记录。
- `CHANGELOG.md` 的 `[未发布]` 能整理为 v0.0.3 release section。
- `nox_core_version()`、Cargo version、C header、README 和 release checklist 不互相矛盾。

建议首批任务：

- 已从 `git log 01d42e6..HEAD` 汇总 v0.0.3 对外变化；`CHANGELOG.md` 的 `[未发布]`
  节已覆盖 manifest、project check/test/fmt、namespace import、Session/ModuleGraph、
  runtime allowlist、async task、C ABI userdata、diagnostics、benchmark/robustness
  和 v0.0.3 设计决策。
- 已在 `docs/release-checklist.md` 增加 v0.0.3 release candidate dry run 小节，并把
  发布示例命令切到 `0.0.3` / `v0.0.3`。
- 本阶段不做 release-prep commit：`Cargo.toml` 仍保持 `0.0.2`，`CHANGELOG.md`
  仍保留 `[未发布]` 作为 v0.0.3 候选节；实际发布前再单独 bump 到 `0.0.3`、切
  changelog 标题并打 tag。

验证记录（2026-05-21）：

- 已跑 `cargo fmt --all --check`、`cargo test --all`、
  `cargo clippy --all-targets -- -D warnings`。
- 已跑 `scripts/robustness-smoke.sh` 和 `scripts/bench-smoke.sh`。
- 已跑 CLI smoke：`run`、`check`、允许 `check --json examples/type-error.nox`
  返回非零、`test`、`test --json`、`fmt`、`fmt --check`、`inspect-bytecode --compact`。
- 已跑 C embedding smoke；`nox_core_version()` 当前输出 `0.0.2`，符合 dry run 不
  bump 版本的约定。
- 已跑 `git diff --check HEAD` 和本地 Markdown 链接检查。

### 22.2 Rust API / C ABI 兼容矩阵

状态：已完成。

设计方向：

- 把公开 Rust API 和 C ABI 分层：稳定表面、实验表面、内部不稳定表面。
- C ABI 枚举、函数、owned handle、string ownership、userdata、callback 错误和 last_error
  都要有兼容规则。
- Rust API 的 `Engine`、`Session`、`ModuleGraph`、`RuntimePermissions`、`Value` 和
  diagnostics 需要明确文档承诺。

验收标准：

- `docs/embedding.md` 或独立文档有兼容矩阵。
- C header 注释和文档一致。
- 有测试或 smoke 覆盖矩阵中的高风险项。

建议首批任务：

- 已从 `crates/nox_core/src/lib.rs`、`crates/nox/src/lib.rs` 和
  `crates/nox_core/include/nox_core.h` 盘点公开 Rust API、默认 runtime API 和 C ABI。
- 已在 `docs/embedding.md` 增加兼容矩阵，覆盖 `Engine`、`Session` / `ModuleGraph`、
  `HostFunctionBuilder` / `Type` / `Value`、`Diagnostic`、`Runtime` /
  `RuntimePermissions`、C enum、`NoxCoreValue`、C engine functions、callback /
  userdata。
- 已写清 C ABI release 前检查项：enum 数值不变、函数签名不变、新函数只追加、
  `NoxCoreValue` 字段顺序和 ownership 注释不漂移。
- 已扩展 C embedding smoke，覆盖 callback error、`last_error` 包含 callback 名、
  `nox_core_engine_clear_error` 清空错误槽，以及原有 userdata fallback 和 compound
  handle free 路径。

验证记录（2026-05-21）：

- 已跑 `cargo fmt --all --check`。
- 已跑 `cargo test -p nox_core c_abi_ --lib`。
- 已跑 `cargo build -p nox_core`。
- 已跑 C embedding smoke：
  `cc -Icrates/nox_core/include examples/embed/c_embedding.c -Ltarget/debug -lnox_core -Wl,-rpath,target/debug -o /tmp/nox_c_embedding && /tmp/nox_c_embedding`。
- 已跑 `git diff --check HEAD` 和本地 Markdown 链接检查。

### 22.3 发布脚本和本地门禁收敛

状态：已完成。

设计方向：

- 不急于自动发布 GitHub Release；先把本地 release gate 做成一条可复制命令。
- 复用现有 `scripts/bench-smoke.sh`、`scripts/robustness-smoke.sh` 和固定 Cargo gate。
- 输出要适合粘贴到 release PR 或 tag message。

验收标准：

- 有一个脚本或文档化命令序列覆盖 release 前固定验证。
- 失败时能清楚指出是哪一类 gate 失败。
- 文档说明该 gate 不会 push、不打 tag、不发布外部资产。

建议首批任务：

- 已新增 `scripts/release-gate.sh`，串行执行 Cargo gate、CLI smoke、scoreboard project smoke、
  embedding regression、robustness smoke、benchmark smoke、Markdown 链接检查和
  `git diff --check HEAD`。
- 脚本输出以 `release gate: <name>` 标出当前 gate；失败时停在对应 gate，便于粘贴到
  release PR 或 tag message。
- 已更新 `docs/release-checklist.md` 和 `docs/development.md`，明确该 gate 只做本地验证，
  不 push、不打 tag、不发布 GitHub Release 或外部资产。
- Benchmark smoke 仍只要求 case 成功和输出格式稳定，不引入数字硬阈值。
- 验证：`scripts/release-gate.sh`、`cargo fmt --all --check`、`cargo test --all`、
  `cargo clippy --all-targets -- -D warnings`、Markdown 链接检查、`git diff --check HEAD`。

## 阶段 23：真实项目体验和工具链一键化

目标：让 Nox 不只适合单文件示例，也适合一个小型多模块项目长期维护。

### 23.1 Sample project fixture

状态：已完成。

设计方向：

- 在 `examples/` 或 `examples/projects/` 下创建一个小型项目 fixture。
- 覆盖 manifest main、source dirs、test dirs、namespace import、runtime stdlib 和 formatter。
- 这个 fixture 既是用户示例，也是 CLI/LSP/project check 的回归来源。

验收标准：

- `nox run`、`nox check`、`nox test`、`nox fmt --check` 对该项目都有集成测试或 smoke。
- README 能指向该项目作为多文件入门示例。
- fixture 不依赖网络、环境变量或不稳定本机路径。

建议首批任务：

- 已新增 `examples/projects/scoreboard/` 多模块项目 fixture，包含 `nox.toml`、
  `src/main.nox`、`src/scoring.nox`、`src/labels.nox` 和 `tests/scoring_test.nox`。
- fixture 覆盖 manifest main、`modules.source_dirs`、`modules.test_dirs`、namespace
  import、默认 runtime stdlib `sqrt` / `to_int`、项目级 `run/check/test/fmt --check`。
- 已在 README 和 `examples/README.md` 中加入 scoreboard 项目入口；该 fixture 不依赖网络、
  环境变量或本机绝对路径。
- 已补 CLI 集成测试 `sample_project_supports_project_workflow`，直接以项目根运行
  `nox run`、`nox check --json`、`nox test` 和 `nox fmt --check`。

验证记录（2026-05-21）：

- 已跑 `cargo test -p nox sample_project_supports_project_workflow --test cli`。
- 已在 `examples/projects/scoreboard` 下跑 `cargo run -p nox -- run`、
  `cargo run -p nox -- check --json`、`cargo run -p nox -- test`、
  `cargo run -p nox -- fmt --check`。

### 23.2 `nox project` 或等价验证入口

状态：已完成。

设计方向：

- 评估新增 `nox project check`、`nox check --project` 或文档化脚本。
- 目标是聚合 `check`、`test`、`fmt --check`，不是引入包管理器。
- JSON 输出可以后续再定；第一步先让人类 CLI 可用。

验收标准：

- 项目根和子目录运行行为一致。
- 显式 path 仍然优先，不破坏单文件工作流。
- 文档说明这个入口与普通 `check/test/fmt` 的关系。

建议首批任务：

- 已新增 `nox project check`，作为项目级人类可读验证入口，顺序聚合
  `nox check`、`nox test` 和 `nox fmt --check`。
- 每个子步骤都复用已有 manifest 默认行为；从项目根或子目录运行时都会发现同一个
  `nox.toml`，围绕 manifest main、`modules.source_dirs` 和 `modules.test_dirs` 工作。
- 该入口不做包管理、不接受文件参数、不提供 JSON 输出、不授予额外 runtime permissions；
  单文件或机器可读工作流继续使用 `check/test/fmt` 的显式 path 或 JSON 模式。
- 已补 CLI 集成测试，覆盖 scoreboard 项目根和 `src/` 子目录的成功路径，以及格式化
  不稳定时的失败路径。
- 已更新 `docs/cli.md`、README、`examples/README.md` 和 CHANGELOG。

验证记录（2026-05-21）：

- 已跑 `cargo fmt --all --check`。
- 已跑 `cargo test -p nox project_check --test cli`。
- 已跑 `cargo test -p nox --test cli`。
- 已在 `examples/projects/scoreboard` 和 `examples/projects/scoreboard/src` 下分别跑
  `cargo run -p nox -- project check`。

### 23.3 LSP 项目体验复审

状态：已完成。

完成内容：

- 已用 `examples/projects/scoreboard` 覆盖 LSP diagnostics、`name.` completion 和
  formatting，验证打开 `src/main.nox` 时能通过父级 `nox.toml` 的 `modules.source_dirs`
  解析 `scoring.nox` / `labels.nox`。
- 已保留 didChange import 方和被 import 方 diagnostics 回归测试。
- 已在 `docs/cli.md` 写清当前支持的 request、manifest import 搜索、open-document
  overlay，以及不支持 workspace symbol、rename、go-to-definition、watch mode / daemon。
- 未实现后台 watch，项目体验问题限定在当前 stdio LSP 能力内收口。

验证记录（2026-05-21）：

- 已跑 `cargo fmt --all --check`。
- 已跑 `cargo test -p nox lsp_ --test cli`。
- 已跑 `cargo test -p nox --test cli`。
- 已跑 `cargo test -p nox`。
- 已跑 `git diff --check HEAD`。
- 已跑 Markdown 本地链接检查。

## 阶段 24：标准库静态模块化第一批

目标：兑现 20.3 的命名策略，让新增标准库能力从 `std/*` 静态模块开始，而不是继续扩大全局函数。

### 24.1 `std/*` 模块加载设计

状态：已完成。

完成内容：

- 已写 ADR 0013，决定 `std/*` 采用 `nox` runtime 安装的虚拟内置模块，不属于
  `nox_core`，也不进入普通文件 import 搜索路径。
- 规范路径使用 `std/fs.nox`、`std/env.nox`、`std/time.nox`，暂不推荐无后缀短名。
- 第一批模块只覆盖已有全局能力：`std/fs.nox`、`std/env.nox`、`std/time.nox`。
- 明确 import 不授予 runtime permission；权限仍在调用 `fs.read_text`、
  `env.get`、`time.sleep_ms` 等函数时检查。
- 已同步 ADR 索引、runtime 文档和 CHANGELOG。

验证记录（2026-05-21）：

- 已跑 Markdown 本地链接检查。
- 已跑 `git diff --check HEAD`。
- 已跑 `cargo run -p nox -- project check`（在 `examples/projects/scoreboard`）。
- 已跑 `cargo run -p nox -- check examples/projects/scoreboard/src/runtime_info.nox`。
- 已跑 `cargo run -p nox -- fmt --check examples/projects/scoreboard/src/runtime_info.nox`。

### 24.2 文件、环境、时间模块迁移表面

状态：已完成。

完成内容：

- 已按 ADR 0013 实现 runtime 虚拟模块：`std/fs.nox`、`std/env.nox`、`std/time.nox`。
- `std/fs.nox` 导出 `read_text`、`exists`、`write_text`，复用已有 filesystem 权限检查。
- `std/env.nox` 导出 `get`、`list`，复用已有 environment 权限检查。
- `std/time.nox` 导出 `sleep_ms`，复用已有 timer 权限检查。
- 旧全局函数继续保留；新增模块通过内部 host symbol 包装，不改变 `nox_core`。
- 未实现 warning 机制，旧全局函数迁移提示留给诊断等级设计。
- 已给 scoreboard sample project 增加 `src/runtime_info.nox`，覆盖 `std/*` 推荐写法。
- 已更新 README、docs/runtime.md、docs/cli.md、examples/README.md 和 CHANGELOG。

验证记录（2026-05-21）：

- 已跑 `cargo fmt --all --check`。
- 已跑 `cargo test -p nox std_ --test cli`。
- 已跑 `cargo test -p nox --test cli`。
- 已跑 `cargo test -p nox runtime_resolves_std_fs_module --lib`。
- 已跑 `cargo test -p nox std_module --lib`。
- 已跑 `cargo test -p nox`。
- 已跑 Markdown 本地链接检查。
- 已跑 `git diff --check HEAD`。

### 24.3 标准库文档和迁移窗口

状态：已完成。

完成内容：

- 已在 `docs/runtime.md` 将 `std/fs.nox`、`std/env.nox`、`std/time.nox` 作为推荐表面，
  并列出成员、所需权限和权限不足时的诊断 message。
- 已写清 `std/*` import 不授予权限，未知 `std/*` 模块返回 `module.not-found`，
  缺失成员返回 `module.member-not-found`。
- 已明确旧全局函数至少保留到 v0.0.4 完成，当前不发 warning，formatter 不自动改写。
- 已保持 `docs/runtime.md` 为标准库入口，并从 `docs/language-v0.md` 链接过去。
- 已在 `docs/diagnostics.md` 增补 `module.not-found` 稳定 code。
- 已在 `docs/release-checklist.md` 增加 scoreboard project / `runtime_info.nox` 的
  std module smoke。
- 已确认 CHANGELOG 和 examples 记录推荐表面变化和兼容窗口。

验证记录（2026-05-21）：

- 已跑 Markdown 本地链接检查。
- 已跑 `git diff --check HEAD`。

## 阶段 25：嵌入宿主回归和长期运行风险

目标：让宿主能更有信心地长期持有 engine/session/runtime，并能定位 host 边界错误。

### 25.1 Embedding regression suite

状态：已完成。

完成内容：

- 已新增 `scripts/embedding-regression.sh`，串联 `nox_core` Rust API 测试、`nox`
  默认 runtime 的 Session/Runtime 组合测试、`nox_core` 构建和 C embedding smoke。
- C smoke 已覆盖 version、userdata fallback、callback error、last_error/clear_error、
  string free、array/map/record handle free。
- 已新增高层 Rust 测试，覆盖 `Session` 与 `Runtime` 同时存在时，runtime std module
  权限不会隐式进入 `Session`。
- 已更新 `docs/embedding.md`、`docs/development.md` 和 `docs/release-checklist.md`，
  使 release 前宿主回归有一条可复制命令。
- 已检查 C header 示例路径继续指向 `crates/nox_core/include/nox_core.h`，C smoke 仍使用
  真实 header 编译。

验证记录（2026-05-21）：

- 已跑 `cargo fmt --all --check`。
- 已跑 `cargo test -p nox session_and_runtime --lib`。
- 已跑 `scripts/embedding-regression.sh`。
- 已跑 Markdown 本地链接检查。
- 已跑 `git diff --check HEAD`。

### 25.2 取消、超时和预算语义复审

状态：已完成。

设计方向：

- 统一 instruction budget、runtime async task、timer、host callback 长耗时之间的边界。
- 明确取消只能取消 VM 执行还是也影响 runtime pending task。
- 不引入多线程 event loop，除非另开设计。

验收标准：

- docs/runtime.md 和 docs/embedding.md 说明取消/预算语义。
- 测试覆盖 budget exhaustion 后 engine/session 是否仍可复用。
- async pending task 在取消/错误后没有泄漏。

建议首批任务：

- 已补 engine/session 在 instruction budget exhaustion 后重置预算继续复用的测试。
- 已补 runtime eval 失败时清理本次调用新建 pending async task、但保留既有 pending task 的组合测试。
- 已在 `docs/runtime.md` 和 `docs/embedding.md` 说明 VM budget、host callback、阻塞 timer、
  runtime async task 和无多线程 event loop 的边界。
- 验证：`cargo fmt --all --check`、`cargo test -p nox_core budget`、
  `cargo test -p nox async_task --lib`、`cargo test -p nox_core`、`cargo test -p nox`、
  `cargo test --all`、`cargo clippy --all-targets -- -D warnings`、Markdown 链接检查、
  `git diff --check HEAD`。

### 25.3 Heap 和 host-held value 压力扩展

状态：已完成。

设计方向：

- 在 17.3/21.3 基础上补更接近宿主场景的长循环压力。
- 继续保持 `Rc + Weak`，除非测试暴露无法接受的泄漏或性能问题。
- 避免用不稳定时间阈值做硬门禁。

验收标准：

- 有回归测试覆盖反复 eval/check、host-held value 创建释放、C handle free。
- heap live count 或等价观测能证明释放路径生效。
- docs/heap-design.md 更新压力边界。

建议首批任务：

- 已补 `repeated_eval_and_check_collect_transient_heap_values`：250 次嵌套 record/array/map/string
  的 `check` 和 `eval`，用 `heap_object_count()` 证明 transient heap 值可回收。
- 已补 `repeated_host_callback_returns_do_not_accumulate_heap_values`：200 次 host callback
  返回 string/array/map 后由脚本消费，收集后 heap count 回到 0。
- 已补 `repeated_c_abi_handle_free_collects_nested_heap_values`：120 次 C record handle 创建/free，
  验证 nested string/array/map 随 explicit free 释放，不引入 registry。
- 已更新 `docs/heap-design.md` 的压力测试覆盖和 v0.0.4-beta 复审结论。
- 验证：`cargo fmt --all --check`、`cargo test -p nox_core heap --lib`、
  `cargo test -p nox_core host_callback --lib`、`cargo test --all`、
  `cargo clippy --all-targets -- -D warnings`、Markdown 链接检查、`git diff --check HEAD`。

## 阶段 26：可靠性、观测和诊断覆盖

目标：把“没 panic、能解释错误、能观察趋势”变成 release 前的常规证据。

### 26.1 Robustness corpus 扩展

状态：已完成。

设计方向：

- 扩展 `examples/malformed/`，覆盖 parser recovery、formatter、module resolver、manifest 和 LSP 输入。
- 所有发现先进入固定 corpus，不急于接随机 fuzz。
- malformed fixture 要分清“语法错误仍可 fmt 失败”和“静态错误可 fmt 成功”。

验收标准：

- `scripts/robustness-smoke.sh` 覆盖新增 corpus。
- parser/formatter/LSP 不 panic。
- docs/development.md 说明如何添加新 corpus case。

建议首批任务：

- 已增加 manifest 缺字段/未知权限、namespace missing member、深层 record/map 类型错误、
  LSP 半截源码等固定 corpus。
- 已扩展 `scripts/robustness-smoke.sh`：覆盖新增 corpus，继续输出 `case command status result`
  TSV；语法/词法错误返回 1，静态/module 错误 `check=1` 且 `fmt=0`，manifest 配置错误返回 2，
  LSP stdio session 返回 0。
- 已更新 `docs/development.md` 和 `examples/README.md`，说明新增 corpus 的分类和维护方式。
- 验证：`cargo fmt --all --check`、`scripts/robustness-smoke.sh`、`cargo test --all`、
  `cargo clippy --all-targets -- -D warnings`、Markdown 链接检查、`git diff --check HEAD`。

### 26.2 Benchmark 分段和趋势记录

状态：已完成。

设计方向：

- 当前 benchmark 是端到端 CLI real time；下一步评估 parse/typecheck/compile/eval 分段观测。
- 不把性能数字作为 CI 硬门禁。
- 输出格式要稳定，方便后续比较不同提交。

验收标准：

- docs/benchmarks.md 说明端到端与分段数字的差异。
- 如果新增分段命令，必须有 smoke 覆盖。
- benchmark case 覆盖模块加载、容器、递归、循环和 test runner。

建议首批任务：

- 已扩展 `scripts/bench-smoke.sh`：保留 release profile 和端到端 case，同时对 `.nox`
  benchmark 增加 `check`、`compile`、`e2e` 三类稳定 CLI 阶段代理；不新增用户 CLI flag。
- `compile` 代理使用 `inspect-bytecode --compact`，用于趋势观察，不把数字相减成内部阶段耗时。
- benchmark case 继续覆盖递归、循环、容器、模块加载和 `nox test examples/example_test.nox`。
- 已更新 `docs/benchmarks.md` 与 `docs/development.md`，说明端到端和阶段代理数字的差异、
  release profile 边界和非硬门禁性质。
- 验证：`cargo fmt --all --check`、`scripts/bench-smoke.sh`、`cargo test --all`、
  `cargo clippy --all-targets -- -D warnings`、Markdown 链接检查、`git diff --check HEAD`。

### 26.3 诊断覆盖率审计

状态：已完成。

设计方向：

- 对 `docs/diagnostics.md` 中稳定 code 做反向映射：每个 code 至少有一个结构化测试。
- 减少只断言英文 message 的测试。
- 明确哪些错误仍是通用 `error`，避免假稳定。

验收标准：

- 诊断 code 表中稳定项有测试索引或审计记录。
- JSON 和 LSP 至少覆盖核心 parser/type/module/runtime/test code。
- 文档说明新增 code 的流程。

建议首批任务：

- 已用 `docs/diagnostics.md` 稳定 code 表反查 `crates` 中的 code 设置和测试断言。
- 已补最小结构化断言：`check --json` 覆盖 `parse.expected-token` 和
  `module.member-not-found`，LSP 覆盖 `parse.expected-token`、`module.not-found` 和
  `module.member-not-found`，`test --json` 覆盖 `runtime.division-by-zero`。
- 已更新 `docs/diagnostics.md`：稳定 code 表增加覆盖索引，说明新增稳定 code 流程，
  并明确普通文件 loader、manifest、host callback、权限、文件系统、网络等仍可保留通用 `error`。
- 验证：`cargo test -p nox check_json --test cli`、
  `cargo test -p nox lsp_reports_module_not_found_code --test cli`、
  `cargo test -p nox test_json_reports_runtime_diagnostic_code --test cli`、
  `cargo fmt --all --check`、`cargo test --all`、`cargo clippy --all-targets -- -D warnings`、
  Markdown 链接检查、`git diff --check HEAD`。

## 阶段 27：v0.0.4 语言设计闸门

目标：让语言能力扩张由真实使用压力驱动，而不是为了语法完整性提前复杂化。

### 27.1 小型真实项目语言缺口复盘

状态：已完成。

设计方向：

- 基于 23.1 sample project 和 std module 实现过程，记录真实痛点。
- 区分“能用现有语言绕开”和“必须新增语言能力”。
- 不把暂缓项自动推进到实现。

验收标准：

- `docs/language-v0.md` 或 ADR 记录 v0.0.4 语言缺口清单。
- 每个候选能力都有至少一个来自项目/stdlib/embedding 的用例。
- 没有用例的能力继续暂缓。

建议首批任务：

- 已在 `docs/language-v0.md` 增加 v0.0.4 语言缺口复盘，基于 scoreboard sample project、
  `std/*` 模块迁移和 embedding/runtime 边界记录候选能力。
- 结论：`option[T]` / `result[T, E]` 有来自 std/fs、std/env、map lookup、async task 和
  host callback 的真实压力，进入 27.2 二次评估；可变数组、源码级函数类型、动态 std object
  暂无足够证据，不进入 v0.0.4 实现。
- 已明确 namespace import、export 和 `std/*` 虚拟模块已经覆盖当前 sample project 的模块边界痛点。
- 验证：`cargo fmt --all --check`、`cargo test --all`、
  `cargo clippy --all-targets -- -D warnings`、Markdown 链接检查、`git diff --check HEAD`。

### 27.2 错误处理模型二次评估

状态：已完成。

设计方向：

- 只在 std/fs、std/env、async task 和 host callback 场景证明需要时，重启
  `option[T]` / `result[T, E]`。
- 继续禁止隐式 nullable。
- 设计必须覆盖类型语法、构造/解包、控制流收窄、Rust `Value` 和 C ABI 表示。

验收标准：

- 新 ADR 明确继续暂缓还是进入实现。
- 如果进入实现，有 parser/type/VM/API/formatter/LSP/test 的分阶段计划。
- 如果暂缓，runtime 文档继续给出 guard + diagnostic 的推荐模式。

建议首批任务：

- 已新增 `docs/decisions/0014-restart-option-result-design.md`，基于 std/fs、std/env、
  map lookup、async task 和 host callback 的失败模式，决定重启 `option[T]` /
  `result[T, E]` 设计但暂不实现。
- 已明确继续禁止隐式 nullable，不引入异常/try-catch，不在同一批改变现有 runtime API。
- ADR 已给出如后续进入实现时的 parser/type、Value/VM、控制流解包、Rust/C ABI、
  stdlib 迁移、formatter/LSP/test 分阶段计划。
- 已更新 `docs/runtime.md`、`docs/language-v0.md`、`docs/decisions/0009-defer-option-result.md`
  和 ADR 索引，继续保留 guard + diagnostic 推荐模式。
- 验证：`cargo fmt --all --check`、`cargo test --all`、
  `cargo clippy --all-targets -- -D warnings`、Markdown 链接检查、`git diff --check HEAD`。

### 27.3 容器和函数能力二次评估

状态：已完成。

设计方向：

- 可变数组、copy-on-write 数组、slice、函数类型和高阶函数必须分别评估。
- `const` 与容器可变性的语义必须先定。
- C ABI 只读 handle 和 heap 模型不能被隐式破坏。

验收标准：

- ADR 比较至少两种容器增长方案和函数类型方案。
- 明确哪些能力不进入 v0.0.4。
- 如果任何能力进入实现，必须先更新 formatter golden fixture 和负向测试计划。

建议首批任务：

- 已新增 `docs/decisions/0015-defer-container-function-expansion.md`，基于 sample project、
  `std/*` 模块迁移、embedding regression、heap 压力测试和 benchmark/diagnostics 审计，
  决定 v0.0.4 不实现可变数组、slice、copy-on-write helper、源码级函数类型、高阶函数或
  C ABI function handle。
- ADR 已比较原地可变数组、copy-on-write 数组增长 helper、slice、完整源码级函数类型和
  C ABI function handle；结论是数组继续不可变，函数继续不是源码级一等值，heap 保持
  `Rc` + `Weak`，C ABI 复合值 handle 继续只读。
- 已更新 `docs/language-v0.md`、`docs/decisions/0010-defer-mutable-arrays.md`、
  `docs/decisions/0011-defer-function-types.md` 和 ADR 索引，明确 27.3 没有能力进入实现，
  因此无需新增 formatter golden fixture 或负向语法测试。
- 验证：`cargo fmt --all --check`、`cargo test --all`、
  `cargo clippy --all-targets -- -D warnings`、Markdown 链接检查、`git diff --check HEAD`。

## 推荐执行顺序

1. 阶段 31.1：先做诊断契约硬化第二批，优先选择 manifest/project、permission 或 verifier 中
   能形成稳定测试证据的错误族。
2. 阶段 31.2：在 `nox.project-check.v1` 兼容前提下增强失败摘要和 CI 可读性。
3. 阶段 31.3：复审 CLI / LSP 项目边界一致性，用 scoreboard 或最小负向 fixture 补双路径证据。
4. 阶段 31.4：复审 embedding 和权限边界，默认不扩 C ABI，除非前面阶段暴露真实宿主缺口。
5. 阶段 31.5：把已稳定的 v0.0.7 新路径接入 release gate 和本地分发复审。
6. 阶段 31.6：最后单独做 v0.0.7 release-prep commit 和本地 tag。
7. 阶段 15-30 已完成，后续只在发现回归或文档漂移时回补，不再作为主线推进。

## 阶段依赖

必须按依赖推进的事项：

- 15.1、15.2 先于所有 v0.0.3 实现工作。v0.0.2 发布前只允许 release 阻断修复。
- 16.1 先于 16.3。没有稳定 manifest 语义时，不做项目级 `check/test` 默认行为。
- 17.1 先于 19.1。LSP project awareness 复用 session/module graph，不单独发明缓存层。
- 16.2 先于 20.3。如果 stdlib 要模块化命名，必须先确定模块成员访问语义。
- 17.3 和 18.2 先于 21.3。heap 模型复审要基于长期值持有和可变容器的真实压力。
- 19.2 是所有新语法的前置门禁。新语法进入 parser 后必须同步进入 formatter fixture。
- 22.1 先于所有 v0.0.4 实现工作。v0.0.3 发布冻结阶段只允许 release 阻断修复。
- 23.1 先于 23.2、23.3、24.2 和 27.1。真实项目 fixture 是下一轮体验和语言缺口的证据来源。
- 24.1 先于 24.2。没有 std module loader 设计时，不实现 `std/*` wrapper。
- 24.2 先于 24.3。迁移文档必须基于真实已实现模块。
- 25.1 先于 22.3 的最终 release gate 脚本收敛，避免脚本只跑 Cargo gate 而漏宿主边界。
- 27.1 先于 27.2、27.3。语言能力二次评估必须来自真实项目或 stdlib 证据。
- 28.1 先于 28.2。没有 option/result 最小语义和测试计划时，不迁移 stdlib 表面。
- 28.2 先于任何大规模 runtime API 改造。先用一个 stdlib 试点证明错误模型。
- 28.3 可以和 28.4 并行，但本地分发脚本不能依赖未完成的 option/result 实现。
- 29.1 先于 29.2。第二批可恢复 API 必须从真实项目压力出发，避免为了 API 完整性扩面。
- 29.2 和 29.3 先于 29.4。release gate 只接入已经稳定的 API 或项目体验行为。
- 29.4 先于 29.5。没有通过 release gate 和 local dist smoke 时，不做 release-prep 和 tag。
- 30.1 先于 30.3 的诊断输出收紧。项目体验不要继续绑定不稳定或通用 `error` code。
- 30.2 可以在 30.1 后设计，但 async task API 实现前必须先稳定状态 record、id 生命周期和权限边界。
- 30.3 先于 30.5。release gate 只接入已经稳定的项目输出和 LSP 行为。
- 30.4 先于任何 v0.0.6 C ABI 扩展。没有兼容矩阵和 regression，不追加 header 函数。
- 30.5 先于 30.6。没有通过 release gate 和 local dist smoke 时，不做 release-prep 和 tag。
- 31.1 先于 31.2 和 31.3。项目 JSON 和 LSP 复审不要继续绑定不稳定或通用 `error` code。
- 31.2 先于 31.5。release gate 只接入已经稳定的 `nox.project-check.v1` 行为。
- 31.3 先于 31.4。embedding 权限文档要基于 CLI/LSP 项目边界的最终诊断语义。
- 31.4 先于任何 v0.0.7 C ABI 扩展。没有真实宿主用例、兼容矩阵和 C smoke，不追加 header 函数。
- 31.5 先于 31.6。没有通过 release gate 和 local dist smoke 时，不做 release-prep 和 tag。

可以并行推进的事项：

- 15.1 文档审计可以和 15.2 版本/tag 策略并行，但 release commit 必须最后做。
- 18.1、18.2、18.3 都是设计阶段，可以并行写 ADR；实现优先级由 v0.0.3 目标再定。
- 21.1 benchmark 文档化可以在 v0.0.3-alpha 期间启动，只要不把数字当作 CI 硬门禁。
- 19.3 诊断 code 表可以和 16.1、16.3 并行推进，因为它主要收敛测试断言方式。
- 22.2 兼容矩阵可以和 22.1 文档审计并行，但 release 冻结结论必须引用矩阵。
- 23.2 项目验证入口可以和 23.3 LSP 复审并行，只要都复用同一个 sample project。
- 25.3 heap 压力扩展可以和 26.1 robustness corpus 并行，二者分别覆盖宿主生命周期和输入鲁棒性。
- 26.2 benchmark 分段可以和 26.3 diagnostics 审计并行，不互相阻塞。
- 28.3 本地分发 smoke 和 28.4 embedding 文档可以并行推进，因为二者不改变语言语义。
- 29.2 可恢复 API 评估和 29.3 项目/LSP 体验收紧可以并行探索，但最终 gate 要基于已完成项。
- 30.1 诊断 code 审计和 30.4 embedding 文档复审可以并行，但同一错误边界只能有一个提交负责收敛。
- 30.2 async task 状态 API 设计和 30.3 manifest/project 体验审计可以并行，前提是不同时改同一批 CLI/LSP tests。
- 31.1 诊断 code 审计和 31.2 项目 JSON failure detail 设计可以并行探索，但最终测试断言必须使用同一稳定 code。
- 31.3 CLI/LSP 项目边界复审可以和 31.4 embedding 文档预审并行，前提是不同时修改 C header 或 release gate。

## 当前状态看板

| 阶段 | 状态 | 下一步 | 阻塞 |
| --- | --- | --- | --- |
| 15.1 Release candidate 审计 | 已完成 | 无 | 无 |
| 15.2 版本号与 tag 策略 | 已完成 | 无 | 无 |
| 16.1 Manifest 语义升级 | 已完成 | 无 | 无 |
| 16.2 命名空间 import 设计 | 已完成 | 无 | 20.3 依赖已解除 |
| 16.3 项目级检查和测试 | 已完成 | 无 | 无 |
| 17.1 Rust API session/module graph | 已完成 | 无 | 无 |
| 17.2 C ABI 错误与 userdata | 已完成 | 无 | 无 |
| 17.3 Host-held Value 生命周期 | 已完成 | 无 | 无 |
| 18.1 可选值 / Result | 已完成 | 无 | 无 |
| 18.2 可变数组设计 | 已完成 | 无 | 无 |
| 18.3 函数类型边界 | 已完成 | 无 | 无 |
| 19.1 LSP project awareness | 已完成 | 无 | 无 |
| 19.2 Formatter 稳定性 | 已完成 | 无 | 无 |
| 19.3 诊断 code 稳定性 | 已完成 | 无 | 无 |
| 20.1 文件系统能力细分 | 已完成 | 无 | 无 |
| 20.2 Async task 生命周期 | 已完成 | 无 | 无 |
| 20.3 标准库命名和分层 | 已完成 | 无 | 无 |
| 21.1 性能基线自动化 | 已完成 | 无 | 无 |
| 21.2 Fuzz / parser robustness | 已完成 | 无 | 无 |
| 21.3 Heap 模型复审 | 已完成 | 无 | 无 |
| 22.1 v0.0.3 Release candidate 审计 | 已完成 | 无 | 无 |
| 22.2 Rust API / C ABI 兼容矩阵 | 已完成 | 无 | 无 |
| 22.3 发布脚本和本地门禁 | 已完成 | 无 | 无 |
| 23.1 Sample project fixture | 已完成 | 无 | 无 |
| 23.2 项目验证入口 | 已完成 | 无 | 无 |
| 23.3 LSP 项目体验复审 | 已完成 | 无 | 无 |
| 24.1 `std/*` 模块加载设计 | 已完成 | 无 | 无 |
| 24.2 文件/环境/时间模块迁移 | 已完成 | 无 | 无 |
| 24.3 标准库文档和迁移窗口 | 已完成 | 无 | 无 |
| 25.1 Embedding regression suite | 已完成 | 无 | 无 |
| 25.2 取消、超时和预算复审 | 已完成 | 无 | 无 |
| 25.3 Heap/host-held 压力扩展 | 已完成 | 无 | 无 |
| 26.1 Robustness corpus 扩展 | 已完成 | 无 | 无 |
| 26.2 Benchmark 分段和趋势 | 已完成 | 无 | 无 |
| 26.3 诊断覆盖率审计 | 已完成 | 无 | 无 |
| 27.1 真实项目语言缺口复盘 | 已完成 | 无 | 无 |
| 27.2 错误处理模型二次评估 | 已完成 | 无 | 无 |
| 27.3 容器和函数能力二次评估 | 已完成 | 无 | 无 |
| 28.1 `option[T]` / `result[T, E]` 最小语义落地计划 | 已完成 | 无 | 无 |
| 28.2 stdlib 错误模型迁移试点 | 已完成 | 无 | 无 |
| 28.3 本地分发和安装 smoke | 已完成 | 无 | 无 |
| 28.4 Embedding 文档和示例二次打磨 | 已完成 | 无 | 无 |
| 29.1 v0.0.5 真实项目压力设计 | 已完成 | 无 | 无 |
| 29.2 可恢复 API 第二批 | 已完成 | 无 | 无 |
| 29.3 项目和 LSP 体验收紧 | 已完成 | 无 | 无 |
| 29.4 v0.0.5 release gate 和本地分发复审 | 已完成 | 无 | 无 |
| 29.5 v0.0.5 发布收口 | 已完成 | 无 | 无 |
| 30.1 诊断契约硬化第一批 | 已完成 | 无 | 无 |
| 30.2 Async task 可恢复状态 API 评估 | 已完成 | 无 | 无 |
| 30.3 Manifest 和项目体验收紧 | 已完成 | 无 | 无 |
| 30.4 Embedding 兼容矩阵复审 | 已完成 | 无 | 无 |
| 30.5 v0.0.6 Release gate 和本地分发复审 | 已完成 | 无 | 无 |
| 30.6 v0.0.6 发布收口 | 已完成 | 无 | 无 |
| 31.1 诊断契约硬化第二批 | 已完成 | 无 | 无 |
| 31.2 Project check JSON 失败细节增强 | 待启动 | 设计兼容的失败摘要字段和 fixture | 无 |
| 31.3 CLI / LSP 项目边界一致性复审 | 待启动 | 选择 scoreboard 子目录或 manifest 负向场景 | 无 |
| 31.4 Embedding 和权限边界复审 | 待启动 | 复审宿主可见诊断和权限文档 | 依赖 31.3 |
| 31.5 v0.0.7 Release gate 和本地分发复审 | 待启动 | 接入 v0.0.7 新路径 smoke | 依赖 31.1-31.4 |
| 31.6 v0.0.7 发布收口 | 待启动 | 版本号、CHANGELOG、PLAN 和 tag 收口 | 依赖 31.5 |

## v0.0.7 完成定义

v0.0.7 不是把所有暂缓语言能力都实现完才发布。v0.0.7 的最小完成定义是：

- `v0.0.6` 已完成本地 release commit 和 tag，且 release checklist 能复制执行。
- 第二批至少一个高价值错误族从通用 `error` 收敛成稳定 diagnostic code，并有 CLI JSON、LSP 或
  release gate 的可复制证据。
- `nox project check --json` 完成一个兼容增强，让失败路径或项目边界更适合 CI 读取。
- CLI / LSP 项目边界至少完成一次复审或修正，并继续复用 scoreboard fixture。
- embedding 兼容矩阵、C header、Rust example、C smoke、runtime permissions 和 async task 暂缓结论
  与实际行为一致。
- docs/diagnostics.md、docs/cli.md、docs/runtime.md、docs/embedding.md、CHANGELOG、release checklist
  和 PLAN 与实际行为一致。
- `scripts/release-gate.sh` 覆盖 v0.0.7 新增路径，`scripts/local-dist-smoke.sh` 继续通过。
- Rust/C embedding regression、robustness smoke、benchmark smoke 和 Markdown 链接检查通过。

package registry、watch mode、后台 daemon、tracing GC、可变数组、完整函数类型、高阶函数、
动态 std object 都不是 v0.0.7 最小完成项。它们可以以 ADR 或继续暂缓结论进入 v0.0.7 文档，
但不能拖住诊断契约、项目体验、embedding 稳定性和本地可发布主线。

## 风险和控制

- 风险：manifest 升级把简单 CLI 用法复杂化。控制：显式 path 永远优先，单文件脚本继续可跑。
- 风险：session/module graph 过早抽象。控制：只抽 CLI、LSP、宿主 API 已经重复的状态。
- 风险：namespace import 被误实现成动态 object。控制：module member 只存在于静态语义层。
- 风险：runtime 权限声明被误解为自动授权。控制：manifest 只声明/检查，危险能力仍由宿主或 CLI 显式授予。
- 风险：C ABI 兼容被测试遗漏。控制：enum 只末尾追加、函数只末尾新增、release 前跑 C smoke。
- 风险：语言表面扩张压过嵌入目标。控制：18.x 默认先 ADR，未过设计闸门不实现。
- 风险：性能优化在无基线时改变架构。控制：先记录 benchmark 和压力数据，再讨论 heap/VM 改造。
- 风险：v0.0.3 已完成功能没有可复现 release。控制：阶段 22 先做发布冻结和 dry run，再进入 v0.0.4 实现。
- 风险：sample project 变成展示项目而不是回归 fixture。控制：23.1 必须被 CLI/LSP/std module 测试复用。
- 风险：`std/*` 模块化绕过权限模型。控制：导入只提供静态表面，不自动授予 runtime permissions。
- 风险：项目验证入口变成包管理器雏形。控制：23.2 只聚合 check/test/fmt，不做 dependency resolver。
- 风险：embedding regression 只覆盖 happy path。控制：25.1 必须包含 callback error、userdata、handle free 和 version。
- 风险：语言暂缓项被顺手实现。控制：27.x 先从真实项目证据写 ADR，未过闸门不进 parser/type checker。
- 风险：诊断 code 硬化变成一次性大重命名。控制：30.1 只按错误族分批推进，每批都有 CLI JSON 和 LSP 覆盖。
- 风险：async task 可恢复 API 过早暴露半成品状态 record。控制：30.2 先写 ADR 和状态机，不稳定就继续暂缓。
- 风险：项目体验收紧滑向 package manager 或 daemon。控制：30.3 只处理 manifest/project 输出和 LSP 复用，不做 resolver/watch。
- 风险：embedding 兼容矩阵复审顺手扩 C ABI。控制：30.4 默认修文档和 regression，新增 C ABI 必须有宿主用例和 header 末尾追加。
- 风险：v0.0.7 诊断第二批把 message 优化误当作契约稳定。控制：31.1 只把 code、span/source 和 JSON/LSP/release gate 证据作为完成条件。
- 风险：`nox.project-check.v1` 增强破坏 CI 兼容。控制：31.2 只能追加字段或补充语义，不删除/重命名现有字段；破坏式变更必须另起 v2 设计。
- 风险：CLI/LSP 项目边界复审滑向 watcher 或 daemon。控制：31.3 只复用现有 manifest discovery 和 session/module graph。
- 风险：permission diagnostic 被误解为 manifest 自动授权。控制：31.4 明确 manifest 声明只用于检查，危险能力仍由 CLI 或宿主显式授予。
- 风险：release gate 只验证 happy path。控制：31.5 必须至少接入一个 v0.0.7 新增的负向或失败路径 smoke。

## 文档产物清单

阶段推进时优先更新这些文档，不新增重复入口：

- `CHANGELOG.md`：所有对外可见语言、CLI、Rust API、C ABI、文档行为变化。
- `docs/release-checklist.md`：发布命令、版本号、C ABI 兼容检查和 rollback 流程。
- `docs/package-manifest-design.md`：manifest 字段、优先级、错误诊断和项目 root 发现。
- `docs/module-system-design.md`：import/export、namespace import、冲突规则和迁移策略。
- `docs/embedding.md`：Rust/C API、session、userdata、错误传播和值生命周期。
- `docs/runtime.md`：权限、文件系统 allowlist、async task 状态机和 stdlib 能力边界。
- `docs/cli.md`：命令默认行为、退出码、JSON schema、诊断 code 契约。
- `docs/language-v0.md`：已实现语言表面，只记录真实可运行能力。
- `docs/array-design.md`、`docs/heap-design.md`：可变容器和长期值持有的设计结论。
- `docs/benchmarks.md`、`docs/development.md`：benchmark、fuzz/corpus、固定验证命令。

## 计划维护规则

- 完成一个子阶段后，把对应状态改为 `已完成`，并在该阶段下补一行简短验证记录。
- 发现设计方向不成立时，把状态改为 `暂缓`，并写清暂缓原因和重新启动条件。
- 新增阶段必须先说明它为什么不能归入现有阶段，避免计划膨胀。
- 实现代码前如果发现本计划和 `.agents/GOAL.md`、README、ADR 冲突，优先更新设计文档再写代码。
- 每次 release 或 tag 后，在文件顶部更新日期，并把“当前基线”改成新发布后的事实。
- 本文件不记录临时命令输出；验证命令失败时，在对应 PR、commit message 或专门记录中保存细节。

## 每批次固定验证

每完成一个可提交批次，至少执行：

- `cargo fmt --all --check`
- `cargo test --all`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check HEAD`
- 相关 CLI smoke：
  - `cargo run -p nox -- run examples/hello.nox`
  - `cargo run -p nox -- check examples/hello.nox`
  - `cargo run -p nox -- check --json examples/type-error.nox`
  - `cargo run -p nox -- test examples/example_test.nox`
  - `cargo run -p nox -- test --json examples/example_test.nox`
  - `cargo run -p nox -- fmt examples/hello.nox`
  - `cargo run -p nox -- inspect-bytecode --compact examples/hello.nox`
- C ABI 或 header 改动时运行 C embedding smoke：
  - `cargo build -p nox_core`
  - `cc -Icrates/nox_core/include examples/embed/c_embedding.c -Ltarget/debug -lnox_core -Wl,-rpath,target/debug -o /tmp/nox_c_embedding`
  - `/tmp/nox_c_embedding`
- 文档改动时运行本地 Markdown 链接检查。

## 暂不做

- JavaScript/Node.js 兼容。
- JIT。
- 浏览器 API。
- 动态 object / prototype。
- 默认授予脚本网络、环境、文件写入或 shell 执行能力。
- 包 registry。
- 在可变数组语义确定前实现 `push`、元素赋值或切片。
- 在明确增量检查、watch 和 daemon 边界前做 watch mode 或后台 daemon。
