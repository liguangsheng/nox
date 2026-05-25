# 0031 - 集成式 LSP 第四轮路线

- 状态：已采纳
- 日期：2026-05-25
- 涉及：工具链 / LSP / CLI / 编辑器集成 / 发布

## 背景

`nox lsp` 已经覆盖 diagnostics、hover、formatting、completion、signature help、
code action、code lens、document symbol、workspace symbol、semantic tokens、definition
和当前文件保守 rename。阶段 84 加入词法级 semantic tokens 后，IDE 表面已经足够支撑日常编辑，
但仍有几个明显方向会扩大长期承诺：

- 跨文件 rename 需要精确处理 shadowing、import alias、manifest source dirs、module cache 和
  external dependency，当前测试覆盖还不足以承诺全量语义正确。
- 后台 watch daemon 或跨 invocation typecheck 服务会改变资源生命周期、缓存失效和发布资产边界。
- generated/codegen source map 会影响 formatter、diagnostics、definition、semantic tokens 和
  `nox doc`，不应作为普通 LSP 增量顺手引入。
- external dependency navigation 必须遵守 GitHub/git URL module、lockfile 和 cache hash 边界，
  不能伪造不可审计的本地位置。

因此第四轮 LSP 继续采用集成式、保守增量路线。

## 决策

LSP 继续只作为 `nox` CLI 内的 `nox lsp` 子命令交付。不拆独立 `nox-lsp` crate、binary、
release asset 或 package；VS Code 扩展和其他编辑器继续启动同一个 `nox` binary。

第四轮只接受满足以下条件的 IDE 增量：

- 能在单次 stdio LSP session 内完成，不要求后台 daemon。
- 能用 open-document overlay、manifest `modules.source_dirs` 和现有 module cache 语义解释。
- 不要求跨 invocation 持久 typecheck 服务。
- 不改变 CLI JSON diagnostic schema，除非同批更新 compatibility golden 和 diagnostics docs。
- 对 external dependency 保守：可以用于 diagnostics、completion 或 symbol 展示，但 definition /
  rename 不返回未经 lockfile/cache 证明的本地源码位置。

本轮优先级：

1. 更具体的 code action capability 和稳定 action kind。
2. 当前文件安全 rename 的预检继续收紧，但不承诺跨文件 rename。
3. semantic token 精度可以逐步提升，但仍只针对已打开文档和普通源码。
4. project symbol graph / diagnostic cache 可以改善失效策略，但只在当前 LSP 进程内生效。

首个实现切片选择 code action：`initialize` 声明具体 `quickfix`、`source.fixAll.nox` 和
`source.format.nox` kind；`textDocument/codeAction` 返回可执行的 `nox.check` /
`nox.format` source action，并对源码中的 `TODO` marker 提供精确范围的 quickfix edit。

## 非目标

- 不拆独立 LSP 二进制或单独发布 LSP package。
- 不做后台 watch daemon、长期驻留 project server 或跨 invocation cache。
- 不开放跨文件 rename，除非后续测试能覆盖 shadowing、import、module cache 和 external dependency
  关键风险。
- 不为 generated/codegen source map 扩展 LSP 协议表面。
- 不让 LSP 绕过 GitHub/git URL module lockfile、cache hash 或 offline 边界。

## 阶段 101 复审

阶段 101 在 code action、semantic tokens、cross-file definition、external dependency 保守诊断和
`[codegen]` manifest 元数据落地后，重新评估 LSP/IDE 第五轮。结论是继续保持集成式 `nox lsp`，
不拆独立二进制，并把阶段 102 收敛到一个小的、可测试的 LSP 增量。

当前证据：

- `initialize` 已声明 hover、definition、rename prepare、document/workspace symbol、formatting、
  completion、signature help、semantic tokens、code action 和 code lens。
- `textDocument/definition` 已能跳到当前文档和 manifest source dirs 中 imported module 的 exported
  顶层声明；std module 和 external dependency 仍保守返回 `null`，避免伪造不可编辑位置。
- `textDocument/rename` 仍限制在当前文件顶层 symbol，并在同名局部声明或参数存在时拒绝。
- `[codegen]` manifest 元数据现在能由 `project check` 做只读审计，但 LSP 还没有任何 generated
  source 标注；这比跨模板 definition/rename 更适合作为下一步最小 IDE 反馈。

阶段 101 决策：

- LSP 继续只作为 `nox lsp` 子命令交付；不新增 `nox-lsp` binary、crate、release asset、
  package 或 registry 表面。
- 阶段 102 首选实现是 generated source 的只读 IDE 标注：当打开文件匹配 manifest `[codegen]`
  artifact 的 `generated` 路径时，LSP 可以通过 hover、document symbol detail、semantic token
  readonly modifier 或 code action disabled message 暴露“由外部 codegen 生成”的信息。
- 该标注只读取 manifest 元数据，不执行生成器，不读取模板内容，不校验 input hash，不改变
  diagnostics span、formatter 输出、definition 目标或 rename 范围。
- external dependency navigation 仍保持保守；若阶段 102 不选 generated source 标注，第二候选是
  对 lockfile/cache 已证明的 dependency 提供更清楚的 hover/diagnostic 说明，而不是直接跳转。
- 跨文件 rename、后台 daemon、跨 invocation index 和真正 source-map 穿透继续暂缓。

阶段 102 的完成标准：

- 至少一个 LSP request 对 generated source 元数据有可见、机器可测输出。
- 覆盖 manifest root、open-document URI 与 generated path 匹配的 stdio integration test。
- 覆盖未声明 `[codegen]` 或普通源码时不改变现有 LSP 输出。
- 同步 CLI/LSP 文档和 CHANGELOG；如果改变 LSP JSON schema 或 diagnostic code，再同步
  compatibility golden 和 diagnostics docs。

## 阶段 117 复审

阶段 117 在静态 trait、result/option 错误模型、async/await、external codegen source-map
元数据和 XML helper 都经过后续批次复评后，重新确认 LSP/IDE 第六轮边界。结论仍然是：
当前 IDE 表面已经够用，后续只补可审计的小型 parity 缺口，不把 LSP 作为语言深水区入口。

当前证据：

- `nox lsp` 仍作为 `nox` CLI 集成子命令交付，正式文档明确不拆独立二进制或单独 package。
- hover / signature help 已覆盖 async 调用侧 `task[T]`、源签名和泛型 trait bound。
- completion / workspace symbol / document symbol 已覆盖项目顶层 `fn`、`record`、`enum`、
  `trait` 和 `type`，并复用当前进程内 symbol graph cache。
- generated source hover 已展示 artifact、generator、template、input hash 和 command，但阶段
  114 新增的 `source_map` / `source_map_hash` 还没有在 IDE 只读标注中出现。

阶段 117 决策：

- 阶段 118 首选实现是把已通过 manifest 审计的 `source_map` 和 `source_map_hash` 加入
  generated-source hover note。它只显示元数据，不读取或解释 source-map 内容。
- 不把 source-map 用于 diagnostics range、definition、rename、formatting、semantic tokens、
  `nox doc` 或 template navigation。
- 不开放跨文件 rename、后台 watch daemon、跨 invocation index、external dependency 跳转或
  generated/codegen template 穿透。
- 不新增 LSP JSON schema、diagnostic code、capability kind、release asset 或 package 表面。

阶段 118 的完成标准：

- 覆盖 manifest `[codegen]` artifact 同时声明 `source_map` 和 `source_map_hash` 时的 LSP hover。
- 覆盖 hover 文案只展示路径/hash 元数据，不改变普通源码或未声明 source-map 的 generated source
  行为。
- 同步中英文 CLI 文档和 CHANGELOG。

## 后果

这个路线能继续提高编辑器可用性，同时保持发布资产、权限边界和回滚路径简单。用户仍然只需要安装
一个 `nox` binary，编辑器也不需要跟踪额外版本矩阵。

代价是部分 IDE 能力会保持保守：跨文件 rename、后台索引和 generated source map 暂时不可用。
这些能力未来如果重启，必须先补设计和回归证据，而不是在现有 LSP handler 里隐式扩张。

## 验证要求

每个 LSP 第四轮实现切片至少覆盖：

- `initialize` capability。
- 目标 LSP request 的 stdio integration test。
- open-document overlay 或 manifest project 边界中至少一个关键路径。
- docs、CHANGELOG 和 `git diff --check`。

涉及 typechecker、diagnostic code、module resolver 或 external dependency 的切片还必须跑
`scripts/release-gate.sh`。

## 备选方案

- 拆独立 LSP server。未选择，因为会扩大 release asset、安装路径、版本兼容和编辑器配置矩阵，
  也违背当前包生态不做额外 registry/package 的路线。
- 直接实现跨文件 rename。未选择，因为当前缺少足够强的 semantic rename 证据，容易误改项目源码。
- 引入后台 daemon。未选择，因为它会改变资源生命周期、取消、缓存失效、权限和发布回滚边界。
- 先做 generated/codegen source map。未选择，因为宏系统和外部 codegen 仍在复评阶段，source map
  不应先于语言/工具链设计落地。
