# 性能基线

Nox 还没有微基准、回归门禁或自动化性能曲线。这一页只解决一个问题：在动手做
重构或运行时优化前，能否拿到当前的耗时数字，作为"感觉更快"之外的判断依据。

## 跑法

可复制的 smoke 命令：

```sh
scripts/bench-smoke.sh
```

脚本默认先执行 `cargo build --release -p nox`，然后使用 `target/release/nox` 输出
tab-separated 结果。需要测试其他二进制时，可以显式传 `NOX_BIN=/path/to/nox`，此时脚本
不会重建 release：

```text
case    mode    command status  real_seconds    output
loop    e2e     ...     ok      0.040000        loop-ok
```

`status` 必须全是 `ok`。脚本会断言每个 case 的预期输出片段，例如 `fib-ok`、
`loop-ok` 和 test summary；如果系统有 `timeout` 命令，默认还会给每个 case 加 10 秒
smoke 阈值，防止明显卡死。可以用 `NOX_BENCH_SMOKE_MAX_SECONDS=30` 调大阈值，或用
`NOX_BENCH_SMOKE_TIMEOUT=/path/to/timeout` 指定 timeout 程序。`real_seconds` 用于同一台机器上
前后对比，不作为 CI 硬门禁。`output` 保留脚本输出，便于确认 benchmark 仍在跑预期路径。

`mode` 不是 VM 内部 profiler，而是稳定 CLI 入口的阶段代理：

- `check`：运行 `nox check <file>`，覆盖 lex / parse / import resolve / typecheck /
  bytecode compile / verifier，但不执行脚本。
- `compile`：运行 `nox inspect-bytecode --compact <file>`，产出 compact bytecode，便于观察
  编译和模块展开路径是否仍可用。
- `e2e`：运行 `nox run <file>` 或 `nox test <dir>`，包含进程启动、文件 I/O、编译和执行。

这些数字不能相减成"纯 parse"或"纯 eval"耗时；它们只用于趋势观察。如果未来需要真正的
内部阶段计时，应另开 profiling 设计，不把调试计时 flag 混进用户 CLI 默认表面。

阶段 15 还引入了 Criterion 微基准，用于观察 `nox_core` 内部关键路径和 `nox`
runtime capability 路径的统计分布：

```sh
cargo bench -p nox_core --bench core_paths
cargo bench -p nox --bench runtime_capabilities
```

它当前覆盖：

- `core/check-recursion`：递归函数源码的 parse/typecheck/compile/verifier 路径。
- `core/eval-loop`：整数 while loop 执行路径。
- `core/eval-containers`：array / map 构造、索引和循环路径。
- `core/eval-lambda`：函数值、lambda、闭包 capture 和高阶调用。
- `runtime/fs-read-text`：permissioned filesystem host helper。
- `runtime/async-task-ready`：permissioned async task host helper。
- `runtime/http-get-mock`：permissioned HTTP host helper，使用 `MockNetwork`
  避免依赖真实外部网络。

Criterion 输出会包含均值、离群点和置信区间，并在 `target/criterion/` 生成 HTML 报告。需要把
Criterion 放进 smoke 脚本时使用：

```sh
NOX_BENCH_CRITERION=1 scripts/bench-smoke.sh
```

`scripts/bench-smoke.sh` 的原有 tab-separated CLI smoke 仍是快速硬阈值；Criterion 是补充证据，
用于同机跨 commit 比较，不替代当前 per-case budget。

也可以手动构建 release 版本，然后用 `time` 跑示例：

```sh
cargo build --release -p nox
for f in tests/benchmarks/bench-fib.nox tests/benchmarks/bench-loop.nox \
         tests/benchmarks/bench-containers.nox tests/benchmarks/bench-modules.nox \
         tests/benchmarks/bench-lambda.nox tests/benchmarks/bench-host-capabilities.nox; do
    echo "=== $f ==="
    /usr/bin/time -f "real %e seconds (user %U / sys %S)" \
        target/release/nox run "$f"
done
```

每个示例都打印 `*-ok` / `*-bad` 字样，便于断言行为正确。耗时从 shell `time`
读取——不需要在 Rust 端做埋点。

如果只想验证 bench 仍然能跑通（行为正确性），用 debug build 也可以：

```sh
cargo run -p nox -- run tests/benchmarks/bench-loop.nox
```

debug 版的数字不能拿来当基线，只是 smoke。

## 覆盖范围

| 示例 | 验证维度 |
| --- | --- |
| `tests/benchmarks/bench-fib.nox` | 递归函数调用、整数算术、控制流。 |
| `tests/benchmarks/bench-loop.nox` | `while` 循环、整数累加、20 万次迭代。 |
| `tests/benchmarks/bench-containers.nox` | 数组字面量、map 字面量、索引、1 万次迭代。 |
| `tests/benchmarks/bench-modules.nox` | import 解析、跨模块函数调用、循环调用。 |
| `tests/benchmarks/bench-lambda.nox` | 函数值、lambda、闭包 capture 和高阶调用。 |
| `tests/benchmarks/bench-host-capabilities.nox` | permissioned host filesystem helper：`fs.read_text`、`fs.list_dir` 与字符串检查。 |
| `nox test tests/fixtures/example_test.nox` | 测试签名检查、测试函数执行和汇总输出。 |

脚本对六个 `.nox` benchmark 都跑 `check`、`compile` 和 `e2e`；对
`nox test tests/fixtures/example_test.nox` 只跑 `e2e`。这些 case 覆盖递归、循环、
容器构造、模块加载、lambda / closure、permissioned host helper 和 test runner。
`compile` mode 会断言 compact bytecode 输出包含 `0000`，避免 inspect 路径静默输出空内容。

## 当前基线（参考）

下表是 2026 年 5 月在普通 Linux x86_64 开发机上 release 模式的耗时，只用来
作为 sanity check：

| case | mode | real |
| --- | --- | --- |
| `bench-fib.nox` | `e2e` | 0.03 s |
| `bench-loop.nox` | `e2e` | 0.04 s |
| `bench-containers.nox` | `e2e` | 0.01 s |
| `bench-modules.nox` | `e2e` | < 0.01 s |
| `.nox` benchmark files | `check` / `compile` | < 0.01 s |

具体数字会因硬件、文件系统和负载而波动。重构 / 优化前先在本地跑一遍记录，
对比同一台机器上的前后差异。

## 不在范围内

- 没有内存/堆分配统计。
- 没有 parse/typecheck/compile/eval 分阶段 profiler；`nox profile <file.nox>` 已提供 VM
  函数调用级 call count / 累计耗时，以及 host callback、容器 literal、index、match
  pattern、map helper 等 operation counter。benchmark smoke 仍只记录 CLI 入口的阶段代理，
  Criterion 只覆盖少量 `nox_core` 关键路径和端到端 real time。

引入这些之前，先把"是否真的需要"想清楚——动机不足时维护成本会反过来拖慢开发。
