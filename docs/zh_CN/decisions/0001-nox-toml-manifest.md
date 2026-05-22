# 0001 - nox.toml 项目 manifest

- 状态：已采纳
- 日期：2026-05-20
- 涉及：模块 / 工具链

## 背景

v0.0.1 只支持入口文件目录内的相对 import，没有项目身份和项目根。这让多文件项目
要么把所有模块塞进同一目录，要么在 import 时写长相对路径。计划在 v0.0.2 让脚本
具备小项目结构，但又不希望引入 Node.js 风格的 registry/依赖求解。

## 决策

- 引入 `nox.toml` manifest，保守支持四个字段：`package.name`、`package.version`、
  `entrypoints.main`、`modules.source_dirs`。
- CLI 在加载入口文件前从入口所在目录向上查找 `nox.toml`；找不到时保持 v0.0.1
  行为，找到时把 `modules.source_dirs` 加入 import 备选根目录。
- import 解析仍优先用相对当前文件的路径，只有相对路径找不到文件时才回退
  `source_dirs`，避免破坏旧脚本。
- 自带最小 TOML 子集解析器，只接受字符串和字符串数组，不引入第三方 `toml` crate。

## 后果

- 工具链获得统一的项目入口（`nox.toml`），后续 `nox test`、LSP 项目根、增量
  检查可以共用。
- import 路径解析行为完全向后兼容：未启用 manifest 的项目零影响。
- 自写 TOML parser 限制了 manifest 的表达能力。若未来需要表（inline table）或
  数字、布尔值，需要扩展或切到第三方 crate。

## 备选方案

- 使用第三方 `toml` crate：被拒绝，因为 v0.0.2 阶段优先保持零外部依赖。
- 沿用 `package.json` 风格 JSON：被拒绝，TOML 更适合写小型配置且与 Rust 生态
  保持一致。
- 不引入 manifest，继续靠相对路径：被拒绝，无法承载项目级工具链需求。
