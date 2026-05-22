# 架构

Nox 拆成一个小核心引擎和一个宿主可控的默认运行时。

- `nox_core` 是可嵌入引擎，负责源码处理、词法分析、解析、静态类型检查、
  字节码编译、VM 执行、值模型、诊断、宿主函数注册、取消执行和 C ABI。
- `nox` 是默认运行时和 CLI，负责文件系统加载、相对 import 策略、标准库宿主函数、
  命令行参数注入、权限控制、LSP 和命令行体验。

核心边界很重要：`nox_core` 不读文件、不访问网络、不读环境变量、不睡眠，也不创建
运行时任务。所有外部能力都由宿主或 `nox` 运行时显式授予。

## 当前实现切片

当前实现已经超过空 scaffold，具备以下能力：

- `nox_core` 将源码切成带 byte span 的 token。
- parser 支持带类型变量、带类型函数、字面量、赋值、调用、块、`if`、`match`、`while`、
  半开 `for` range、`return`、数组、map、record、字段访问和 import/export。
- type checker 在编译前检查静态类型。变量、参数、返回值都要求显式类型。
- compiler 将 AST 编译成 flat bytecode instruction stream。
- VM 执行已验证的 bytecode，并按指令预算支持取消执行。
- import 由 `nox_core` 定义语义，源码由宿主 loader 提供；默认 `nox` 运行时使用入口文件目录解析相对路径。
- `nox_core` 暴露 Rust API 和 C ABI，可注册带类型宿主函数并返回标量/字符串结果。
- `nox` 默认 stdlib 提供 `args()` 和 `env_list()`；前者返回 `run` 的位置参数，
  后者在 `environment` 权限下返回环境字典。
- `nox` 提供 `run`、`check`、`test`、`fmt`、`lsp`、`inspect-bytecode` 命令。
- `check --json` 和 LSP diagnostics 输出稳定的 diagnostic `code`。

## 执行流水线

一次正常检查或执行大致经过：

```text
source
  -> lexer
  -> parser
  -> import resolver
  -> type checker
  -> bytecode compiler
  -> verifier
  -> VM execution
```

`check` 到 verifier 为止，不执行程序。`run` 会继续进入 VM。`test` 会执行测试模块，
再调用顶层 `test_*` 函数并收集结果。`fmt` 只需要 lexer/parser 并打印稳定格式化结果。
`inspect-bytecode` 到 compiler/verifier 后打印 bytecode。

## Bytecode 与 verifier

bytecode 模块只暴露 VM 实际执行的 flat instruction stream。语句级编译结构保持内部实现细节，
避免公开两个调试表面。

verifier 目前检查：

- jump target 是否有效。
- stack 是否下溢。
- lexical scope 是否平衡。
- map、record、field 等复合指令的基础 stack effect。

后续 verifier 可以扩展到控制流合流 stack height、`break`/`continue` 和更复杂的 return 分析。

## 堆和值模型

脚本字符串、函数、数组、map 和 record 通过 `GcHeap` 跟踪。宿主可以读取 live object count，
也可以通过 `Engine::collect_garbage` 触发回收。

当前堆模型用 weak handle 做对象数量跟踪，并让脚本函数通过 weak environment 避免递归定义形成永久引用环。
这不是最终 GC 设计，但已经为后续 tracing heap 或 handle arena 留出了边界。

## 语言表面

规范语言说明见 [language-v0.md](language-v0.md)。架构层只保留一个简短示例：

```nox
import "math.nox";

record User {
    name: str,
    score: int,
}

fn add(left: int, right: int) -> int {
    return left + right;
}

let total: int = 0;
for i in 0..5 {
    total = add(total, i);
}

let user: User = User { name: "nox", score: total };
user.score;
```

## 运行时边界

默认运行时安装的宿主函数包括：

```text
sqrt(value: float) -> float
args() -> [str]
env_get(name: str) -> str
env_list() -> map[str, str]
sleep_ms(ms: int) -> null
tcp_connect(host: str, port: int) -> bool
task_sleep_ms(ms: int) -> int
task_ready(id: int) -> bool
```

`std/env.nox` 还提供模块成员 `try_get(name: str) -> option[str]`，用于可恢复处理缺失
环境变量；`std/fs.nox` 提供 `try_read_text(path: str) -> result[str, str]`，用于可恢复处理
普通读取失败。除了 `sqrt` 和 `nox_core` 内置数值转换外，外部能力由
`RuntimePermissions` 控制。
CLI 默认只授予文件系统读能力，用于读取入口文件和 import；脚本内文件读写可以由宿主
进一步限制到指定 root。

## 嵌入边界

Rust 宿主通过 `Engine`、`HostFunctionBuilder`、`Type`、`Value`、`Diagnostic` 等公开类型嵌入。
lexer token、AST、bytecode 内部结构和 heap 实现保持 crate-private。

C ABI 位于 `crates/nox_core/include/nox_core.h`。v0 C host callback 边界支持
`null`、`bool`、`int`、`float` 标量；`eval` 可以返回 owned string，宿主必须用
`nox_core_string_free` 释放。

## 设计约束

- 新语法先写清类型规则，再落 parser、type checker、bytecode、VM、CLI、文档和测试。
- 系统能力放在 `nox` 或宿主，不放进 `nox_core`。
- Rust API 和 C ABI 暴露后按兼容契约维护。
- 内部 compiler/VM 结构可以演进，但公开命令和嵌入边界需要文档和测试覆盖。
