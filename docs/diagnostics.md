# 诊断 code

Nox 诊断包含人类可读 `message`、byte span、可选 source location，以及机器可读
`code`。CLI JSON 和 LSP diagnostics 都透出同一套 `code`。

工具应优先根据 `schema`、`code`、`span` 和 `source` 做判断，不应依赖英文
`message` 的完整文本。人类可读 message 可以在本地开发版本中优化措辞；稳定 code 的
含义和触发场景保持向后兼容。

## 稳定 code

| Code | 状态 | 场景 | 覆盖索引 |
| --- | --- | --- | --- |
| `parse.expected-token` | 稳定 | parser 期望某个固定 token，但源码中缺失或出现了其他 token。 | `compiler_tests::rejects_untyped_variable_declaration`、`cli::check_json_reports_parser_code`、`cli::check_json_and_lsp_report_parser_code` |
| `type.mismatch` | 稳定 | 静态类型检查发现 expected type 和 actual type 不一致。 | `language_tests::check_diagnostics_collects_independent_top_level_type_errors`、`cli::check_json_and_lsp_report_matching_precise_ranges`、`cli::check_json_and_lsp_report_multiple_type_errors_in_one_file` |
| `runtime.division-by-zero` | 稳定 | VM 执行 `int` 或 `float` 除法时除数为零。 | `language_tests::rejects_numeric_runtime_errors`、`cli::test_json_reports_runtime_diagnostic_code` |
| `module.name-conflict` | 稳定 | 顶层声明、导入表面或命名空间 alias 产生名称冲突。 | `api_tests::rejects_top_level_redeclarations`、`api_tests::rejects_import_conflict_with_entry_declaration`、`cli::check_json_reports_module_name_conflicts` |
| `module.not-found` | 稳定 | import 指向的 `std/*` specifier 未命中运行时内置标准库模块，或普通文件 module loader 无法读取目标模块。 | `cli::check_reports_unknown_std_module_without_filesystem_fallback`、`cli::check_json_and_lsp_report_relative_module_not_found_code`、`cli::lsp_reports_module_not_found_code` |
| `module.member-not-found` | 稳定 | 命名空间 import 的 `alias.member` 不存在或不可见。 | `api_tests::namespace_import_rejects_missing_members_and_alias_conflicts`、`cli::check_json_and_lsp_report_module_member_code` |
| `manifest.invalid` | 稳定 | 已发现的 `nox.toml` 无法按 Nox manifest 子集解析，或缺少必需 section/key，或字段类型/权限声明无效。 | `manifest::tests::rejects_missing_required_keys`、`manifest::tests::rejects_unknown_runtime_permission`、`cli::check_json_reports_invalid_manifest_code` |
| `project.discovery` | 稳定 | 项目模式下无法发现 `nox.toml`，或 manifest 展开的入口、source dir、test dir 不存在或不是预期文件类型。 | `cli::check_json_reports_missing_project_discovery_code`、`cli::check_json_reports_missing_manifest_project_dir` |
| `permission.denied` | 稳定 | runtime 权限未授予，或文件系统 read/write allowlist 拒绝目标路径。 | `tests::file_evaluation_requires_filesystem_capability`、`tests::filesystem_read_allowlist_allows_inside_and_denies_escape`、`cli::test_json_reports_permission_denied_code` |
| `host.callback` | 稳定 | 宿主函数返回未细分的 callback diagnostic；如果宿主 diagnostic 已带更具体 code，则保留原 code。 | `api_tests::host_callback_error_includes_function_name`、`api_tests::host_callback_error_preserves_diagnostic_code` |
| `bytecode.verifier` | 稳定 | bytecode verifier 拒绝非法跳转、栈深度不一致、scope 深度不一致或内部 malformed bytecode。 | `compiler_tests::bytecode_verifier_rejects_invalid_jump_target`、`compiler_tests::bytecode_verifier_rejects_stack_underflow`、`compiler_tests::bytecode_verifier_rejects_branch_exit_scope_underflow`、`compiler_tests::bytecode_verifier_rejects_malformed_nested_function_body` |
| `test.signature` | 稳定 | `nox test` 发现 `test_*` 函数参数或返回类型不符合约定。 | `api_tests::run_tests_rejects_invalid_test_signature`、`cli::test_json_reports_invalid_signature_code` |

## 通用 code

| Code | 状态 | 场景 |
| --- | --- | --- |
| `error` | 通用兜底 | 还没有细分 code 的 parser、type checker、runtime、loader 或 host callback 错误。 |

`error` 只表示“这是一条诊断”，不承诺更细的机器语义。工具可以把它作为 fallback 展示给
用户，但不要据此分支实现特定行为。

## 实验和内部 code

当前没有对外登记的实验 code 或内部专用 code。未登记的内部错误继续使用 `error`，不作为
CLI、LSP 或宿主集成的稳定分支条件。

## 兼容规则

- 稳定 code 不在 v0.0.x 本地开发阶段改变含义。
- 新增更细 code 时，优先只替换原本的 `error`；如果要拆分稳定 code，需要先更新
  `nox.check.v1` / `nox.test.v1` 兼容说明或升级 schema。
- CLI JSON 使用 byte span 和 1-based source line/column；LSP diagnostics 使用同一
  byte span 映射到 0-based LSP range。
- 测试应优先断言 `code`、span/range、file/source 和 schema；message substring 只用于
  人类输出或补充可读性检查。

## 新增稳定 code 流程

1. 先确认该错误已经是对外契约的一部分，而不是内部实现细节或临时 message。
2. 在产生诊断的位置添加 `with_code("<domain>.<name>")`，并保持旧的 `message` 可读。
3. 至少补一个 core 单元测试断言 `Diagnostic.code`。
4. 如果该错误能通过 CLI `check --json` 或 `test --json` 到达宿主，补结构化 JSON 断言；
   如果能通过 LSP 到达编辑器，补 LSP `publishDiagnostics` 的 `code` / range 断言。
5. 更新上面的稳定 code 表、覆盖索引，以及相关 schema/兼容说明。

普通文件系统 I/O 和网络连接失败当前多数仍使用 `error`。只有当工具链需要稳定分支语义时，
才把其中一类提升为稳定 code。
