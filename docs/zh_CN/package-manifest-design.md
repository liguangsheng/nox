# 包 Manifest

本文记录已实现的 `nox.toml` 项目 manifest。v0.0.3 起，manifest 是项目入口、
测试目录、import 搜索根、GitHub/git dependency 声明和运行时权限声明的统一来源；它仍不
引入 registry、依赖求解或版本范围语法。

## 目标

- 提供一个最小的 `nox.toml` manifest，承载项目身份、默认入口、测试目录和 import
  项目根路径。
- 保持 import 行为向后兼容：未提供 manifest 时，行为与之前完全一致。
- 为后续 CLI、LSP、`nox test` 和项目级检查提供统一入口。
- 允许项目声明 pinned GitHub/git 依赖，并通过 `nox fetch` 生成 lockfile、填充 cache；
  `import "<dependency>/<path>.nox"` 可以从 lock/cache 解析 external module。
- 允许项目声明外部 codegen 生成的 `.nox` 文件及其生成器/模板/hash 元数据，供
  `project check` 做只读审计；Nox 不执行生成器。
- 允许项目声明需要的 runtime 权限，但声明只用于文档和后续检查，不自动授予危险能力。

## 非目标

- 不做 package registry。
- 不做依赖求解。
- 不做版本范围语法。
- 不做 `node_modules` 布局。
- 不在 `run` / `check` 缺失 lock/cache 时静默联网下载依赖。
- 不在 import、typecheck、LSP、`doc` 或 `project check` 中隐式执行 codegen。
- manifest 不会改变相对 import 的语义，只是在相对路径解析失败时再尝试
  项目内 source 根。

## 文件名

manifest 文件名固定为：

```text
nox.toml
```

## 形状

第一版形状保持保守：

```toml
[package]
name = "example"
version = "0.0.1"
description = "optional short description"

[entrypoints]
main = "src/main.nox"
admin = "src/admin.nox"

[modules]
source_dirs = ["src"]
test_dirs = ["tests"]

[dependencies]
mathx = { github = "owner/mathx", rev = "0123456789abcdef0123456789abcdef01234567" }
tools = { git = "https://github.com/owner/tools.git", tag = "v0.2.0" }

[codegen]
api = { generated = "src/generated/api.nox", generator = "tools/gen-api", template = "schemas/api.tpl", input_hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", command = "tools/gen-api schemas/api.tpl > src/generated/api.nox" }

[runtime]
permissions = ["filesystem.read"]
```

字段含义：

- `package.name`：项目内身份字符串。当前只用于 manifest 自身校验，
  不是全局 registry key。
- `package.version`：当前只是信息字段。包管理器引入前不参与解析。
- `package.description`：可选。项目描述字符串，只用于元数据展示或文档。
- `entrypoints.main`：可选。manifest 所在目录的相对路径，指向项目默认入口。
  `nox run` 无显式路径时会使用该入口。
- `entrypoints.<name>`：可选命名入口。当前只解析和保留，不自动执行；后续 CLI 可以
  在不改变 manifest 形状的前提下接入命名入口。
- `modules.source_dirs`：可选字符串数组，相对 manifest 所在目录，不能使用绝对路径或
  `..` 逃逸项目根，重复目录会被拒绝。
  import 解析在相对当前文件失败时，会按数组顺序尝试这些目录。
- `modules.test_dirs`：可选字符串数组，相对 manifest 所在目录，不能使用绝对路径或
  `..` 逃逸项目根，重复目录会被拒绝。`nox test` 无显式路径时
  优先递归发现这些目录下的 `*_test.nox`；未配置时回退到 `source_dirs`，再回退到项目根。
- `dependencies.<name>`：可选 inline table。当前会解析、校验；`nox fetch` 会下载到
  module cache 并生成 `nox.lock`，`project check` 会校验对应 `nox.lock`；
  `import "<dependency>/<path>.nox"` 会从 lock/cache 解析 external module。每个 dependency 必须提供且只
  提供一个 source：`github = "owner/repo"` 或 `git = "https://..."` / `ssh://...` /
  `file://...`；同时必须提供且只提供一个 pin：完整 40 字符 commit hash `rev = "..."` 或
  `tag = "..."`。
- `runtime.permissions`：可选字符串数组。允许值为 `filesystem.read`、`filesystem.write`、
  `network`、`timers`、`environment`、`async_tasks`、`process_run`。这些值只声明项目期望能力，不会让
  CLI 或宿主自动授予权限，也不会配置文件系统 allowlist。宿主仍需要用
  `RuntimePermissions` 显式授予能力和路径 root。
- `codegen.<name>`：可选 inline table，用于声明一个外部生成的 `.nox` 文件。`generated`
  是必填的项目内相对路径；`generator`、`template`、`input_hash`、`source_map`、
  `source_map_hash` 和 `command` 是可选元数据。`input_hash` 和 `source_map_hash` 如果存在，
  必须使用 `sha256:<64 hex>`；`source_map_hash` 必须和 `source_map` 一起声明。
  `project check` 只验证 `generated` 和声明的 `source_map` 文件存在并在 JSON 中报告这些字段，
  不执行 `command`，也不让诊断、definition、rename 或 formatter 穿透到模板。

## 解析规则

- manifest 解析器接受字符串、字符串数组，以及 `[dependencies]` 内的字符串 inline table。
  其他 TOML 类型（数字、布尔、嵌套表等）当前都不支持，遇到时返回诊断。
- manifest schema 是封闭的：只支持 `[package]`、`[entrypoints]`、`[modules]`、
  `[dependencies]`、`[codegen]`、`[runtime]` 六个 section；`[package]`、`[modules]`、
  `[runtime]`、dependency inline table 和 codegen inline table 中未知 key 会返回
  `manifest.invalid`。`[entrypoints]` 允许除 `main` 之外的命名入口。
- `[package]` 必须包含 `name` 和 `version`；`description` 可选。
- `[entrypoints]` 中所有键都必须是字符串。`main` 是默认入口，其他键作为命名入口保留。
- `[modules]` 中 `source_dirs` 和 `test_dirs` 都必须是字符串数组，路径必须留在项目根内且
  单个数组内不能重复。
- `[dependencies]` 中每个 dependency 都必须是 inline table，并且只能使用 `github` / `git`
  与 `rev` / `tag` 组合；浮动默认分支不属于生产稳定 pin。
- `[runtime]` 中 `permissions` 必须是字符串数组，未知权限名返回诊断。
- `[codegen]` 中每个 artifact 都必须是 inline table。`generated`、`template` 和 `source_map`
  路径必须留在项目根内；`generator` 和 `command` 只是审计文本，不会被执行。
- 字符串里目前不支持嵌入 `"`；如有特殊需要请使用更简单的标识符。
- 注释以 `#` 开头，到行尾结束。字符串中的 `#` 不视为注释。
- 重复的 `[section]` 或同一节内的重复键被视为错误。

## CLI 行为

显式入口路径始终优先于 manifest。`nox run <file.nox>`、`nox check <file.nox>`、
`nox fmt <file.nox>`、`nox inspect-bytecode <file.nox>` 在加载入口文件之前，会从入口文件
所在目录向上查找 `nox.toml`：

- 找到时：解析 manifest，把 `modules.source_dirs` 加入 import 搜索路径。
  manifest 解析失败时输出诊断并返回非零退出码。
- 找不到时：保留原有行为，import 只在相对当前文件的目录中解析。

`nox run` 无显式路径时，会从当前目录向上查找 `nox.toml` 并执行 `entrypoints.main`。
找不到 manifest 或 manifest 没有 `main` 时返回用法错误。显式指定的入口路径始终可用，
无论是否存在 manifest；manifest 没有 `main` 不会阻止显式入口运行。

`nox test` 无显式路径时，会按以下顺序选择测试发现根：

1. manifest 的 `modules.test_dirs`。
2. manifest 的 `modules.source_dirs`。
3. manifest 根目录。
4. 没有 manifest 时使用当前工作目录。

## Lockfile

lockfile 文件名固定为 `nox.lock`，位于 manifest root。只要 manifest 声明了
`[dependencies]`，`nox project check` 就要求 `nox.lock` 存在并与 manifest 匹配；缺少、
格式错误、source/pin drift 或多余依赖都会让项目检查失败。`nox fetch` 是显式生成或更新
lockfile 的入口；`project check` 本身不会联网 fetch。

## Codegen 元数据

外部 codegen 仍是项目自己的构建步骤。Nox 只读取 manifest 中的 `[codegen]` 元数据：

- `nox project check --json` 输出 `codegen` 对象，包含 `ok`、`artifacts[]` 和
  `diagnostics[]`。
- 每个 artifact 报告 `name`、绝对 `generated` 路径、`exists`、`generator`、绝对
  `template` 路径、`input_hash`、绝对 `source_map` 路径、`source_map_exists`、
  `source_map_hash` 和 `command`。
- `generated` 或声明的 `source_map` 文件不存在时，`project check` 返回非零并报告
  `manifest.invalid`。
- Nox 不运行生成器、不重新生成源码、不校验模板内部 span，也不把 codegen 元数据用于 import
  resolver、typechecker、formatter 或 runtime。

当前 lockfile 使用封闭的 TOML 子集：

```toml
[lock]
version = "1"

[dependencies.mathx]
source_kind = "github"
source = "owner/mathx"
pin_kind = "rev"
pin = "0123456789abcdef0123456789abcdef01234567"
resolved = "0123456789abcdef0123456789abcdef01234567"
content_hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
cache_key = "github-owner-mathx-0123456789abcdef0123456789abcdef01234567"
tool = "nox 0.0.4"
```

`resolved` 必须是完整 commit hash；`content_hash` 必须是 `sha256:<64 hex>`；
`cache_key` 和 `tool` 必须非空。`tag` pin 会在 lockfile 中保留原始 tag，同时用
`resolved` 固定实际 commit。

`nox fetch [--offline] [--check|--locked] [--cache-dir <dir>]` 会发现 manifest，按 dependency
source 创建或更新 module cache，解析 pin，计算 content hash，并在默认模式写入 `nox.lock`。
默认 cache 目录优先使用 `NOX_MODULE_CACHE`，否则使用 `$HOME/.cache/nox/modules`。
`--offline` 不执行 git fetch，只消费已有 cache；cache 缺失或损坏时失败。下载动作不会给脚本
运行阶段授予 runtime permissions。`--check` 和 `--locked` 是只读模式：验证现有 lockfile/cache
是否匹配但不改写 `nox.lock`。

module cache 是本地可丢弃缓存，不属于项目源码状态。删除 cache 后，提交的 `nox.toml` 和
`nox.lock` 仍是复现依据；重新运行 `nox fetch` 可以恢复 cache。CI 如果需要完全锁网，
应固定 `--cache-dir <dir>`，预先填充该目录，然后用 `nox fetch --offline --cache-dir <dir>`
校验 cache，后续命令通过 `NOX_MODULE_CACHE=<dir>` 读取同一份 cache。

`nox project check` 要求能从当前目录或父目录发现 manifest，然后在 manifest root 下依次校验
lockfile 并运行 `check`、`test` 和 `fmt --check`。当前 `project check` 不下载 dependency；
缺少 lockfile 也不会联网。`project check --json` 输出 `nox.project-check.v1`，显式报告
manifest root、package name/version、dependency 声明、lockfile 状态和三个子步骤的退出码，
用于 CI 识别项目边界。

阶段 105 复审后，dependency 生态继续按 GitHub/git URL module 路线推进，不做自建 registry、
publish 命令、中心索引或版本范围求解。`nox fetch --check` / `--locked` 提供只读检查模式，
让 CI 可以验证 manifest、`nox.lock` 和 module cache 是否仍一致，而不改写 lockfile，也不让
`project check` 承担联网下载职责。

## import 解析

`import "specifier";` 的解析顺序：

1. `<当前文件所在目录>/<specifier>` —— 与历史行为一致。
2. 如果发现 manifest 且 `modules.source_dirs` 非空，则按数组顺序尝试
   `<manifest 根>/<source_dir>/<specifier>`。
3. 如果 `specifier` 形如 `<dependency>/<path>.nox`，并且 manifest `[dependencies]` 中存在
   同名 dependency，则从 `nox.lock` 指向的 module cache 读取对应 resolved commit 下的
   `<path>.nox`，并校验 cache archive hash 与 lockfile `content_hash` 一致。
4. 全部失败时，按第一步的相对路径报告找不到文件的诊断。

manifest 不会改变 import 的语义，只是在相对路径找不到文件时给出第二次
机会。这样既保留旧脚本的工作方式，又允许小项目通过 `source_dirs` 让 import
从项目根解析。

External dependency import 不会触发联网下载；缺少 lockfile、cache miss、cache corrupt 或
hash mismatch 都是诊断错误。若 `nox fetch` 使用了非默认 `--cache-dir`，后续命令需要设置
`NOX_MODULE_CACHE` 指向同一个 cache。

## 后续工作

- cache inspect/clean、private repo cookbook 和 checksum mismatch 文案改善。
- external import resolution 已接入 `run` / `check` / `test` / LSP diagnostics；LSP
  definition/workspace symbol 对 external dependency 保持保守，不伪造文件位置、不混入项目符号。
  `nox doc` 仍只扫描传入文件本身，外部 module 文档聚合留给后续专门设计。
