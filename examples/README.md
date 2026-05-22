# Nox 示例

这里的文件是当前 v0 语言切片的可执行示例、负向 fixture 和嵌入 smoke。
`examples/embed/c_embedding.c` 是宿主侧 C ABI 最小验证程序；
`crates/nox/examples/rust_embedding.rs` 是 Rust 长期宿主流程示例。

运行任意 `.nox` 示例：

```sh
cargo run -p nox -- run examples/hello.nox
```

检查负向 fixture：

```sh
cargo run -p nox -- check examples/type-error.nox
cargo run -p nox -- check --json examples/type-error.nox
```

运行测试 fixture：

```sh
cargo run -p nox -- test examples
```

运行多模块项目 fixture：

```sh
cd examples/projects/scoreboard
cargo run -p nox -- project check
```

## 正向示例

- `hello.nox`：import `math.nox`，调用带类型函数并输出 `84`。
- `math.nox`：被 `hello.nox` 导入的简单函数模块。
- `arrays.nox`：同质数组、整数索引和 `len(array)`。
- `maps.nox`：`map[str, int]`、字符串 key、map 索引和 `map_get`。
- `records.nox`：命名 record、record 字面量和字段访问。
- `control-flow.nox`：带类型函数、`while`、赋值和 `if`。
- `else-if.nox`：链式 `else if`。
- `for-range.nox`：`for i in start..end` 半开整数区间。
- `match.nox`：受限 `match`，支持 `int` / `str` 字面量 case 和 `_` 默认分支。
- `logical.nox`：短路 `&&` 和 `||`。
- `conversions.nox`：显式 `to_float` 和 `to_int` 转换。
- `numeric-boundaries.nox`：整数除法和数值边界。
- `recursion.nox`：递归函数调用。
- `scopes.nox`：块作用域遮蔽。
- `strings.nox`：字符串参数和字符串拼接。
- `string-escapes.nox`：`\n`、`\t`、`\"`、`\\`。
- `stdlib.nox`：默认运行时的 `sqrt(value: float) -> float`。
- `export-main.nox`：只导入 `export-math.nox` 的导出表面。
- `export-math.nox`：导出 `double`，保留私有 helper。
- `formatter-golden.nox`：覆盖 formatter golden、option/result 类型与构造、
  `match` payload binding 和二次格式化稳定性。
- `example_test.nox`：`nox test` 的最小测试文件。
- `projects/scoreboard/`：多模块项目 fixture，覆盖 manifest main、source dirs、
  test dirs、namespace import、`std/*` 静态模块、`option` / `result` match 和项目级
  check/test/fmt。

## 运行时错误 fixture

- `runtime-error-array-bounds.nox`：数组越界访问。
- `runtime-error-divide-zero.nox`：除零。
- `runtime-error-map-key.nox`：缺失 map key。

## 语法错误 fixture

- `syntax-error-string-escape.nox`：不支持的字符串 escape。
- `syntax-errors.nox`：parser 多错误恢复 fixture。
- `malformed/`：panic-free robustness smoke corpus，覆盖 lexer、parser、formatter、
  type checker、module resolver、manifest 和 LSP 的坏输入边界。

## 类型错误 fixture

- `type-error.nox`：基础静态类型错误。
- `type-error-array-element.nox`：混合数组元素类型。
- `type-error-array-index.nox`：非 `int` 数组下标。
- `type-error-array-len.nox`：对非数组调用 `len`。
- `type-error-for-range.nox`：非 `int` range 边界。
- `type-error-int-float.nox`：混合 `int` 和 `float` 运算。
- `type-error-logical.nox`：非 `bool` 逻辑操作数。
- `type-error-map-index.nox`：非 `str` map 下标。
- `type-error-map-key.nox`：非字符串 map 字面量 key。
- `type-error-map-value.nox`：map value 类型不匹配。
- `type-error-record-duplicate-field.nox`：record 字段重复。
- `type-error-record-extra-field.nox`：record 字面量包含额外字段。
- `type-error-record-field-access.nox`：访问未知 record 字段。
- `type-error-record-field-type.nox`：record 字段值类型不匹配。
- `type-error-record-missing-field.nox`：record 字面量缺少必填字段。
- `type-error-sqrt-int.nox`：向 `float` API 传入 `int`。
- `type-error-sleep-float.nox`：向 `int` API 传入 `float`。
