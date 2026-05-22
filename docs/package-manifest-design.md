# 包 Manifest

本文记录已实现的 `nox.toml` 项目 manifest。v0.0.3 起，manifest 是项目入口、
测试目录、import 搜索根和运行时权限声明的统一来源；它仍不引入 registry、依赖求解或
版本范围语法。

## 目标

- 提供一个最小的 `nox.toml` manifest，承载项目身份、默认入口、测试目录和 import
  项目根路径。
- 保持 import 行为向后兼容：未提供 manifest 时，行为与之前完全一致。
- 为后续 CLI、LSP、`nox test` 和项目级检查提供统一入口。
- 允许项目声明需要的 runtime 权限，但声明只用于文档和后续检查，不自动授予危险能力。

## 非目标

- 不做 package registry。
- 不做依赖求解。
- 不做版本范围语法。
- 不做 `node_modules` 布局。
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
- `runtime.permissions`：可选字符串数组。允许值为 `filesystem.read`、`filesystem.write`、
  `network`、`timers`、`environment`、`async_tasks`。这些值只声明项目期望能力，不会让
  CLI 或宿主自动授予权限，也不会配置文件系统 allowlist。宿主仍需要用
  `RuntimePermissions` 显式授予能力和路径 root。

## 解析规则

- manifest 解析器只接受字符串和字符串数组值。其他 TOML 类型（数字、表、
  内联表、布尔）当前都不支持，遇到时返回诊断。
- `[package]` 必须包含 `name` 和 `version`；`description` 可选。
- `[entrypoints]` 中所有键都必须是字符串。`main` 是默认入口，其他键作为命名入口保留。
- `[modules]` 中 `source_dirs` 和 `test_dirs` 都必须是字符串数组，路径必须留在项目根内且
  单个数组内不能重复。
- `[runtime]` 中 `permissions` 必须是字符串数组，未知权限名返回诊断。
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

`nox project check` 要求能从当前目录或父目录发现 manifest，然后在 manifest root 下依次运行
`check`、`test` 和 `fmt --check`。`project check --json` 输出 `nox.project-check.v1`，
显式报告 manifest root、package name/version 和三个子步骤的退出码，用于 CI 识别项目边界。

## import 解析

`import "specifier";` 的解析顺序：

1. `<当前文件所在目录>/<specifier>` —— 与历史行为一致。
2. 如果发现 manifest 且 `modules.source_dirs` 非空，则按数组顺序尝试
   `<manifest 根>/<source_dir>/<specifier>`。
3. 全部失败时，按第一步的相对路径报告找不到文件的诊断。

manifest 不会改变 import 的语义，只是在相对路径找不到文件时给出第二次
机会。这样既保留旧脚本的工作方式，又允许小项目通过 `source_dirs` 让 import
从项目根解析。

## 后续工作

- LSP 在打开文档时使用 manifest 提供项目根，避免依赖磁盘。
- manifest 内的命名空间 import（`import "math.nox" as math;`）属于阶段 16.2 的独立设计。
- runtime 权限声明进入检查/提示层，但危险能力仍由宿主或 CLI 显式授予。
