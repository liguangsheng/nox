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

## v0.0.6 release candidate dry run

v0.0.6 当前仍处于 release candidate 审计阶段。审计批次只验证当前 `main` 上的
候选能力，不改版本号、不切 CHANGELOG 标题、不打 tag、不 push。实际发布时仍需要
单独 release-prep commit。

在 release-prep commit 之前，当前一致状态应当是：

- `[workspace.package].version` 仍是上一个已准备发布版本，例如 `0.0.5`。
- `CHANGELOG.md` 的 `[未发布]` 节完整描述 v0.0.6 候选变更。
- `nox_core_version()` 继续来自 `CARGO_PKG_VERSION`，因此 dry run 输出仍匹配当前
  Cargo 版本；只有 release-prep commit 才会变成 `0.0.6`。
- C header 中已有 enum 数值、公开函数签名和 ownership 注释没有破坏性调整。

v0.0.6 dry run 在 `main` 上执行本地 release gate：

```sh
scripts/release-gate.sh
```

该脚本只做本地验证，不 push、不打 tag、不发布 GitHub Release 或外部资产。输出以
`release gate: <name>` 标出当前 gate；失败时 shell 会停在对应 gate，便于把最后一个
gate 名称和错误输出粘贴到 release PR 或 tag message。

`scripts/release-gate.sh` 覆盖：

- `cargo fmt --all --check`、`cargo test --all`、`cargo clippy --all-targets -- -D warnings`。
- CLI smoke：`--version`、`run`、`check`、`check --json`、`test`、`test --json`、`fmt`、
  `fmt --check`、`inspect-bytecode`、相对 module-not-found JSON diagnostic 和 `map_get`
  示例/bytecode smoke。
- `examples/projects/scoreboard` 的 `project check` / `project check --json`，以及 `runtime_info.nox` 的
  `std/fs.nox`、`std/env.nox`、`std/time.nox` 模块 smoke。
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
      embedding regression、robustness smoke、benchmark smoke、Markdown 链接检查和
      `git diff --check HEAD`。
- [ ] 如本次改动涉及 LSP 协议或 editor 行为，额外手动跑一次 `nox lsp` 初始化/关闭 smoke；
      malformed corpus 的半截源码 LSP 回归已经包含在 release gate 的 robustness smoke 中。
- [ ] 如需要验证本地分发产物，运行 `scripts/local-dist-smoke.sh`；该脚本只构建并检查本地
      安装目录，并会运行 hello、`map_get` 示例和 scoreboard `project check --json`；
      不 push、不打 tag、不创建 GitHub Release。
- [ ] CI（GitHub Actions）在最新 commit 上 green。
- [ ] CHANGELOG 的"未发布"节内容完整，没有遗漏的对外可见变更。
- [ ] `nox_core.h` 与代码生成的常量、枚举值一致：检查 `NoxCoreStatus`、
      `NoxCoreValueKind` 等数值未被无意改动。
- [ ] 文档中的版本号、命令示例与即将发布的版本匹配。

## 发布 checklist

确认准备项都完成后，按顺序执行：

1. [ ] 在 `Cargo.toml` 中把 `[workspace.package].version` 改成目标版本，例如
       `0.0.6`。运行 `cargo build` 让 `Cargo.lock` 更新。
2. [ ] 把 CHANGELOG 的"未发布"标题改为 `## [0.0.6] — YYYY-MM-DD`，并在文件
       顶部再开一个新的"未发布"节，留给下一轮变更。
3. [ ] 创建一条单独的 release commit：`Prepare v0.0.6 release`。提交内容只包含
       版本号 bump、`Cargo.lock` 更新和 CHANGELOG 标题切换。
4. [ ] 打 tag：`git tag v0.0.6`。tag 名格式固定为 `vMAJOR.MINOR.PATCH`。
5. [ ] 把 release commit 和 tag 都推到 origin：
       `git push origin main && git push origin v0.0.6`。
6. [ ] 在 GitHub Releases 中基于 tag 创建 release，把 CHANGELOG 对应版本节的
       内容粘进去；不要重新组织文字，让 CHANGELOG 是单一来源。
7. [ ] 如果需要构建二进制（CLI、C ABI 共享库），在 release 工件里附上目标三元组
       清单（当前仅承诺 Linux x86_64，其他目标 best-effort），并先跑
       `scripts/local-dist-smoke.sh`。
8. [ ] 发布后确认文档中的版本号、命令示例和 CHANGELOG 版本节仍匹配。

## C ABI 兼容检查

每次发布前手动确认 `crates/nox_core/include/nox_core.h` 的兼容性：

- [ ] 没有改动现有 enum 数值（删除 / 重排会破坏 ABI）。
- [ ] 没有改动现有函数签名（参数类型 / 数量 / 返回类型）。
- [ ] 新增 enum 值放在末尾；新增函数放在 header 末尾。
- [ ] 与 README / embedding.md 中描述的 host callback 生命周期一致。

破坏 ABI 的变更必须先升级 SemVer 主版本号（v0.0.x → v1.0.0 或更高），并在 CHANGELOG
和 ADR 中说明。

## 回滚

如果发布后发现严重问题：

1. 立即在 README / CHANGELOG 中标注该版本已撤回。
2. 如果是包含错误代码的 tag：保留 tag（git 历史不可篡改），但推一个 hotfix
   `0.0.y+1`，CHANGELOG 在新版本节明确写"撤回 0.0.y 的某次破坏"。
3. 不直接 `git push --force` 覆盖已发布 tag——下游可能已经拉取，强推会让
   不同环境状态分裂。
