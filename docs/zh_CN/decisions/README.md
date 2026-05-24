# 决策记录

本目录保存对 Nox 语言、模块系统、运行时和 ABI 的关键决策。每条记录是一份独立的
Markdown 文件，命名固定为 `NNNN-short-slug.md`，`NNNN` 是 4 位顺序编号。

## 何时写决策记录

需要写 ADR 的场景：

- 引入或拒绝语言级特性（语法、类型规则）。
- 改变模块系统、import 解析或 manifest 形状。
- 调整对外的 Rust API、C ABI 或 CLI JSON schema。
- 改变堆、GC、值生命周期或 verifier 的核心假设。
- 选择技术栈（依赖、CI、发布机制）。

`README.md` 改个段落、bug fix 和小重构不需要 ADR；它们走 git history 和
`CHANGELOG.md`。

## 模板

```markdown
# NNNN - <短标题>

- 状态：草案 / 已采纳 / 已废弃 / 被 NNNN 取代
- 日期：YYYY-MM-DD
- 涉及：<语言 / 模块 / 运行时 / ABI / 工具链 / 发布>

## 背景

<问题陈述、当前现状、为什么需要决定>

## 决策

<决定做什么，明确范围和不做什么>

## 后果

<带来的好处、付出的代价、留出的口子>

## 备选方案

<考虑过但未选择的方案，以及为什么不选>
```

## 索引

- [0001 - nox.toml 项目 manifest](0001-nox-toml-manifest.md)
- [0002 - check --json schema 稳定化](0002-check-json-schema.md)
- [0003 - 不在 v0.0.2 引入第三方依赖](0003-no-third-party-deps.md)
- [0004 - nox test 与 JSON schema](0004-nox-test-schema.md)
- [0005 - C ABI 复合值只读 handle](0005-c-abi-compound-handles.md)
- [0006 - 受限 match 语句](0006-limited-match-statement.md)
- [0007 - Rust Session 与 ModuleGraph](0007-rust-session-module-graph.md)
- [0008 - 命名空间 import](0008-namespace-import.md)
- [0009 - 暂缓语言级 option / result](0009-defer-option-result.md)
- [0010 - 暂缓可变数组](0010-defer-mutable-arrays.md)
- [0011 - 暂缓源码级函数类型](0011-defer-function-types.md)
- [0012 - 标准库命名分层策略](0012-stdlib-namespace-strategy.md)
- [0013 - std/* 静态模块加载](0013-stdlib-module-loader.md)
- [0014 - 重启 option / result 设计但暂不实现](0014-restart-option-result-design.md)
- [0015 - 暂缓容器和函数能力扩张](0015-defer-container-function-expansion.md)
- [0016 - 暂缓 async task 状态 API](0016-defer-async-task-status-api.md)
- [0017 - 字符字面量与 byte 字面量边界](0017-char-byte-literals.md)
- [0018 - 重启可变集合与 slice 设计](0018-restart-mutable-collections-and-slice.md)
- [0019 - 重启函数值、闭包与高阶函数设计](0019-restart-function-values-and-closures.md)
- [0020 - 暂缓 trait / interface，引入受限结构化约束](0020-structural-constraints-defer-traits.md)
- [0021 - 暂缓运算符重载与异常模型](0021-defer-operator-overload-and-exceptions.md)
- [0022 - 仅启动 watch 模式，暂缓 daemon / incremental cache / hot reload](0022-watch-only-defer-daemon-and-hot-reload.md)
- [0023 - 暂缓 cycle collector 与 arena handle](0023-defer-cycle-collector-and-arena-handles.md)
- [0024 - JSON tagged enum 使用 adjacent 表示](0024-json-tagged-enum-adjacent.md)
- [0025 - release gate CLI 二进制大小上限重校准](0025-release-gate-cli-size-cap.md)
- [0026 - GitHub / git URL module 生态路线](0026-github-git-module-ecosystem.md)
- [0027 - 静态 trait 系统路线](0027-static-trait-system.md)
- [0028 - result / option 错误模型与 try block 暂缓](0028-result-option-error-model.md)
- [0029 - 暂缓宏系统，优先使用函数、trait 与外部 codegen](0029-defer-macro-system.md)
- [0030 - 分阶段引入 async/await](0030-staged-async-await.md)
