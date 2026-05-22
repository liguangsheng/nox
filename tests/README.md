# Nox 测试输入

这里存放给自动化验证使用的 `.nox` 输入，不作为面向用户的学习示例入口。
可运行的正向示例在 `examples/`。

## 目录

- `fixtures/`：CLI、parser、type checker、runtime、formatter 和 `nox test` 使用的固定输入。
- `malformed/`：panic-free robustness smoke corpus，覆盖 lexer、parser、formatter、type checker、module resolver、manifest 和 LSP 的坏输入边界。
- `benchmarks/`：benchmark smoke 输入，覆盖递归、循环、容器、模块和 test runner。

## 常用命令

```sh
cargo run -p nox -- check tests/fixtures/type-error.nox
cargo run -p nox -- check --json tests/fixtures/type-error.nox
cargo run -p nox -- test tests/fixtures/example_test.nox
scripts/robustness-smoke.sh
scripts/bench-smoke.sh
```

新增语言或运行时行为时，优先判断输入属于用户示例还是测试证据。用户会学习的正向脚本放 `examples/`；只服务断言、负向诊断、鲁棒性或性能 smoke 的输入放这里。
