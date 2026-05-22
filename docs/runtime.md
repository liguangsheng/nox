# Nox 运行时

`nox_core` 是可嵌入引擎，不直接读文件、开 socket、sleep、读取环境变量或创建任务。
`nox` crate 在 `nox_core` 之上提供默认运行时，安装小型标准库，并通过
`RuntimePermissions` 控制外部能力。

## 构造运行时

无外部能力运行时：

```rust
let mut runtime = nox::Runtime::new();
```

显式授予能力：

```rust
let mut runtime = nox::Runtime::with_permissions(nox::RuntimePermissions {
    filesystem: true,
    filesystem_write: false,
    filesystem_read_roots: Vec::new(),
    filesystem_write_roots: Vec::new(),
    network: false,
    timers: false,
    environment: false,
    async_tasks: false,
});
```

把脚本内文件读写限制到指定目录：

```rust
let mut runtime = nox::Runtime::with_permissions(
    nox::RuntimePermissions::none()
        .allow_filesystem_read_under("project/data")
        .allow_filesystem_write_under("project/out"),
);
```

`RuntimePermissions::cli()` 授予 `filesystem` 读权限，但不授予 `filesystem_write`。
CLI 用它读取入口文件、文件 import 和 `read_text`/`exists`；写文件需要宿主显式
打开 `filesystem_write`。入口文件和 import 读取权限不等于任意脚本读写权限；宿主可以
用 read/write root 把 `read_text`、`exists` 和 `write_text` 限制到指定目录。
`args()` 不需要额外权限；`env_get()`、`env_list()` 和 `std/env.nox` 的 `try_get`
需要 `environment` 权限。

## 权限开关

`RuntimePermissions` 当前包含：

- `filesystem`：允许 `eval_file`、`check_file`、`inspect_bytecode_file`、文件
  import、`read_text(path: str) -> str` 和 `exists(path: str) -> bool`。
- `filesystem_write`：允许 `write_text(path: str, contents: str) -> null`。
  与 `filesystem` 独立，避免一开始就把读权限默认升级成写。
- `filesystem_read_roots`：可选读 allowlist。为空表示读权限不按路径限制；非空时
  `read_text` 和 `exists` 只能访问规范化后位于这些目录下的路径。
- `filesystem_write_roots`：可选写 allowlist。为空表示写权限不按路径限制；非空时
  `write_text` 只能写入规范化后位于这些目录下的路径。
- `environment`：允许 `env_get(name: str) -> str`、`env_list() -> map[str, str]` 和
  `std/env.nox` 的 `try_get(name: str) -> option[str]`。
- `timers`：允许 `sleep_ms(ms: int) -> null`。
- `network`：允许 `tcp_connect(host: str, port: int) -> bool`。
- `async_tasks`：允许 `task_sleep_ms(ms: int) -> int`、`task_ready(id: int) -> bool`
  和 `task_cancel(id: int) -> null`。

权限检查发生在文件加载入口或宿主函数内部。脚本可以通过静态类型检查，但在调用未授权能力时运行失败。

## 标准库

默认运行时安装这些带类型宿主函数。`map_get(map, key) -> option[T]` 是引擎内置的
可恢复 map lookup，不需要运行时权限。文件、环境和时间能力的推荐写法是静态
`std/*` 模块导入：

```nox
import "std/fs.nox" as fs;
import "std/env.nox" as env;
import "std/time.nox" as time;

fs.exists("nox.toml");
env.list();
time.sleep_ms(0);
```

当前可用模块表面：

| 模块 | 成员 | 权限 | 权限不足时的诊断 |
| --- | --- | --- | --- |
| `std/fs.nox` | `read_text(path: str) -> str` | `filesystem` | `filesystem capability is required to call read_text` |
| `std/fs.nox` | `try_read_text(path: str) -> result[str, str]` | `filesystem` | `filesystem capability is required to call try_read_text` |
| `std/fs.nox` | `exists(path: str) -> bool` | `filesystem` | `filesystem capability is required to call exists` |
| `std/fs.nox` | `write_text(path: str, contents: str) -> null` | `filesystem_write` | `filesystem write capability is required to call write_text` |
| `std/env.nox` | `get(name: str) -> str` | `environment` | `environment capability is required to call env_get` |
| `std/env.nox` | `try_get(name: str) -> option[str]` | `environment` | `environment capability is required to call env_try_get` |
| `std/env.nox` | `list() -> map[str, str]` | `environment` | `environment capability is required to call env_list` |
| `std/time.nox` | `sleep_ms(ms: int) -> null` | `timers` | `timer capability is required to call sleep_ms` |

导入 `std/*` 模块只提供静态表面，不会授予权限。缺失的内置标准库模块返回
`module.not-found`，缺失成员返回 `module.member-not-found`。普通项目中的
`std/fs.nox` 文件不会覆盖内置模块；如需导入用户文件，使用相对路径，例如
`import "./std/fs.nox" as local_fs;`。

旧全局函数仍作为兼容表面保留：

```text
sqrt(value: float) -> float
args() -> [str]
read_text(path: str) -> str
exists(path: str) -> bool
write_text(path: str, contents: str) -> null
env_get(name: str) -> str
env_list() -> map[str, str]
sleep_ms(ms: int) -> null
tcp_connect(host: str, port: int) -> bool
task_sleep_ms(ms: int) -> int
task_ready(id: int) -> bool
task_cancel(id: int) -> null
```

`sqrt` 不需要外部能力，总是可用。核心数值转换由 `nox_core` 自身提供：

```text
to_float(value: int) -> float
to_int(value: float) -> int
```

这些全局函数是 v0.0.3 兼容表面，至少保留到 v0.0.4 完成。当前不对旧全局函数发 warning，
formatter 也不会自动改写旧调用；迁移建议只在文档和示例中体现。新增标准库能力默认不继续扩大全局命名空间；长期分层
策略见 [0012 - 标准库命名分层策略](decisions/0012-stdlib-namespace-strategy.md)，
`std/*` 静态模块加载规则见
[0013 - std/* 静态模块加载](decisions/0013-stdlib-module-loader.md)。导入
`std/fs.nox` 不会授予文件系统权限，权限仍在调用具体函数时检查。

`read_text` 和 `exists` 需要 `filesystem` 读权限。`write_text` 需要 `filesystem_write`
权限：未授予时返回 `filesystem write capability is required` 诊断且不创建/修改
文件。配置 root allowlist 后，运行时会把相对路径转成当前进程工作目录下的绝对路径，
并对已存在路径使用 `canonicalize` 解析符号链接和 `..`。规范化后的路径必须位于对应
root 下，否则返回 `filesystem read permission denied` 或
`filesystem write permission denied`。空路径返回 `invalid filesystem path`。
如果目标文件尚不存在，运行时会向上找到最近的已存在父路径并解析符号链接，再拼回缺失的
后缀；因此经由 allowlist 内部 symlink 写到外部目录的缺失文件也会被拒绝。
权限未授予和文件系统 allowlist 拒绝使用稳定诊断 code `permission.denied`；普通 I/O
失败、无效路径和参数错误仍按各自错误语义报告。
缺失但位于允许 root 下的路径仍按普通 I/O 行为处理：`exists` 返回 `false`，
`read_text` / `write_text` 返回底层 not found 诊断，`std/fs.nox` 的 `try_read_text`
返回 `err(message)`。权限不足、allowlist 越界和无效路径仍是 diagnostic，不会被包装成
`err`。

`args` 返回宿主注入的位置参数。`nox run script.nox a b` 中，脚本看到的是
`["a", "b"]`，不包含 `nox`、子命令或入口路径。

`env_get` 读取环境变量，缺失变量会产生诊断。`std/env.nox` 的 `try_get` 把缺失变量
表达为 `none`，存在时返回 `some(value)`；权限不足和非 UTF-8 环境值仍是 diagnostic。
`env_list` 返回当前进程环境的字典；如果环境变量名或值不是有效 Unicode，会返回诊断，
不会把进程环境读取 panic 暴露给脚本。这些环境能力都需要 `environment` 权限。`sleep_ms`
阻塞当前线程，参数必须是非负毫秒。

`tcp_connect(host, port) -> bool` 当前是连通性 **probe**：它尝试建立 TCP
连接、读取握手结果、再立即关闭，**返回成功与否的布尔值**。它不是 socket API；
脚本拿不到 file descriptor、不能 read/write、也无法保持连接。如果需要真正的
socket，宿主应自己注册更完整的 host function。port 必须落在 `0..=65535`。
未授予 `network` 时调用会返回 `permission.denied` 诊断；授予后，连接被拒绝、DNS
解析失败或其他普通连接失败会返回 `false`，不会升级成权限诊断。

`task_sleep_ms` 创建一个内部 sleep task 并返回非负 `int` 作为 task id。id 在同一个
`Runtime` 内单调递增，不复用。当前 task 状态机：

| 状态 | 进入方式 | 后续行为 |
| --- | --- | --- |
| pending | `task_sleep_ms(ms)` 成功返回 id | `task_ready(id)` 在 deadline 前返回 `false`，任务保留。 |
| completed | `task_ready(id)` 到达 deadline | 返回 `true`，同时释放任务；之后 id 变成 unknown。 |
| cancelled | `task_cancel(id)` 成功 | 返回 `null`，同时释放任务；之后 id 变成 unknown。 |
| rejected | `task_sleep_ms` 收到负数或未授权 | 不创建 id，不进入任务表。 |
| unknown | 从未创建、已 completed 或已 cancelled 的 id | `task_ready` / `task_cancel` 返回 `unknown async task id` 诊断。 |

`Runtime::pending_async_task_count()` 返回当前 pending task 数，用于宿主观察和测试释放
行为。完成和取消都会把任务从表中移除；重复 poll completed task、poll cancelled task、
或 cancel unknown task 都是诊断。

这些 task helper 是最小宿主函数，不是 event loop，也不是语言级 async 语法。任务状态只活在
`Runtime` 实例内部，跨 `Runtime` 不共享；CLI 和嵌入 API 使用同一个 `Runtime`
实现，因此生命周期语义一致。`task_cancel` 只取消指定 pending task id；VM instruction
budget 耗尽不会自动取消已经存在的 pending task。`Runtime::eval` 或 `Runtime::run_test_file`
这类顶层调用失败时，会释放本次调用中新建且仍 pending 的 task；instruction budget 耗尽也按
顶层调用失败处理。失败前已经由更早调用创建的 pending task 仍保留，直到宿主继续 poll、cancel
或丢弃整个 `Runtime`。

v0.0.6 评估后继续暂缓脚本级 `task_status` / `task_poll` API。原因是当前 completed 和
cancelled task 都会立即释放，新增非消费式状态查询需要先定义稳定 `TaskStatus` 表示、unknown id
是否可恢复、以及 completed/cancelled tombstone 的生命周期。决策记录见
[0016 - 暂缓 async task 状态 API](decisions/0016-defer-async-task-status-api.md)。

## 文件加载和 import

`Runtime::eval_file(path)` 读取 `path`，并安装以入口文件目录为根的 module loader。
脚本中的：

```nox
import "math.nox";
```

会解析为入口文件目录下的 `math.nox`。重复 import 同一 specifier 在一次编译中只加载一次；
循环 import 会诊断失败。

文件加载能力只属于 `nox` 运行时。直接使用 `nox_core::Engine` 的宿主必须自己提供 loader。

## 取消执行

运行时可以设置 instruction budget。VM 每执行一条 bytecode instruction 消耗一次预算。
预算耗尽时，当前 VM 执行停止并返回诊断；`Engine` / `Session` 本身仍可复用，宿主重置
预算后可以继续执行后续源码。使用 `None` 表示不限制。

```rust
let mut runtime = nox::Runtime::new();
runtime.set_instruction_budget(Some(10_000));
```

嵌入 `nox_core` 时也可以直接在 `Engine` 上设置同一预算。

instruction budget 的边界是 VM bytecode，不是线程抢占。已经进入的 host function、host
callback、文件/环境/网络调用，以及 `sleep_ms` 这类阻塞当前线程的 timer helper，不会被 VM
预算中途打断；宿主如果需要这些操作具备超时或取消能力，应在自己的 host function 内实现。
当前运行时也不包含多线程 event loop。

## 运行时错误

运行时错误包括：

- 数组越界。
- map key 不存在。
- 整数或浮点除零。
- 整数溢出。
- float 非有限结果。
- 调用未授权的宿主能力。
- 宿主函数返回值不符合声明类型。

Nox v0.0.5-dev 已有语言级 `option[T]` / `result[T, E]` 类型、构造和受限 `match`
解包，并开始把可预判失败迁移到可恢复返回值。推荐写法：

- 文件读取：调用 `std/fs.nox` 的 `try_read_text(path) -> result[str, str]`。
- map 读取：调用 `map_get(map, key) -> option[T]`。
- 环境变量：调用 `std/env.nox` 的 `try_get(name) -> option[str]`。
- TCP 连通性：使用 `tcp_connect(host, port) -> bool` 作为 probe。

旧的 `exists` + `read_text`、`contains` + index、`env_list` + `contains` guard 仍可用，
不会变成 warning 或 error。未通过可恢复 API 或 guard 的缺失值、权限错误和宿主 callback
错误继续以 diagnostic 形式中止当前执行。v0.0.3 暂缓依据见
[0009 - 暂缓语言级 option / result](decisions/0009-defer-option-result.md)；v0.0.4 重启设计见
[0014 - 重启 option / result 设计但暂不实现](decisions/0014-restart-option-result-design.md)。

诊断会带 span 和 message；部分稳定类别会带 `code`，例如 `runtime.division-by-zero`。

## 后续边界

- 文件系统标准库默认不按路径限制；宿主可以通过 `filesystem_read_roots` /
  `filesystem_write_roots` 或 `allow_filesystem_*_under` helper 把脚本读写限制到指定 root。
- 网络 API 当前是最小 `tcp_connect` probe。如果未来需要真实的 socket，会
  在 PLAN 中先写设计文档（包含 fd 生命周期、并发模型、与 async task 的
  集成）。
- async task helper 当前只覆盖 sleep；未来增加 IO 任务时，`task_cancel` 和
  `task_ready` 的"未知 id"约定保持不变。脚本级 task status API 的重启条件见
  [0016 - 暂缓 async task 状态 API](decisions/0016-defer-async-task-status-api.md)。
