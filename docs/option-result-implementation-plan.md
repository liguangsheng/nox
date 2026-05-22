# Option / Result Implementation Plan

本文把 ADR 0014 拆成可执行的 v0.0.4-dev 实施清单。当前已经完成类型语法地基、首批
值构造/VM 表示/C ABI handle 和 `match` 解包，但还没有 stdlib 迁移；真实语言状态仍以
[language-v0.md](language-v0.md) 和 [runtime.md](runtime.md) 为准。

## 目标

第一批实现只解决脚本内可恢复缺失值和可恢复错误，不引入异常、try/catch、隐式
nullable 或错误传播语法。

必须保持的边界：

- `null` 继续是独立类型，不自动成为 `T`、`option[T]` 或 `result[T, E]` 的成员。
- 现有 `env_get`、`read_text`、map index、host callback 和 C ABI 行为保持兼容。
- 可变数组、slice、源码级函数类型、高阶函数和动态 `std` object 继续不进入本批。
- 新能力进入 parser 前，formatter golden fixture 和负向测试计划必须已经确定。

## 源码语义

类型语法：

```nox
option[int]
result[str, str]
result[int, FsError]
```

`option` 必须有一个类型参数；`result` 必须有两个类型参数。泛型参数可以是当前可命名的
v0 类型、record、array、map、option 或 result。`map` 的 key 仍固定为 `str`。

构造语义：

```nox
let found: option[int] = some(42);
let missing: option[int] = none;

let ok_value: result[str, str] = ok("body");
let err_value: result[str, str] = err("not found");
```

- `some(value)` 可从 payload 推导为 `option[T]`。
- `none` 只能在 expected type 为 `option[T]` 的位置出现。
- `ok(value)` 和 `err(value)` 只能在 expected type 为 `result[T, E]` 的位置出现，避免
  单边构造时推不出另一个类型参数。
- 不新增泛型函数调用语法；`some`、`none`、`ok`、`err` 是保留的内置构造表面。

解包语义第一批使用受限 `match` 扩展，不先加入 `unwrap`、`?` 或异常式传播：

```nox
match (found) {
    some(value) => {
        value + 1;
    }
    none => {
        0;
    }
}

match (ok_value) {
    ok(body) => {
        body;
    }
    err(message) => {
        message;
    }
}
```

分支 payload 绑定只在该分支块内有效。`option[T]` 必须覆盖 `some` 和 `none`；
`result[T, E]` 必须覆盖 `ok` 和 `err`。第一批不支持嵌套模式、不支持 guard、不支持把
payload 绑定写在 `if` 条件里。

## 实施清单

### 1. Parser 和 AST

- 已在 `Type` 中增加 `Option(Box<Type>)` 和 `Result { ok, err }`。
- 已扩展 `parse_type`，识别 `option[...]` 和 `result[..., ...]`。
- 已给 `option[]`、`option[int, str]`、`result[int]`、`result[int, str, bool]` 添加负向
  parser tests。
- 在 AST 中增加 option/result 构造表达式，或者把 `some` / `ok` call 在 type checker 中
  降级为构造表达式；实现前二选一，不能让普通用户函数覆盖这些内置构造。
- 已把 `some` / `none` / `ok` / `err` 路由为内置构造表面，并禁止 top-level 声明覆盖
  这些名称。
- 已扩展 `match` case AST，支持 `some(name)`、`none`、`ok(name)`、`err(name)`。

### 2. Type Checker

- 已让 `validate_type` 递归验证 option/result 参数。
- 已实现 `none`、`ok`、`err` expected-type 检查；没有 expected type 时返回静态错误。
- 已实现 `some(value)` 推导为 `option[T]`，expected type 为 `option[T]` 时约束 payload。
- 已实现 `match option`：
  - 只接受 `some(name)` 和 `none` case。
  - `some` 分支内把 `name` 绑定为 payload 类型。
  - 缺失任一 case 或重复 case 返回静态错误。
- 已实现 `match result`：
  - 只接受 `ok(name)` 和 `err(name)` case。
  - `ok` 分支绑定成功类型，`err` 分支绑定错误类型。
  - 分支类型和 return-path 规则复用现有 `match` 检查。
- 未解包的 `option[T]` 不能当作 `T` 使用；`result[T, E]` 不能当作 `T` 或 `E` 使用。

当前已完成的切片允许 option/result 出现在类型位置，通过 `some` / `none` /
`ok` / `err` 构造和返回值，并通过受限 `match` 解包 payload。例如
`let value: option[int] = some(1); value;` 可以 eval 为 `some(1)`。
`let value: option[int] = 1;` 仍会按 `type.mismatch` 拒绝。

### 3. VM 和 Value

- 已在 `Value` 中增加 `Option(Rc<OptionValue>)` 和 `Result(Rc<ResultValue>)`。
- 已让 `OptionValue` 保存 payload type 和可选 payload；`ResultValue` 保存 ok/err 类型
  和当前 variant payload。
- 已让 heap 追踪继续走 `Rc + Weak`；payload 中的 array/map/record/function/string 仍由
  既有 value ownership 保持可达。
- 已新增 VM 解包指令，支持 option/result match 判别和 payload 绑定。
- Equality 第一批只允许 scalar payload 的结构相等；如果 payload 是 array/map/record/function，
  延续当前容器相等性边界并由 type checker 优先拦截。
- Display/debug 输出只服务 diagnostics 和 tests，不作为稳定序列化格式。

### 4. Rust API 和 C ABI

- 已给 Rust `Value` 增加 `Value::some`、`Value::none`、`Value::ok`、`Value::err`。
- 已在 C ABI 末尾追加 `NOX_CORE_VALUE_OPTION` 和 `NOX_CORE_VALUE_RESULT`。
- 已让 C ABI 使用只读 owning handle，与 array/map/record 一致：
  - option: kind、is_some、payload、free。
  - result: kind、is_ok、payload、free。
- 旧 C ABI 函数签名不改；新增函数只追加到 header 末尾。

### 5. Formatter、LSP 和 CLI

- 已让 `examples/formatter-golden.nox` 包含 `option[...]`、`result[..., ...]` 和
  `match` payload binding。
- 已由 `fmt_golden_fixture_is_idempotent` 证明新语法 idempotent。
- LSP hover 对 `option[T]` / `result[T, E]` 使用 `Type::Display` 的稳定文本。
- Completion 第一批不增加复杂模式补全，只需不破坏 `some`、`none`、`ok`、`err` 的解析。
- CLI JSON diagnostics 继续使用现有 schema；新增错误优先复用 `type.mismatch`，必要时再
  通过 [diagnostics.md](diagnostics.md) 增加稳定 code。

### 6. Tests 和 Fixtures

第一批必须至少添加这些 fixtures 或等价单元测试：

正向 fixture：

```nox
fn describe(input: option[int]) -> str {
    match (input) {
        some(value) => {
            return "some";
        }
        none => {
            return "none";
        }
    }
}

describe(some(1));
```

负向 fixture：

```nox
let value: int = none;
```

```nox
let value: int = some(1);
```

```nox
let value: result[int, str] = ok(1);
let copied: int = value;
```

```nox
let value: option[int] = some(1);
match (value) {
    some(inner) => {
        inner;
    }
}
```

每个负向 fixture 都必须断言 diagnostic message、span/source 和退出码。至少一个 case
必须覆盖 LSP diagnostics，至少一个 case 必须覆盖 formatter 对新语法的输出。

### 7. Stdlib 迁移窗口

第一批语言实现不直接改旧 API。28.2 再选择一个 stdlib 试点，候选顺序：

1. 已让 `std/env.nox` 新增 `try_get(name: str) -> option[str]`。
2. 已让 `std/fs.nox` 新增 `try_read_text(path: str) -> result[str, str]`。
3. 已让 map lookup 新增 `map_get(map, key) -> option[T]`，作为引擎内置特殊规则从
   `map[str, T]` 推导返回类型，不公开通用泛型函数机制。
4. async task 状态 API 改成 result/record 前，需要先稳定 task 状态 record 形状。

旧 `env_get`、`read_text`、`contains` + map index 和 `task_ready` 至少保留一个 minor 阶段。

## 不进入第一批

- `?`、`try`、异常、隐式返回传播。
- `T?` 或任何隐式 nullable。
- 方法语法，例如 `value.unwrap()`。
- 高阶 helper，例如 `map_option`、`and_then`。
- 复杂 error hierarchy。
- 动态 std object 或 package registry。

## 完成定义

28.1 只在本计划被提交、ADR 0014 链接本计划、`PLAN.md` 状态更新并通过固定验证后完成。
后续实现阶段完成前，不得把 `docs/language-v0.md` 或 `docs/runtime.md` 改写成
option/result 已可用。
