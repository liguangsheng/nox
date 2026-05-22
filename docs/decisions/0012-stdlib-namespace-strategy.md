# 0012 - 标准库命名分层策略

- 状态：已采纳
- 日期：2026-05-21
- 涉及：运行时 / 模块 / 工具链

## 背景

Nox v0.0.2 已经有一组全局 host function。`nox_core` 内置纯数值转换，`nox` 默认
运行时安装文件、环境、网络、计时器和 async task helper：

| 类别 | 当前全局函数 |
| --- | --- |
| 核心纯函数 | `to_float`、`to_int` |
| 运行时纯函数 | `sqrt` |
| 脚本参数 | `args` |
| 文件系统 | `read_text`、`exists`、`write_text` |
| 环境变量 | `env_get`、`env_list` |
| 计时器 | `sleep_ms` |
| 网络 | `tcp_connect` |
| async task | `task_sleep_ms`、`task_ready`、`task_cancel` |

这些名字对小脚本直接可用，但如果继续把新能力加到全局作用域，会造成命名拥挤，也会
让权限边界和能力来源不清。阶段 16.2 已经引入命名空间 import，阶段 20.3 需要确定
标准库是否继续增长全局函数，或改用模块化命名。

## 决策

v0.0.3 保留现有全局函数，不在本阶段重命名或移除，避免破坏 v0.0.2 脚本和文档示例。

从 v0.0.3 开始，新增标准库能力默认不再直接增加全局函数。新增能力应优先设计为
命名空间模块表面，例如未来的：

```nox
import "std/fs.nox" as fs;
import "std/env.nox" as env;

if (fs.exists("data.txt")) {
    fs.read_text("data.txt");
}
```

标准库命名空间仍是静态模块表面，不是运行时 object。它复用 ADR 0008 的 namespace
import 规则：module member 在解析阶段静态绑定，不进入动态值模型，也不改变 C ABI。

分层方向：

- `std/fs`：文件系统 helper，例如 `read_text`、`exists`、`write_text`。
- `std/env`：环境变量 helper，例如 `get`、`list`。
- `std/time`：阻塞计时器 helper，例如 `sleep_ms`。
- `std/net`：网络 probe 或未来网络能力。
- `std/task`：runtime task helper，例如 `sleep_ms`、`ready`、`cancel`。
- `std/math`：运行时数学 helper，例如 `sqrt`。`to_float` / `to_int` 继续留在核心语言
  运行时，不纳入 `std/math` 迁移硬要求。

实现标准库命名空间前，文档和示例继续使用现有全局函数。引入 `std/*` 后，旧全局函数
至少保留一个 minor 阶段；是否警告、如何在 formatter/LSP 中推荐新名字，需要单独设计。

## 后果

v0.0.3 不需要一次性改动 parser、module loader、runtime 安装和示例，因此不会让现有脚本
因为命名策略而失效。同时，后续标准库扩展有了明确方向：能力按模块分层，权限边界更
容易解释，completion 也能在 `fs.` / `env.` 之后给出更聚焦的成员列表。

代价是 v0.0.3 仍会同时存在一组全局函数和未来的模块化方向。文档必须把全局函数标注为
兼容表面，而不是鼓励无限制扩张全局命名空间。

## 备选方案

- 立即把 `read_text` 改成 `std.fs.read_text`：长期更干净，但需要先落地内置 std module
  loader、迁移文档、旧名兼容和 LSP completion 策略，风险超过 v0.0.3 收敛目标。
- 继续所有标准库都放全局：实现最简单，但会持续增加命名冲突和权限来源不清的问题。
- 引入动态 `std` object：调用形式自然，但会绕过当前静态 namespace import 设计，并把
  标准库命名和动态 object 模型耦合。
