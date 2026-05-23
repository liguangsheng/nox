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
- `math-intrinsics.nox`：默认运行时数学内建函数。
- `arrays.nox`：同质数组、整数索引和 `len(array)`。
- `maps.nox`：`map[str, int]`、字符串 key、map 索引、`map_get` 和 map ergonomics。
- `records.nox`：命名 record、record 字面量、字段访问和 record method 调用糖。
- `tuples.nox`：tuple 类型和值、tuple 解构和 record 解构。
- `type-alias.nox`：`type` 别名、tuple alias 和 record 字段中的 alias 展开。
- `enums.nox`：用户自定义 `enum`、variant 构造和穷尽 `match`。
- `generic-functions.nox`：函数级泛型参数、实参推导和空容器返回类型推导。
- `bitwise.nox`：整数位运算 `& | ^ << >> ~`。
- `control-flow.nox`：带类型函数、`while`、赋值和 `if`。
- `control-flow-let-patterns.nox`：`if let`、`let ... else` 和 `while let` pattern 控制流。
- `else-if.nox`：链式 `else if`。
- `for-range.nox`：`for i in start..end` 半开整数区间。
- `match.nox`：受限 `match`，支持数字 / 字符串字面量、`int` 半开范围和嵌套
  `option` / `result` pattern。
- `logical.nox`：短路 `&&` 和 `||`。
- `conversions.nox`：显式 `to_float` 和 `to_int` 转换。
- `numeric-boundaries.nox`：整数除法和数值边界。
- `recursion.nox`：递归函数调用。
- `scopes.nox`：块作用域遮蔽。
- `spread.nox`：array / map literal spread 创建新容器和 map 覆盖顺序。
- `collections-config.nox`：`std/map.nox` 配置合并、删除和默认值读取。
- `collections-summary.nox`：`std/array.nox` 与 `std/map.nox` 分组统计和排序。
- `result-chain.nox`：`result` / `option` 的 `?` 错误传播。
- `error-summary.nox`：`std/option.nox` 与 `std/result.nox` 状态判断和 fallback。
- `process-stdio.nox`：`std/process.nox` 的 argv、stdin、stderr 和退出码。
- `path-summary.nox`：`std/path.nox` 的 join、normalize、basename、dirname 和 extension。
- `fs-summary.nox`：`std/fs.nox` 的 is_file、is_dir 和 list_dir。
- `strings.nox`：字符串参数、字符串拼接、`${expr}` 插值和 `std/string.nox` 文本处理函数。
- `string-escapes.nox`：`\n`、`\t`、`\"`、`\\`。
- `time.nox`：`std/time.nox` 的 Unix 时间格式化、解析和 duration helper。
- `json.nox`：`std/json.nox` 的 JSON parse/stringify、kind、array/object helper。
- `delimited-text.nox`：`std/csv.nox` 与 `std/tsv.nox` 的单行解析和格式化 helper。
- `print.nox`：`print` 和 `to_str_int` 输出内建。
- `stdlib.nox`：默认运行时的 `sqrt(value: float) -> float`。
- `export-main.nox`：只导入 `export-math.nox` 的导出表面。
- `export-math.nox`：导出 `double`，保留私有 helper。
- `projects/scoreboard/`：多模块项目 fixture，覆盖 manifest main、source dirs、
  test dirs、namespace import、`std/*` 静态模块、`option` / `result` match 和项目级
  check/test/fmt。
