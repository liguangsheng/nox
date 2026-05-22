# 开发 Nox

Nox 是一个 Rust workspace，当前有两个 crate：

```text
crates/nox_core  可嵌入引擎
crates/nox       默认运行时和 CLI
```

目录归属见 [directory-structure.md](directory-structure.md)。

## 常用验证

交付一批改动前至少运行：

```sh
cargo fmt --all --check
cargo test --all
cargo clippy --all-targets -- -D warnings
git diff --check HEAD
```

GitHub Actions (`.github/workflows/ci.yml`) 跑同一套命令，并额外执行 C embedding
smoke 和本地 Markdown 链接检查。本地命令保持与 CI 一致，避免“本地通过 CI 失败”。

发布前或需要完整本地门禁时，运行：

```sh
scripts/release-gate.sh
```

该脚本串行执行 Cargo gate、CLI version/run/check/test/fmt smoke、scoreboard project smoke、embedding regression、
robustness smoke、benchmark smoke、Markdown 链接检查和 `git diff --check HEAD`。它只做
本地验证，不 push、不打 tag、不发布外部资产。

验证本地分发包和安装目录时，运行：

```sh
scripts/local-dist-smoke.sh
```

该脚本构建 release CLI 和 `nox_core` 动态库，把 `nox`、`nox_core.h`、`libnox_core`
和最小示例复制到临时目录，然后从该目录运行 `nox --version`、`examples/hello.nox` 和
C header smoke。它只写入临时目录或 `NOX_LOCAL_DIST_DIR` 指定目录，不提交构建产物。

改 parser、type checker、bytecode、VM、runtime、CLI 或示例时，补跑相关 CLI smoke：

```sh
cargo run -p nox -- run examples/hello.nox
cargo run -p nox -- check examples/hello.nox
cargo run -p nox -- check --json tests/fixtures/type-error.nox
cargo run -p nox -- test tests/fixtures/example_test.nox
cargo run -p nox -- test --json tests/fixtures/example_test.nox
cargo run -p nox -- fmt examples/hello.nox
cargo run -p nox -- inspect-bytecode --compact examples/hello.nox
```

改 Rust/C embedding 表面、C ABI 或 header 时，运行 embedding regression：

```sh
scripts/embedding-regression.sh
```

改文档时，至少运行本地 Markdown 链接检查：

```sh
python3 -c 'import pathlib,re,sys
roots=[pathlib.Path(p) for p in ["README.md","README_zh_CN.md","docs/en","docs/zh_CN","examples/README.md"]]
files=[]
for root in roots:
    if root.is_dir(): files.extend(root.rglob("*.md"))
    elif root.exists(): files.append(root)
missing=[]
for path in files:
    text=path.read_text()
    for target in re.findall(r"\[[^\]]+\]\(([^)#][^)]+)\)", text):
        if "://" in target or target.startswith("mailto:"): continue
        target_path=(path.parent/target).resolve()
        if not target_path.exists(): missing.append((str(path),target))
if missing:
    print("missing markdown links:")
    [print(f"{p}: {t}") for p,t in missing]
    sys.exit(1)
print("markdown links ok")'
```

## 测试分布

`crates/nox_core/src/compiler_tests.rs` 覆盖：

- lexer 和 span。
- parser 恢复和多条语法诊断。
- AST 到 bytecode 编译。
- verifier 对非法 jump、stack underflow、scope underflow、branch exit scope underflow、
  nested function malformed bytecode、map/record/field stack effect 的拒绝。
- bytecode inspect。

`crates/nox_core/src/language_tests.rs` 覆盖：

- 变量、赋值、算术、函数和返回。
- `int` / `float` 分离和显式转换。
- 字符串、短路逻辑、控制流、递归、作用域。
- 数组、map、record、字段访问和容器错误。
- 运行时错误和 instruction budget。

`crates/nox_core/src/api_tests.rs` 覆盖：

- Rust host function 注册和返回值验证。
- C ABI callback、版本、字符串返回、last-error 和复合 handle。
- import loader、重复 import、循环 import。
- heap 统计和回收。

`crates/nox/tests/cli.rs` 覆盖：

- CLI 对示例的正向运行。
- `check`、`check --json`、多文件诊断。
- `test` 和 `test --json` 的通过、失败、运行时错误和发现规则。
- `fmt`、`lsp`、`inspect-bytecode`。
- import、export、私有声明和循环 import。

性能 smoke：

```sh
scripts/bench-smoke.sh
```

该脚本输出 release CLI 的 tab-separated 耗时，覆盖递归、循环、容器、模块加载和
`nox test`。`.nox` benchmark 会记录 `check`、`compile`、`e2e` 三类 CLI 阶段代理；
`nox test` 记录端到端耗时。脚本断言预期输出片段，并在系统支持 `timeout` 时默认限制单个
case 10 秒。数字只用于同机前后对比，不作为 CI 硬门禁。

Malformed source smoke：

```sh
scripts/robustness-smoke.sh
```

该脚本使用 `tests/malformed/` 的固定 corpus，覆盖未闭合字符串、深嵌套表达式、
非法 token、半截 import、错误 record、namespace import 错误、深层 record/map 类型错误、
manifest 错误和 LSP 半截源码。语法/词法错误要求 `check` 与 `fmt` 稳定返回 `1`；
静态类型错误要求 `check` 返回 `1` 且 `fmt` 返回 `0`；manifest 配置错误当前是 CLI
用法/项目发现层错误，要求返回 `2`；LSP 半截源码要求 stdio session 正常退出 `0` 并发布
diagnostics。其它退出码视为回归，尤其用于捕捉 panic。

`nox_core` 还包含生成式大输入回归，覆盖大量重复 malformed declaration 和独立 type mismatch。
这些测试不进入 `tests/malformed/`，避免把机械 corpus 变成用户示例；验证入口是
`cargo test -p nox_core parser`。

新增 robustness corpus 时按错误边界分类：

- 语法/词法错误放在 `tests/malformed/*.nox`，并加入脚本中 `check=1` / `fmt=1` 的列表。
- 可格式化的静态或 module resolver 错误放在 `tests/malformed/*.nox`，并加入
  `check=1` / `fmt=0` 的列表。
- manifest 错误放在 `tests/malformed/manifest-*` 目录，脚本从目录内运行无显式路径的
  `check` 和 `fmt --check`。
- LSP-only 输入放在 `tests/malformed/`，脚本生成 Content-Length frame 后喂给
  `nox lsp`。

如果新 case 触发 panic，先保留最小 fixture，再修实现；不要把随机 fuzz 输出直接塞进
corpus。

## 修改规则

- 语言新增语法时，同步 parser、type checker、bytecode、VM、示例、负向 fixture、文档和 CLI 测试。
- 系统能力放在 `crates/nox`，不要塞进 `nox_core`。
- 公开 Rust API、C ABI、CLI JSON 或 LSP 输出时，先写清兼容边界。
- 新增或拆分诊断 code 时，同步 [诊断 code](diagnostics.md)，JSON/LSP 测试优先断言
  code、span/range 和 source。
- 文档中的实现状态要跟代码一致；已实现行为写成事实，未来计划写成计划。
- 不把 `target/`、本地 agent 配置或临时二进制提交到源码树。
