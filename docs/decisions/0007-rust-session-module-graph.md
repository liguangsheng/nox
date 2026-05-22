# 0007 - Rust Session 与 ModuleGraph

- 状态：已采纳
- 日期：2026-05-21
- 涉及：Rust API / 模块 / LSP / 嵌入

## 背景

`Engine::eval`、`Engine::check`、`Engine::hover_type` 等简单 API 会在每次调用中重新解析
入口源码和 import graph。resolver 目前只在单次调用内维护 `loaded` / `loading`，可以避免
重复 import 和 diamond import 造成的重复加载，但不会跨调用复用模块源码。

默认 `nox` runtime 还在外层重复维护项目语义：CLI 通过 manifest 构造 import 搜索路径，
LSP 通过 open document overlay 覆盖磁盘源码。随着项目级 `check/test` 和 LSP project
awareness 继续推进，需要一个对宿主可见、生命周期明确的高级 API，用来复用 module cache、
表达 overlay，并让 CLI/LSP/嵌入宿主共享同一套概念。

## 决策

新增 Rust 高级 API：

- `Session`：长期会话对象，持有一个 `Engine` 和一个 `ModuleGraph`。
- `ModuleGraph`：只缓存 import specifier 到源码字符串的映射，不缓存 AST、typecheck 结果或
  bytecode。

第一批 API 只承诺源码级缓存和 overlay：

- `Session::new()` 创建带 core intrinsics 的会话。
- `Session::engine_mut()` 暴露底层 `Engine`，让宿主继续注册 host function、设置 instruction
  budget 或访问简单 API。
- `Session::clear_module_cache()` 清空 graph 缓存。
- `Session::set_module_overlay(specifier, source)` 写入或覆盖某个 import specifier 的源码。
- `Session::remove_module_overlay(specifier)` 删除 overlay 和同名缓存。
- `Session::eval/check/check_diagnostics/hover_type` 复用 graph loader 后调用现有 `Engine` pipeline。

`Engine::eval/check` 等简单 API 保持不变。`Session` 不替代 `Engine`，只为长期宿主和 LSP 提供
可复用的高级层。runtime permission 仍属于 `nox` runtime 或宿主，不进入 `nox_core` 的
`Session`；源码模块不会通过 session 自动获得文件、网络、环境或定时器能力。

`ModuleGraph` 第一批不公开解析后的 module unit。AST、bytecode、typecheck 环境继续是内部实现。
这样可以先统一缓存和 overlay 语义，同时避免把内部编译结构过早稳定为 API。

## 后果

长期宿主可以避免每次检查都重新调用 loader 读取不变 import。LSP 可以把打开文档作为 overlay
写入 `Session`，再由同一套 loader 语义处理 diagnostics 和 hover。CLI 仍可继续使用简单
`Runtime` API，后续 project awareness 再复用 `Session`。

代价是第一批缓存粒度较粗：overlay 或宿主知道模块变化时，需要显式清理对应 overlay/cache 或
整个 graph。因为缓存的是源码字符串，typecheck 和 bytecode 仍会按调用重新执行；这保留了诊断
正确性，也避免缓存失效规则一次性扩得过大。

## 备选方案

- 不新增 API，只继续使用 `Engine`：被拒绝。CLI、LSP 和嵌入宿主会继续各自维护 overlay 和
  缓存语义，后续项目级检查会重复实现同一套边界。
- 只暴露 `ModuleGraph`，不引入 `Session`：被拒绝。宿主仍需要自己协调 graph、Engine、host
  function 和 budget 生命周期，API 组合不够明确。
- 暴露完整 AST/typechecked module graph：暂不选择。内部 AST、bytecode 和 typecheck 环境仍是
  不稳定实现细节，过早公开会锁死后续模块系统和命名空间 import 设计。
- 在 `nox_core` 中处理 runtime permission：被拒绝。权限是宿主/runtime 能力，不属于源码模块
  本身；`Session` 只负责语言核心和模块源码图。
