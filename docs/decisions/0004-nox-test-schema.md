# 0004 - nox test 与 JSON schema

- 状态：已采纳
- 日期：2026-05-20
- 涉及：工具链 / CLI JSON

## 背景

Nox 已有 `run`、`check`、`fmt`、`lsp` 和 `inspect-bytecode`，但缺少项目内测试命令。
PLAN.md 要求 `nox test` 有明确测试语义、退出码和输出格式。因为 `check --json` 已经有
schema 版本，测试输出也需要同样的版本边界，避免工具消费者依赖临时文本。

## 决策

引入 `nox test [--json] [path...]`。测试文件命名为 `*_test.nox`；未给路径时从当前
目录发现测试文件，若发现 `nox.toml`，则从 `modules.source_dirs` 递归发现。显式路径
可以是目录或符合命名约定的测试文件。

测试函数是顶层 `fn test_*() -> bool`。返回 `true` 通过，返回 `false` 失败；运行时诊断
也记为失败。签名错误、语法错误、类型错误和 import 错误属于模块级失败。

JSON 输出固定使用 `"schema":"nox.test.v1"`，包含：

- `ok`：所有测试通过时为 `true`。
- `tests[]`：每条记录包含 `file`、`name`、`ok`、`diagnostic`。
- `summary`：`tests`、`passed`、`failed`。

退出码保持三类：全部通过为 `0`，测试或源码失败为 `1`，CLI 用法错误为 `2`。

## 后果

`nox test` 能在不引入宏、assert 库和并发执行模型的前提下覆盖项目级测试需求。JSON
schema 给后续编辑器、CI 或外部工具提供稳定入口。当前模型要求测试函数显式返回 bool，
因此失败信息主要来自运行时诊断；更丰富的断言库可以在后续标准库阶段补充。

## 备选方案

- 引入 `assert` 语法或测试宏：暂不选择，因为会扩大语言表面并耦合 parser/type checker。
- 只输出人类可读文本：被拒绝，工具消费者无法稳定解析。
- 自动运行所有顶层表达式并把最终值当测试结果：被拒绝，测试粒度不清，也不利于报告单个失败。
