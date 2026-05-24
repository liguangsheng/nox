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
nox test [--json] [--filter <substr>] [--retry <N>] [--export-failures <dir>] [--export-failures-classified <dir>] [file-or-dir ...]
nox fmt [--check | --write] <file.nox> [file.nox ...]
nox project check
nox fetch [--offline] [--cache-dir <dir>]
nox new <name> [--dir <path>] [--force]
nox repl
nox lsp
nox dap
nox profile <file.nox>
nox coverage <file.nox>
nox inspect-bytecode [--compact] <file.nox>
nox watch [--interval-ms <ms>] (check|test|run) [args...]
nox lint [--json] <file.nox> [file.nox ...]
nox doc <file.nox>
```

## `new`

`nox new <name> [--dir <path>] [--force]` 创建一个最小项目，包含 `nox.toml`、
`src/main.nox`、`tests/main_test.nox` 和 `README.md`。不传 `--dir` 时目标目录是
`./<name>`；传入 `--dir` 时目标目录就是该路径，package name 仍来自 `<name>`。
生成后的项目可以直接运行 `nox project check`、`nox run`、`nox test` 和
`nox fmt --check`。目标目录已存在且非空时默认拒绝；`--force` 只覆盖脚手架文件，
不会删除未知用户文件。

## `doc`

`nox doc` 输出一个 Markdown 文档，列出脚本中所有顶层 `fn`、`async fn`、
`record`、`enum`、`type` 和 `trait` 声明（export 和 local），并把每个声明前的
`///` doc comment（紧邻在
声明上方的连续 `///` 行）作为描述。每个章节包含 `Kind:` 与 `Visibility:` 标签。
`async fn` 章节会额外展示调用侧返回类型 `task[T]`，同时保留源码签名
`async fn ... -> T`。注释中的空白后第一个空格会被吞掉以保持 Markdown 对齐。
当前实现是 text-based 扫描，不解析完整 AST；LSP hover 已复用顶层声明的相邻 doc comment。富 AST 文档生成与
`nox` 自身 stdlib 自动校验留作后续 session。

## `lint`

`nox lint` 报告非阻断质量提示。当前规则集：`lint.unused-variable`、
`lint.unused-function`、`lint.unused-import`、`lint.unreachable-code`、
`lint.shadowed-variable`、`lint.constant-condition`。扫描顶层声明（`let`、`fn`、
`import as alias`）并对比 AST 中变量引用集合。下划线开头的标识符（如 `_ignored`）
默认跳过 unused-variable 检查。退出码 0（即使有 warning）；`--json` 输出
`nox.lint.v1` schema，包含 `summary.capabilities`（按导入 + 调用模式推断脚本
所需的 runtime capability 集合，例如 `filesystem` / `filesystem_write` /
`process_run` / `environment` / `network` / `timers` / `async_tasks`）。
文本模式追加 `capabilities: ...` 行。

## `watch`

`nox watch` 包装单个 `check`/`test`/`run` 子命令，文件变化时重新触发。监视范围按
manifest 的 `source_dirs` + `test_dirs`；没有 manifest 则使用当前目录。采用 `stat`
轮询（默认 500ms，可用 `--interval-ms` 调整），前台运行，CTRL-C 退出。按 ADR 0022，
不引入 daemon、增量 typecheck 缓存或 hot reload。watch 启动时监视路径不存在返回稳定
诊断 code `watch.path-not-found`。

## `run`

```sh
cargo run -p nox -- run examples/hello.nox
```

`run` 会读取入口文件，相对于入口文件目录解析 import，执行解析、静态类型检查、
字节码编译、verifier，然后运行 VM 并打印最终值。

如果没有显式传入入口文件，CLI 会从当前目录向上查找 `nox.toml`，并执行
`[entrypoints].main` 指向的脚本。找不到 manifest 或 manifest 没有 `main` 时返回用法错误。
显式路径始终优先于 manifest main。

入口路径后面的剩余位置参数会传给脚本的 `args()` 和 `std/process.nox` 的 `argv()`；二者都不包含
脚本路径：

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
`std/process.nox` 的 `read_stdin()` 读取完整 stdin，`print_err(value)` 向 stderr 写一行，
`exit(code)` 接受 `0..255` 并在脚本成功结束后作为 `nox run` 的进程退出码。

## `--version`

```sh
nox --version
```

`--version` 打印当前 CLI 版本，输出格式固定为 `nox X.Y.Z`。本地分发 smoke 使用它确认
安装目录里的二进制来自当前构建。

## `repl`

```sh
nox repl
```

`repl` 从 stdin 逐行读取 Nox statement 或 expression，求值后打印非 `null` 的最终值。
会话保留前面成功输入的顶层声明，因此可以先定义变量或函数，再在后续输入中使用。
输入 `:quit`、`:exit` 或发送 EOF 退出。

## `profile` / `coverage`

```sh
nox profile tests/benchmarks/bench-fib.nox
nox coverage tests/benchmarks/bench-fib.nox
```

`profile` 执行脚本并输出 tab-separated 报告：函数表包含 `function`、`call_count`、
`total_us`；operation 表包含 `operation`、名称、`count`、`total_us`，覆盖 host
callback、容器 literal、index、match pattern 和 map helper 等 VM 热路径。脚本函数调用通过 VM 调用路径记录，递归函数会累计真实调用次数；`<module>` 表示顶层模块执行。
`coverage` 复用同一执行数据，输出执行过的函数行、VM span 级 statement 执行次数和 branch
true/false 次数，适合 release 前确认入口路径、语句和分支覆盖情况。
`--json` 输出聚合 schema（`nox.profile.v1` / `nox.coverage.v1`，含 `functions` 与
`operations`；coverage 还会兼容新增 `statements` / `branches` 数组，包含 byte span 与
1-based source location）；`--ndjson` 输出函数和 operation 事件 schema（`nox.profile.event.v1` /
`nox.coverage.event.v1`），coverage 还会输出 `kind:"statement"` / `kind:"branch"` 事件，
便于工具流式消费。
`--json` 与 `--ndjson` 互斥。

## `trace`

```sh
nox trace --ndjson script.nox
```

`trace` 输出 `nox.trace.event.v1` NDJSON 事件流，包含运行开始、静态 capability 摘要、
逐 capability 的 `permission_check` requirement、runtime `io` / `timer` / `task`
事件、捕获的 stdout/stderr、函数 / operation profile 行、host callback summary、
逐调用 `host_callback_call` enter/exit 事件、diagnostic 和运行结束。每行都包含稳定
`trace_id` 与递增 `seq`，便于 CLI/LSP 或外部工具关联一次运行的诊断与事件。diagnostic
事件同时携带 `span`、`source`，以及运行时错误可用的 `stack_frames`。runtime `io`
事件覆盖 stdout/stderr write、stdin read、顶层文件 helper，以及 `std/fs.nox`
filesystem 操作。

LSP publishDiagnostics 也会在每条诊断的 `data.trace_id` 中携带确定性关联 id，便于编辑器或
外部工具把 LSP 诊断和 trace/log 记录对应起来。

## `dap`

```sh
nox dap
```

`dap` 通过 stdio 提供 Debug Adapter Protocol 最小子集，使用 `Content-Length` frame。
当前支持 `initialize`、`setBreakpoints`（含 condition metadata 与最小条件求值）、
`setExceptionBreakpoints`、`launch`、`configurationDone`、`threads`、`stackTrace`、
`scopes`、`variables`、`continue`、`next` 和 `disconnect`。VS Code 扩展会通过同一个
`nox` binary 启动该 adapter。
条件断点当前支持 launch 求值后的 `result == value` / `result != value` 判断；条件不匹配时不会伪造
breakpoint stop。开启 `raised` exception filter 后，launch 阶段 runtime error 会发布
`reason:"exception"` 的 stopped event。
`variables` request 支持可选 `depth` / `maxDepth` 参数：`0` 表示不返回可展开子引用，
更大值会暴露受深度限制的 `debugState` 子变量，并可查看 condition / exception 调试状态。

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
- `stack_frames`：可选；运行时诊断经过脚本函数调用栈传播时出现。每个 frame 包含
  `name`、`span` 和可选 `source`，顺序为最近的调用帧在前。

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
- `tests`：每个测试一条记录，包含 `file`、`name`、`ok`、`attempts`、`retried`、
  `duration_us`、`stdout`、`stderr`、`diagnostic`、`snapshot_diff`、`kind` 和
  `mock_events`。
- `suites`：按测试文件分组的 suite/case hierarchy，当前每个 suite 使用 `file`
  作为标识，`cases` 列出该文件内进入本次报告的测试名。
- `summary`：`tests`、`passed`、`failed` 计数。

示例：

```json
{"schema":"nox.test.v1","ok":false,"tests":[{"file":"tests/math_test.nox","name":"test_add","ok":true,"attempts":1,"retried":false,"duration_us":120,"stdout":"setup\n","stderr":"","diagnostic":null},{"file":"tests/math_test.nox","name":"test_division","ok":false,"attempts":1,"retried":false,"duration_us":95,"stdout":"","stderr":"","diagnostic":{"code":"runtime.division-by-zero","message":"division by zero","span":{"start":48,"end":53},"source":{"name":"tests/math_test.nox","line":2,"column":22},"stack_frames":[{"name":"test_division","span":{"start":0,"end":42},"source":{"name":"tests/math_test.nox","line":1,"column":1}}]}}],"summary":{"tests":2,"passed":1,"failed":1}}
```

`--json` 模式下，测试结果写到 stdout；测试失败的诊断嵌入 JSON，不额外写 stderr。
测试脚本中的 `print(...)` 和 `std/process.nox` `print_err(...)` 会按 case 捕获到
对应记录的 `stdout` / `stderr` 字段，不污染 JSON 外层 stdout/stderr。语法、类型检查或测试签名错误会作为 `<module>` 失败记录输出。
`kind` 把测试记录分类为 `unit`、`integration` 或 `fixture`：路径组件包含
`fixtures` 的文件报告为 `fixture`，位于 `tests` 下的文件报告为 `integration`，
其他 test file 报告为 `unit`。
`std/test.nox` 的 `assert_snapshot` 失败时，记录会额外带
`snapshot_diff: {label, actual, expected}`；其他测试为 `null`。
`mock_events` 是为 mock capability harness 预留的兼容新增数组；普通 CLI runner
当前输出空数组。
`--export-failures <dir>` 是 opt-in fuzz bridge：当失败诊断包含 `std/test.nox`
property replay metadata 时，runner 会把原始测试源码和 source/test/diagnostic
注释写成 `.nox` corpus 文件放到 `<dir>`。根据用途可把目录指向 `fuzz/corpus/...`
或 `tests/malformed/...`，分别服务 cargo-fuzz 或确定性坏输入回归。
`--export-failures-classified <dir>` 保持 `--export-failures` 的兼容行为，同时把可导出
失败写到 `<dir>/<classification>/`：property replay 失败进入 `property`，模块级坏输入
按 diagnostic code 分到 `parser`、`typecheck`、`verifier` 或 `runtime`。
`diagnostic.stack_frames` 是兼容新增字段，只在运行时错误跨脚本函数调用传播时出现；旧 consumer
可以忽略该字段。

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

`project check` 不做包管理、不安装依赖，也不授予额外 runtime permissions。声明了
`[dependencies]` 时，它要求 manifest root 下存在匹配的 `nox.lock`，并报告缺失、格式错误或
source/pin drift；它不会因为缺 lock/cache 而联网。默认输出是
人类可读的本地验证日志；`--json` 输出 `nox.project-check.v1`，包含 manifest root、
package name/version、manifest schema 校验摘要、入口点、manifest 声明的 runtime capability、
GitHub/git dependency 声明、lockfile 状态、模块图、三个子步骤的退出码以及被捕获的
stdout/stderr，便于 CI 判断项目发现边界、能力边界和失败步骤。任一子步骤或 lockfile 校验失败时，
命令返回非零；用法错误或缺少 manifest 返回 `2`。

## `fetch`

```sh
cargo run -p nox -- fetch
cargo run -p nox -- fetch --offline
cargo run -p nox -- fetch --cache-dir /tmp/nox-modules
```

`fetch` 是 GitHub/git dependency 的显式下载步骤。它从当前目录向上发现 `nox.toml`，
读取 `[dependencies]` 中声明的 pinned dependency，把每个 dependency clone 或 fetch 到
module cache，解析 tag 或 commit pin 得到完整 commit hash，基于对应 git archive 计算
`sha256:<hex>` content hash，然后在 manifest root 写入 `nox.lock`。

默认 cache 目录优先使用 `NOX_MODULE_CACHE`，否则使用 `$HOME/.cache/nox/modules`；
`--cache-dir <dir>` 可在 CI 或测试中固定 cache 位置。`--offline` 不执行 `git fetch`，
只使用已有 cache；cache 缺失或损坏时返回非零并报告 cache miss / corrupt cache。
module cache 是可丢弃的本地 git 对象缓存，删除 cache 目录不会改变项目源码或 `nox.lock`。
恢复方式是重新运行 `nox fetch`；在锁网 CI 中，可以预先填充同一路径后运行
`nox fetch --offline --cache-dir <dir>` 验证 cache 可用。若使用非默认 cache，后续 `run`、
`check`、`test` 和 `nox lsp` 需要设置 `NOX_MODULE_CACHE=<dir>`。

`fetch` 只是工具链下载动作，不会给脚本运行阶段授予 filesystem、network、environment、
timer、process 或 async runtime permissions。生成 `nox.lock` 后，可以用
`import "<dependency>/<path>.nox"` 引用 external dependency 中的源码。`run`、`check`、
`test` 和 LSP diagnostics 会从 module cache 读取 lockfile 固定的 commit，并校验 cached
archive hash。LSP go-to-definition 和 workspace symbol 对 external dependency 保持保守，
因为源码来自 pinned git object，而不是可编辑的 workspace 文件；它们不会伪造跳转位置或把外部
依赖混入项目符号。普通命令不会静默联网。如果 `fetch` 使用了非默认 `--cache-dir`，后续命令
需要通过 `NOX_MODULE_CACHE` 指向同一个 cache。

`nox.project-check.v1` 的顶层结构如下：

```json
{
  "schema": "nox.project-check.v1",
  "ok": true,
  "manifest": {
    "root": "/path/to/project",
    "package": { "name": "scoreboard", "version": "0.0.3" }
  },
  "schema_validation": {
    "ok": true,
    "manifest_sections": ["package", "entrypoints", "modules", "dependencies", "runtime"],
    "unknown_sections": "rejected",
    "unknown_keys": "rejected"
  },
  "entrypoints": { "main": "/path/to/project/src/main.nox", "named": [] },
  "capabilities": { "declared": ["filesystem.read"] },
  "dependencies": {
    "declared": [
      {
        "name": "mathx",
        "source": { "kind": "github", "value": "owner/mathx" },
        "pin": {
          "kind": "rev",
          "value": "0123456789abcdef0123456789abcdef01234567"
        }
      }
    ],
    "lockfile": {
      "path": "/path/to/project/nox.lock",
      "ok": true,
      "status": "ok",
      "diagnostics": []
    }
  },
  "module_graph": {
    "roots": ["/path/to/project/src"],
    "files": ["/path/to/project/src/main.nox"]
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
- `textDocument/documentSymbol`
- `workspace/symbol`
- `textDocument/definition`
- `nox/testDiscovery`
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
`fn(int) -> int`。对 `async fn` 调用点，hover 会显示调用侧 `task[T]` 结果，并附带
源码签名；signature help 保留源码返回类型，同时标注调用侧返回 `task[T]`。
在 namespace import alias 上 hover 时，例如 `import "std/fs.nox" as fs;` 中的 `fs`，
会返回 module specifier 和 exported surface；项目模块同样只展示该模块的导出表面。
LSP 同时读取本地进程内注册的 host function metadata，用于 completion detail、hover
和 signature help；这些 metadata 包含签名、docstring 与声明的 capability 名称。

`textDocument/formatting` 复用 `fmt` 的格式化逻辑。如果文档已经格式化，返回
空 edit 数组；否则返回一个覆盖整个文档的 `TextEdit`。源码解析失败时返回 `null`，
保持人类可见诊断不变。

`textDocument/completion` 默认返回保留关键字、语言/runtime 内建函数和当前文档中光标
位置之前的标识符。内建函数包含 `len`、`contains`、`map_get`、`args`、`sqrt`
等常用全局表面。
普通 completion 还会提示 manifest `modules.source_dirs` 下项目顶层 `fn`、`record`、`enum`、
`trait` 和 `type` 声明；项目级 `let` / `const` 不进入该列表，以免把运行时状态混入符号建议。
在 `import "..."` 字符串内触发时，completion 会提示 `std/*` 虚拟模块和 manifest
`modules.source_dirs` 下的项目模块路径。
在 `alias.` 之后触发时，如果当前文档存在 `import "path" as alias;`，completion 会按
该模块的导出表面返回成员，并复用 open-document overlay。`std/fs.nox`、`std/env.nox`
和 `std/time.nox` 是 runtime 虚拟模块，不要求磁盘上存在对应文件。
在 `value.` 之后触发时，如果当前文档能通过 `let value: Type` 明确 receiver 类型，
completion 会保守提示同文档中第一个参数为该类型的函数，以及 `impl Trait for Type`
里的方法；无法明确类型时保持空结果，不做跨文件或隐式推断。

`textDocument/documentSymbol` 返回当前文档顶层 `fn`、`async fn`、`record`、`enum`、
`trait`、`type`、`let` 和 `const` 声明；`export` 前缀会被跳过。
`textDocument/definition` 支持当前文档顶层声明，
也支持通过 `import "path" as alias; alias.member` 和直接 `import "path"; Symbol`
引用跳转到被导入模块的 exported 顶层声明。跨文件 definition lookup 会遵循 manifest
`modules.source_dirs` 并复用 open-document overlay；虚拟 stdlib module 会保守返回
`null`，external dependency module 也会保守返回 `null`。当前不解析局部变量作用域。
`textDocument/prepareRename` 和 `textDocument/rename` 只支持当前文件顶层 symbol；如果同名
局部声明或参数会让编辑不安全，prepare/rename 会返回 `null`。当前不做跨文件 rename。
`workspace/symbol` 返回项目内顶层 `fn`、`record`、`enum`、`trait` 和 `type` 声明；发现
manifest 时按 `modules.source_dirs` 扫描 `.nox` 文件，并合并已打开文档 overlay。
workspace symbol 和项目顶层 completion 复用 LSP 进程内 symbol graph cache；`didOpen` 与
`didChange` 会在发布 diagnostics 或回答 symbol 请求前让缓存失效并重建，避免编辑器看到旧顶层声明。

`nox/testDiscovery` 是编辑器集成用的 Nox 扩展请求，参数使用
`params.textDocument.uri`。响应为当前文档内所有顶层 `test_*` 函数的数组，每项包含
`uri`、`name` 和 `range`。`textDocument/codeLens` 使用同一发现规则生成
`nox.runTest` 命令入口。

当前 LSP 不支持跨文件 rename、后台 watch mode 或 daemon 模式；这些 request 不在
`initialize` capability 中声明。

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
