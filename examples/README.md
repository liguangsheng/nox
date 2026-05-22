# Nox 示例

这里的文件是当前 v0 语言切片的可执行正向示例、示例项目和嵌入 smoke。
`examples/embed/c_embedding.c` 是宿主侧 C ABI 最小验证程序；
`crates/nox/examples/rust_embedding.rs` 是 Rust 长期宿主流程示例。

运行任意 `.nox` 示例：

```sh
cargo run -p nox -- run examples/hello.nox
```

测试 fixture、负向输入和 benchmark corpus 已移到 `tests/`。检查基础负向 fixture：

```sh
cargo run -p nox -- check tests/fixtures/type-error.nox
cargo run -p nox -- check --json tests/fixtures/type-error.nox
```

运行测试 fixture：

```sh
cargo run -p nox -- test tests/fixtures/example_test.nox
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
- `projects/scoreboard/`：多模块项目 fixture，覆盖 manifest main、source dirs、
  test dirs、namespace import、`std/*` 静态模块、`option` / `result` match 和项目级
  check/test/fmt。
