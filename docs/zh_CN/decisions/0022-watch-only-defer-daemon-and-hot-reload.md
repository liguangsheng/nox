# 0022 - 仅启动 watch 模式，暂缓 daemon、增量 typecheck 缓存和 hot reload

- 状态：已采纳
- 日期：2026-05-23
- 涉及：CLI / 工具链 / LSP / 缓存 / 发布

## 背景

PLAN.md 阶段 27 计划评估"watch、增量 typecheck、可选 daemon、hot reload / 模块热替换"
四组工具运行模式。当前 CLI 工作模式是 one-shot：

- `nox run` / `check` / `test` / `fmt` 每次都重新解析整个项目。
- LSP 服务进程内缓存当前 open buffer，但不复用 CLI 的解析或 typecheck 结果。
- 没有任何 watch、daemon、incremental cache 机制。

阶段 23-25 落地后，typecheck 工作量会上升一档（mutation 检查、闭包 capture 分析、
约束推导），CLI 多次执行的等待感会更明显。PLAN.md 阶段 27 要求一次性给出这四组方向
的取舍。

## 决策

v0.0.x 开发阶段**仅启动 watch 模式**，daemon / incremental typecheck cache /
hot reload 全部继续暂缓。

启动范围（watch）：

- 新增 `nox watch <subcommand>`，包装 `check` / `test` / `run` 中的一个；只在前台
  阻塞运行，CTRL-C 退出。
- 触发条件：watch 范围是当前项目 manifest `source_dirs` 与 `test_dirs`；变化检测使用
  跨平台的 stat-poll fallback（不引入 inotify / kqueue 平台依赖）。轮询间隔默认 500ms，
  通过 `--interval-ms` 调整。
- 每次触发执行被包装命令并打印结构化时间戳；执行错误不让 watch 退出。
- watch 模式与现有 capability、JSON 输出、退出码完全一致。

显式不做：

- daemon：不引入后台常驻进程、socket protocol、客户端/服务端 schema、stop/status/log
  接口。
- incremental typecheck cache：每次触发仍重新 parse + typecheck 整个项目。不共享 LSP
  缓存，不引入 disk-resident cache 格式。
- hot reload / 模块热替换：脚本运行中替换模块或函数体不在能力范围内。
- watch 配合 `--json`：watch 默认仅人类可读输出；机器消费仍走 `nox check --json`
  one-shot 调用。

兼容影响：

- 新增 CLI 子命令 `watch`。这是兼容扩展（之前 `nox watch` 返回 unknown subcommand）。
- 不引入新的 capability、manifest 字段或 JSON schema 字段。
- LSP 不受影响：仍按 didOpen / didChange 处理 buffer。

权限边界：

- watch 进程只读 manifest `source_dirs` / `test_dirs` 文件路径，不需要 filesystem_write
  capability，也不要求 filesystem_read 授权（watch 本身在 CLI shell 层运行，不是脚本
  capability）。
- watch 启动的子命令仍按 manifest 中的 capability 声明运行；watch 不能提权。

embedding API 影响：

- 完全不变。Rust API、C ABI 没有新增。watch 仅是 CLI shell 层 helper。

诊断方案：

- watch 失败诊断沿用被包装命令的 diagnostic code 与文本；watch 本身只新增
  `watch.path-not-found`（稳定，新增 code）：监视路径在初始化时不存在。
- watch 启动 / 终止 / 重新执行的事件用 plain text 行表示；机器消费者请用 one-shot
  CLI。

测试矩阵（实现 PR 必须覆盖）：

- 单元：watch 启动后修改 source 文件，被包装命令重新执行。
- 单元：watch interval 调整、CTRL-C 退出、被包装命令失败时 watch 继续运行。
- 单元：监视路径不存在时启动报 `watch.path-not-found`。
- 跨平台：Linux 与 macOS 的 stat-poll 行为；Windows 通过 stat-poll fallback 保持等价。
  不为 Windows 引入 ReadDirectoryChangesW。
- CLI integration：`nox watch check` / `nox watch test` / `nox watch run` 三种组合。

放弃条件：

- 真实使用证明 stat-poll 在大型项目（>10k 文件）下不可用。届时再评估 inotify / kqueue
  专用 watcher，并写新 ADR 重启该决策。
- watch 与未来的 daemon 模式有冲突需要先重启。

daemon / incremental cache / hot reload 的重启条件：

- 至少有 3 个真实用户脚本项目报告 watch + one-shot 仍然太慢（> 2s typecheck）。
- 阶段 30 的测试框架落地后，确认 `nox test --watch` 在普通项目上响应 < 1s 不可达。
- 同时存在稳定的进程间 IPC 框架可借鉴（避免 Nox 自造 protocol）。

任一缺失则继续暂缓 daemon、incremental cache、hot reload；watch + one-shot 是默认工具
运行模式。

## 后果

PLAN.md 阶段 27 的实现批次只覆盖 watch 一项；增量缓存、daemon、hot reload 显式不进入。
保持 CLI 与 LSP 的状态空间小：每次 invocation 都是无状态，方便诊断与回滚。代价是大型
项目的开发循环延迟仍是"完整 typecheck"，热点优化要等阶段 33 性能工程或本 ADR 重启。

## 备选方案

- 一次性做 watch + daemon + incremental cache：表达力强，但同时引入进程间 IPC、cache
  invalidation、daemon 生命周期、日志、错误恢复，与小核心原则冲突，也超出阶段 27 单批
  容量。
- 完全不引入 watch：让用户自己写 shell 循环。代价是 watch 是被高频请求的功能，缺失会
  让 CLI 体验差一档；同时手写 shell 循环容易遗漏触发抖动 / 错误处理。
- 只做 incremental typecheck cache，不做 watch：缓存命中需要外部触发器，反而要求用户
  自己写 watch shell。组合优先级颠倒，从 user-flow 角度不划算。
