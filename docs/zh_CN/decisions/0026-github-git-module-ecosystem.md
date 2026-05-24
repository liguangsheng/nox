# 0026 - GitHub / git URL module 生态路线

- 状态：已采纳
- 日期：2026-05-24
- 涉及：模块 / 工具链 / 发布

## 背景

Nox 已有 `nox.toml`、manifest source dirs、相对 import、stdlib 静态模块、project check
和 LSP project 解析能力。下一步需要让项目复用第三方 `.nox` 模块，但 Nox 当前规模和生产
目标都不适合立即承诺自建 package registry。

自建 registry 会同时引入账号、上传、命名所有权、撤回、签名、索引同步、镜像、滥用治理、
可用性 SLA 和长期兼容承诺。相比之下，Go module 的早期可借鉴点是直接把 VCS source 作为
依赖源：项目声明依赖，工具解析到具体版本，缓存源码，并用 lock/sum 信息保证复现。Nox 可以
先复用 GitHub 和通用 git URL 生态，避免在语言和标准库仍快速演进时过早运营 registry。

## 决策

Nox 第一阶段包生态不做自建 registry。项目依赖先通过 GitHub shorthand 或显式 git URL
声明，并且必须 pin 到不可变版本：

```toml
[dependencies]
mathx = { github = "owner/repo", rev = "0123456789abcdef0123456789abcdef01234567" }
tools = { git = "https://github.com/owner/tools.git", tag = "v0.2.0" }
```

依赖 spec 允许以下 source：

- `github = "owner/repo"`：等价于 GitHub HTTPS git remote。
- `git = "https://..."`：显式 git remote URL。
- 后续可追加 `ssh` / enterprise host policy，但不作为第一阶段承诺。

依赖必须且只能提供一个 pin：

- `rev`：完整 commit hash，推荐用于生产。
- `tag`：解析后写入 lockfile 的具体 commit；tag 是否允许移动由 lockfile drift 检查发现。
- `branch` 或默认分支不作为生产稳定 pin；可以作为开发实验入口，但 release gate 必须拒绝未锁定
  或仍浮动的 dependency。

工具链生成并维护 lockfile。lockfile 至少记录：

- manifest dependency name。
- 原始 source spec。
- resolved commit。
- content hash。
- cache key。
- fetch time 或工具版本信息。

module cache 放在用户级 Nox cache 目录，不写入项目源码目录。项目源码只提交 manifest 和
lockfile；cache 可以删除并通过 lockfile 复现。离线模式只允许使用已有 cache，cache miss、
hash mismatch、lock drift 和 source 不可达必须产生可机器识别的诊断。

下载是工具链行为，不是脚本运行时能力。`nox fetch`、`nox project check --fetch` 或等价入口
必须显式 opt-in；普通 `nox run` / `nox check` 不应在缺失 lock/cache 时悄悄联网。导入第三方
module 不自动授予 `filesystem`、`network`、`environment`、`timer` 或 `async` capability；
capability 仍由运行时配置和宿主决定。

import resolution 采用清晰分层：

1. `std/...` 只解析到内置 stdlib。
2. 相对 import 只解析当前文件相对路径。
3. manifest source dirs 解析项目本地模块。
4. 外部 dependency 只通过 manifest dependency name 映射到 lockfile/cache 中的只读源码。

## 后果

这个路线让 Nox 可以利用 GitHub 和 git 托管生态快速获得模块复用能力，同时避免承担 registry
运营和命名治理。依赖由 commit/content hash 固定，项目可以离线复现，也能在 release gate 中
检查 lockfile drift。

代价是用户需要理解 git source、tag/rev 和 lockfile；没有中心化搜索、name ownership 或
撤回机制。GitHub 不可用、仓库删除或 force-push 时，项目只能依靠已提交 lockfile、已有 cache
或自行 mirror。Nox 第一阶段也不会承诺 Go proxy/sumdb 等全局校验基础设施。

这个 ADR 不改变现有 import 行为，只确定后续实现顺序：

1. 扩展 manifest schema 和 lockfile skeleton。
2. 实现受控 fetch/cache 和离线模式。
3. 将 import resolution 接入 external module，并让 CLI/LSP/doc 复用同一解析逻辑。

## 备选方案

- 自建 registry。暂不选择，因为运营面和兼容承诺过大，会分散当前语言、runtime 和 release
  稳定性工作。
- 直接使用 crates.io/npm 风格中心 registry。暂不选择，因为 Nox module 是源码级 `.nox`
  依赖，不适合立即绑定到 Rust crate 或 JavaScript package 生态。
- 只允许 vendored dependency。暂不选择，因为它能复现但会让升级、缓存和 LSP/source graph
  体验变差；后续可以作为离线部署策略补充。
- 允许默认分支浮动 import。未选择作为生产路径，因为它不可复现，且会让 release gate 和
  下游升级诊断失去稳定证据。
