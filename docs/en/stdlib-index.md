# Standard Library Index

This document groups Nox's current standard library modules by topic and tags
the stability of each entry. Full signatures live in `docs/en/runtime.md`; the
runtime implementation is in `crates/nox/src/lib.rs` under
`install_std_module_aliases`.

Stability tags:

- **stable**: signature, return type, and error model are kept backward
  compatible through the v0.0.x cycle. CLI / LSP keep their stable diagnostic
  codes too.
- **stable, permissioned**: stable but requires an explicit runtime capability
  (`network`, `filesystem`, `filesystem_write`, `environment`, `timers`,
  `async task`, `process_run`).
- **experimental**: still iterating; the signature may change in a future
  release. Callers should scope usage tightly.

| Topic | Module | Stability | Notes |
| --- | --- | --- | --- |
| Text & data | `std/string.nox` | stable | split / substring / trim / replace / contains / pad / parse_int / parse_float / join / lines |
| Text & data | `std/json.nox` | stable | parse / stringify / kind / array_len / array_get / object_has / object_get / require_field / validate_schema / validate_object / apply_defaults / apply_defaults_deep / to_json / from_json / variant_name / variant_payload / decode_record3 / decode_adjacent_enum3 / as_int / as_float / as_str / as_bool / as_array / as_object |
| Text & data | `std/csv.nox` / `std/tsv.nox` | stable | single-row parse / format |
| Collections | `std/array.nox` | stable | len / is_empty / push_copy / concat / slice_copy / reverse_copy / sort_copy_int / sort_copy_str / set / append / pop / map_fn / filter_fn / reduce / for_each / dedupe / contains_value |
| Collections | `std/map.nox` | stable | keys / values / entries / merge / remove_copy / get_or / set / delete |
| Option/Result | `std/option.nox` / `std/result.nox` | stable | is_some / is_none / is_ok / is_err / unwrap_or / map_err_to_str |
| Encoding | `std/encoding.nox` | stable | base64 / hex encode/decode |
| Configuration | `std/dotenv.nox` | stable | parse (KEY=value with comments and quoting) |
| Configuration | `std/ini.nox` | stable | parse simple INI sections and key/value pairs |
| Configuration | `std/toml.nox` | stable | parse minimum TOML subset to json |
| URL | `std/url.nox` | stable | parse / build / query_encode / query_decode |
| HTTP | `std/http.nox` | stable, permissioned (`network`) | get / post / get_binary / post_binary over plain HTTP/1.1; 1 MiB response cap; 30s default timeout |
| Time | `std/time.nox` | stable | sleep_ms (permissioned) / now_unix / now_unix_ms / duration_ms / format_unix / parse_unix / from_* / to_* / iso8601_format / iso8601_parse / deadline_ms / is_past_deadline_ms |
| Async tasks | `std/task.nox` | stable, permissioned (`async task`) | sleep_ms / is_ready / cancel / wait / wait_or_timeout / pending_count |
| Filesystem | `std/fs.nox` | stable, permissioned (`filesystem` / `filesystem_write`) | exists / read_text / try_read_text / write_text / read_binary / write_binary / canonicalize / is_file / is_dir / list_dir |
| Path | `std/path.nox` | stable | join / basename / dirname / extension / normalize (lexical only) |
| Environment | `std/env.nox` | stable, permissioned (`environment`) | get / try_get / list |
| Process | `std/process.nox` | stable / stable, permissioned (`process_run`) | argv / read_stdin / print_err / exit / run / run_with (run/run_with require `process_run`) |
| Terminal | `std/term.nox` | stable | is_tty_stdout / is_tty_stderr / color_enabled / style_color / style_bold / pad_column / prompt / confirm / select / progress / prompt_password |
| Testing | `std/test.nox` | stable | assert_eq / assert_ne / assert_true / assert_false / assert_contains / fail / assert_snapshot / assert_table_row / gen_int / gen_bool / gen_string / gen_int_array / gen_int_map / gen_record3 / gen_enum3 / assert_property_int / assert_property_int_array / assert_property_int_map / assert_property_record3 / assert_property_enum3 |
| Randomness | `std/random.nox` | stable | next_int / next_bool / next_float_unit (seeded xorshift64, pure) |
| Bytes | `std/bytes.nox` | stable | encode_utf8 / decode_utf8 / len / get / slice_copy / equal / base64_encode / base64_decode / hex_encode / hex_decode |

All "stable, permissioned" modules default-deny when their capability is not
declared in the manifest. The exact behaviour is documented per row in
`docs/en/runtime.md`.

## Maintenance rules

- Adding a stdlib helper must extend `tests/fixtures/stdlib-surface.nox` so the
  surface guardrail covers its type signature.
- Adding a capability must update `RuntimePermissionDecl`,
  `manifest_permission_name`, `RuntimePermissions`, and the CLI permission docs.
- An entry tagged experimental must explain its boundary at the top of its own
  module documentation; remove the tag when moving to stable.
- Doc vs. registry consistency is currently human-maintained; a future ADR will
  decide whether to add a semi-automatic check (compare entries registered in
  `install_std_module_aliases` against this index).
