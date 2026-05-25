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

- `int`：没有小数点，例如 `42`。整数支持 `0xff` 十六进制、`0b1010` 二进制、
  `0o17` 八进制，以及 `_` 分隔符（如 `1_000_000`）。
- `float`：小数点两侧都有数字，例如 `3.14`。

字符串字面量使用双引号，支持：

```text
\n \t \" \\ \$
```

不支持的 escape 是词法错误。普通双引号字符串中不能直接包含换行，需使用 `\n`。
多行字符串使用 `"""..."""`，内部换行原样保留；raw 字符串使用 `r"..."`，
不解释 `\n`、`\t` 或 `\${...}` 等转义/插值内容。
单引号字符字面量如 `'A'`、`'界'` 和 `'\n'` 会降为长度为一个 Unicode scalar 的
`str` 值；空字符、多字符、未闭合和不支持的字符 escape 使用
`lex.invalid-character` 诊断。Nox 当前没有独立 `char` 或 `bytes` 类型。

普通双引号字符串支持 `${expr}` 插值。占位表达式按普通表达式解析，并可自动转换
`null`、`bool`、`int`、`float` 和 `str`：

```nox
let name: str = "nox";
let count: int = 3;
let label: str = "name=${name}, count=${count + 1}";
```

不能稳定转换为字符串的 `json`、容器、record、option、result 和 function 会报
`string.interpolation` 诊断。需要输出字面量 `$` 时使用 `\$`。

## 类型

v0 内置命名类型：

```text
null bool int float str json
```

顶层可以用 `type` 定义类型别名。别名在类型检查时透明展开，可指向内置类型、
tuple、array、map、option/result 或 record 类型：

```nox
type UserId = int;
type Pair = (UserId, str);
let pair: Pair = (42, "nox");
```

循环别名会报稳定诊断 `type-alias.cyclic`。

顶层可以用 `enum` 定义命名 sum type。variant 可以没有 payload，也可以携带一个显式
payload 类型；构造时使用 `EnumName.Variant` 或 `EnumName.Variant(value)`：

```nox
enum LoadState {
    Loading,
    Ready(int),
    Failed(str),
}

let state: LoadState = LoadState.Ready(42);
```

用户 enum 用 `match` 解包，必须覆盖所有 variant；缺失分支使用稳定诊断
`match.non-exhaustive`，不存在的 variant 使用 `enum.variant-not-found`。

`null` 是独立类型，不会隐式加入其他类型，也没有隐式 nullable 类型。
`json` 是 `std/json.nox` 使用的不透明 JSON 值类型，可承载 RFC 8259 的
number/string/bool/null/array/object 六类形态；当前通过 `parse` / `stringify`
读写，不提供直接字段或索引访问。

Tuple 类型使用圆括号列出元素类型，元素数量固定且至少为 2：

```nox
let pair: (int, str) = (42, "nox");
let (count, name) = pair;
```

Tuple 解构 `let (a, b) = pair;` 会按位置绑定元素；元素数量不匹配使用
`tuple.arity-mismatch`，元素类型不匹配使用 `tuple.element-type-mismatch`。

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

`?` 是后缀错误传播运算符，只能在函数内使用。对 `result[T, E]` 使用时，
`ok(value)?` 求值为 `value`，`err(error)?` 会立即从当前函数返回同类型错误；当前
函数必须返回 `result[U, E]`。对 `option[T]` 使用时，`some(value)?` 求值为
`value`，`none?` 路径会立即返回当前函数返回类型对应的 `none`；当前函数必须返回
`option[U]`。

```nox
fn load_name() -> result[str, str] {
    return ok("nox");
}

fn describe() -> result[str, str] {
    let name: str = load_name()?;
    return ok("name=${name}");
}
```

不兼容的外层返回类型会报稳定诊断 `result.question-mark.mismatch`。

Nox 不提供用户可见的 `throw` / `catch` / `finally` 异常机制，也暂缓 Rust 风格
`try {}` block。可恢复错误应作为 `result` / `option` 值流动；权限不足、资源上限、
host callback panic、parser/typechecker 失败和 runtime diagnostic 不会被脚本捕获或包装成
`err`。错误模型边界见
[0028 - result / option 错误模型与 try block 暂缓](decisions/0028-result-option-error-model.md)。

数组类型：

```nox
let values: [int] = [1, 2, 3];
let empty: [str] = [];
```

数组支持显式 mutation：可以通过 `arr[i] = value` 写入，也可以使用
`std/array.nox` 的 `set`、`append`、`pop` 等 helper。越界写入会报稳定诊断
`runtime.index-out-of-range`。slice 语法仍不属于语言表面；需要拷贝切片时使用
`array.slice_copy`。

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
let { name, score } = user;
```

Record method syntax 是函数调用糖。`record_value.method(args)` 会解析为
`method(record_value, args)`；`method` 必须是当前模块或导入模块可见的函数，且第一个参数类型
必须匹配 receiver 的 record 类型：

```nox
fn label(user: User) -> str {
    return "${user.name}:${user.score}";
}

let text: str = user.label();
```

找不到匹配方法时会报稳定诊断 `record.method-not-found`。字段访问仍使用
`record_value.field`，不会隐式查找方法。

函数值内部有函数签名类型。源码中通过 `fn name(params) -> type` 声明函数；函数声明
可以带函数级泛型参数，例如：

```nox
fn id<T>(value: T) -> T {
    return value;
}
```

泛型参数只在该函数签名和函数体内有效；调用时由实参类型推导，也可以由 expected
return type 辅助推导空数组等需要上下文的返回值。推导冲突或无法推导时使用稳定诊断
`generic.infer-failed`。v0 只支持函数级泛型参数，不支持泛型 record、泛型 trait 或显式
type argument 调用。函数值使用源码级 `fn(T) -> U` 类型标注，lambda 形态为
`fn(x: int) -> int { return x + 1; }`；函数值可以绑定到变量、作为参数传递、放入容器，
并供 `array.map_fn`、`array.filter_fn`、`array.reduce`、`array.for_each` 等高阶 helper
调用。闭包按 lexical env 捕获外部 binding；跨 C ABI 的 function handle 仍不作为稳定表面。

静态 trait MVP 支持声明 required method、为 nominal record/enum 写 `impl Trait for Type`，
并在函数级泛型中使用 `T: Trait` bound：

```nox
trait Display {
    fn to_str(self: Self) -> str;
}

record User {
    name: str,
}

impl Display for User {
    fn to_str(self: User) -> str {
        return self.name;
    }
}

fn label<T: Display>(value: T) -> str {
    return value.to_str();
}
```

当前 trait 能力是实验性的纯静态 MVP：不支持 `interface` 别名、动态 dispatch、trait object、
blanket impl、generic impl、associated type 或 higher-kinded type。impl method 会编译为内部
mangled 函数名，并按 receiver nominal type 分派，因此不同类型可以实现同名 trait method。
源码级顶层函数仍是普通 record method 糖的入口；当顶层函数第一个参数匹配 receiver 时，
record-style method 保持优先，否则 concrete receiver 可以使用同名的唯一 trait impl method。

method lookup 保持保守：record-style method 和 namespace member 在唯一命中时优先；之后才查找
receiver concrete type 上的唯一 trait impl，或泛型 receiver 的 trait bound 中的唯一 method。
trait / record 候选冲突时拒绝，不根据返回类型猜测。

标准库第一批 trait 落点是 `std/array.nox` 的 `Eq` trait，以及
`contains_equal<T: Eq>` / `dedupe_equal<T: Eq>`。第三轮 trait 表面新增实验性
`std/traits.nox` 小核心，导出 `Eq`、`Display`、`equal<T: Eq>` 和
`display<T: Display>`。基础 primitive 的内置 impl 覆盖 `null`、`bool`、`int`、
`float` 和 `str`；用户 record/enum 可在直接导入定义 trait 的模块后实现同一个 trait。
旧的 `contains_value<T: Equatable>` / `dedupe<T: Equatable>` 继续保留，作为内建 marker
兼容层。长期边界见
[0027 - 静态 trait 系统路线](decisions/0027-static-trait-system.md)。

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

`if let`、`let ... else` 和 `while let` 复用 `match` pattern 语义，用于在控制流中解包
`option`、`result` 或用户 `enum`。pattern 绑定只在成功分支或循环体内有效；`let ... else`
成功绑定在语句之后可见，且 `else` 分支必须提前 `return`，否则使用稳定诊断
`control-flow.let-else-fallthrough`：

```nox
if let some(value) = maybe_value {
    value;
} else {
    0;
}

let ok(body) = loaded else {
    return err("missing");
};

while let some(item) = next(index) {
    index = index + 1;
}
```

`match` 是语句形式的受限分支。匹配值可以是 `int`、`float`、`str`、
`option[T]` 或 `result[T, E]`。没有 fallthrough，每个分支 body 都是独立块。

`int` / `float` / `str` match 的每个 case 是同类型字面量；`int` 还支持
`start..end` 半开范围 pattern。`_` 默认分支必填：

```nox
match (answer) {
    0..10 => {
        answer = 5;
    }
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

`option[T]` match 只接受 `some(pattern)` 和 `none` case，且必须覆盖两者。`some`
分支内的 payload 绑定只在该分支块内有效。pattern 可以嵌套，因此
`option[option[int]]` 可以写成 `some(some(value))` / `some(none)` / `none`：

```nox
match (nested) {
    some(some(value)) => {
        value + 1;
    }
    some(none) => {
        0;
    }
    none => {
        -1;
    }
}
```

`result[T, E]` match 只接受 `ok(pattern)` 和 `err(pattern)` case，且必须覆盖两者。
`ok` 分支 payload 类型为 `T`，`err` 分支 payload 类型为 `E`；payload pattern 同样可嵌套：

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
- `~value` 要求 `int`，返回按 two's-complement 位取反后的 `int`。

二元运算：

- `&&` 和 `||` 要求 `bool` 操作数，返回 `bool`，并短路右侧表达式。
- `+` 支持 `int + int -> int`、`float + float -> float`、`str + str -> str`。
- `-`、`*`、`/` 要求两边同为 `int` 或同为 `float`，返回同类型。
- `&`、`|`、`^`、`<<`、`>>` 要求两边同为 `int`，返回 `int`；非 `int` 使用稳定诊断
  `type.bitwise-non-int`。
- `>`、`>=`、`<`、`<=` 要求两边同为 `int` 或同为 `float`，返回 `bool`。
- `==` 和 `!=` 要求两边同类型，返回 `bool`。

`int / int` 是整数除法，向零截断。`int` 和 `float` 除零都是运行时诊断。整数运算和整数除法溢出报告
`integer overflow`，不会 wrap 或 panic。float 运算结果必须 finite，infinity 和 NaN 是运行时诊断。
位运算按 64-bit signed `int` 的 two's-complement 表示执行；`>>` 是算术右移，保留符号位。
移位计数必须在 `0..64` 内，越界是运行时诊断。

数组和 map literal 支持 spread：`[...items, value]` 与 `{...defaults, "k": value}`。
spread 总是创建新容器，不 mutation 源容器；数组 spread 要求源表达式为 `[T]`，map spread 要求源表达式为
`map[str, T]`，value 类型必须与同一 literal 里的其他元素一致。map 合并按书写顺序执行，后面的 key 覆盖
前面的 key。spread 类型不匹配使用稳定诊断 `type.spread-mismatch`。

函数调用要求 callee 是函数类型，参数数量精确匹配，每个参数类型匹配声明类型。泛型
函数调用会在调用点推导函数级类型参数；推导失败使用 `generic.infer-failed`。函数类型
等价在内部按结构判断：参数数量、参数类型顺序和返回类型都相同才匹配。源码当前不能
直接书写函数类型。

数组字面量是同质的。赋给 `[T]` 的字面量只能包含 `T`；空数组需要 expected array type。
索引写作 `array[index]`，index 必须是 `int`，越界是运行时诊断。`len(array)` 返回 `int`。
`len` 也接受 `str`，返回该字符串的 Unicode 字符数（不是 UTF-8 字节数），其它类型
会得到静态错误。

Map 字面量以 `str` 为 key，value 同质。赋给 `map[str, T]` 的字面量只能包含字符串 key 和 `T` 类型 value。
空 map 需要 expected map type。索引写作 `map[key]`，key 必须是 `str`，缺失 key 是运行时诊断。
`contains(map, key)` 接受 `map[str, T]` 和 `str`，返回 `bool`，表示该 key 是否存在；
不会触发缺失 key 的诊断，可用来在索引前显式判断。`map_get(map, key)` 接受
`map[str, T]` 和 `str`，返回 `option[T]`；存在时为 `some(value)`，缺失时为 `none`。
`map_has(map, key)` 是 `contains` 的 map 命名别名。`map_keys(map) -> [str]` 和
`map_values(map) -> [T]` 按 key 字典序返回 key/value 数组；`map_size(map) -> int`
返回条目数量。

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
print(value: str) -> null
to_str_int(value: int) -> str
to_str_float(value: float) -> str
to_str_bool(value: bool) -> str
to_str_null(value: null) -> str
to_str_str(value: str) -> str
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

`print` 直接向标准输出写入一行文本，返回 `null`；`nox run` 对最终 `null` 不再额外打印 `null`。
`to_str_*` 辅助函数把基础值转换为字符串，供输出、日志和后续字符串插值能力复用。

文件系统、环境变量、定时器、字符串处理和 JSON 处理也可以通过 `std/fs.nox`、`std/env.nox`、
`std/time.nox`、`std/string.nox` 和 `std/json.nox` 静态模块导入。`std/string.nox` 提供纯计算函数：
`split(value, separator) -> [str]`、`join(values, separator) -> str`、
`substring(value, start, length) -> str`、`trim(value) -> str`、
`replace(value, from, to) -> str`、`starts_with(value, prefix) -> bool`、
`ends_with(value, suffix) -> bool`、`index_of(value, needle) -> int`（缺失时返回 `-1`）、
`contains(value, needle) -> bool`、`last_index_of(value, needle) -> int`、
`repeat(value, count) -> str`、`pad_left(value, width, fill) -> str`、
`pad_right(value, width, fill) -> str`、`parse_int(value) -> result[int, str]`、
`parse_float(value) -> result[float, str]`、`lines(value) -> [str]`、
`to_upper(value) -> str` 和 `to_lower(value) -> str`。这些函数不需要 runtime capability。
`std/json.nox` 提供 `parse(value: str) -> result[json, str]`、
`stringify(value: json) -> str`、`kind(value) -> str` 和基础 array/object 访问 helper，覆盖
number/string/bool/null/array/object 六类基础形态，也是纯计算模块。`std/csv.nox` 与
`std/tsv.nox` 提供单行分隔文本解析和格式化 helper，不是 streaming parser。
`std/array.nox`、`std/map.nox`、`std/option.nox` 和 `std/result.nox` 提供集合和
可恢复值 helper，包括 array copy/sort/slice、array/map 显式 mutation helper、map
entries/merge/remove/get_or，以及 option/result 状态判断和 fallback。`std/array.nox`
还提供基于函数值的 `map_fn`、`filter_fn`、`reduce` 和 `for_each`。数组和 map value
类型标注可以包含 tuple、map、option、result 和函数类型，例如 `[(str, int)]`、
`map[str, option[int]]` 与 `[fn(int) -> int]`。
`std/process.nox` 提供命令行脚本入口 helper：`argv`、`read_stdin`、`print_err` 和 `exit`。
`std/path.nox` 提供 `join`、`basename`、`dirname`、`extension` 和 `normalize`；`std/fs.nox`
在既有 filesystem capability 下提供文件分类和目录列表 helper。

`std/env.nox` 额外提供
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
lex.invalid-integer
type-alias.cyclic
enum.variant-not-found
generic.infer-failed
type.bitwise-non-int
control-flow.let-else-fallthrough
type.spread-mismatch
tuple.arity-mismatch
tuple.element-type-mismatch
bytecode.verifier
test.signature
```

CLI JSON 和 LSP diagnostics 使用同一套 span，并分别映射为 line/column 或 LSP range。
parser 可以在部分语法错误后恢复到后续顶层语句。type checker 会对独立顶层声明继续检查，
让同一文件中的多个互不依赖类型错误一起返回；函数体、块和表达式内部仍保持保守的
fail-fast 边界，避免制造误导性二次错误。

## v0.0.4 语言缺口复盘

本节保留 v0.0.4 开发期的设计闸门记录。后续阶段已经根据 ADR 0018、0019 和实际实现
推进了部分候选能力；当前实现状态以本页上文、runtime 文档和 stdlib index 为准。

| 候选能力 | 证据来源 | 当前结论 |
| --- | --- | --- |
| `option[T]` / `result[T, E]` | `std/fs.nox` 的 `exists` + `read_text`、`std/env.nox` 的 `list` + `get`、map `contains` + index、async task unknown id、host callback diagnostic。 | 已实现语言内类型、构造、`match` 解包和 `?` 传播；`std/env.nox try_get`、`std/fs.nox try_read_text`、`map_get(map, key)` 等可恢复 API 已进入稳定表面。见 [0014 - 重启 option / result 设计但暂不实现](decisions/0014-restart-option-result-design.md)。 |
| 可变数组 / 数组增长 / slice | sample project 只用固定 record/map/array 字面量；bench 和 tests 也以固定输入为主；stdlib 暂无脚本内逐步收集大型列表的需求。 | v0.0.4 后续阶段已按 [0018 - 重启可变集合与 slice 设计](decisions/0018-restart-mutable-collections-and-slice.md) 落地数组 / map 显式 mutation helper 与 index assignment；slice 语法仍不进入语言表面，使用 `slice_copy` helper。 |
| 源码级函数类型 / 高阶函数 | sample project 通过命名函数和 namespace import 完成分层；stdlib 通过静态模块成员表达能力；embedding 通过 host function 注册提供回调入口。 | v0.0.4 后续阶段已按 [0019 - 重启函数值、闭包与高阶函数设计](decisions/0019-restart-function-values-and-closures.md) 落地 `fn(T) -> U` 类型标注、lambda、闭包和受限高阶 stdlib helper；跨 C ABI function handle 仍不进入稳定表面。 |
| 模块/stdlib 命名边界 | scoreboard 使用 `import "labels.nox" as labels`、`import "scoring.nox" as scoring`；runtime_info 使用 `std/env.nox`、`std/fs.nox`、`std/time.nox`。 | 已由 namespace import、export 和 `std/*` 虚拟模块解决；v0.0.4 不需要为此引入动态 object 或 `std` object。 |
| 错误处理语法糖 | 当前 runtime 文档推荐 `result` / `option` / `?`、可恢复 stdlib helper 和不可恢复 diagnostic 边界。 | 阶段 64 已按 [0028 - result / option 错误模型与 try block 暂缓](decisions/0028-result-option-error-model.md) 确认不引入通用异常，也暂缓 Rust 风格 `try {}` block；阶段 65 改做 helper / 文档 / 测试收敛。 |

阶段 27 的原始结论本身不再代表当前完整能力面；它只解释当时为什么需要先经过 ADR 复审，
再进入实现。仍保持暂缓的事项包括动态 object、slice 语法、异常、宏、trait object /
动态 dispatch、完整 async runtime 能力和通用 package registry。

`async fn f(...) -> T` 已进入分阶段 MVP：调用返回 `task[T]`，`await expr` 只能出现在
另一个 `async fn` 内，且 `expr` 必须是 `task[T]`。当前实现仍是单线程、无 IO reactor、
无隐式权限、无 async trait、无 top-level await；脚本最终值如果是未消费的 task，会产生
`async.top-level-task` 诊断。`async fn f() -> result[T, E]` 或
`async fn f() -> option[T]` 内部的后缀 `?` 按声明的 payload `result` / `option`
返回类型传播，语义与同步函数一致；runtime diagnostic 仍不会被转换成 `err` 或 `none`。泛型
函数调用可以从 `task[T]` 参数推断 payload 类型，例如 `fn identity<T>(value: task[T]) ->
task[T]` 能直接接受 `task[int]`。

## v0 暂不支持

v0 不包含异常、通用 package registry、宏、完整 async runtime、Unicode escape、泛型 record、
泛型 trait、trait object、动态 dispatch、C 风格 `for`、切片语法、iterator protocol
或跨 C ABI 的稳定 function handle。类似宏的重复目前应通过函数、trait、stdlib helper 或显式
外部 codegen 解决；生成后的 `.nox` 文件按普通源码进入 `nox check` / `nox test` / LSP /
release gate。宏系统暂缓依据见
[0029 - 暂缓宏系统，优先使用函数、trait 与外部 codegen](decisions/0029-defer-macro-system.md)；
async/await 后续路线见
[0030 - 分阶段引入 async/await](decisions/0030-staged-async-await.md)。

命令用法见 [cli.md](cli.md)，运行时权限见 [runtime.md](runtime.md)，嵌入 API 见 [embedding.md](embedding.md)。
