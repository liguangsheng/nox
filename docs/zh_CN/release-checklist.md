# Release Checklist

本文记录准备一个 Nox 本地版本 checkpoint 时手动执行的步骤。正式发布前不自动发布，
所有动作都明确写在这里，避免遗漏。每次本地版本收口时复制下方"准备"和"发布"两个
checklist 到 issue/PR 中逐项勾选。

## 版本号约定

- 本地开发阶段使用 `0.0.x`：主版本号和次版本号都锁定为 0。
- 修订号（`0.0.x → 0.0.x+1`）在以下场景递增：
  - 引入新的语言特性（v0.0.2 路径中已经发生）。
  - 改动对外可见的 CLI JSON schema、Rust API 或 C ABI（即使是兼容扩展）。
  - 累积足够数量的修复或小改进，希望打 tag。
- 不使用形如 `0.N.0` 的次版本号推进本地开发版本；正式对外发布前次版本号保持 0。
- 破坏性变更原则上推迟到 v1.0.0；如果本地开发阶段确需破坏，必须在 CHANGELOG 和
  ADR 中明确说明。

`Cargo.toml`（workspace 根）的 `[workspace.package].version` 是唯一发布版本号
来源；`crates/nox` 和 `crates/nox_core` 都通过 `version.workspace = true` 继承。

## 当前本地 checkpoint 状态

当前 checkout 的 Cargo workspace 版本、`nox --version` 和 CHANGELOG 最新已发布节均为
`0.0.5`。本仓库已经配置 `origin` remote；release 收口时必须同时核对本地 tag、远端 tag
和 GitHub Release，不能只看本地 gate。

下一轮候选版本从 `v0.0.5` 开始。进入 `v0.0.5` release candidate 审计时，审计批次只验证当前
`main` 上的候选能力，不改版本号、不切 CHANGELOG 标题、不打 tag、不 push。实际发布时仍需要单独
release-prep commit。

在下一轮 release-prep commit 之前，一致状态应当是：

- `[workspace.package].version` 仍是上一个已准备发布版本，例如 `0.0.4`。
- `CHANGELOG.md` 的 `[未发布]` 节完整描述下一轮候选变更。
- `nox_core_version()` 继续来自 `CARGO_PKG_VERSION`，因此 dry run 输出仍匹配当前 Cargo 版本；
  只有 release-prep commit 才会变成下一轮目标版本。
- C header 中已有 enum 数值、公开函数签名和 ownership 注释没有破坏性调整。

release candidate dry run 在 `main` 上执行本地 release gate：

```sh
scripts/release-gate.sh
```

该脚本只做本地验证，不 push、不打 tag、不发布 GitHub Release 或外部资产。输出以
`release gate: <name>` 标出当前 gate；失败时 shell 会停在对应 gate，便于把最后一个
gate 名称和错误输出粘贴到 release PR 或 tag message。

production release 终验还要运行：

```sh
NOX_RELEASE_CI_EVIDENCE=<CI run URL or id> scripts/release-audit.sh
```

`scripts/release-audit.sh` 不构建外部资产、不 push、不打 tag；它只审计当前 checkout 是否已经具备
production release 的身份和证据。该脚本会检查 Cargo/CLI/CHANGELOG 版本、工作树是否处在 release
commit、`vX.Y.Z` tag 是否指向 HEAD、git remote 是否存在、是否提供远端 CI evidence、正式文档是否链接
内部 handoff 文件，以及 rollback/release 说明是否存在。无 remote、无 tag、无 CI evidence 或还有源码
diff 时，脚本必须失败；这表示只能继续作为 checkpoint 或 release-candidate dry run，不能称 production
release。

本地 `scripts/release-gate.sh` 会用 `NOX_RELEASE_AUDIT_EXPECT_BLOCKED=1` 跑一次 blocker smoke，确认
checkpoint 状态下的缺失证据会被脚本识别出来。正式 production release 终验必须使用上面的命令，不得设置
`NOX_RELEASE_AUDIT_EXPECT_BLOCKED`。

`scripts/release-gate.sh` 覆盖：

- `cargo fmt --all --check`、`cargo test --all`、`cargo clippy --all-targets -- -D warnings`。
- CLI smoke：`--version`、`run`、`check`、`check --json`、`test`、`test --json`、`fmt`、
  `fmt --check`、`inspect-bytecode`、相对 module-not-found JSON diagnostic 和 `map_get`
  示例/bytecode smoke。
- `examples/projects/scoreboard` 的 `project check` / `project check --json`、`test --json`、
  `fmt --check`，以及 `runtime_info.nox` 的 `std/fs.nox`、`std/env.nox`、`std/time.nox` 模块 smoke。
- module dependency lockfile guardrail：所有 tracked `nox.toml` 如果声明 `[dependencies]`，
  同目录必须提交 `nox.lock`；`project check` 还会校验 lockfile source/pin drift。
- module ecosystem regression：显式覆盖 project check lockfile JSON、`nox fetch`
  offline/cache、external import cache/hash mismatch，以及集成式 `nox lsp` external import
  diagnostics。
- compatibility golden：显式覆盖 parser AST 形状、CLI diagnostic JSON、LSP diagnostic JSON、
  `nox doc` 输出、project lockfile JSON、host-metadata API JSON、C ABI enum 数值和 async
  Rust API task 行为。
- Cargo 集成测试覆盖 LSP builtin completion，确保 `map_get` 等内建函数可被发现；同时覆盖
  `nox.project-check.v1` 项目 JSON summary。
- `scripts/embedding-regression.sh`，覆盖 Rust API、默认 runtime、Rust embedding 示例和
  C ABI smoke；以及 `scripts/robustness-smoke.sh` 和 `scripts/bench-smoke.sh`。
- 本地 Markdown 链接检查和 `git diff --check HEAD`。

Benchmark smoke 只要求 case 成功和 tab-separated 输出格式稳定；数字用于同机前后对比，
不作为硬阈值。

## 准备 checklist

发布前在 `main` 上完成：

- [ ] `scripts/release-gate.sh` 通过；该 gate 已覆盖 Cargo、CLI smoke、scoreboard project、
      compatibility golden、embedding regression、robustness smoke、benchmark smoke、
      Markdown 链接检查和 `git diff --check HEAD`。
- [ ] 如本次改动涉及 LSP 协议或 editor 行为，额外手动跑一次 `nox lsp` 初始化/关闭 smoke；
      malformed corpus 的半截源码 LSP 回归已经包含在 release gate 的 robustness smoke 中。
- [ ] 如需要验证本地分发产物，运行 `scripts/local-dist-smoke.sh`；该脚本只构建并检查本地
      安装目录，并会运行 hello、`map_get` 示例和 scoreboard `project check --json`；
      不 push、不打 tag、不创建 GitHub Release。
- [ ] 如本次 release 要包含非 host CLI 资产，运行 `scripts/cross-cli-smoke.sh` 或确认
      GitHub Actions 中的 `Cross CLI smoke (x86_64 musl)` 已在 release commit 上通过。
      当前只承诺 `x86_64-unknown-linux-musl` 的 CLI-only 目标，不承诺该目标的嵌入式 SDK。
- [ ] CI（GitHub Actions）在最新 commit 上 green。
- [ ] `NOX_RELEASE_CI_EVIDENCE=<CI run URL or id> scripts/release-audit.sh` 通过，证明当前
      release commit、tag、remote、CI evidence、CHANGELOG、正式文档边界和 rollback 说明互相一致。
- [ ] CHANGELOG 的"未发布"节内容完整，没有遗漏的对外可见变更。
- [ ] `nox_core.h` 与代码生成的常量、枚举值一致：检查 `NoxCoreStatus`、
      `NoxCoreValueKind` 等数值未被无意改动。
- [ ] 文档中的版本号、命令示例与即将发布的版本匹配。

## 发布 checklist

确认准备项都完成后，按顺序执行：

1. [ ] 在 `Cargo.toml` 中把 `[workspace.package].version` 改成目标版本，例如
       `0.0.5`。推荐运行
       `scripts/prepare-release-version.sh --check-only 0.0.5 YYYY-MM-DD` 先做只读锚点预检，再运行
       `scripts/prepare-release-version.sh 0.0.5 YYYY-MM-DD` 生成 release-prep diff；该脚本只改
       版本身份文件并运行 `cargo build` 更新 `Cargo.lock`，不 commit、不 tag、不 push、不上传
       GitHub Release。真实 release-prep 前先运行 `scripts/release-prep-dry-run.sh`，在临时副本里
       验证同一版本切换、cutover readiness 和 release notes extraction，不修改当前 checkout。
2. [ ] 确认 CHANGELOG 的"未发布"标题已经变成目标版本标题，例如
       `## [0.0.5] — YYYY-MM-DD`，并在文件顶部重新开了新的"未发布"节，留给下一轮变更。
3. [ ] 创建一条单独的 release commit，例如 `Prepare v0.0.5 release`。提交内容只包含
       版本号 bump、`Cargo.lock` 更新、CHANGELOG 标题切换，以及 README / docs 中当前生产版本
       口径同步。
4. [ ] 打 tag，例如 `git tag v0.0.5`。tag 名格式固定为 `vMAJOR.MINOR.PATCH`。
5. [ ] 把 release commit 和 tag 都推到 origin：
       `git push origin main && git push origin vMAJOR.MINOR.PATCH`。
6. [ ] 在 GitHub Releases 中基于 tag 创建 release，用
       `NOX_RELEASE_VERSION=0.0.5 scripts/release-notes.sh` 生成 release notes；
       不要重新组织文字，让 CHANGELOG 是单一来源。在执行任何已授权的 commit、tag、push、
       资产上传或严格审计前，先用 `scripts/release-command-plan.sh` 打印完整 Phase 77 命令顺序。
       release-prep commit 后和资产生成后，再用 `scripts/release-evidence-report.sh` 汇总
       cutover status JSON、toolchain status JSON、必需资产清单和命令计划，作为可审阅证据。
7. [ ] **必做**：先跑 `scripts/local-dist-smoke.sh` 确认本地分发产物可用；然后跑
       `scripts/release-toolchain-status.sh` 确认 release asset manifest 需要的本地 Rust target
       已安装，尤其是 CLI-only 的 `x86_64-unknown-linux-musl` target；再跑
       `scripts/build-release-assets.sh`（无参数默认使用当前 Cargo 版本对应的 tag；也可用
       `NOX_RELEASE_TAG=vX.Y.Z` 明确指定 tag；如需非默认输出目录，使用
       `NOX_RELEASE_ASSET_DIR=/tmp/nox-release-assets-vX.Y.Z`，并在 upload plan 中复用同一值）。
       该脚本在隔离 worktree 上 release build，默认构建当前 Rust host triple，并按每个完整
       SDK target 产出四个文件到 `/tmp/nox-release-assets-vX.Y.Z/`：
       - `nox-cli-vX.Y.Z-<target-triple>.tar.gz`（`bin/nox` + `examples/`，不含 embed/）
       - `nox-cli-vX.Y.Z-<target-triple>.sha256`
       - `nox-embed-vX.Y.Z-<target-triple>.tar.gz`（`lib/libnox_core.so` + `include/nox_core.h` + embed C 示例 + 项目 README/CHANGELOG）
       - `nox-embed-vX.Y.Z-<target-triple>.sha256`

       Inspect 内容后先运行 `NOX_RELEASE_ASSET_DIR=/tmp/nox-release-assets-vX.Y.Z scripts/release-upload-plan.sh`
       生成 `gh release upload` 命令，再人工执行输出的命令上传。每个 tarball 必须有对应
       `.sha256` 与之一并上传。当前完整 SDK 仅承诺 `x86_64-unknown-linux-gnu`；其他完整
       SDK 目标三元组 best-effort（用
       `TARGET_TRIPLES="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu ..."` 环境变量启用矩阵），
       且只有在目标 toolchain 和 C ABI smoke 覆盖就绪后才纳入正式承诺。

       对已通过 CI smoke 但尚未承诺 C ABI 的 CLI-only 目标，用
       `CLI_ONLY_TARGET_TRIPLES="x86_64-unknown-linux-musl" scripts/build-release-assets.sh`
       额外生成 `nox-cli-vX.Y.Z-x86_64-unknown-linux-musl.*`。如果只是验证 CLI-only 产物路径，
       可以用 `TARGET_TRIPLES="" CLI_ONLY_TARGET_TRIPLES="x86_64-unknown-linux-musl"`
       跳过完整 SDK 构建。`x86_64-unknown-linux-musl` 在当前规划中只表示 CLI 资产承诺；
       不上传 `nox-embed-...-x86_64-unknown-linux-musl`。本步骤不能跳过——release 缺少二进制
       资产等于下游用户只能从源码构建。
8. [ ] 发布后用 `NOX_RELEASE_CI_EVIDENCE=<CI run URL> scripts/release-audit.sh` 复跑一次终验，
       期望输出 `GOAL implementation: ACHIEVED on vX.Y.Z`；同时确认文档中的版本号、命令示例和
       CHANGELOG 版本节仍匹配。

## crates.io 发布预检

本 release 线暂缓 crates.io 发布。模块生态采用 pinned GitHub / git URL，而不是 package
registry；Rust crate 当前也没有可由本项目直接承诺的 registry 名称：`nox` 已被其他项目占用，
crates.io 会把 `nox_core` 解析到已有的 `nox-core` crate。当前支持的分发路径仍是 GitHub tag
安装、release tarball 和源码 checkout。

未来如果重新打开 registry 发布，必须记住 crates.io 已发布版本不能覆盖。发布 `nox_core` 或
`nox` 前必须确认：

- [ ] 两个 crate 都有完整 package metadata：description、repository、readme、keywords、
      categories、license、version。
- [ ] 确认两个 crate 的 registry 名称可用或所有权已解决。当前 `nox` 和 `nox_core`
      都不是本项目可直接发布承诺的 package name。
- [ ] `nox` 依赖 `nox_core` 时同时写明本地 `path` 和精确 workspace version，确保本地开发与
      registry 发布解析到同一组公开 API。
- [ ] 重新审计 SemVer 风险：Rust API、C ABI、CLI JSON、diagnostic code、manifest 行为和
      已文档化权限。任何破坏性公共变更都必须在发布 crate 前写入 CHANGELOG 和 release notes。
- [ ] 依次运行 `cargo publish --dry-run -p nox_core`、
      `cargo publish --dry-run -p nox`。未提交工作树中只能用 `--allow-dirty` 作为预检证据。
      如果 `nox_core` 尚未以已解决的 registry 名称实际发布，`nox` dry-run 因 registry
      依赖缺失而失败是预期结果；必须等 core crate 名称和 CLI package name 问题都解决后重跑。

## C ABI 兼容检查

每次发布前手动确认 `crates/nox_core/include/nox_core.h` 的兼容性：

- [ ] 没有改动现有 enum 数值（删除 / 重排会破坏 ABI）。
- [ ] 没有改动现有函数签名（参数类型 / 数量 / 返回类型）。
- [ ] 新增 enum 值放在末尾；新增函数放在 header 末尾。
- [ ] 与 README / embedding.md 中描述的 host callback 生命周期一致。

破坏 ABI 的变更必须先升级 SemVer 主版本号（v0.0.x → v1.0.0 或更高），并在 CHANGELOG
和 ADR 中说明。

## 量化指标附录

production release 终验除上面的"准备 checklist"和"发布 checklist"之外，还要核对一组生产级长期目标的
量化指标。指标的完整定义（包括度量命令、阈值与冻结批次）由项目内部 agent 计划维护，本附录列出
release-time 必跑项：

- 小型（PLAN 完成定义第 10 项）：release CLI 二进制大小、`libnox_core` 动态库大小、workspace 第三方
  运行时依赖数（应为 0）、Rust 源码 LOC 趋势记录。具体命令见 PLAN 映射表"小型"子表；阈值由 `P8.3`
  冻结，未冻结期间只记录测量值。
- 快速（第 11 项）：`tests/benchmarks/` 下 `bench-fib`、`bench-loop`、`bench-containers`、
  `bench-modules`、`bench-lambda`、`bench-host-capabilities` 的单次 wall time；测量取
  release 构建、空闲环境、连续 N 次最小值。阈值由 `P8.4` 冻结。
- 宿主友好（第 12 项前半）：Rust 嵌入示例与 C 嵌入示例从零跑通时间；C ABI header↔library symbol diff
  与 enum 数值稳定性（已在 `P4.2`/`P4.3` 落地）。阈值由 `P8.5` 冻结。
- 实用（第 12 项后半）：标准库 fs/env/time/net/async/math/string/json/csv/tsv/array/map/option/result 最小入口存在；CLI 子命令
  `run`/`check`/`test`/`fmt`/`inspect-bytecode`/`project`/`lsp` 全部 `--help` 返回 0；至少一个非
  scoreboard 的真实生产场景示例项目可跑通；README 零上下文快速开始命令全部成功。阈值由 `P8.5` 冻结。
- 产品形态非回归（第 9 项）：parser/typecheck/bytecode/runtime/embedding 既有 cargo test 集合通过；
  CLI 子命令存在。阈值由 `P8.2` 冻结为 release gate 综合断言。
- 暂缓项守护（第 13 项）：项目暂缓项清单（由内部 agent 计划维护）与上一 release tag 之间无静默 diff；
  公开 API/CLI 不出现暂缓项关键词（mutable array、slice type、closure、higher-order、watch mode、
  incremental typecheck、tracing gc、package registry）的未文档化命中。由 `P8.6` 在
  release-audit 中执行。

阈值列未冻结的指标暂时只测量并记录；批次 `P8.6` 完成后 `scripts/release-audit.sh` 会综合本附录全部项，
直接输出 production release / 未达到判定。

## 回滚

如果发布后发现严重问题：

1. 保留已经发布的 git tag 和 release commit；不要 `git push --force` 覆盖已发布 tag，
   下游可能已经拉取，强推会让不同环境状态分裂。
2. 在 GitHub Release 页面把问题版本标记为 withdrawn / deprecated，并在 release notes
   顶部写清影响范围、建议动作和替代版本。不要删除 release，除非包含凭据或法律必须删除的资产。
3. 立即在 README / CHANGELOG 中标注该版本已撤回。CHANGELOG 保留原版本节，在顶部或
   新 hotfix 节说明"撤回 v0.0.y"和原因摘要。
4. 从最后一个健康 tag 切 hotfix 分支或直接在 `main` 上修复，发布 `0.0.y+1`。hotfix
   CHANGELOG 必须包含：受影响版本、修复内容、是否破坏兼容、下游升级命令或重新构建步骤。
5. 如果问题只影响文档或 release notes，不改代码时仍发布说明更新；如果已发布二进制资产错误，
   新版本必须重新跑 `scripts/release-gate.sh` 和 `scripts/local-dist-smoke.sh`，并替换为新的 tag/资产。
6. 通知下游升级路径：推荐版本、需要跳过的版本、是否需要清理缓存或重新生成 C header/bindings。
   对 C ABI 或 CLI JSON 破坏，明确列出受影响 symbol/schema/code。
