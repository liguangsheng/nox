# Nox CLI

`nox` crate 提供 `.nox` 文件的默认命令行入口。开发时通常通过 Cargo 运行：

```sh
cargo run -p nox -- <command> <file.nox>
```

直接构建或安装二进制后，命令形状是：

```sh
nox run [file.nox]
nox --version
nox check [--json] [file.nox ...]
nox test [--json] [file-or-dir ...]
nox fmt [--check | --write] <file.nox> [file.nox ...]
nox project check
nox lsp
nox inspect-bytecode [--compact] <file.nox>
```

## `run`

```sh
cargo run -p nox -- run examples/hello.nox
```

`run` 会读取入口文件，相对于入口文件目录解析 import，执行解析、静态类型检查、
字节码编译、verifier，然后运行 VM 并打印最终值。

如果没有显式传入入口文件，CLI 会从当前目录向上查找 `nox.toml`，并执行
`[entrypoints].main` 指向的脚本。找不到 manifest 或 manifest 没有 `main` 时返回用法错误。
显式路径始终优先于 manifest main。

入口路径后面的剩余位置参数会传给脚本的 `args()`：

```sh
cargo run -p nox -- run examples/args.nox alpha beta
```

`examples/hello.nox` 的输出是：

```text
84
```

CLI 运行默认只授予文件系统读能力，用于入口文件和 import。环境、定时器、网络和
异步任务权限不会默认授予。

CLI 在加载入口文件之前会从入口文件所在目录向上查找 `nox.toml`。找到时，
manifest 的 `modules.source_dirs` 会作为 import 解析的备选根目录；
manifest 解析失败时输出诊断并返回非零退出码。manifest 行为见
[package-manifest-design.md](package-manifest-design.md)。

manifest 的 `[runtime].permissions` 只声明项目期望能力，不会让 CLI 自动授予环境、
定时器、网络、异步任务或文件写入权限。

`args() -> [str]` 不需要额外权限。`env_list() -> map[str, str]`、`env_get`
和 `std/env.nox` 的 `try_get(name: str) -> option[str]` 一样受 `environment` 权限控制。

## `--version`

```sh
nox --version
```

`--version` 打印当前 CLI 版本，输出格式固定为 `nox X.Y.Z`。本地分发 smoke 使用它确认
安装目录里的二进制来自当前构建。

## `check`

```sh
cargo run -p nox -- check examples/hello.nox
```

`check` 做解析、import 解析、静态类型检查、字节码编译和 verifier，但不执行程序。
它适合快速验证和编辑器集成。

`check` 可以接收多个文件路径。每个入口文件独立检查；任意文件失败时，命令返回非零。
不给路径时，CLI 从当前目录向上查找 `nox.toml`，并检查 `[entrypoints].main`、
`modules.source_dirs` 和 `modules.test_dirs` 展开的 `.nox` 文件；重复文件会去重。没有
manifest 时返回用法错误 `2`。

成功输出：

```text
examples/hello.nox: ok
```

静态类型错误会返回 `1`。例如：

```sh
cargo run -p nox -- check tests/fixtures/type-error.nox
```

会输出类似：

```text
tests/fixtures/type-error.nox:1:19: expected int, got str
```

parser 已有有限恢复能力，同一文件中多个独立语法错误可以一次输出。type checker
也会继续检查后续独立顶层声明，因此同一文件里的多个互不依赖类型错误可以一次输出。
块内复杂恢复仍保持保守，遇到不可靠的局部状态时只报告该顶层声明的第一条错误。

## `check --json`

工具和编辑器应使用 JSON 输出：

```sh
cargo run -p nox -- check --json tests/fixtures/type-error.nox
```

输出是一个 JSON 对象，顶层字段如下：

- `schema`：schema 版本标签。当前固定为 `"nox.check.v1"`。新增字段时保持向后兼容；
  破坏性变更会先升级该标签。
- `ok`：所有入口文件都通过时为 `true`，任意文件失败时为 `false`。
- `diagnostics`：诊断数组，按入口文件顺序排列。
- `files`：每个入口文件一条记录，按命令行顺序排列。
- `summary`：聚合计数，便于工具直接读取。

成功输出：

```json
{"schema":"nox.check.v1","ok":true,"diagnostics":[],"files":[{"path":"examples/hello.nox","ok":true,"diagnostic_count":0}],"summary":{"checked":1,"passed":1,"failed":0,"diagnostic_count":0}}
```

失败时，命令仍返回 `1`，不向 stderr 输出人类诊断，而是在 stdout 输出收集到的诊断：

```json
{"schema":"nox.check.v1","ok":false,"diagnostics":[{"file":"tests/fixtures/type-error.nox","code":"type.mismatch","message":"expected int, got str","span":{"start":18,"end":29},"source":{"name":"tests/fixtures/type-error.nox","line":1,"column":19}}],"files":[{"path":"tests/fixtures/type-error.nox","ok":false,"diagnostic_count":1}],"summary":{"checked":1,"passed":0,"failed":1,"diagnostic_count":1}}
```

项目发现错误返回 `2`，但 `--json` 仍输出同一 schema，并使用 `project.discovery` 诊断 code。
例如当前目录没有 `nox.toml`，或 manifest 展开的 `source_dirs` / `test_dirs` 不存在时，
`diagnostics[0].file` 和 `files[0].path` 使用 `<project>`，便于工具把错误归到项目配置层。

`diagnostics[]` 元素字段：

- `file`：产生该诊断的入口文件。
- `code`：机器可读诊断类别，例如 `type.mismatch`。
- `message`：面向人的错误说明。
- `span`：源码 byte offset 范围。
- `source`：文件名、1-based line 和 1-based column。

`files[]` 元素字段：

- `path`：入口文件路径，原样取自命令行；manifest 模式下是展开后的实际文件路径。
- `ok`：该入口文件是否通过所有检查。
- `diagnostic_count`：该入口文件产生的诊断数量。

`summary` 字段：

- `checked`：本次命令检查的入口文件总数。
- `passed`：通过的入口文件数量。
- `failed`：失败的入口文件数量。
- `diagnostic_count`：所有入口文件产生的诊断总数。

工具可以根据 `schema` 字段判断输出版本；后续 minor 阶段保持已有字段不变，未来如需破坏性变更
会升级 schema 标签，并在文档中明确兼容窗口。
机器可读 `code` 的稳定性见 [诊断 code](diagnostics.md)。

## `test`

```sh
cargo run -p nox -- test tests
```

`test` 顺序运行 `*_test.nox` 文件中的顶层测试函数。测试函数必须命名为
`test_*`，不接收参数，并返回 `bool`。返回 `true` 表示通过，返回 `false` 表示失败；
测试函数执行过程中产生运行时诊断也表示失败。

不给路径时，CLI 从当前目录发现测试文件；如果当前目录向上能找到 `nox.toml`，则优先从
manifest 的 `modules.test_dirs` 递归发现。没有 `test_dirs` 时回退到 `modules.source_dirs`，
再回退到 manifest 根目录。给出路径时，每个路径可以是测试文件或目录；目录会递归收集
`*_test.nox`。显式文件也必须符合 `*_test.nox` 命名约定。

人类可读输出按测试打印：

```text
tests/math_test.nox::test_add PASS
tests/math_test.nox::test_division FAIL
summary: 2 tests, 1 passed, 1 failed
```

测试失败返回 `1`。CLI 用法错误，例如未知 flag、路径不存在或显式文件不是
`*_test.nox`，返回 `2`。

### `test --json`

工具应使用 JSON 输出：

```sh
cargo run -p nox -- test --json tests
```

输出字段：

- `schema`：固定为 `"nox.test.v1"`。
- `ok`：所有测试通过时为 `true`。
- `tests`：每个测试一条记录，包含 `file`、`name`、`ok` 和 `diagnostic`。
- `summary`：`tests`、`passed`、`failed` 计数。

示例：

```json
{"schema":"nox.test.v1","ok":false,"tests":[{"file":"tests/math_test.nox","name":"test_add","ok":true,"diagnostic":null},{"file":"tests/math_test.nox","name":"test_division","ok":false,"diagnostic":{"code":"runtime.division-by-zero","message":"division by zero","span":{"start":48,"end":53},"source":{"name":"tests/math_test.nox","line":2,"column":22}}}],"summary":{"tests":2,"passed":1,"failed":1}}
```

`--json` 模式下，测试结果写到 stdout；测试失败的诊断嵌入 JSON，不额外写 stderr。
语法、类型检查或测试签名错误会作为 `<module>` 失败记录输出。

## `fmt`

```sh
cargo run -p nox -- fmt examples/hello.nox
```

默认形式 `nox fmt <file.nox>` 解析源码并把稳定格式化结果打印到 stdout，不会原地
改写文件。源码无效时返回 `1`，诊断格式与其他人类可读 CLI 命令一致。stdout 形式
一次只接受一个 `.nox` 文件，避免误把多个文件拼接到 stdout。

formatter 从 AST 重新打印源码，当前不会保留源码注释。需要保留注释的工作流应暂时
避免对含注释文件执行 `fmt --write`。

### `fmt --check`

```sh
cargo run -p nox -- fmt --check src/main.nox src/util.nox
cargo run -p nox -- fmt --check examples
```

`--check` 不修改文件。入口可以是文件或目录；目录会递归展开其中的 `.nox` 文件。
未传入口径时，`fmt --check` 会按当前目录发现的 `nox.toml` 展开项目入口、
`source_dirs` 和 `test_dirs`。对每个入口文件做格式化，并把内容已经不一致的文件路径输出到
stdout，每行一个。所有文件已经格式化时退出码为 `0`、stdout 为空；任意一个文件需要
重新格式化时退出码为 `1`。源码无效或读文件失败仍走人类可读诊断并退出 `1`。

适合在 CI 中执行：把不一致文件列表交给后续步骤，或仅根据退出码判断。

### `fmt --write`

```sh
cargo run -p nox -- fmt --write src/main.nox src/util.nox
cargo run -p nox -- fmt --write examples
```

`--write` 显式改写文件。入口可以是文件或目录；目录会递归展开其中的 `.nox` 文件。
未传入口径时，`fmt --write` 使用与 `fmt --check` 相同的 manifest 发现规则。每个入口文件被格式化后写回原路径；内容已经一致的文件不会
重新写入。源码无效或写文件失败时输出诊断并返回 `1`。

`--check` 和 `--write` 互斥；二者同时出现会立即返回 `2`。

## `project check`

```sh
cargo run -p nox -- project check
cargo run -p nox -- project check --json
```

`project check` 是项目级人类可读验证入口。它要求当前目录或父目录能发现 `nox.toml`，
然后按顺序执行同一项目语义下的三步：

1. `nox check`
2. `nox test`
3. `nox fmt --check`

每一步都复用对应命令无显式 path 时的 manifest 发现规则，因此在项目根或子目录运行时，
都会围绕同一个 manifest main、`modules.source_dirs` 和 `modules.test_dirs` 工作。
该入口不接受文件参数；需要检查单个文件时继续使用 `check/test/fmt` 的显式 path。

`project check` 不做包管理、不安装依赖，也不授予额外 runtime permissions。默认输出是
人类可读的本地验证日志；`--json` 输出 `nox.project-check.v1`，包含 manifest root、
package name/version、三个子步骤的退出码以及被捕获的 stdout/stderr，便于 CI 判断项目发现
边界和失败步骤。任一子步骤失败时，
命令返回非零；用法错误或缺少 manifest 返回 `2`。

`nox.project-check.v1` 的顶层结构如下：

```json
{
  "schema": "nox.project-check.v1",
  "ok": true,
  "manifest": {
    "root": "/path/to/project",
    "package": { "name": "scoreboard", "version": "0.0.3" }
  },
  "steps": [
    { "name": "check", "ok": true, "status": 0, "stdout": "...", "stderr": "" },
    { "name": "test", "ok": true, "status": 0, "stdout": "...", "stderr": "" },
    { "name": "fmt", "ok": true, "status": 0, "stdout": "", "stderr": "" }
  ],
  "summary": { "steps": 3, "passed": 3, "failed": 0 }
}
```

## `lsp`

```sh
cargo run -p nox -- lsp
```

`lsp` 启动最小 stdio Language Server Protocol 服务。当前支持：

- `initialize`
- full-document `textDocument/didOpen`
- full-document `textDocument/didChange`
- diagnostics
- `textDocument/hover`
- `textDocument/formatting`
- `textDocument/completion`
- `shutdown`
- `exit`

diagnostics 复用 `check` 的 parser、import resolver、type checker 和 verifier。
`file://` 文档的相对 import 从打开文件所在目录解析，并会向父目录发现 `nox.toml`；
发现 manifest 后，LSP 与 CLI 一样使用 `modules.source_dirs` 作为 import 搜索根。
manifest 解析失败时，LSP 会先发布 `manifest.invalid` 诊断，不会继续把同一问题伪装成
module resolution 失败。
LSP 会把所有已打开的 `file://` 文档作为 import overlay 注入 module loader：当
import specifier 命中一个已经在编辑器打开的文件时，使用编辑器中的内容而不是读磁盘，
编辑期间无需先保存。
LSP 内部复用 `Session` / `ModuleGraph` 的源码缓存和 overlay 语义；任何 didOpen 或
didChange 都会重新发布所有已打开文档的 diagnostics，因此被 import 文件变化后，导入它的
打开文档也会刷新诊断。当前不做后台文件 watch，未打开文件只会在打开文档触发检查时按
manifest/import 规则读取。

hover 使用静态类型检查结果，返回 plaintext 类型，例如 `int`、`str`、`[int]`、
`fn(int) -> int`。

`textDocument/formatting` 复用 `fmt` 的格式化逻辑。如果文档已经格式化，返回
空 edit 数组；否则返回一个覆盖整个文档的 `TextEdit`。源码解析失败时返回 `null`，
保持人类可见诊断不变。

`textDocument/completion` 默认返回保留关键字、语言/runtime 内建函数和当前文档中光标
位置之前的标识符。内建函数包含 `len`、`contains`、`map_get`、`args`、`sqrt`
等常用全局表面。
在 `alias.` 之后触发时，如果当前文档存在 `import "path" as alias;`，completion 会按
该模块的导出表面返回成员，并复用 open-document overlay。`std/fs.nox`、`std/env.nox`
和 `std/time.nox` 是 runtime 虚拟模块，不要求磁盘上存在对应文件。

当前 LSP 不支持 workspace symbol、rename、go-to-definition、后台 watch mode 或
daemon 模式；这些 request 不在 `initialize` capability 中声明。

## `inspect-bytecode`

```sh
cargo run -p nox -- inspect-bytecode examples/hello.nox
```

`inspect-bytecode` 走 parse/import/typecheck/compile/verify，但不执行程序。输出的
bytecode 是 VM 当前执行的 flat instruction stream，用于实现调试，不是稳定序列化格式。

使用紧凑输出：

```sh
cargo run -p nox -- inspect-bytecode --compact examples/hello.nox
```

紧凑输出每行一个 instruction，并以前置序号标识位置：

```text
0000 Function double(value: float) -> float [4 instructions]
0001 Define double
```

## 退出码

- `0`：命令成功。
- `1`：源码、import、类型检查、编译、运行时或宿主边界错误。
- `2`：CLI 用法错误，例如缺少路径或未知命令。

## 权限提醒

CLI 的文件系统权限覆盖入口文件、相对 import 加载和默认运行时文件读能力，但入口/import
读取不表示脚本应被默认授予任意文件读写。嵌入宿主需要收紧脚本文件访问时，应使用
`RuntimePermissions` 的 read/write root allowlist。脚本调用 `env_get`、`sleep_ms`、
`tcp_connect`、`task_sleep_ms` 或 `task_ready` 时，可以通过静态类型检查，但如果宿主没有授予对应权限，
运行时会失败。权限模型见 [runtime.md](runtime.md)。
