# 诊断 code

Nox 诊断包含人类可读 `message`、byte span、可选 source location、可选 runtime
`stack_frames`，以及机器可读 `code`。CLI JSON 和 LSP diagnostics 都透出同一套 `code`。
LSP diagnostics 还会在 `data.trace_id` 中携带确定性的单条诊断标识，方便编辑器或外部工具把
诊断与 trace/log 记录关联起来。

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
| `lex.invalid-integer` | 稳定 | 整数字面量的进制前缀、数字或 `_` 分隔符不合法。 | `compiler_tests::lexer_integer_literal_rejects_malformed_inputs` |
| `type-alias.cyclic` | 稳定 | `type` 别名直接或间接引用自身，无法透明展开。 | `language_tests::type_alias_rejects_cycles`、`cli::check_json_reports_type_alias_cyclic_code` |
| `enum.variant-not-found` | 稳定 | 用户 enum 构造或 match pattern 引用了未定义的 variant。 | `language_tests::user_enum_rejects_missing_variants`、`cli::check_json_reports_enum_codes` |
| `generic.infer-failed` | 稳定 | 泛型函数调用的函数级类型参数无法从实参或 expected return type 一致推导。 | `language_tests::generic_function_reports_conflicting_inference`、`cli::check_json_reports_generic_infer_failed_code` |
| `type.bitwise-non-int` | 稳定 | 位运算符 `&`、`|`、`^`、`<<`、`>>` 或 `~` 收到非 `int` 操作数。 | `language_tests::bitwise_ops_reject_non_int_operands`、`cli::check_json_reports_bitwise_non_int_code` |
| `control-flow.let-else-fallthrough` | 稳定 | `let ... else` 的 `else` 分支没有提前 `return`，成功 pattern 绑定可能在未初始化时被使用。 | `language_tests::control_flow_let_patterns_require_let_else_to_exit`、`cli::check_json_reports_let_else_fallthrough_code` |
| `type.spread-mismatch` | 稳定 | array 或 map spread 的源表达式不是同类容器，或无法与 literal 的元素/value 类型统一。 | `language_tests::spread_operator_rejects_non_container_sources`、`cli::check_json_reports_spread_mismatch_code` |
| `tuple.arity-mismatch` | 稳定 | tuple literal 或 tuple destructuring 的元素数量不匹配，或 tuple 形状无效。 | `language_tests::rejects_tuple_literal_arity_mismatch`、`language_tests::rejects_tuple_destructuring_arity_mismatch` |
| `tuple.element-type-mismatch` | 稳定 | tuple literal 的某个元素与 expected tuple element type 不一致。 | `language_tests::rejects_tuple_literal_element_type_mismatch` |
| `bytecode.verifier` | 稳定 | bytecode verifier 拒绝非法跳转、栈深度不一致、scope 深度不一致或内部 malformed bytecode。 | `compiler_tests::bytecode_verifier_rejects_invalid_jump_target`、`compiler_tests::bytecode_verifier_rejects_stack_underflow`、`compiler_tests::bytecode_verifier_rejects_branch_exit_scope_underflow`、`compiler_tests::bytecode_verifier_rejects_malformed_nested_function_body` |
| `test.signature` | 稳定 | `nox test` 发现 `test_*` 函数参数或返回类型不符合约定。 | `api_tests::run_tests_rejects_invalid_test_signature`、`cli::test_json_reports_invalid_signature_code` |
| `runtime.index-out-of-range` | 稳定 | `arr[i] = value` 数组索引赋值时 `i` 越界，或类似越界 mutation。 | `tests::array_index_assignment_reports_out_of_range_at_runtime` |
| `type.assign-target` | 稳定 | 索引赋值或普通赋值的 LHS 不是合法赋值目标（不是变量、数组或 map 索引）。 | `parser::assignment` 路径 |
| `generic.constraint-unsatisfied` | 稳定 | 泛型函数调用时实参类型不满足声明的内建 marker 约束（Equatable / Comparable / Stringify / Hashable）。 | `language_tests::constraint_unsatisfied_with_function_value_reports_stable_code` |
| `generic.constraint-unknown` | 稳定 | 在旧内建 marker 约束路径中引用未知 marker。阶段 62 起，`<T: Name>` 优先按 trait bound 解析，未知名称使用 `trait.not-found`。 | ADR 0020 兼容层 |
| `trait.duplicate` | 实验 | 同一 trait 内重复声明 required method。重复顶层 trait 名称继续按 `module.name-conflict` 处理。 | `language_tests::trait_duplicate_method_reports_stable_code` |
| `trait.not-found` | 实验 | trait bound 或 `impl Trait for Type` 引用了未定义 trait。 | `language_tests::unknown_trait_bound_is_rejected_with_stable_code` |
| `trait.impl-duplicate` | 实验 | 同一 `(Trait, Type)` 出现重复 impl，或同一 impl 中重复 method。 | Phase 62 trait MVP |
| `trait.impl-orphan` | 实验 | `impl Trait for Type` 同时不在 trait 定义模块、也不在 nominal type 定义模块。 | typechecker orphan rule |
| `trait.impl-incomplete` | 实验 | `impl` 缺少 trait required method。 | `language_tests::trait_impl_requires_all_methods` |
| `trait.method-signature-mismatch` | 实验 | impl method 签名与 trait required method 不一致。 | `language_tests::trait_impl_rejects_signature_mismatch` |
| `trait.bound-unsatisfied` | 实验 | 泛型函数调用时实际类型没有实现声明的 trait bound。 | `language_tests::trait_bound_rejects_unimplemented_type` |
| `trait.method-not-found` | 实验 | 泛型 bound/impl 场景下找不到 trait method。 | Phase 62 trait MVP |
| `trait.method-ambiguous` | 实验 | 多个 trait/record/imported method 候选无法保守解析；receiver 类型能唯一确定时，顶层 record-style function 和 trait impl method 可以同名。 | Phase 62 trait MVP |
| `parse.reserved-keyword` | 稳定 | 源码使用 `try` / `catch` / `panic` / `defer` / `finally` 作为标识符。ADR 0028 继续暂缓异常与 `try {}` block，这些词保持保留。 | `language_tests::reserved_keywords_for_future_exceptions_are_rejected` |
| `watch.path-not-found` | 稳定 | `nox watch` 启动时 manifest 声明的 `source_dirs` / `test_dirs` 中某个路径不存在。 | `cli::watch_reports_missing_path_with_stable_code` |
| `test.assertion-failed` | 稳定 | `std/test.nox` 中的 `assert_*` / `fail` helper 在断言失败时产生的诊断。 | `cli::test_assertion_helpers_pass_and_fail_with_stable_code` |
| `runtime.call-stack-overflow` | 稳定 | 脚本调用栈深度超过 `Engine::set_max_call_stack_depth` 配置的上限。未配置时无 native stack 上限兜底，不触发该诊断。 | `language_tests::rejects_deep_recursion_when_max_call_depth_is_set` |
| `runtime.string-length-cap` | 稳定 | 字符串拼接 (`+`) 后长度超过 `Engine::set_max_string_length` 配置的上限。未配置时不触发该诊断。 | `language_tests::max_string_length_rejects_concatenation_beyond_cap` |
| `runtime.array-length-cap` | 稳定 | 数组字面量构造或 `array.append` 增长超过 `Engine::set_max_array_length` 配置的上限。未配置时不触发该诊断。 | `language_tests::max_array_length_rejects_construction_beyond_cap` |
| `runtime.map-size-cap` | 稳定 | map 字面量构造、`map.set` 或 map 索引赋值增长超过 `Engine::set_max_map_entries` 配置的上限；更新既有 key 不增加 entry 数。未配置时不触发该诊断。 | `language_tests::max_map_entries_rejects_construction_beyond_cap` |
| `runtime.heap-object-cap` | 稳定 | VM 分配或 host callback 返回值让 engine heap object 总数超过 `Engine::set_max_heap_objects` 配置的上限。未配置时不触发该诊断。 | `language_tests::max_heap_objects_rejects_vm_allocations_beyond_cap` |
| `runtime.task-pending-cap` | 稳定 | `task_sleep_ms` 或可 await 的 `task_sleep` / `task.sleep` 将让当前 `Runtime` 的 pending sleep task 数超过 `RuntimePermissions::async_task_max_pending` 配置上限。默认上限为 1024。 | `tests::async_task_sleep_respects_pending_task_cap` |
| `async.await-outside-async` | 实验 | `await` 出现在非 `async fn` 上下文。 | `language_tests::await_outside_async_is_rejected_with_stable_code` |
| `async.await-non-task` | 实验 | `await` 的表达式不是 `task[T]`。 | `language_tests::await_non_task_is_rejected_with_stable_code` |
| `async.top-level-task` | 实验 | 脚本最终值是未消费的 `task[T]`；当前不支持 top-level await。 | `language_tests::top_level_async_task_is_rejected_with_stable_code` |
| `lint.unused-variable` | 稳定 | `nox lint` 发现顶层 `let` 声明在模块内未被引用；下划线前缀变量豁免。 | `cli::lint_reports_unused_top_level_variables` |
| `lint.unused-function` | 稳定 | `nox lint` 发现非 `export` 的顶层 `fn` 声明未被引用；`main` 入口豁免。 | `cli::lint_reports_unused_top_level_variables` |
| `lint.unused-import` | 稳定 | `nox lint` 发现 `import ... as alias` 引入的 alias 在模块内未被引用。 | `cli::lint_reports_unused_top_level_variables` |
| `lint.unreachable-code` | 稳定 | `nox lint` 发现函数体、if 分支、while/for body 或 lambda body 中 `return` / `break` / `continue` 之后的语句永远不可达。 | `language_tests::lint_flags_unreachable_code_after_return`、`cli::lint_reports_unreachable_code_after_return` |
| `lint.shadowed-variable` | 稳定 | `nox lint` 发现内层 `let` 声明遮蔽外层同名 binding（函数参数或外层 `let`）；下划线前缀名豁免。 | `language_tests::lint_flags_shadowing_in_nested_block`、`cli::lint_reports_shadowed_variables_in_nested_blocks` |
| `lint.constant-condition` | 稳定 | `nox lint` 发现 `if (true)` / `if (false)` / `while (false)` 等常量条件；`while (true)` 作为 forever-loop idiom 豁免。 | `language_tests::lint_flags_constant_true_in_if_condition`、`language_tests::lint_flags_while_false_never_executed`、`cli::lint_reports_constant_if_condition` |
| `lint.duplicate-match-arm` | 稳定 | `nox lint` 发现 `match` 语句中两条 arm 的模式完全相同（int / float / str / range / enum-variant / some / none / ok / err 递归对比），后一条永远不可达。 | `language_tests::lint_flags_duplicate_match_arm_with_equal_pattern`、`cli::lint_reports_duplicate_match_arm` |

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
  byte span 映射到 0-based LSP range，并在 `data.trace_id` 中提供稳定关联 id。
  运行时错误如果经过脚本函数调用，会附带 `stack_frames`；frame 顺序为最近的调用帧在前，
  包含函数名、call-site span、可选 source location 和 `kind` 字段。`kind` 取值为
  `"script"`（用户脚本函数）或 `"host"`（host callback / 注册的 host function）；
  CLI 文本输出以 `[script]` / `[host]` 标签呈现。LSP 会把 `trace_id` 和可用的
  `stack_frames` 放在同一个 `data` 对象中。
- 人类可读 message 在 `undefined variable`、`record '...' has no field '...'`、
  `enum '...' has no variant '...'` 和 `unknown type '...'` 错误中，可能在末尾
  追加 `, did you mean 'X'?` 建议。该后缀仅供阅读，不进入 `code` 或机器语义；
  工具不应据此分支判断。
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
