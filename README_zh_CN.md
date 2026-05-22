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

## 当前状态

Nox 当前最新正式发布版本是 `v0.0.2`。该版本的 Cargo 版本、git tag、CHANGELOG、
release checklist、GitHub Release、远端 CI、本地 release gate 和分发 smoke 已经对齐。
后续 `0.0.x` 版本仍会继续演进语言、运行时和嵌入 API；破坏性变更必须在 CHANGELOG、
相关文档和 release notes 中明确说明。

生产边界按工程发布口径理解：没有已知高危缺陷、没有未说明的兼容破坏、默认权限保守、
发布步骤可审计且可回滚。它不表示数学意义上的绝对零风险。

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

### 使用 Release 包

GitHub Releases 从 `v0.0.2` 开始把命令行工具和嵌入式 SDK 分开发布：

- `nox-cli-v0.0.2-x86_64-unknown-linux-gnu.tar.gz`：面向终端用户，包含 `bin/nox`、
  README、CHANGELOG 和脚本示例。
- `nox-embed-v0.0.2-x86_64-unknown-linux-gnu.tar.gz`：面向宿主应用，包含
  `lib/libnox_core.so`、`include/nox_core.h`、README、CHANGELOG 和 C embedding 示例。

下载、校验并安装 CLI 到 `/usr/local/bin/nox`：

```sh
curl -LO https://github.com/liguangsheng/nox/releases/download/v0.0.2/nox-cli-v0.0.2-x86_64-unknown-linux-gnu.tar.gz
curl -LO https://github.com/liguangsheng/nox/releases/download/v0.0.2/nox-cli-v0.0.2-x86_64-unknown-linux-gnu.sha256
sha256sum -c nox-cli-v0.0.2-x86_64-unknown-linux-gnu.sha256
tar -xzf nox-cli-v0.0.2-x86_64-unknown-linux-gnu.tar.gz
sudo install -m 0755 nox-cli-v0.0.2-x86_64-unknown-linux-gnu/bin/nox /usr/local/bin/nox
nox --version
nox run ./nox-cli-v0.0.2-x86_64-unknown-linux-gnu/examples/hello.nox
```

下载并校验嵌入式 SDK 包：

```sh
curl -LO https://github.com/liguangsheng/nox/releases/download/v0.0.2/nox-embed-v0.0.2-x86_64-unknown-linux-gnu.tar.gz
curl -LO https://github.com/liguangsheng/nox/releases/download/v0.0.2/nox-embed-v0.0.2-x86_64-unknown-linux-gnu.sha256
sha256sum -c nox-embed-v0.0.2-x86_64-unknown-linux-gnu.sha256
tar -xzf nox-embed-v0.0.2-x86_64-unknown-linux-gnu.tar.gz
cc -Inox-embed-v0.0.2-x86_64-unknown-linux-gnu/include \
  nox-embed-v0.0.2-x86_64-unknown-linux-gnu/examples/embed/c_embedding.c \
  -Lnox-embed-v0.0.2-x86_64-unknown-linux-gnu/lib -lnox_core \
  -Wl,-rpath,"$PWD/nox-embed-v0.0.2-x86_64-unknown-linux-gnu/lib" \
  -o /tmp/nox-c-embedding-smoke
/tmp/nox-c-embedding-smoke
```

当前 release 资产只承诺 `x86_64-unknown-linux-gnu`。其他平台可以从源码构建。

### 从源码构建

本地构建 CLI：

```sh
cargo build -p nox
target/debug/nox --version
```

运行主示例：

```sh
cargo run -p nox -- run examples/hello.nox
cargo run -p nox -- check examples/hello.nox
cargo run -p nox -- check --json tests/fixtures/type-error.nox
cargo run -p nox -- test tests/fixtures/example_test.nox
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

- [docs/zh_CN/README.md](docs/zh_CN/README.md)：中文文档索引。
- [docs/zh_CN/language-v0.md](docs/zh_CN/language-v0.md)：已实现语言切片。
- [docs/zh_CN/cli.md](docs/zh_CN/cli.md)：命令行为和退出码。
- [docs/zh_CN/runtime.md](docs/zh_CN/runtime.md)：运行时权限和标准库。
- [docs/zh_CN/embedding.md](docs/zh_CN/embedding.md)：Rust 和 C 嵌入指南。
- [docs/zh_CN/diagnostics.md](docs/zh_CN/diagnostics.md)：机器可读诊断 code。
- [docs/zh_CN/benchmarks.md](docs/zh_CN/benchmarks.md)：benchmark smoke 跑法。
- [docs/zh_CN/development.md](docs/zh_CN/development.md)：验证、测试和迭代说明。
- [docs/zh_CN/directory-structure.md](docs/zh_CN/directory-structure.md)：目录结构和文件归属。
- [docs/en/README.md](docs/en/README.md)：英文文档索引。
