# 嵌入 Nox

Nox 拆成 `nox_core` 和 `nox`：

- 需要直接控制源码加载、宿主函数、取消执行和权限时，嵌入 `nox_core`。
- 需要默认文件运行时、标准库和 CLI 行为时，使用 `nox`。

语言表面见 [language-v0.md](language-v0.md)，默认运行时权限见 [runtime.md](runtime.md)。

## Rust API

稳定 Rust 嵌入表面保持小而明确：

- `Engine`：一次性求值、检查、模块加载、instruction budget、heap collection。
- `Session` / `ModuleGraph`：长期会话、import 源码缓存和 open document overlay。
- `HostFunctionBuilder`：声明带类型宿主回调。
- `Type` 和 `Value`：宿主函数签名和值边界。
- `Diagnostic`、`SourceLocation`、`Span`：错误报告。

lexer token、parser AST、bytecode module、verifier 内部结构和 heap 实现类型是 crate-private。
宿主应使用 `Engine::check`、`Engine::check_diagnostics`、`Engine::eval`、
`Engine::hover_type`、`Engine::format_source` 和 `Engine::inspect_bytecode`。

诊断包含稳定 `code` 字段，供机器消费；人类可读 `message` 继续用于 CLI。Rust API、
`nox check --json` 和 LSP diagnostics 会暴露同一套 code。

### Rust API 分层

生产级发布前，Rust 宿主应按下面的分层使用 `nox_core` 和 `nox`。未列入稳定或实验表面的
crate-private 类型、模块和测试 helper 都不构成兼容承诺。

| 层级 | API | 承诺 |
| --- | --- | --- |
| 稳定嵌入入口 | `Engine::new`、`Engine::eval`、`Engine::check`、`Engine::check_diagnostics`、`Engine::run_tests` | 适合宿主直接依赖；签名、诊断传播和复用语义变化必须进入 CHANGELOG。 |
| 稳定宿主扩展 | `HostFunctionBuilder`、`Engine::register_host_function`、`Type`、`Value`、`Value::{string,array,map,some,none,ok,err}` | 用于声明 host function 和传递值；新增类型或改变返回值 ownership 需要同步 Rust/C 文档。 |
| 稳定错误表面 | `Diagnostic`、`Diagnostic::{new,with_code,with_source}`、`Span`、`SourceLocation` | `code`、byte span 和 source location 是机器契约；`message` 只保证可读，不作为稳定分支条件。 |
| 稳定会话入口 | `Session`、`Session::engine_mut`、`Session::{set_module_loader,clear_module_cache,set_module_overlay,remove_module_overlay,eval,check,check_diagnostics,hover_type}`、`ModuleGraph::{new,clear_cache,set_overlay,remove_overlay,cached_source}` | 适合编辑器和长期宿主会话；overlay 优先级、缓存失效和 module loader 错误语义必须保持文档一致。 |
| 稳定运行时入口 | `nox::Runtime`、`RuntimePermissions`、`RuntimePermissions::{none,cli,allow_filesystem_read_under,allow_filesystem_write_under}`、`Runtime::{new,with_permissions,set_args,set_instruction_budget,pending_async_task_count,eval,eval_file,check_file,check_file_diagnostics,run_test_file,format_file,hover_type_source_with_base}` | `nox` 默认运行时表面；危险能力默认关闭，新增权限位不得隐式授权。 |
| 工具/实验表面 | `Engine::{inspect_bytecode,inspect_bytecode_compact,format_source,hover_type,collect_garbage,heap_object_count,set_instruction_budget}` | 当前用于 CLI、调试、编辑器和回归测试；可继续使用，但输出格式、heap 统计细节和 bytecode 文本不作为长期稳定数据模型。 |
| 内部不承诺 | lexer token、parser AST、bytecode module、verifier、VM、heap layout、crate-private compiler/runtime helper | 不面向宿主；正式文档不得要求下游依赖这些结构。 |

如果某 API 从工具/实验层提升为稳定层，必须同批补文档、CHANGELOG 和至少一个 embedding regression。

## 兼容矩阵

v0.0.x 本地开发阶段仍允许调整公共表面，但本地版本 checkpoint 前必须按下表审计。patch release
默认保持同一 minor 内兼容，除非 CHANGELOG 明确说明安全或正确性 break。

| 表面 | 当前承诺 | 可兼容扩展 | 破坏性变更 |
| --- | --- | --- | --- |
| `Engine` | 稳定 Rust 嵌入入口；`eval`、`check`、`check_diagnostics`、host function、module loader、budget、heap 观察语义需要文档同步。 | 新增方法、新增非必填配置、放宽诊断收集能力。 | 改已有方法签名、改变默认 intrinsics、改变 host function 类型检查或 budget 失败语义。 |
| `Session` / `ModuleGraph` | 稳定高级会话入口；缓存源码字符串、overlay 优先级和 `engine_mut()` 兼容简单 API。 | 新增缓存统计、清理粒度、只读查询方法。 | 改 overlay 优先级、让权限隐式进入 session、暴露 AST/bytecode 作为稳定结构。 |
| `HostFunctionBuilder` / `Type` / `Value` | 稳定宿主函数和值边界；容器值可被 Rust 宿主持有，函数值不能跨 C ABI 调用。 | 新增类型 variant 或构造 helper，前提是 docs、formatter、C/Rust API 表示同步。 | 改现有 variant 含义、改变容器相等性/ownership、让 `Value` 跨 engine 共享变成隐式支持。 |
| `Diagnostic` / `Span` / `SourceLocation` | 稳定结构化错误载体；`code` 是机器契约，`message` 可优化。 | 新增稳定 code、新增 source 填充场景。 | 删除或重命名稳定 code、改变 span 为非 byte offset、让 JSON/LSP code 与 Rust API 分裂。 |
| `Runtime` / `RuntimePermissions` | 默认运行时 API；权限显式，入口/import 读文件不等于脚本获得任意文件能力。 | 新增权限位、allowlist helper、观测方法。 | 默认授予危险能力、让 manifest `runtime.permissions` 自动授权、改变 async task id 生命周期。 |
| C enums | `NoxCoreStatus`、`NoxCoreValueKind` 数值是 ABI 契约。 | 只允许在末尾追加 enum 值。 | 删除、重排或改已有数值。 |
| `NoxCoreValue` | `repr(C)` value carrier；string 和 compound handle 所有权由 matching free 函数释放。 | 末尾追加字段需要 minor release 和 header/docs 同步。 | 改字段顺序/类型、改变已有 kind 的所有权规则。 |
| C engine functions | exported symbol 和签名是 ABI 契约；null pointer 返回规则和 last_error 生命周期要保持。 | 在 header 末尾新增函数。 | 改已有函数参数/返回值、改变 error slot 清理时机、让 returned pointer 所有权转移规则变化。 |
| C callback / userdata | callback 同步执行；`ctx` 由宿主管，`ctx == NULL` 时使用 engine userdata。 | 新增辅助 setter/getter 或错误查询函数。 | Nox 解引用/free `ctx`、改变 userdata 优先级、让 callback 变成跨线程或 reentrant 契约。 |

### v0.0.6 兼容复审结论

v0.0.6 到阶段 30.4 为止没有新增 Rust embedding API、C enum、C struct 字段或 exported C
function。30.1 只把普通文件 import 失败提升为稳定 `module.not-found` 诊断 code；这属于
`Diagnostic.code` 的兼容扩展。30.2 明确暂缓脚本级和 C ABI async task status API，现有
`Runtime::pending_async_task_count()` 仍是唯一宿主观察入口。30.3 新增的
`nox.project-check.v1` 是 CLI schema，不改变 `nox_core`、`Runtime` 或 C ABI。

因此 v0.0.6 当前 embedding 边界保持：

- `crates/nox_core/include/nox_core.h` 不追加 symbol，不改 enum 数值，不改 `NoxCoreValue`
  layout。
- C callback 仍同步执行；`ctx == NULL` 时继续使用 engine userdata。
- `nox_core_engine_last_error` 仍由 engine 持有，不要求宿主释放；`clear_error` 显式清空。
- array/map/record/option/result 继续只读 owning handle，仍由对应 free 函数释放。
- async task id 生命周期不进入 C ABI，completed/cancelled id 继续释放后变成 unknown。

## Rust 宿主函数

创建 engine，注册带类型宿主函数，然后执行源码：

```rust
use nox_core::{Engine, HostFunctionBuilder, Type, Value};

let mut engine = Engine::new();

engine.register_host_function(
    HostFunctionBuilder::new("host_add", Type::Int)
        .param("left", Type::Int)
        .param("right", Type::Int),
    |args| match args {
        [Value::Int(left), Value::Int(right)] => Ok(Value::Int(left + right)),
        _ => unreachable!("static type checking guarantees the argument types"),
    },
)?;

let value = engine.eval("host_add(20, 22);")?;
assert_eq!(value, Value::Int(42));
```

宿主函数签名会进入静态 type checker 的全局作用域。callback 返回值在交回脚本前会再次
检查是否匹配声明返回类型。

callback 返回的 `Err(Diagnostic)` 会在脚本端被包成 `host function '<name>': <message>`
诊断，让 CLI 输出和 LSP diagnostics 能直接识别错误来源。如果 callback 自己
已经在 message 中写明 `host function` / `host callback`，则保留原文。callback
panic 会被隔离成 `host.callback` 诊断，`Engine` 仍可继续复用；返回类型不匹配声明时给出
`host function '<name>' returned <T>, expected <U>`。

`Engine::new()` 会安装核心数值转换：

```text
to_float(value: int) -> float
to_int(value: float) -> int
```

## 模块加载

`nox_core` 不读文件。宿主通过 loader 提供 import 源码：

```rust
use nox_core::{Diagnostic, Engine, Span};

let mut engine = Engine::new();
engine.set_module_loader(|specifier| {
    if specifier == "math.nox" {
        Ok("fn double(value: int) -> int { return value * 2; }".to_string())
    } else {
        Err(Diagnostic::new("module not found", Span { start: 0, end: 0 }))
    }
});

let value = engine.eval(r#"
    import "math.nox";
    double(21);
"#)?;
```

import 在类型检查和编译前解析，所以导入声明和入口文件声明共享同一静态检查路径。

## 长期 Session

长期宿主或 LSP 可以使用 `Session` 复用 module graph。第一批 `ModuleGraph` 只缓存
import specifier 对应的源码字符串，不暴露 AST、typecheck 环境或 bytecode：

```rust
use nox_core::{Session, Value};

let mut session = Session::new();
session.set_module_loader(|specifier| {
    assert_eq!(specifier, "math.nox");
    Ok("fn double(value: int) -> int { return value * 2; }".to_string())
});

let source = r#"
    import "math.nox";
    double(21);
"#;

assert_eq!(session.eval(source)?, Value::Int(42));
assert_eq!(session.eval(source)?, Value::Int(42));
```

第二次 `eval` 会复用 `math.nox` 的缓存源码，不再调用 loader。宿主知道磁盘文件变化时，
可以调用 `clear_module_cache()` 清空缓存。

编辑器可以用 overlay 覆盖某个 import specifier 的源码：

```rust
session.set_module_overlay(
    "math.nox",
    "fn double(value: int) -> int { return value * 3; }",
);
```

overlay 优先于缓存和 loader。`remove_module_overlay(specifier)` 会同时移除 overlay 和同名缓存，
下一次调用会重新走 loader。`Session::engine_mut()` 暴露底层 `Engine`，用于注册 host function、
设置 instruction budget 或执行其他简单 API。runtime permission 仍由宿主或 `nox` runtime 管理，
不会因为源码模块进入同一个 `Session` 而自动获得文件、网络、环境或定时器能力。

## 长期宿主 cookbook

长期进程嵌入 Nox 时，把语言核心和系统能力分开初始化：

1. 用 `Session` 或 `Engine` 承载纯语言求值、host function 和模块缓存。
2. 对需要 CLI 风格标准库的脚本单独创建 `Runtime`，并显式传入 `RuntimePermissions`。
3. 把文件、环境、网络、定时器和 async task 能力当成宿主策略，不从 manifest 或 import
   自动授权。
4. 每次调用后读取 `Diagnostic::code` 和 `message`；机器分支用 `code`，日志用 `message`。
5. 对长期持有的 `Value` 明确生命周期；释放宿主持有值后再调用 `collect_garbage()` 做内存回收。

下面的 Rust 片段覆盖 host callback 错误、可恢复文件读取、权限边界和 async task 清理：

```rust
use nox::{Runtime, RuntimePermissions};
use nox_core::{Diagnostic, Engine, HostFunctionBuilder, Span, Type, Value};
use std::{env, fs};

let mut engine = Engine::new();
engine.register_host_function(
    HostFunctionBuilder::new("host_config", Type::Str),
    |_| Err(Diagnostic::new("config unavailable", Span { start: 0, end: 0 })),
)?;

let err = engine.eval("host_config();").unwrap_err();
assert_eq!(err.code, "error");
assert!(err.message.contains("host function 'host_config'"));

let dir = env::temp_dir().join("nox-host-cookbook");
fs::create_dir_all(&dir)?;
let data = dir.join("input.txt");
let script = dir.join("main.nox");
fs::write(&data, "from host")?;
fs::write(
    &script,
    format!(
        r#"import "std/fs.nox" as fs;

fn load(path: str) -> str {{
    let loaded: result[str, str] = fs.try_read_text(path);
    match (loaded) {{
        ok(body) => {{
            return body;
        }}
        err(message) => {{
            return message;
        }}
    }}
}}

load("{}");
"#,
        data.display()
    ),
)?;

let mut runtime = Runtime::with_permissions(
    RuntimePermissions::none().allow_filesystem_read_under(&dir),
);
let value = runtime.eval_file(&script)?;
assert!(matches!(value, Value::String(_)));

let denied_script = dir.join("denied.nox");
fs::write(
    &denied_script,
    r#"import "std/fs.nox" as fs;

fs.try_read_text("../Cargo.toml");
"#,
)?;
let denied = runtime
    .eval_file(&denied_script)
    .unwrap_err();
assert!(denied.message.contains("filesystem read permission denied"));

let mut tasks = Runtime::with_permissions(RuntimePermissions {
    async_tasks: true,
    ..RuntimePermissions::none()
});
let before = tasks.pending_async_task_count();
let err = tasks.eval("task_sleep_ms(60000); task_ready(999);").unwrap_err();
assert!(err.message.contains("unknown async task id"));
assert_eq!(tasks.pending_async_task_count(), before);
```

如果宿主需要文件能力但不想暴露整个进程工作目录，优先使用
`allow_filesystem_read_under(path)` / `allow_filesystem_write_under(path)`。`std/fs.nox`
的 `try_read_text` 只把普通 I/O 失败变成 `err(message)`；权限不足、allowlist 越界和无效路径
仍是 diagnostic，方便宿主区分“用户数据缺失”和“能力策略拒绝”。

## C ABI

C ABI 位于 `crates/nox_core/include/nox_core.h`。当前边界刻意保守：

- C host callback 支持 `null`、`bool`、`int`、`float`。
- `nox_core_engine_eval` 可以返回 owned string。
- `nox_core_engine_eval` 返回 array、map、record、option、result 时会给出只读 owning
  handle。
- function 只报告 value kind，不暴露调用或读取 API；C host callback 注册不等同于
  脚本函数 handle，跨 ABI 调用脚本函数需要另开设计。

### C ABI 兼容矩阵

`nox_core.h` 是 C 宿主的权威入口。Rust `ffi.rs` 与 header 必须保持下列矩阵一致；新增或修改任一项
都要同步 header、文档、CHANGELOG 和 C smoke。

| 表面 | 当前 ABI | Ownership / 生命周期 | 稳定性要求 |
| --- | --- | --- | --- |
| `NoxCoreStatus` | `OK=0`、`NULL_POINTER=1`、`INVALID_UTF8=2`、`ERROR=3` | 按值返回，无 ownership。 | 现有数值不可重排或复用；只能在末尾追加。 |
| `NoxCoreValueKind` | `NULL=0`、`BOOL=1`、`INT=2`、`FLOAT=3`、`STRING=4`、`FUNCTION=5`、`ARRAY=6`、`MAP=7`、`RECORD=8`、`OPTION=9`、`RESULT=10` | 按值传递，无 ownership。 | 现有数值不可重排或复用；新增 kind 只能末尾追加并补转换测试。 |
| `NoxCoreValue` | `repr(C)` value carrier，包含 scalar 字段、owned string pointer 和 owned compound handle pointer。 | `STRING` 的 `string_value` 用 `nox_core_string_free` 释放一次；compound handle 用匹配的 `*_free` 释放一次；scalar 无需释放。 | 字段顺序、字段类型和已有 kind 的 ownership 不可静默改变。 |
| `NoxCoreEngine` | opaque engine handle。 | `nox_core_engine_new` 创建，`nox_core_engine_free` 释放一次；释放后所有 engine 相关 pointer 失效。 | 已有 engine 函数签名、null pointer 返回规则和 last_error 行为不可静默改变。 |
| array/map/record/option/result handle | opaque read-only owning handle。 | 每个 handle 由 eval 或读取函数返回给宿主，宿主用对应 free 函数释放；读取返回的新 `NoxCoreValue` 按自身 kind 的 ownership 规则处理。 | handle 只读；不得要求宿主理解内部 layout。 |
| `nox_core_engine_last_error` | 返回 engine 持有的 null-terminated string pointer。 | 宿主不释放；下一次会设置错误的 engine 调用、`clear_error` 或 `engine_free` 后 pointer 失效。 | 错误字符串生命周期必须保持 engine-owned。 |
| `NoxCoreHostCallback` | 同步 callback，`ctx`、args、arg_count、out_value 由 Nox 传入。 | `ctx` 由宿主管理；args 只在 callback 调用期间有效；callback 成功时写入 `out_value`。 | Nox 不解引用/free `ctx`；不承诺 callback 跨线程或 reentrant；callback 不得 unwind 穿过 C ABI。 |

enum 数值由 `api_tests::c_abi_enum_values_are_stable` 固定；header 声明与动态库 exported symbol
的一致性、header 编译和 C smoke 由 `scripts/embedding-regression.sh` 覆盖。

最小示例：

```c
#include "nox_core.h"

static NoxCoreStatus double_int(
    void *ctx,
    const NoxCoreValue *args,
    size_t arg_count,
    NoxCoreValue *out_value
) {
    (void)ctx;
    if (arg_count != 1 || args[0].kind != NOX_CORE_VALUE_INT) {
        return NOX_CORE_ERROR;
    }
    out_value->kind = NOX_CORE_VALUE_INT;
    out_value->bool_value = false;
    out_value->int_value = args[0].int_value * 2;
    out_value->float_value = 0.0;
    return NOX_CORE_OK;
}

NoxCoreEngine *engine = nox_core_engine_new();
NoxCoreValueKind params[] = { NOX_CORE_VALUE_INT };

nox_core_engine_register_host_function(
    engine,
    "double",
    params,
    1,
    NOX_CORE_VALUE_INT,
    double_int,
    NULL
);
```

也可以把共享上下文挂在 engine 上，并在注册 host function 时传 `NULL` 作为 callback `ctx`：

```c
typedef struct HostState {
    int64_t offset;
} HostState;

HostState state = {21};
nox_core_engine_set_userdata(engine, &state);

nox_core_engine_register_host_function(
    engine,
    "add_offset",
    params,
    1,
    NOX_CORE_VALUE_INT,
    add_offset,
    NULL
);
```

### Callback `ctx` 生命周期

`nox_core_engine_register_host_function` 的 `ctx` 是宿主提供的不透明指针，
直接转交给 callback。Nox 不解引用 `ctx`，也不为它做任何 ownership 处理。
约定如下：

- `ctx` 必须在 callback 仍可能被调用的整个时间段内保持有效。这覆盖从
  `register_host_function` 调用到 `nox_core_engine_free(engine)` 之间。
- 如果注册时 `ctx == NULL`，callback 会收到当前 engine userdata。宿主可以用
  `nox_core_engine_set_userdata` 更新这个指针，用 `nox_core_engine_userdata` 读取当前值。
  Nox 只保存指针值，不解引用、不复制、不释放。
- callback 在 engine 调用线程上同步执行；Nox 不为多线程 host 提供保证。
  宿主自己负责跨线程同步。
- 同一个 host function 名字注册两次会覆盖前一个 callback，但旧 `ctx`
  仍然不会被 Nox 释放——这是宿主自己分配的内存。
- 如果 `ctx` 指向堆分配的对象，宿主应在 `nox_core_engine_free` 之后再释放
  它；如果使用静态或栈对象，确保生命周期长于 engine。

### Callback 错误来源

callback 返回 `NOX_CORE_OK` 以外的状态时，Nox 把它转换为脚本端诊断：
`host callback '<name>' returned status <Status>`。诊断里的 `<name>` 是
注册时的 host function 名，便于 CLI 输出和 LSP diagnostics 直接定位是哪个
宿主回调失败。`out_value->kind` 不支持时给出
`host callback '<name>' returned unsupported value kind`，约定一致。

更具体的错误细节通过 `nox_core_engine_last_error` 暴露：callback 自己负责
在宿主侧日志或 userdata 指向的错误槽中记录。当前 C ABI 不提供 callback 内部直接写
engine last_error 的 setter；Nox 写入的 last_error 只承诺包含 host function 名和 status。

### 宿主错误、panic 和重入边界

Rust host callback 的 `Err(Diagnostic)` 会保留更具体的 `code`；未细分错误会提升为
`host.callback`。Rust callback panic 会被隔离成 `host.callback` 诊断，`Engine` 可继续复用。

C callback 通过 `NoxCoreStatus` 表达失败；`NOX_CORE_OK` 以外的状态会写入 engine last_error，
并在脚本端表现为 host callback 诊断。C callback 不能让 panic/unwind 穿过 `extern "C"` 边界；
需要捕获语言运行时异常的宿主应在 callback 内部转换为 `NOX_CORE_ERROR` 或其他状态。

Rust 和 C callback 都是同步调用，运行在当前 eval/check 调用线程上。Nox 不提供跨线程安全保证，
也不把同一个 engine 的 reentrant eval 作为稳定能力；需要从 callback 触发额外脚本执行的宿主，应使用
独立 `Engine` / `Runtime` 实例或在 callback 返回后由宿主调度。

常用函数：

- `nox_core_version`：返回静态 NUL 结尾版本字符串，不需要释放。
- `nox_core_engine_check`：解析、类型检查和编译，但不运行。
- `nox_core_engine_eval`：执行源码。
- `nox_core_engine_set_userdata` / `nox_core_engine_userdata`：设置或读取 engine 级
  宿主上下文指针。
- `nox_core_engine_last_error`：读取 engine 持有的最近错误字符串。
- `nox_core_engine_clear_error`：清空错误槽。
- `nox_core_engine_free`：释放 engine。

当 `nox_core_engine_eval` 返回 `NOX_CORE_VALUE_STRING` 时，`NoxCoreValue.string_value`
是宿主持有的 owned C string，必须调用 `nox_core_string_free` 正好一次。非字符串 value kind
会把 `string_value` 设为 `NULL`。

当返回 `NOX_CORE_VALUE_ARRAY`、`NOX_CORE_VALUE_MAP`、`NOX_CORE_VALUE_RECORD`、
`NOX_CORE_VALUE_OPTION` 或 `NOX_CORE_VALUE_RESULT` 时，`NoxCoreValue` 对应的
`array_handle`、`map_handle`、`record_handle`、`option_handle` 或 `result_handle`
是宿主持有的只读 owning handle，必须分别调用匹配的 free 函数正好一次。handle 只支持读取：

- `nox_core_array_len`、`nox_core_array_get`
- `nox_core_map_len`、`nox_core_map_keys`、`nox_core_map_get`
- `nox_core_record_field`
- `nox_core_option_is_some`、`nox_core_option_payload`
- `nox_core_result_is_ok`、`nox_core_result_payload`

这些读取函数写出的 `NoxCoreValue` 遵守同一套所有权规则：字符串要用
`nox_core_string_free` 释放，复合 handle 要用对应 free 函数释放。读取缺失的 index、
key 或 field 返回 `NOX_CORE_ERROR`；传入 null 指针返回 `NOX_CORE_NULL_POINTER`。
当前 C ABI 不支持从 C 端构造、修改 array/map/record/option/result，也不允许 C callback
把复合值作为参数或返回值注册到脚本类型签名里。

完整 C smoke 见 [../../examples/embed/c_embedding.c](../../examples/embed/c_embedding.c)。
可复制的宿主回归入口：

```sh
scripts/embedding-regression.sh
```

该脚本会运行 `nox_core` Rust API 测试、`nox` 默认 runtime 的 Session/Runtime 组合测试，
然后编译并执行 C embedding smoke。C smoke 覆盖 version、userdata fallback、callback
error、last_error/clear_error、string free 和 array/map/record/option/result handle free。
Rust 回归还覆盖 array/map/record/option/result handle 保活嵌套 heap 值直到宿主释放 handle。

## ABI 兼容规则

`nox_core_version` 返回构建该库的 crate 版本。`1.0.0` 前 C ABI 可以在本地开发版本间变化。
动态加载 `nox_core` 的宿主应在启动时检查版本，并使用匹配的 `nox_core.h` 编译。

同一 minor 的 patch release 默认保持已有 exported symbol 和 struct layout 兼容，除非发布说明明确标出安全或正确性 break。
minor release 可以增加 enum variant、字段、函数，或为此前不支持的 C 边界值类型改变所有权规则。

release 前至少确认：

- header 中现有 enum 数值没有变化。
- header 中现有函数签名没有变化，新函数只追加。
- `NoxCoreValue` 现有字段顺序和所有权注释没有变化。
- C smoke 覆盖 version、userdata fallback、callback error、last_error/clear_error、
  string free、array/map/record/option/result handle free。
- Rust C ABI 回归覆盖复合 handle 保活和释放嵌套 heap 值。

## 默认运行时嵌入

使用 `nox` crate 的默认运行时：

```rust
use nox::{Runtime, RuntimePermissions};

let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
let value = runtime.eval_file("examples/hello.nox")?;
```

`RuntimePermissions::cli()` 只授予文件系统读能力。环境、定时器、网络和异步任务能力需要宿主显式打开。
宿主可以用 `allow_filesystem_read_under(path)` 和 `allow_filesystem_write_under(path)`
把脚本内 `read_text` / `exists` / `write_text` 限制到指定目录；入口文件读取和 import
解析仍由 `eval_file` / module loader 控制，不代表脚本自动获得任意文件读写权限。
异步 task 状态属于单个 `Runtime` 实例；宿主可用 `pending_async_task_count()` 观察
仍在等待的 task 数，完成和取消都会释放对应 id。v0.0.6 不新增脚本级或 C ABI task status
查询；需要更细生命周期的宿主应在 Rust 侧维护自己的 task registry，或注册自定义 host
function。暂缓依据见 [0016 - 暂缓 async task 状态 API](decisions/0016-defer-async-task-status-api.md)。

## 取消执行和堆回收

宿主可以设置 instruction budget：

```rust
let mut engine = nox_core::Engine::new();
engine.set_instruction_budget(Some(10_000));
```

预算耗尽时，当前 VM 执行返回诊断并停止。`Engine` / `Session` 没有被销毁；宿主可以通过
`set_instruction_budget(None)` 或设置新的预算继续复用。`None` 表示不限制。

instruction budget 在 VM bytecode 边界生效，不抢占已经进入的宿主代码。Rust host
function、C host callback、文件/环境/网络调用，以及 `nox` runtime 的阻塞 timer helper
如果自身耗时，VM budget 不会在其中途打断；需要这类超时或取消语义时，宿主应在 callback
内部实现。`nox` runtime 的 async task 状态属于单个 `Runtime`，`task_cancel` 只影响指定
pending task id；顶层 `Runtime::eval` / `run_test_file` 失败时会清理本次调用创建的 pending
task，不会取消更早调用留下的 task。v0.0.6 暂不把 task status 暴露到 C ABI 或脚本级
`TaskStatus` record，避免把字符串状态或 tombstone 生命周期变成半成品契约。

脚本字符串、函数、数组、map 和 record 通过 engine heap 分配。宿主可以观察对象数量并触发回收：

```rust
let mut engine = nox_core::Engine::new();
let value = engine.eval(r#""hello" + " world";"#)?;
assert!(engine.heap_object_count() > 0);
drop(value);
engine.collect_garbage();
assert_eq!(engine.heap_object_count(), 0);
```
