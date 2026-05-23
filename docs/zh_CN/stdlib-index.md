# 标准库索引

本文档按主题归类 Nox 当前的标准库模块，并标注每个模块的稳定性。完整签名见
`docs/zh_CN/runtime.md` 的 stdlib 表，运行时实现见 `crates/nox/src/lib.rs` 中
`install_std_module_aliases`。

稳定性标签：

- **stable**：函数签名、返回类型、错误模型在 v0.0.x 阶段保持向后兼容；CLI / LSP
  也会保持稳定 diagnostic code。
- **stable，permissioned**：稳定，但调用需要显式 capability（`network`、
  `filesystem`、`filesystem_write`、`environment`、`timers`、`async task`、
  `process_run`）。
- **experimental**：仍在迭代，未来可能调整签名；调用方应限制使用范围。

| 主题 | 模块 | 稳定性 | 说明 |
| --- | --- | --- | --- |
| 文本和数据处理 | `std/string.nox` | stable | split / substring / trim / replace / contains / pad / parse_int / parse_float / join / lines |
| 文本和数据处理 | `std/json.nox` | stable | parse / stringify / kind / array_len / array_get / object_has / object_get / require_field / validate_schema / validate_object / apply_defaults / apply_defaults_deep / to_json / from_json / variant_name / variant_payload / decode_record3 / decode_adjacent_enum3 / as_int / as_float / as_str / as_bool / as_array / as_object |
| 文本和数据处理 | `std/csv.nox` / `std/tsv.nox` | stable | 单行 parse / format helper |
| 集合 | `std/array.nox` | stable | len / is_empty / push_copy / concat / slice_copy / reverse_copy / sort_copy_int / sort_copy_str / set / append / pop / map_fn / filter_fn / reduce / for_each / dedupe / contains_value |
| 集合 | `std/map.nox` | stable | keys / values / entries / merge / remove_copy / get_or / set / delete |
| 选项与结果 | `std/option.nox` / `std/result.nox` | stable | is_some / is_none / is_ok / is_err / unwrap_or / map_err_to_str |
| 编码 | `std/encoding.nox` | stable | base64_encode / base64_decode / hex_encode / hex_decode |
| 配置 | `std/dotenv.nox` | stable | parse (KEY=value，支持注释和引号) |
| 配置 | `std/ini.nox` | stable | parse 简单 section 和 key/value |
| 配置 | `std/toml.nox` | stable | parse 最小 TOML 子集到 json |
| URL | `std/url.nox` | stable | parse / build / query_encode / query_decode |
| HTTP | `std/http.nox` | stable, permissioned (`network`) | get / post / get_binary / post_binary over HTTP/1.1（仅明文）；1 MiB 响应上限；30s 默认超时 |
| 时间 | `std/time.nox` | stable | sleep_ms (permissioned) / now_unix / now_unix_ms / duration_ms / format_unix / parse_unix / from_seconds / from_minutes / from_hours / to_seconds / to_minutes / to_hours / iso8601_format / iso8601_parse / deadline_ms / is_past_deadline_ms / add_days / add_months / year_of / month_of / day_of / weekday_of |
| 异步任务 | `std/task.nox` | stable, permissioned (`async task`) | sleep_ms / is_ready / cancel / wait / wait_or_timeout / pending_count |
| 文件系统 | `std/fs.nox` | stable, permissioned (`filesystem` / `filesystem_write`) | exists / read_text / try_read_text / write_text / read_binary / write_binary / canonicalize / is_file / is_dir / list_dir |
| 路径 | `std/path.nox` | stable | join / basename / dirname / extension / normalize（纯计算，不访问文件系统） |
| 环境 | `std/env.nox` | stable, permissioned (`environment`) | get / try_get / list |
| 进程 | `std/process.nox` | stable / stable, permissioned (`process_run`) | argv / read_stdin / print_err / exit / run（run 需要 `process_run`） |
| 终端 | `std/term.nox` | stable | is_tty_stdout / is_tty_stderr / color_enabled / style_color / style_bold / pad_column / prompt / confirm / select / progress / prompt_password |
| 测试 | `std/test.nox` | stable | assert_eq / assert_ne / assert_true / assert_false / assert_contains / fail / assert_snapshot / assert_table_row / gen_int / gen_bool / gen_string / gen_int_array / gen_int_map / gen_record3 / gen_enum3 / assert_property_int / assert_property_int_array / assert_property_int_map / assert_property_record3 / assert_property_enum3 |
| 随机数 | `std/random.nox` | stable | next_int / next_bool / next_float_unit（seeded xorshift64 PRNG，纯计算） |
| 字节 | `std/bytes.nox` | stable | encode_utf8 / decode_utf8 / len / get / slice_copy / equal / base64_encode / base64_decode / hex_encode / hex_decode |

所有"stable, permissioned"模块都按 capability 默认拒绝：未显式声明 capability 时
调用返回稳定 diagnostic（capability is required 错误）。具体行为见
`docs/zh_CN/runtime.md` 的 capability 列。

## 维护规则

- 新增 stdlib helper 必须更新 `tests/fixtures/stdlib-surface.nox` 让 surface
  guardrail 覆盖类型签名。
- 新增 capability 必须同步 `RuntimePermissionDecl`、`manifest_permission_name`、
  `RuntimePermissions` 字段、CLI `--permission` 文档。
- 标注 experimental 的模块必须在自身文档头部说明边界条件；进入 stable 后再移除
  标签。
- 文档与 runtime registry 一致性：本索引由人维护；后续 ADR 决定是否引入
  半自动校验脚本（如对比 `install_std_module_aliases` 注册项 vs 索引行）。
