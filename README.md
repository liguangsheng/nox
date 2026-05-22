# Nox

Nox 是一个用 Rust 编写的可嵌入静态类型脚本引擎和默认运行时。脚本文件使用
`.nox` 扩展名。

当前 workspace 有两个 crate：

- `nox_core`：可嵌入引擎，负责语言前端、静态类型检查、字节码、VM、值模型、
  诊断、宿主函数和 C ABI。
- `nox`：默认运行时和 CLI，构建在 `nox_core` 之上，负责文件加载、权限控制、
  标准库、LSP 和命令行入口。

`nox_core` 暴露 Rust API 和 C ABI。C 头文件位于
`crates/nox_core/include/nox_core.h`。

第一版语言切片已经实现：带 span 的词法 token、递归下降 parser、静态类型检查、
flat bytecode 编译、VM、带类型变量、带类型函数、调用、块、`if`、`while`、
半开 `for` range、`return`、数组、`map[str, T]`、命名 `record`、相对 import
和 `export` 可见性。

默认运行时会相对于入口文件解析 `import "..."`，并安装一个小型带类型标准库。
推荐通过静态模块导入使用文件、环境和时间能力，例如
`import "std/fs.nox" as fs;`；旧全局函数仍作为兼容表面保留。
运行时权限是显式的：CLI 只默认授予入口文件和 import 所需的文件系统读能力；
环境变量、定时器、网络和异步任务辅助函数都由单独权限控制。

## 快速开始

运行主示例：

```sh
cargo run -p nox -- run examples/hello.nox
cargo run -p nox -- check examples/hello.nox
cargo run -p nox -- check --json examples/type-error.nox
cargo run -p nox -- test examples/example_test.nox
cargo run -p nox -- fmt examples/hello.nox
cargo run -p nox -- inspect-bytecode --compact examples/hello.nox
```

运行多模块示例项目：

```sh
cd examples/projects/scoreboard
cargo run -p nox -- project check
```

更多示例位于 `examples/`：

- `arrays.nox`：同质数组、整数索引和 `len(array)`。
- `maps.nox`：`map[str, T]`、字符串 key、map 索引和 `map_get`。
- `control-flow.nox`：带类型函数、`while`、赋值和 `if`。
- `export-main.nox`：显式 `export` 模块边界。
- `example_test.nox`：`nox test` 的最小测试文件。
- `for-range.nox`：半开 `int` 区间循环。
- `match.nox`：受限 `match` 分支。
- `numeric-boundaries.nox`：整数除法和显式数值转换边界。
- `recursion.nox`：递归函数调用。
- `records.nox`：命名 record、record 字面量和字段访问。
- `stdlib.nox`：默认运行时宿主函数调用。
- `projects/scoreboard/`：带 `nox.toml`、namespace import、source/test dirs 的多模块项目。
- `type-error*.nox`、`syntax-errors.nox`、`runtime-error*.nox`：负向 fixture。

## 文档

- [docs/README.md](docs/README.md)：文档索引。
- [docs/language-v0.md](docs/language-v0.md)：已实现语言切片。
- [docs/cli.md](docs/cli.md)：命令行为和退出码。
- [docs/runtime.md](docs/runtime.md)：运行时权限和标准库。
- [docs/embedding.md](docs/embedding.md)：Rust 和 C 嵌入指南。
- [docs/diagnostics.md](docs/diagnostics.md)：机器可读诊断 code。
- [docs/benchmarks.md](docs/benchmarks.md)：benchmark smoke 跑法。
- [docs/development.md](docs/development.md)：验证、测试和迭代说明。
- [docs/directory-structure.md](docs/directory-structure.md)：目录结构和文件归属。
