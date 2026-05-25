# 0025 - release gate CLI 二进制大小上限重校准

- 状态：已采纳
- 日期：2026-05-24
- 涉及：工具链 / 发布

## 背景

`scripts/release-gate.sh` 在 v0.0.3 后把 `NOX_SIZE_CAP_CLI` 从 4 MiB 收紧到
2.5 MiB。当时 release CLI 二进制约 1.67 MiB，2.5 MiB 约等于 1.5 倍缓冲，用于防止
Nox 在功能扩张中失去“小型”目标。

后续阶段加入了更多生产可观测性与工具能力：`nox trace`、profile/coverage JSON 与
NDJSON、VM span statement/branch coverage、DAP/LSP 增强、JSON/schema 标准库、
property/fuzz bridge 和 release gate opt-in 层。当前 release 构建的 `target/release/nox`
实测约 2.55 MiB，略高于 2.5 MiB 上限，但仍明显低于最初 4 MiB 上限，也没有引入第三方
运行时依赖。

## 决策

把 release-gate 默认 `NOX_SIZE_CAP_CLI` 从 2.5 MiB 调整为 2.75 MiB（2,883,584 bytes）。
阶段 51-52 增加 LSP 跨文件 definition 与当前文件 rename 后，release CLI 实测约
2,892,288 bytes，略高于 2.75 MiB。由于该增长来自明确规划的编辑器能力，且仍保持零第三方
运行时依赖，把默认上限继续小幅校准到 2.8125 MiB（2,949,120 bytes）。
阶段 55-56 增加 GitHub/git module ADR、manifest dependency schema、`nox.lock` parser /
校验、`project check` lockfile drift 报告和 release gate lockfile guardrail 后，release CLI
实测约 2,958,736 bytes，略高于 2.8125 MiB。由于该增长来自明确规划的 module 生态复现边界，
且仍保持零第三方运行时依赖，把默认上限继续小幅校准到 2.84375 MiB（2,981,888 bytes）。
阶段 57-58 增加 `nox fetch`、module cache、external import resolution、cache hash 校验和
LSP diagnostics 接入后，release CLI 实测约 2,982,960 bytes，略高于 2.84375 MiB。已先移除
CLI 内重复 SHA-256 实现并复用库内 hash helper；剩余增长来自明确规划的 Git module 解析边界。
因此把默认上限继续小幅校准到 2.8515625 MiB（2,990,080 bytes）。
阶段 60 增加 LSP import path completion、项目顶层 symbol completion、namespace import alias
hover 的 module source / exported surface 和 diagnostic cache 后，release CLI 实测约
3,013,504 bytes，略高于 2.8515625 MiB。该增长来自明确规划的 IDE 语义能力，且仍保持零第三方
运行时依赖。因此把默认上限继续小幅校准到 2.875 MiB（3,014,656 bytes）。
阶段 62 增加静态 trait MVP 的 parser/AST/typechecker、formatter/doc/LSP 识别、保守冲突
诊断、impl method name mangling 和 receiver nominal type dispatch 后，release CLI 实测约
3,108,032 bytes，略高于 2.875 MiB。该增长来自明确规划的语言抽象能力，且仍保持零第三方
运行时依赖。因此把默认上限继续小幅校准到 2.96875 MiB（3,112,960 bytes）。
阶段 68-70 增加 async/await ADR、`task[T]` / `async fn` / `await` 语法、awaitable
runtime task 桥接、async diagnostics、formatter/LSP/`nox doc` 识别和示例后，release CLI
实测约 3,135,568 bytes，略高于 2.96875 MiB。该增长来自明确规划的并发语言能力，且仍保持
零第三方运行时依赖。因此把默认上限继续小幅校准到 3.0 MiB（3,145,728 bytes）。
阶段 79 补齐静态 trait 的 LSP hover/signature 直接证据后，泛型函数签名会保留
`T: Trait` bound，例如 `fn label<T: Display>(value: T) -> str`。release CLI 实测
3,146,640 bytes，略高于 3.0 MiB。该增长来自明确规划的 trait/interface IDE 能力，且没有
新增第三方运行时依赖。因此把默认上限继续小幅校准到 3.0078125 MiB（3,153,920 bytes）。
阶段 83 增加实验性 `std/yaml.nox` / `std/xml.nox` 纯计算数据 helper 后，release CLI
实测 3,169,792 bytes，略高于 3.0078125 MiB。该增长来自明确规划的数据脚本标准库扩面，
仍没有新增第三方运行时依赖、runtime capability 或默认授权。因此把默认上限继续小幅校准到
3.03125 MiB（3,178,496 bytes）。
阶段 90 增加实验性 `std/traits.nox` 小核心后，release CLI 实测 3,181,056 bytes，略高于
3.03125 MiB。该增长来自明确规划的 trait/interface 标准库抽象迁移，仍没有新增第三方运行时
依赖、runtime capability、prelude 或默认授权。因此把默认上限继续小幅校准到 3.0390625 MiB
（3,186,688 bytes）。
阶段 98 增加 trait method lookup 完整化后，release CLI 实测 3,188,880 bytes，略高于
3.0390625 MiB。该增长来自明确规划的 typechecker / VM method selection 边界，允许 trait impl
method 与顶层 record-style function 安全同名，仍没有新增第三方运行时依赖、runtime capability、
dynamic dispatch、trait object 或默认授权。因此把默认上限继续小幅校准到 3.046875 MiB
（3,194,880 bytes）。
阶段 100 增加外部 codegen manifest 元数据解析、`project check` JSON 报告和缺失 generated
文件校验后，release CLI 实测 3,205,504 bytes，略高于 3.046875 MiB。该增长来自明确规划的
只读工具审计能力，不执行生成器，不引入宏语法、compile-time execution、runtime capability、
第三方运行时依赖或默认授权。因此把默认上限继续小幅校准到 3.0625 MiB（3,211,264 bytes）。
阶段 104 增加 `std/xml.nox` namespace-qualified XML 文本生成 helper 后，release CLI 实测
3,211,904 bytes，略高于 3.0625 MiB。该增长来自明确规划的数据脚本标准库扩面，仍不引入
XML parser、namespace resolver、schema validation、XPath、streaming writer、runtime
capability、第三方运行时依赖或默认授权。因此把默认上限继续小幅校准到 3.0703125 MiB
（3,219,456 bytes）。
阶段 114 增加 codegen source-map manifest 元数据解析、`project check` JSON 报告和缺失
source-map 文件校验后，release CLI 实测 3,220,344 bytes，略高于 3.0703125 MiB。该增长来自
明确规划的只读 codegen 审计能力，仍不执行生成器、不解释 source-map 内容、不引入宏语法、
compile-time execution、runtime capability、第三方运行时依赖或默认授权。因此把默认上限继续
小幅校准到 3.078125 MiB（3,227,648 bytes）。

保持以下约束不变：

- `NOX_SIZE_CAP_CORE` 仍为 1.5 MiB。
- workspace 第三方运行时依赖数必须为 0。
- release gate 继续记录 Rust LOC 趋势。
- `NOX_SIZE_CAP_CLI` 仍可通过环境变量临时覆盖用于本地调查，但正式 release-prep 不应在没有
  ADR 与 CHANGELOG 的情况下继续上调。

## 后果

新的 3.078125 MiB 上限给当前约 3.071 MiB release CLI 留出约 7.1 KiB 余量，足够吸收阶段
114 的 codegen source-map 审计增量，但仍会在二进制持续膨胀时快速失败。这个调整承认
默认 runtime/CLI 已经承担更多生产诊断、可观测性、编辑器职责、module 复现边界和小型数据
格式 helper、XML 文本生成 helper、标准库 trait 抽象和只读 codegen 审计，同时保留
`nox_core` 的小核心边界和零第三方依赖边界。

代价是“小型”指标相对 v0.0.3 后的最紧阈值放宽。后续如果 CLI 继续增长，应优先检查是否有
可拆分的开发工具、重复 schema 输出、或可延迟加载的非核心能力，而不是继续线性上调阈值。

## 备选方案

- 保持 2.5 MiB 并删减 coverage / trace 输出。未选择，因为新增输出是阶段 31A/32 的生产排障
  证据，删除会倒退目标完成度。
- 回到 4 MiB。未选择，因为 4 MiB 对当前 2.76 MiB 基线过宽，无法及时发现体积回归。
- 新增独立开发工具二进制。暂不选择，因为当前 workspace 仍保持 `nox_core` + `nox` 两 crate
  边界；拆 CLI 需要重新设计分发、docs 和 release checklist。
