# Nox 语言 v0

本文定义当前已经实现的 v0 语言切片。Nox 从第一版开始就是静态类型语言；后续语法应在
这个基础上扩展，而不是兼容早期动态方言。

## 源文件

Nox 源文件使用 `.nox` 扩展名。源码会被切成带 byte span 的 token。文件场景下，CLI 和
LSP 会把 span 映射成 line/column 或 LSP range。

空白只用于分隔 token。行注释以 `//` 开始，持续到行尾。

## 词法元素

标识符以 ASCII 字母或 `_` 开头，后续可以包含 ASCII 字母、数字或 `_`。

保留关键字：

```text
let const fn return if else match while for in break continue import export record true false null
```

数字字面量分为：

- `int`：没有小数点，例如 `42`。
- `float`：小数点两侧都有数字，例如 `3.14`。

字符串字面量使用双引号，支持：

```text
\n \t \" \\
```

不支持的 escape 是词法错误。字符串中不能直接包含换行，需使用 `\n`。

## 类型

v0 内置命名类型：

```text
null bool int float str
```

`null` 是独立类型，不会隐式加入其他类型，也没有隐式 nullable 类型。

Option / result 类型用于显式表达缺失值和可恢复错误：

```nox
let found: option[int] = some(42);
let missing: option[int] = none;

let loaded: result[str, str] = ok("body");
let failed: result[str, str] = err("not found");
```

`option[T]` 有 `some(value)` 和 `none` 两种构造；`result[T, E]` 有 `ok(value)` 和
`err(error)` 两种构造。`none`、`ok(value)` 和 `err(value)` 需要 expected type；
`some(value)` 可从 payload 推导。`null` 不会自动成为 option/result 的成员。

数组类型：

```nox
let values: [int] = [1, 2, 3];
let empty: [str] = [];
```

数组构造后不可变。当前没有 `push`、元素赋值或切片；v0.0.3 的设计结论见
[0010 - 暂缓可变数组](decisions/0010-defer-mutable-arrays.md)。

Map 类型：

```nox
let scores: map[str, int] = {"core": 20, "runtime": 22};
let empty_scores: map[str, int] = {};
```

`map` 的 key 当前固定为 `str`。

Record 类型：

```nox
record User {
    name: str,
    score: int,
}

let user: User = User { name: "nox", score: 42 };
```

函数值内部有函数签名类型。源码中通过 `fn name(params) -> type` 声明函数；v0 暂不支持在变量类型标注里书写函数类型。
因此 `fn(int) -> int` 只会出现在内部类型展示和 LSP hover 中，不能用于变量、参数、
返回值、数组或 map 的类型标注。函数声明可被调用；一等函数值、高阶函数和闭包逃逸
暂不作为 v0.0.3 稳定表面。

Nox 没有隐式类型转换。声明、运算符、条件、函数参数和返回值都要求类型精确匹配。

## 语句

除带大括号的块语句、`if`、`while`、`for` 和函数声明外，语句以 `;` 结束。

变量声明需要显式类型和初始化表达式：

```nox
let answer: int = 42;
```

`const` 声明语法与 `let` 一致，但不可被重新赋值。`const` 仍在运行时求值，
不做编译期常量折叠。`const` 允许在顶层和块内，并支持 `export`：

```nox
const LIMIT: int = 100;

export const LABEL: str = "nox";
```

对 `const` 绑定赋值是静态错误。内层块中可以用 `let` 同名 shadow，shadow
后的绑定按 `let` 规则可以重新赋值。

赋值更新已有绑定，并求值为被赋的新值：

```nox
answer = answer + 1;
```

函数声明需要显式参数类型和返回类型：

```nox
fn add(left: int, right: int) -> int {
    return left + right;
}
```

每个函数体必须有静态可见的返回路径。`if` 只有在 then/else 两边都返回时才算保证返回；
`while` 和 `for` 不计为保证返回。

`return` 只能出现在函数内部，返回表达式必须匹配函数返回类型。

块引入词法作用域：

```nox
{
    let local: str = "value";
    local;
}
```

`if` 和 `while` 条件必须是 `bool`。`else if` 作为嵌套条件支持：

```nox
if (answer == 42) {
    answer;
} else if (answer == 0) {
    0;
}

while (answer < 100) {
    answer = answer + 1;
}
```

`match` 是语句形式的受限分支。匹配值可以是 `int`、`str`、`option[T]` 或
`result[T, E]`。没有 fallthrough，每个分支 body 都是独立块。

`int` / `str` match 的每个 case 是同类型字面量，`_` 默认分支必填：

```nox
match (answer) {
    1 => {
        answer = 10;
    }
    2 => {
        answer = 20;
    }
    _ => {
        answer = 0;
    }
}
```

`option[T]` match 只接受 `some(name)` 和 `none` case，且必须覆盖两者。`some`
分支内的 payload 绑定类型为 `T`，只在该分支块内有效：

```nox
match (found) {
    some(value) => {
        value + 1;
    }
    none => {
        0;
    }
}
```

`result[T, E]` match 只接受 `ok(name)` 和 `err(name)` case，且必须覆盖两者。
`ok` 分支 payload 类型为 `T`，`err` 分支 payload 类型为 `E`：

```nox
match (loaded) {
    ok(body) => {
        body;
    }
    err(message) => {
        message;
    }
}
```

`for` 遍历半开 `int` 区间，包含 start，不包含 end。上下界必须是 `int`，循环变量是仅在循环体内可见的 `int`：

```nox
for i in 0..10 {
    answer = answer + i;
}
```

`while` 和 `for` 循环体内可以使用 `break;` 提前退出最近一层循环，或使用
`continue;` 跳到下一次迭代。`continue` 在 `while` 中跳回条件判断，在 `for` 中
跳到循环变量自增之前。`break` / `continue` 只作用于最近一层循环——嵌套循环
里的内层 `break` 不会跳出外层。在循环外使用 `break` / `continue` 是静态错误：

```nox
let i: int = 0;
while (i < 10) {
    if (i == 3) {
        break;
    }
    i = i + 1;
}

for j in 0..5 {
    if (j == 2) {
        continue;
    }
}
```

import 在类型检查前加载模块：

```nox
import "math.nox";
```

`nox_core` 定义 import 语义但不读文件。宿主通过 module loader 提供源码。默认 `nox` 运行时从入口文件目录解析相对路径。
同一次编译中重复 import 只加载一次，循环 import 会产生诊断。

也可以用命名空间 import 避免导出名进入当前作用域：

```nox
import "math.nox" as math;

math.double(21);
```

`math` 不是运行时 object；`math.double` 在 import 解析阶段绑定到模块成员。命名空间只
暴露模块导出表面：有 `export` 时只暴露导出顶层声明，没有 `export` 时暴露全部顶层声明。
缺失成员会产生 `module.member-not-found`，alias 与当前模块顶层声明重名会产生
`module.name-conflict`。

模块可用 `export` 标记顶层 `let`、`fn` 和 `record`：

```nox
export fn double(value: int) -> int {
    return helper(value);
}

fn helper(value: int) -> int {
    return value * 2;
}
```

使用 `export` 的模块只向导入者暴露导出声明。私有声明仍可被同模块内部代码使用。
没有任何 `export` 的模块保留早期 v0 行为，顶层声明都进入导入表面。

Record 声明定义固定字段集合：

```nox
record Point {
    x: int,
    y: int,
}
```

字段名必须唯一，字段必须有显式类型。

## 表达式

优先级从低到高：

1. 赋值：`name = value`
2. 逻辑或：`||`
3. 逻辑与：`&&`
4. 相等：`==`、`!=`
5. 数值比较：`>`、`>=`、`<`、`<=`
6. 加减：`+`、`-`
7. 乘除：`*`、`/`
8. 一元：`!`、`-`
9. 调用、索引、字段访问和 primary

括号可以改变分组。

字面量示例：

```nox
null;
true;
false;
42;
3.14;
"text";
[1, 2, 3];
[];
{"core": 20, "runtime": 22};
{};
User { name: "nox", score: 42 };
```

一元运算：

- `!value` 要求 `bool`，返回 `bool`。
- `-value` 要求 `int` 或 `float`，返回同类型。

二元运算：

- `&&` 和 `||` 要求 `bool` 操作数，返回 `bool`，并短路右侧表达式。
- `+` 支持 `int + int -> int`、`float + float -> float`、`str + str -> str`。
- `-`、`*`、`/` 要求两边同为 `int` 或同为 `float`，返回同类型。
- `>`、`>=`、`<`、`<=` 要求两边同为 `int` 或同为 `float`，返回 `bool`。
- `==` 和 `!=` 要求两边同类型，返回 `bool`。

`int / int` 是整数除法，向零截断。`int` 和 `float` 除零都是运行时诊断。整数运算和整数除法溢出报告
`integer overflow`，不会 wrap 或 panic。float 运算结果必须 finite，infinity 和 NaN 是运行时诊断。

函数调用要求 callee 是函数类型，参数数量精确匹配，每个参数类型匹配声明类型。函数
类型等价在内部按结构判断：参数数量、参数类型顺序和返回类型都相同才匹配。源码当前
不能直接书写函数类型。

数组字面量是同质的。赋给 `[T]` 的字面量只能包含 `T`；空数组需要 expected array type。
索引写作 `array[index]`，index 必须是 `int`，越界是运行时诊断。`len(array)` 返回 `int`。
`len` 也接受 `str`，返回该字符串的 Unicode 字符数（不是 UTF-8 字节数），其它类型
会得到静态错误。

Map 字面量以 `str` 为 key，value 同质。赋给 `map[str, T]` 的字面量只能包含字符串 key 和 `T` 类型 value。
空 map 需要 expected map type。索引写作 `map[key]`，key 必须是 `str`，缺失 key 是运行时诊断。
`contains(map, key)` 接受 `map[str, T]` 和 `str`，返回 `bool`，表示该 key 是否存在；
不会触发缺失 key 的诊断，可用来在索引前显式判断。`map_get(map, key)` 接受
`map[str, T]` 和 `str`，返回 `option[T]`；存在时为 `some(value)`，缺失时为 `none`。

Record 字面量必须引用已定义 record，初始化每个字段且不能包含额外字段。字段访问写作 `value.field`，
由静态检查确认字段存在。

数组、map 和 record 的容器相等性暂不支持。

## 模块和声明

import 在静态类型检查和字节码编译前解析。重复 import specifier 在一次编译中只加载一次。
如果导入模块使用 `export`，导入者只能看到导出声明；私有声明仍可供导入模块内部使用。

同一作用域内的函数声明会先预声明，再检查函数体，因此支持直接递归和调用后面声明的 helper。

## 运行时值

v0 运行时值：

```text
null bool int float str array map record function
```

表达式语句产生表达式值。模块返回最后一个产生值的语句结果；如果没有结果，则返回 `null`。

## 宿主边界

Rust 宿主函数用显式 Nox 类型注册，和脚本函数参与同一套静态检查。VM 会在宿主 callback 返回后再次校验返回值类型。

C ABI v0 callback 边界支持 `null`、`bool`、`int`、`float` 参数和返回值。字符串、数组、map、record、function
会作为 value kind 报告，但 v0 不支持作为 C callback 参数或返回值跨边界传递。function
kind 也不提供跨 ABI 调用 API；C host callback 注册是单独的宿主入口。`eval` 返回字符串时使用 owned C string。

## 标准库

`nox_core` 总是提供：

```text
to_float(value: int) -> float
to_int(value: float) -> int
```

`to_int` 向零截断，并拒绝非 finite 或越界 float。

默认 `nox` 运行时额外安装：

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

文件系统、环境变量和定时器也可以通过 `std/fs.nox`、`std/env.nox` 和
`std/time.nox` 静态模块导入。`std/env.nox` 额外提供
`try_get(name: str) -> option[str]`，用于把缺失环境变量表达为 `none`。`std/fs.nox`
额外提供 `try_read_text(path: str) -> result[str, str]`，用于把普通读取失败表达为
`err(message)`。map 可用 `map_get(map, key) -> option[T]` 可恢复处理缺失 key。
旧全局函数仍作为兼容表面保留；推荐表面、权限表和迁移窗口见 [运行时](runtime.md)。

文件系统、环境变量、定时器、网络和异步任务由 `RuntimePermissions` 控制。`nox_core` 不直接提供系统能力。

## 诊断

诊断包含 message、span 和可选 source location。机器消费者可使用 `code` 字段；稳定
code 表见 [诊断 code](diagnostics.md)。当前稳定 code 包括：

```text
parse.expected-token
type.mismatch
runtime.division-by-zero
module.name-conflict
module.not-found
module.member-not-found
manifest.invalid
project.discovery
permission.denied
host.callback
bytecode.verifier
test.signature
```

CLI JSON 和 LSP diagnostics 使用同一套 span，并分别映射为 line/column 或 LSP range。
parser 可以在部分语法错误后恢复到后续顶层语句。type checker 会对独立顶层声明继续检查，
让同一文件中的多个互不依赖类型错误一起返回；函数体、块和表达式内部仍保持保守的
fail-fast 边界，避免制造误导性二次错误。

## v0.0.4 语言缺口复盘

阶段 27.1 基于 sample project、`std/*` 模块迁移和 embedding/runtime 压力测试复盘后，
v0.0.4 候选能力按“真实用例是否足以重启设计”排序如下：

| 候选能力 | 证据来源 | 当前结论 |
| --- | --- | --- |
| `option[T]` / `result[T, E]` | `std/fs.nox` 的 `exists` + `read_text`、`std/env.nox` 的 `list` + `get`、map `contains` + index、async task unknown id、host callback diagnostic。 | 已实现语言内类型、构造和受限 `match` 解包；`std/env.nox try_get`、`std/fs.nox try_read_text` 和 `map_get(map, key)` 已作为可恢复 API 迁移试点。见 [0014 - 重启 option / result 设计但暂不实现](decisions/0014-restart-option-result-design.md)。 |
| 可变数组 / 数组增长 / slice | sample project 只用固定 record/map/array 字面量；bench 和 tests 也以固定输入为主；stdlib 暂无脚本内逐步收集大型列表的需求。 | 继续暂缓；27.3 已决定 v0.0.4 不实现原地 mutation、copy-on-write helper 或 slice，见 [0015 - 暂缓容器和函数能力扩张](decisions/0015-defer-container-function-expansion.md)。 |
| 源码级函数类型 / 高阶函数 | sample project 通过命名函数和 namespace import 完成分层；stdlib 通过静态模块成员表达能力；embedding 通过 host function 注册提供回调入口。 | 继续暂缓；27.3 已决定 v0.0.4 不实现 `fn(T) -> U` 类型位置、高阶函数或 C ABI function handle，见 [0015 - 暂缓容器和函数能力扩张](decisions/0015-defer-container-function-expansion.md)。 |
| 模块/stdlib 命名边界 | scoreboard 使用 `import "labels.nox" as labels`、`import "scoring.nox" as scoring`；runtime_info 使用 `std/env.nox`、`std/fs.nox`、`std/time.nox`。 | 已由 namespace import、export 和 `std/*` 虚拟模块解决；v0.0.4 不需要为此引入动态 object 或 `std` object。 |
| 错误处理语法糖 | 当前 runtime 文档推荐 `exists`、`contains`、`env_list()` guard；不可恢复 host 边界仍用 diagnostic。 | 只作为 27.2 的一部分评估。不单独加入异常、隐式 nullable 或 try/catch。 |

因此，27.1 不把任何暂缓项自动推进到实现。27.2 只重启可恢复错误模型设计但暂不实现；
27.3 继续暂缓可变数组、slice、函数类型、高阶函数和动态 object，直到出现来自项目、
stdlib 或 embedding 的更强用例。

## v0 暂不支持

v0 不包含泛型、方法、异常、包管理、宏、async 语法、Unicode escape、raw string、多行字符串、
type alias、源码级函数类型标注、C 风格 `for`、数组 mutation、切片或 iterator protocol。

命令用法见 [cli.md](cli.md)，运行时权限见 [runtime.md](runtime.md)，嵌入 API 见 [embedding.md](embedding.md)。
