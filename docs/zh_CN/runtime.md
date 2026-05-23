# Nox 运行时

`nox_core` 是可嵌入引擎，不直接读文件、开 socket、sleep、读取环境变量或创建任务。
`nox` crate 在 `nox_core` 之上提供默认运行时，安装小型标准库，并通过
`RuntimePermissions` 控制外部能力。

## 构造运行时

无外部能力运行时：

```rust
let mut runtime = nox::Runtime::new();
```

显式授予能力：

```rust
let mut runtime = nox::Runtime::with_permissions(nox::RuntimePermissions {
    filesystem: true,
    filesystem_write: false,
    filesystem_read_roots: Vec::new(),
    filesystem_write_roots: Vec::new(),
    network: false,
    timers: false,
    environment: false,
    async_tasks: false,
});
```

把脚本内文件读写限制到指定目录：

```rust
let mut runtime = nox::Runtime::with_permissions(
    nox::RuntimePermissions::none()
        .allow_filesystem_read_under("project/data")
        .allow_filesystem_write_under("project/out"),
);
```

`RuntimePermissions::cli()` 授予 `filesystem` 读权限，但不授予 `filesystem_write`。
CLI 用它读取入口文件、文件 import 和 `read_text`/`exists`；写文件需要宿主显式
打开 `filesystem_write`。入口文件和 import 读取权限不等于任意脚本读写权限；宿主可以
用 read/write root 把 `read_text`、`exists` 和 `write_text` 限制到指定目录。
`args()` 不需要额外权限；`env_get()`、`env_list()` 和 `std/env.nox` 的 `try_get`
需要 `environment` 权限。

## 权限开关

`RuntimePermissions` 当前包含：

- `filesystem`：允许 `eval_file`、`check_file`、`inspect_bytecode_file`、文件
  import、`read_text(path: str) -> str` 和 `exists(path: str) -> bool`。
- `filesystem_write`：允许 `write_text(path: str, contents: str) -> null`。
  与 `filesystem` 独立，避免一开始就把读权限默认升级成写。
- `filesystem_read_roots`：可选读 allowlist。为空表示读权限不按路径限制；非空时
  `read_text` 和 `exists` 只能访问规范化后位于这些目录下的路径。
- `filesystem_write_roots`：可选写 allowlist。为空表示写权限不按路径限制；非空时
  `write_text` 只能写入规范化后位于这些目录下的路径。
- `environment`：允许 `env_get(name: str) -> str`、`env_list() -> map[str, str]` 和
  `std/env.nox` 的 `try_get(name: str) -> option[str]`。
- `timers`：允许 `sleep_ms(ms: int) -> null`。
- `network`：允许 `tcp_connect(host: str, port: int) -> bool`。
- `async_tasks`：允许 `task_sleep_ms(ms: int) -> int`、`task_ready(id: int) -> bool`
  和 `task_cancel(id: int) -> null`。
- `process_run`：允许 `std/process.nox` 启动子进程。`process_run_allowlist`
  可限制可执行名；`process_run_max_concurrent` 默认 `Some(8)`，限制单个
  runtime 内同时运行的子进程数量。

权限检查发生在文件加载入口或宿主函数内部。脚本可以通过静态类型检查，但在调用未授权能力时运行失败。

## 标准库

默认运行时安装这些带类型宿主函数。`map_get(map, key) -> option[T]` 是引擎内置的
可恢复 map lookup，不需要运行时权限。文件、环境和时间能力的推荐写法是静态
`std/*` 模块导入：

```nox
import "std/fs.nox" as fs;
import "std/env.nox" as env;
import "std/time.nox" as time;
import "std/string.nox" as string;
import "std/json.nox" as json;

fs.exists("nox.toml");
env.list();
time.sleep_ms(0);
string.trim(" nox ");
json.parse("{\"ok\":true}");
```

当前可用模块表面：

| 模块 | 成员 | 权限 | 权限不足时的诊断 |
| --- | --- | --- | --- |
| `std/fs.nox` | `read_text(path: str) -> str` | `filesystem` | `filesystem capability is required to call read_text` |
| `std/fs.nox` | `try_read_text(path: str) -> result[str, str]` | `filesystem` | `filesystem capability is required to call try_read_text` |
| `std/fs.nox` | `exists(path: str) -> bool` | `filesystem` | `filesystem capability is required to call exists` |
| `std/fs.nox` | `is_file(path: str) -> bool` | `filesystem` | `filesystem capability is required to call is_file` |
| `std/fs.nox` | `is_dir(path: str) -> bool` | `filesystem` | `filesystem capability is required to call is_dir` |
| `std/fs.nox` | `list_dir(path: str) -> result[[str], str]` | `filesystem` | `filesystem capability is required to call list_dir` |
| `std/fs.nox` | `write_text(path: str, contents: str) -> null` | `filesystem_write` | `filesystem write capability is required to call write_text` |
| `std/fs.nox` | `read_binary(path: str) -> result[[int], str]` | `filesystem` | 以 `[int]` 字节数组返回文件内容；权限拒绝沿用 read allowlist；缺失文件返回 `result.err` |
| `std/fs.nox` | `write_binary(path: str, bytes: [int]) -> result[null, str]` | `filesystem_write` | 把 `[int]` 字节数组写入文件；权限沿用 write allowlist；元素超出 `0..=255` 返回 `result.err` |
| `std/fs.nox` | `canonicalize(path: str) -> result[str, str]` | `filesystem` | 解析路径符号链接并返回规范绝对路径；输入路径仍需通过 read allowlist；底层 `fs::canonicalize` 出错返回 `result.err` |
| `std/env.nox` | `get(name: str) -> str` | `environment` | `environment capability is required to call env_get` |
| `std/env.nox` | `try_get(name: str) -> option[str]` | `environment` | `environment capability is required to call env_try_get` |
| `std/env.nox` | `list() -> map[str, str]` | `environment` | `environment capability is required to call env_list` |
| `std/time.nox` | `sleep_ms(ms: int) -> null` | `timers` | `timer capability is required to call sleep_ms` |
| `std/time.nox` | `now_unix() -> int` | 无 | n/a |
| `std/time.nox` | `now_unix_ms() -> int` | 无 | n/a |
| `std/time.nox` | `duration_ms(start: int, end: int) -> int` | 无 | n/a |
| `std/time.nox` | `format_unix(ts: int, fmt: str) -> str` | 无 | n/a |
| `std/time.nox` | `parse_unix(value: str, fmt: str) -> result[int, str]` | 无 | n/a |
| `std/string.nox` | `split(value: str, separator: str) -> [str]` | 无 | n/a |
| `std/string.nox` | `join(values: [str], separator: str) -> str` | 无 | n/a |
| `std/string.nox` | `substring(value: str, start: int, length: int) -> str` | 无 | n/a |
| `std/string.nox` | `trim(value: str) -> str` | 无 | n/a |
| `std/string.nox` | `replace(value: str, from: str, to: str) -> str` | 无 | n/a |
| `std/string.nox` | `starts_with(value: str, prefix: str) -> bool` | 无 | n/a |
| `std/string.nox` | `ends_with(value: str, suffix: str) -> bool` | 无 | n/a |
| `std/string.nox` | `index_of(value: str, needle: str) -> int` | 无 | n/a |
| `std/string.nox` | `contains(value: str, needle: str) -> bool` | 无 | n/a |
| `std/string.nox` | `last_index_of(value: str, needle: str) -> int` | 无 | n/a |
| `std/string.nox` | `repeat(value: str, count: int) -> str` | 无 | n/a |
| `std/string.nox` | `pad_left(value: str, width: int, fill: str) -> str` | 无 | n/a |
| `std/string.nox` | `pad_right(value: str, width: int, fill: str) -> str` | 无 | n/a |
| `std/string.nox` | `parse_int(value: str) -> result[int, str]` | 无 | n/a |
| `std/string.nox` | `parse_float(value: str) -> result[float, str]` | 无 | n/a |
| `std/string.nox` | `lines(value: str) -> [str]` | 无 | n/a |
| `std/string.nox` | `to_upper(value: str) -> str` | 无 | n/a |
| `std/string.nox` | `to_lower(value: str) -> str` | 无 | n/a |
| `std/json.nox` | `parse(value: str) -> result[json, str]` | 无 | n/a |
| `std/json.nox` | `stringify(value: json) -> str` | 无 | n/a |
| `std/json.nox` | `kind(value: json) -> str` | 无 | n/a |
| `std/json.nox` | `array_len(value: json) -> result[int, str]` | 无 | n/a |
| `std/json.nox` | `array_get(value: json, index: int) -> result[json, str]` | 无 | n/a |
| `std/json.nox` | `object_has(value: json, key: str) -> result[bool, str]` | 无 | n/a |
| `std/json.nox` | `object_get(value: json, key: str) -> result[json, str]` | 无 | n/a |
| `std/csv.nox` | `parse_line(value: str) -> result[[str], str]` | 无 | n/a |
| `std/csv.nox` | `format_row(values: [str]) -> str` | 无 | n/a |
| `std/tsv.nox` | `parse_line(value: str) -> result[[str], str]` | 无 | n/a |
| `std/tsv.nox` | `format_row(values: [str]) -> result[str, str]` | 无 | n/a |
| `std/array.nox` | `len<T>(values: [T]) -> int` | 无 | n/a |
| `std/array.nox` | `is_empty<T>(values: [T]) -> bool` | 无 | n/a |
| `std/array.nox` | `push_copy<T>(values: [T], value: T) -> [T]` | 无 | n/a |
| `std/array.nox` | `concat<T>(left: [T], right: [T]) -> [T]` | 无 | n/a |
| `std/array.nox` | `slice_copy<T>(values: [T], start: int, length: int) -> result[[T], str]` | 无 | n/a |
| `std/array.nox` | `reverse_copy<T>(values: [T]) -> [T]` | 无 | n/a |
| `std/array.nox` | `sort_copy_int(values: [int]) -> [int]` | 无 | n/a |
| `std/array.nox` | `sort_copy_str(values: [str]) -> [str]` | 无 | n/a |
| `std/json.nox` | `require_field(value: json, path: str, expected_kind: str) -> result[json, str]` | 无 | 路径形如 `server.port` 或 `tags[1]`；类型不匹配 / 路径不存在返回 err，message 含路径 |
| `std/json.nox` | `validate_schema(value: json, required_fields: [str]) -> result[null, str]` | 无 | 验证 JSON object 包含所有 required 字段（不递归）；缺失字段拼接到 err message |
| `std/json.nox` | `validate_object(value: json, required_fields: [str], allowed_fields: [str]) -> result[null, str]` | 无 | 验证 JSON object 的必填字段与允许字段；缺失字段和未知字段都会进入 err message |
| `std/json.nox` | `apply_defaults(value: json, defaults: json) -> result[json, str]` | 无 | 对两个 JSON object 做顶层默认值注入：只把 `defaults` 中缺失的 key 复制到 `value`，已有 key 不覆盖；非 object 返回 err |
| `std/json.nox` | `apply_defaults_deep(value: json, defaults: json) -> result[json, str]` | 无 | 递归对嵌套 JSON object 做默认值注入；已有字段不覆盖，缺失的嵌套字段从 defaults 复制 |
| `std/json.nox` | `to_json<T>(value: T) -> json` | 无 | 把任意 Nox value 单向序列化为 `json`：record→object（按字段名）、enum 带 payload→`{"_variant", "payload"}` / 无 payload→字符串、tuple→array、map→object、option→payload 或 null、result→`{"_variant": "ok"|"err", "payload"}`；adjacent enum 形状是稳定契约；function 值返回 runtime 诊断 |
| `std/json.nox` | `from_json<T>(value: json) -> result[T, str]` | 无 | 需要调用点有 expected `result[T, str]` 类型；编译器把目标类型写入 bytecode，VM 自动把 JSON 解码为 record / enum / scalar / array / map / option / result；错误返回 path-aware `result.err` |
| `std/json.nox` | `variant_name(value: json) -> result[str, str]` | 无 | 从 adjacent enum JSON 读取 variant 名称；无 payload 字符串和带 payload object 都支持 |
| `std/json.nox` | `variant_payload(value: json) -> result[json, str]` | 无 | 从 adjacent enum object 读取 `payload`；无 payload 字符串返回 err |
| `std/json.nox` | `decode_record3<T>(value: json, path: str, field1: str, kind1: str, field2: str, kind2: str, field3: str, kind3: str, build: fn(json, json, json) -> result[T, str]) -> result[T, str]` | 无 | 验证三个 path-aware 字段后调用显式 builder；适合把 JSON object 映射为 record |
| `std/json.nox` | `decode_adjacent_enum3<T>(value: json, path: str, variant1: str, build1: fn(json) -> result[T, str], variant2: str, build2: fn(json) -> result[T, str], variant3: str, build3: fn(json) -> result[T, str]) -> result[T, str]` | 无 | 按 adjacent enum `_variant` / 字符串表示分派到显式 builder；无 payload 字符串向 builder 传 JSON null |
| `std/json.nox` | `as_int(value: json) -> result[int, str]` | 无 | 解析 JSON number 为 `int`；非数字或带小数 / 非有限值返回 err |
| `std/json.nox` | `as_float(value: json) -> result[float, str]` | 无 | 解析 JSON number 为 `float` |
| `std/json.nox` | `as_str(value: json) -> result[str, str]` | 无 | 解析 JSON string |
| `std/json.nox` | `as_bool(value: json) -> result[bool, str]` | 无 | 解析 JSON bool |
| `std/json.nox` | `as_array(value: json) -> result[[json], str]` | 无 | 解析 JSON array 为 `[json]`（用于继续递归提取）|
| `std/json.nox` | `as_object(value: json) -> result[map[str, json], str]` | 无 | 解析 JSON object 为 `map[str, json]`（用于 record 字段循环提取）|
| `std/random.nox` | `next_int(seed: int, min: int, max: int) -> (int, int)` | 无 | xorshift64 seeded PRNG；返回 `(下一个 seed, 取值)`；`min > max` 报 runtime 诊断 |
| `std/random.nox` | `next_bool(seed: int) -> (int, bool)` | 无 | 返回 `(下一个 seed, true/false)`，纯计算 |
| `std/random.nox` | `next_float_unit(seed: int) -> (int, float)` | 无 | 返回 `(下一个 seed, [0, 1) 浮点数)`，纯计算 |
| `std/time.nox` | `add_days(unix_seconds: int, days: int) -> int` | 无 | 按 86400 秒/天平移，无溢出处理（依赖 i64 范围） |
| `std/time.nox` | `add_months(unix_seconds: int, months: int) -> int` | 无 | 加月数；遇到月末越界时把 day 截到该月最长日 |
| `std/time.nox` | `year_of(unix_seconds: int) -> int` | 无 | UTC 年份 |
| `std/time.nox` | `month_of(unix_seconds: int) -> int` | 无 | UTC 月份（1-12） |
| `std/time.nox` | `day_of(unix_seconds: int) -> int` | 无 | UTC 日（1-31） |
| `std/time.nox` | `weekday_of(unix_seconds: int) -> int` | 无 | ISO weekday：0=Mon..6=Sun |
| `std/term.nox` | `select(message: str, items: [str], default_index: int) -> result[int, str]` | 无 | 简单数字选择菜单：把 `items` 标号到 stderr 渲染，从 stdin 读取整数选项（1 起始）；空输入或 EOF + 合法 default_index 时返回 default；越界或非数字输入返回 err |
| `std/term.nox` | `is_tty_stderr() -> bool` | 无 | unix 上检测 stderr 是否是 TTY；非 unix 平台返回 false |
| `std/term.nox` | `progress(current: int, total: int, width: int) -> str` | 无 | 纯计算 ASCII 进度条：`[####-----] 4/10 (40%)`；current 截到 `[0, total]`，total 为 0 时显示 0%；width 必须 >= 0；非 TTY 安全（只返回字符串，不打印）|
| `std/term.nox` | `prompt_password(message: str) -> result[str, str]` | 无 | Linux 上通过 termios (`tcgetattr` / `tcsetattr`) 直接关闭回显并在读取后恢复；回显控制不可用时返回稳定 code `term.prompt-password.echo-disable-failed` 的 err；EOF 返回 `term.prompt-password.eof`；读取错误返回 `term.prompt-password.read-failed`。message 写入 stderr。 |
| `std/array.nox` | `set<T>(values: [T], index: int, value: T) -> result[null, str]` | 无 | n/a |
| `std/array.nox` | `append<T>(values: [T], value: T) -> null` | 无 | n/a |
| `std/array.nox` | `pop<T>(values: [T]) -> option[T]` | 无 | n/a |
| `std/map.nox` | `keys<T>(values: map[str, T]) -> [str]` | 无 | n/a |
| `std/map.nox` | `values<T>(values: map[str, T]) -> [T]` | 无 | n/a |
| `std/map.nox` | `entries<T>(values: map[str, T]) -> [(str, T)]` | 无 | n/a |
| `std/map.nox` | `merge<T>(left: map[str, T], right: map[str, T]) -> map[str, T]` | 无 | n/a |
| `std/map.nox` | `remove_copy<T>(values: map[str, T], key: str) -> map[str, T]` | 无 | n/a |
| `std/map.nox` | `get_or<T>(values: map[str, T], key: str, fallback: T) -> T` | 无 | n/a |
| `std/map.nox` | `set<T>(values: map[str, T], key: str, value: T) -> null` | 无 | n/a |
| `std/map.nox` | `delete<T>(values: map[str, T], key: str) -> bool` | 无 | n/a |
| `std/url.nox` | `parse(url: str) -> result[(str, str, int, str, str), str]` | 无 | n/a |
| `std/url.nox` | `build(scheme: str, host: str, port: int, path: str, query: str) -> str` | 无 | n/a |
| `std/url.nox` | `query_encode(value: str) -> str` | 无 | n/a |
| `std/url.nox` | `query_decode(value: str) -> result[str, str]` | 无 | n/a |
| `std/http.nox` | `get(url: str, timeout_ms: int) -> result[(int, str), str]` | `network` | 仅支持 `http://`；响应 ≤ 1 MiB |
| `std/http.nox` | `post(url: str, body: str, timeout_ms: int) -> result[(int, str), str]` | `network` | 同上 |
| `std/http.nox` | `get_binary(url: str, timeout_ms: int) -> result[(int, [int]), str]` | `network` | 返回二进制 body 为 `[int]` 字节数组；同样 1 MiB 上限 |
| `std/http.nox` | `post_binary(url: str, body: [int], timeout_ms: int) -> result[(int, [int]), str]` | `network` | body 为 `[int]` 字节数组；同上 |
| `std/task.nox` | `sleep_ms(ms: int) -> int` | `async task` | 返回 task id |
| `std/task.nox` | `is_ready(id: int) -> bool` | `async task` | 非阻塞 |
| `std/task.nox` | `cancel(id: int) -> null` | `async task` | 取消 sleep task |
| `std/task.nox` | `wait(id: int) -> bool` | `async task` | 阻塞直到 task 完成 |
| `std/task.nox` | `wait_or_timeout(id: int, timeout_ms: int) -> bool` | `async task` | 超时返回 false 并自动 cancel |
| `std/task.nox` | `pending_count() -> int` | `async task` | 返回当前 pending sleep task 数 |
| `std/test.nox` | `assert_eq<T: Equatable>(actual: T, expected: T, label: str) -> null` | 无 | 失败返回 `test.assertion-failed` |
| `std/test.nox` | `assert_ne<T: Equatable>(actual: T, unexpected: T, label: str) -> null` | 无 | 同上 |
| `std/test.nox` | `assert_true(condition: bool, label: str) -> null` | 无 | 同上 |
| `std/test.nox` | `assert_false(condition: bool, label: str) -> null` | 无 | 同上 |
| `std/test.nox` | `assert_contains(haystack: str, needle: str, label: str) -> null` | 无 | 子串查找；失败时报 `test.assertion-failed` |
| `std/test.nox` | `fail(label: str, message: str) -> null` | 无 | 强制失败 |
| `std/test.nox` | `assert_snapshot(label: str, actual: str, expected: str) -> null` | 无 | 文本快照对比，失败时附 actual/expected 预览 |
| `std/test.nox` | `assert_table_row<T: Equatable>(label: str, index: int, actual: T, expected: T) -> null` | 无 | table-driven 测试 helper，失败时记录 row index |
| `std/test.nox` | `gen_int(seed: int, min: int, max: int) -> (int, int)` | 无 | deterministic property 生成器，返回 `(下一个 seed, value)` |
| `std/test.nox` | `gen_bool(seed: int) -> (int, bool)` | 无 | deterministic bool 生成器 |
| `std/test.nox` | `gen_string(seed: int, max_len: int) -> (int, str)` | 无 | 生成 `a`/`b`/`c` 字符组成的短字符串 |
| `std/test.nox` | `gen_int_array(seed: int, len: int, min: int, max: int) -> (int, [int])` | 无 | 生成固定长度 int array |
| `std/test.nox` | `gen_int_map(seed: int, len: int, min: int, max: int) -> (int, map[str, int])` | 无 | 生成 `k0`、`k1`... key 的 int map |
| `std/test.nox` | `gen_record3<T>(seed: int, min: int, max: int, build: fn(int, str, bool) -> T) -> (int, T)` | 无 | 通过显式 builder 生成三字段 record-like 值 |
| `std/test.nox` | `gen_enum3<T>(seed: int, min: int, max: int, max_len: int, build_int: fn(int) -> T, build_str: fn(str) -> T, build_bool: fn(bool) -> T) -> (int, T)` | 无 | 通过三个 variant builder 生成 enum-like 值 |
| `std/test.nox` | `assert_property_int(label: str, seed: int, cases: int, min: int, max: int, property: fn(int) -> bool) -> null` | 无 | 运行 deterministic int property；失败时 shrink 并在诊断中写 seed / case / value / minimized / replay metadata |
| `std/test.nox` | `assert_property_int_array(label: str, seed: int, cases: int, len: int, min: int, max: int, property: fn([int]) -> bool) -> null` | 无 | 运行 deterministic int array property；失败时先缩短 failing prefix，再把元素向 0 shrink |
| `std/test.nox` | `assert_property_int_map(label: str, seed: int, cases: int, len: int, min: int, max: int, property: fn(map[str, int]) -> bool) -> null` | 无 | 运行 deterministic int map property；失败时先缩短 `k0..` key prefix，再把 value 向 0 shrink |
| `std/test.nox` | `assert_property_record3<T>(label: str, seed: int, cases: int, min: int, max: int, build: fn(int, str, bool) -> T, property: fn(T) -> bool) -> null` | 无 | 运行显式 builder record property；失败时 shrink int / str / bool 字段并写 replay metadata |
| `std/test.nox` | `assert_property_enum3<T>(label: str, seed: int, cases: int, min: int, max: int, max_len: int, build_int: fn(int) -> T, build_str: fn(str) -> T, build_bool: fn(bool) -> T, property: fn(T) -> bool) -> null` | 无 | 运行显式 builder enum property；失败时 shrink payload 与 variant 选择并写 replay metadata |
| `std/encoding.nox` | `base64_encode(value: str) -> str` | 无 | UTF-8 字节流 → base64 |
| `std/encoding.nox` | `base64_decode(value: str) -> result[str, str]` | 无 | 失败返回 result.err；非 UTF-8 字节返回 err |
| `std/encoding.nox` | `hex_encode(value: str) -> str` | 无 | UTF-8 字节流 → 小写 hex |
| `std/encoding.nox` | `hex_decode(value: str) -> result[str, str]` | 无 | 同上 |
| `std/dotenv.nox` | `parse(source: str) -> result[map[str, str], str]` | 无 | 解析 KEY=value，支持 `#` 注释、双/单引号、`export` 前缀；非法 key 返回 err |
| `std/ini.nox` | `parse(source: str) -> result[map[str, map[str, str]], str]` | 无 | 解析简单 INI：`[section]` 分节，key/value 支持 `=` 或 `:`；顶层 key 放在空字符串 section 下；`#` / `;` 为注释 |
| `std/toml.nox` | `parse(source: str) -> result[json, str]` | 无 | 最小 TOML reader：支持 table、dotted key、字符串、bool、数字和数组，返回 JSON object；datetime / array-of-tables 等完整 TOML 特性返回 err |
| `std/bytes.nox` | `encode_utf8(text: str) -> [int]` / `decode_utf8(values: [int]) -> result[str, str]` | 无 | 用 `[int]` 表示字节数组（0..255）；非 UTF-8 返回 err |
| `std/bytes.nox` | `len(values: [int]) -> int` / `get(values: [int], index: int) -> result[int, str]` | 无 | helper 形式的长度与索引；越界索引返回 err |
| `std/bytes.nox` | `slice_copy(values: [int], start: int, length: int) -> result[[int], str]` / `equal(left: [int], right: [int]) -> bool` | 无 | helper 形式的复制 slice 与比较；显示格式使用 hex/base64 helper |
| `std/bytes.nox` | `base64_encode(values: [int]) -> str` / `base64_decode(value: str) -> result[[int], str]` | 无 | 直接处理字节数组的 base64 编解码 |
| `std/bytes.nox` | `hex_encode(values: [int]) -> str` / `hex_decode(value: str) -> result[[int], str]` | 无 | 字节数组 hex 编解码 |
| `std/term.nox` | `is_tty_stdout() -> bool` / `color_enabled() -> bool` | 无 | Unix 用 `isatty` 判定，Windows 用 `GetConsoleMode`；`NO_COLOR` env 关闭颜色 |
| `std/term.nox` | `style_color(value: str, color: str) -> str` / `style_bold(value: str) -> str` | 无 | 支持 red/green/yellow/blue/magenta/cyan/bold；非 TTY 时透明返回原文 |
| `std/term.nox` | `pad_column(value: str, width: int) -> str` | 无 | 右侧填充空格至指定宽度（字符数） |
| `std/term.nox` | `prompt(message: str) -> result[str, str]` | 无 | 写 message 到 stderr，从 stdin 读取一行；EOF 返回 err |
| `std/term.nox` | `confirm(message: str, default_yes: bool) -> result[bool, str]` | 无 | 显示 `[Y/n]` 或 `[y/N]`；EOF 返回默认值 |
| `std/time.nox` | `from_seconds(s) -> int` / `from_minutes` / `from_hours` | 无 | duration 换算为毫秒 |
| `std/time.nox` | `to_seconds(ms) -> int` / `to_minutes` / `to_hours` | 无 | 毫秒换算为秒/分/小时 |
| `std/time.nox` | `iso8601_format(unix_seconds: int) -> str` | 无 | UTC，格式 `YYYY-MM-DDTHH:MM:SSZ` |
| `std/time.nox` | `iso8601_parse(value: str) -> result[int, str]` | 无 | 仅支持 UTC（`Z` 或 `+00:00`）；非 UTC 返回 err |
| `std/time.nox` | `deadline_ms(timeout_ms: int) -> int` | 无 | 返回 `now_unix_ms + timeout_ms` |
| `std/time.nox` | `is_past_deadline_ms(deadline_ms: int) -> bool` | 无 | 与 `now_unix_ms` 比较 |
| `std/option.nox` | `is_some<T>(value: option[T]) -> bool` | 无 | n/a |
| `std/option.nox` | `is_none<T>(value: option[T]) -> bool` | 无 | n/a |
| `std/option.nox` | `unwrap_or<T>(value: option[T], fallback: T) -> T` | 无 | n/a |
| `std/result.nox` | `is_ok<T, E>(value: result[T, E]) -> bool` | 无 | n/a |
| `std/result.nox` | `is_err<T, E>(value: result[T, E]) -> bool` | 无 | n/a |
| `std/result.nox` | `unwrap_or<T, E>(value: result[T, E], fallback: T) -> T` | 无 | n/a |
| `std/result.nox` | `map_err_to_str<T>(value: result[T, str]) -> result[T, str]` | 无 | n/a |
| `std/process.nox` | `argv() -> [str]` | 无 | n/a |
| `std/process.nox` | `read_stdin() -> str` | 无 | n/a |
| `std/process.nox` | `print_err(value: str) -> null` | 无 | n/a |
| `std/process.nox` | `exit(code: int) -> null` | 无 | n/a |
| `std/process.nox` | `run(program: str, args: [str], stdin: str, timeout_ms: int) -> result[(int, str, str), str]` | `process_run` | 返回 `(exit_code, stdout, stderr)`；输出上限 4 MiB；timeout > 0 触发 kill |
| `std/process.nox` | `run_with(program: str, args: [str], stdin: str, timeout_ms: int, cwd: str, env_pairs: [(str, str)]) -> result[(int, str, str), str]` | `process_run` | 同 `run`，额外支持工作目录与环境变量覆盖：`cwd` 为空时继承当前工作目录，非空作为 `Command::current_dir`；`env_pairs` 在继承父进程环境基础上叠加 key/value（空列表表示完全继承），value 为 `"<unset>"` 时删除该变量，空字符串表示设置为空值；单 runtime 默认最多同时运行 8 个子进程 |

`run` / `run_with` 的 `result.err` 消息以稳定 code 前缀开头，便于工具区分失败原因：

- `process_run.allowlist-denied`：可执行名不在 `process_run_allowlist`。
- `process_run.concurrent-limit`：达到单 runtime 子进程并发上限。
- `process_run.spawn-failed`：OS 拒绝创建子进程（命令不存在、无权限等）。
- `process_run.timeout`：子进程超过 `timeout_ms` 已被 kill。
- `process_run.output-cap-stdout` / `process_run.output-cap-stderr`：输出超过 4 MiB 上限，子进程被 kill。
- `process_run.stdin-write-failed`：写 stdin 到子进程管道失败。
- `process_run.wait-failed`：等待子进程时 OS 报错。

冒号后是人类可读的详细信息；消费者按第一个 `: ` 拆分 code 与 message。
| `std/path.nox` | `join(left: str, right: str) -> str` | 无 | n/a |
| `std/path.nox` | `basename(path: str) -> str` | 无 | n/a |
| `std/path.nox` | `dirname(path: str) -> str` | 无 | n/a |
| `std/path.nox` | `extension(path: str) -> str` | 无 | n/a |
| `std/path.nox` | `normalize(path: str) -> str` | 无 | n/a |

导入 `std/*` 模块只提供静态表面，不会授予权限。缺失的内置标准库模块返回
`module.not-found`，缺失成员返回 `module.member-not-found`。普通项目中的
`std/fs.nox` 文件不会覆盖内置模块；如需导入用户文件，使用相对路径，例如
`import "./std/fs.nox" as local_fs;`。

旧全局函数仍作为兼容表面保留：

```text
abs(value: float) -> float
min(left: float, right: float) -> float
max(left: float, right: float) -> float
sqrt(value: float) -> float
pow(left: float, right: float) -> float
floor(value: float) -> float
ceil(value: float) -> float
round(value: float) -> float
log(value: float) -> float
log2(value: float) -> float
sin(value: float) -> float
cos(value: float) -> float
tan(value: float) -> float
pi() -> float
e() -> float
args() -> [str]
read_text(path: str) -> str
exists(path: str) -> bool
write_text(path: str, contents: str) -> null
env_get(name: str) -> str
env_list() -> map[str, str]
sleep_ms(ms: int) -> null
tcp_connect(host: str, port: int) -> bool
task_sleep_ms(ms: int) -> int
task_ready(id: int) -> bool
task_cancel(id: int) -> null
```

数学内建不需要外部能力，总是可用。`sqrt` 要求非负输入，`log` / `log2` 要求正数输入；
非法边界返回 runtime diagnostic 而不是 panic。核心数值转换由 `nox_core` 自身提供：

`std/time.nox` 的 Unix 时间 API 使用 UTC，不引入时区数据库。`format_unix` / `parse_unix`
支持的最小格式 token 是 `%Y`、`%m`、`%d`、`%H`、`%M`、`%S` 和 `%%`。`parse_unix`
返回 `result[int, str]`，格式不匹配或日期越界会返回 `err(message)`。

```text
to_float(value: int) -> float
to_int(value: float) -> int
```

这些全局函数是 v0.0.3 兼容表面，至少保留到 v0.0.4 完成。当前不对旧全局函数发 warning，
formatter 也不会自动改写旧调用；迁移建议只在文档和示例中体现。新增标准库能力默认不继续扩大全局命名空间；长期分层
策略见 [0012 - 标准库命名分层策略](decisions/0012-stdlib-namespace-strategy.md)，
`std/*` 静态模块加载规则见
[0013 - std/* 静态模块加载](decisions/0013-stdlib-module-loader.md)。导入
`std/fs.nox` 不会授予文件系统权限，权限仍在调用具体函数时检查。

`std/string.nox` 是纯计算模块，不读取外部环境也不需要 capability。`split`、`replace`、
`index_of`、`contains` 和 `last_index_of` 拒绝空 separator / target / needle，避免生成隐式字符边界语义；
`substring` 按 Unicode scalar value 的字符索引截取，`start` 和 `length` 必须非负且范围不能越界。
`index_of` / `last_index_of` 返回字符索引，找不到时返回 `-1`。`parse_int` / `parse_float` 返回
`result`，解析失败不会产生 runtime diagnostic；`pad_left` / `pad_right` 的 fill 必须正好是一个字符。

`std/json.nox` 也是纯计算模块，不读取外部环境也不需要 capability。`parse` 返回
`result[json, str]`，malformed JSON 走 `err(message)` 而不是 runtime panic；
`stringify` 输出紧凑 JSON 文本，object key 按字典序稳定输出。`kind` 返回 `null`、`bool`、`number`、
`string`、`array` 或 `object`；`array_*` / `object_*` helper 对错误 kind、越界 index 或缺失 key 返回
`result.err(message)`。

`std/csv.nox` 和 `std/tsv.nox` 提供单行解析/格式化 helper，不是 streaming parser。CSV 支持双引号
字段和 `""` 转义；TSV 以 tab 分隔并拒绝格式化包含 tab 的字段。坏输入返回 `result.err(message)`。

`std/array.nox`、`std/map.nox`、`std/option.nox` 和 `std/result.nox` 也是纯计算模块，不需要
capability。多数 array 和 map helper 仍返回拷贝；`slice_copy` 越界或负数范围返回
`result.err(message)`。option/result helper 提供状态判断和 fallback，不引入闭包或高阶函数。

`std/array.nox` 与 `std/map.nox` 另有一组就地 mutation helper，会修改所有 alias 共享的底层
存储：`array.set(values, index, value) -> result[null, str]`（`index` 越界返回
`result.err(message)`）、`array.append(values, value) -> null`、
`array.pop(values) -> option[T]`、`map.set(values, key, value) -> null`、
`map.delete(values, key) -> bool`。语言层也支持 `arr[i] = value` 和 `map[key] = value`
索引赋值语法糖；运行时把它们编译为 `IndexAssign` 指令并直接修改底层存储，等价于上述
helper 但语法更简洁。数组越界写入返回稳定诊断 code `runtime.index-out-of-range`，
非数组/非 map LHS 在 typecheck 阶段返回 `type.assign-target`。

`std/process.nox` 提供命令行脚本入口 helper。`argv() -> [str]` 返回入口路径之后的脚本参数，
不包含脚本路径；`read_stdin() -> str` 读取全部 stdin；`print_err(value) -> null` 向 stderr 写入
一行；`exit(code) -> null` 接受 `0..255`，脚本成功结束后 `nox run` 使用该值作为进程退出码。

`std/path.nox` 是纯计算模块，提供 `join`、`basename`、`dirname`、`extension` 和词法
`normalize`，不访问文件系统。`std/fs.nox` 额外提供 `is_file`、`is_dir` 和 `list_dir`；这些函数
沿用 `read_text` / `exists` 的 filesystem read capability 和 allowlist 检查，`list_dir` 按名称排序返回目录项。

`read_text` 和 `exists` 需要 `filesystem` 读权限。`write_text` 需要 `filesystem_write`
权限：未授予时返回 `filesystem write capability is required` 诊断且不创建/修改
文件。配置 root allowlist 后，运行时会把相对路径转成当前进程工作目录下的绝对路径，
并对已存在路径使用 `canonicalize` 解析符号链接和 `..`。规范化后的路径必须位于对应
root 下，否则返回 `filesystem read permission denied` 或
`filesystem write permission denied`。空路径返回 `invalid filesystem path`。
如果目标文件尚不存在，运行时会向上找到最近的已存在父路径并解析符号链接，再拼回缺失的
后缀；因此经由 allowlist 内部 symlink 写到外部目录的缺失文件也会被拒绝。
权限未授予和文件系统 allowlist 拒绝使用稳定诊断 code `permission.denied`；普通 I/O
失败、无效路径和参数错误仍按各自错误语义报告。
缺失但位于允许 root 下的路径仍按普通 I/O 行为处理：`exists` 返回 `false`，
`read_text` / `write_text` 返回底层 not found 诊断，`std/fs.nox` 的 `try_read_text`
返回 `err(message)`。权限不足、allowlist 越界和无效路径仍是 diagnostic，不会被包装成
`err`。

嵌入宿主可以用 `Runtime::set_mock_filesystem(Some(MockFilesystem))` 给单个 runtime
替换 `std/fs.nox` 的读侧。mock 覆盖文本读取、二进制读取、存在/类型检查、目录列表和
`canonicalize`，也覆盖 `write_text` 和 `write_binary`。权限语义不变：每次 mock
读写仍必须先具备对应 filesystem capability，并通过对应 allowlist 后才会查询或修改
mock。启用 mock 后写入 helper 写入 mock 存储，不触碰真实文件系统。

`args` 返回宿主注入的位置参数。`nox run script.nox a b` 中，脚本看到的是
`["a", "b"]`，不包含 `nox`、子命令或入口路径。

`env_get` 读取环境变量，缺失变量会产生诊断。`std/env.nox` 的 `try_get` 把缺失变量
表达为 `none`，存在时返回 `some(value)`；权限不足和非 UTF-8 环境值仍是 diagnostic。
`env_list` 返回当前进程环境的字典；如果环境变量名或值不是有效 Unicode，会返回诊断，
不会把进程环境读取 panic 暴露给脚本。这些环境能力都需要 `environment` 权限。`sleep_ms`
阻塞当前线程，参数必须是非负毫秒。

`tcp_connect(host, port) -> bool` 当前是连通性 **probe**：它尝试建立 TCP
连接、读取握手结果、再立即关闭，**返回成功与否的布尔值**。它不是 socket API；
脚本拿不到 file descriptor、不能 read/write、也无法保持连接。如果需要真正的
socket，宿主应自己注册更完整的 host function。port 必须落在 `0..=65535`。
未授予 `network` 时调用会返回 `permission.denied` 诊断；授予后，连接被拒绝、DNS
解析失败或其他普通连接失败会返回 `false`，不会升级成权限诊断。

嵌入宿主可以用 `Runtime::set_mock_network(Some(MockNetwork))` 给单个 runtime
替换 `tcp_connect` 和 `std/http.nox`。mock 仍受 `network` capability 控制；
未配置的 mock HTTP 响应返回 `result.err`，不会回落到真实网络。

`task_sleep_ms` 创建一个内部 sleep task 并返回非负 `int` 作为 task id。id 在同一个
`Runtime` 内单调递增，不复用。当前 task 状态机：

| 状态 | 进入方式 | 后续行为 |
| --- | --- | --- |
| pending | `task_sleep_ms(ms)` 成功返回 id | `task_ready(id)` 在 deadline 前返回 `false`，任务保留。 |
| completed | `task_ready(id)` 到达 deadline | 返回 `true`，同时释放任务；之后 id 变成 unknown。 |
| cancelled | `task_cancel(id)` 成功 | 返回 `null`，同时释放任务；之后 id 变成 unknown。 |
| rejected | `task_sleep_ms` 收到负数、未授权或超过 pending task 上限 | 不创建 id，不进入任务表。 |
| unknown | 从未创建、已 completed 或已 cancelled 的 id | `task_ready` / `task_cancel` 返回 `unknown async task id` 诊断。 |

`Runtime::pending_async_task_count()` 返回当前 pending task 数，用于宿主观察和测试释放
行为。完成和取消都会把任务从表中移除；重复 poll completed task、poll cancelled task、
或 cancel unknown task 都是诊断。

`RuntimePermissions::async_task_max_pending` 默认是 `Some(1024)`，限制单个
`Runtime` 中 pending sleep task 数。达到上限时，`task_sleep_ms` 在创建新 id 前返回
稳定 diagnostic code `runtime.task-pending-cap`。可信宿主如需自行做任务配额，可以设为
`None`。

这些 task helper 是最小宿主函数，不是 event loop，也不是语言级 async 语法。任务状态只活在
`Runtime` 实例内部，跨 `Runtime` 不共享；CLI 和嵌入 API 使用同一个 `Runtime`
实现，因此生命周期语义一致。`task_cancel` 只取消指定 pending task id；VM instruction
budget 耗尽不会自动取消已经存在的 pending task。`Runtime::eval` 或 `Runtime::run_test_file`
这类顶层调用失败时，会释放本次调用中新建且仍 pending 的 task；instruction budget 耗尽也按
顶层调用失败处理。失败前已经由更早调用创建的 pending task 仍保留，直到宿主继续 poll、cancel
或丢弃整个 `Runtime`。

v0.0.6 评估后继续暂缓脚本级 `task_status` / `task_poll` API。原因是当前 completed 和
cancelled task 都会立即释放，新增非消费式状态查询需要先定义稳定 `TaskStatus` 表示、unknown id
是否可恢复、以及 completed/cancelled tombstone 的生命周期。决策记录见
[0016 - 暂缓 async task 状态 API](decisions/0016-defer-async-task-status-api.md)。

## 文件加载和 import

`Runtime::eval_file(path)` 读取 `path`，并安装以入口文件目录为根的 module loader。
脚本中的：

```nox
import "math.nox";
```

会解析为入口文件目录下的 `math.nox`。重复 import 同一 specifier 在一次编译中只加载一次；
循环 import 会诊断失败。

文件加载能力只属于 `nox` 运行时。直接使用 `nox_core::Engine` 的宿主必须自己提供 loader。

## 取消执行

运行时可以设置 instruction budget。VM 每执行一条 bytecode instruction 消耗一次预算。
预算耗尽时，当前 VM 执行停止并返回诊断；`Engine` / `Session` 本身仍可复用，宿主重置
预算后可以继续执行后续源码。使用 `None` 表示不限制。

```rust
let mut runtime = nox::Runtime::new();
runtime.set_instruction_budget(Some(10_000));
```

嵌入 `nox_core` 时也可以直接在 `Engine` 上设置同一预算。

instruction budget 的边界是 VM bytecode，不是线程抢占。已经进入的 host function、host
callback、文件/环境/网络调用，以及 `sleep_ms` 这类阻塞当前线程的 timer helper，不会被 VM
预算中途打断；宿主如果需要这些操作具备超时或取消能力，应在自己的 host function 内实现。
当前运行时也不包含多线程 event loop。

## 运行时错误

运行时错误包括：

- 数组越界。
- map key 不存在。
- 整数或浮点除零。
- 整数溢出。
- float 非有限结果。
- 调用未授权的宿主能力。
- 宿主函数返回值不符合声明类型。

Nox v0.0.5-dev 已有语言级 `option[T]` / `result[T, E]` 类型、构造和受限 `match`
解包，并开始把可预判失败迁移到可恢复返回值。推荐写法：

- 文件读取：调用 `std/fs.nox` 的 `try_read_text(path) -> result[str, str]`。
- map 读取：调用 `map_get(map, key) -> option[T]`。
- 环境变量：调用 `std/env.nox` 的 `try_get(name) -> option[str]`。
- TCP 连通性：使用 `tcp_connect(host, port) -> bool` 作为 probe。

旧的 `exists` + `read_text`、`contains` + index、`env_list` + `contains` guard 仍可用，
不会变成 warning 或 error。未通过可恢复 API 或 guard 的缺失值、权限错误和宿主 callback
错误继续以 diagnostic 形式中止当前执行。v0.0.3 暂缓依据见
[0009 - 暂缓语言级 option / result](decisions/0009-defer-option-result.md)；v0.0.4 重启设计见
[0014 - 重启 option / result 设计但暂不实现](decisions/0014-restart-option-result-design.md)。

诊断会带 span 和 message；部分稳定类别会带 `code`，例如 `runtime.division-by-zero`。

## 后续边界

- 文件系统标准库默认不按路径限制；宿主可以通过 `filesystem_read_roots` /
  `filesystem_write_roots` 或 `allow_filesystem_*_under` helper 把脚本读写限制到指定 root。
- 网络 API 当前是最小 `tcp_connect` probe。如果未来需要真实的 socket，会
  在 PLAN 中先写设计文档（包含 fd 生命周期、并发模型、与 async task 的
  集成）。
- async task helper 当前只覆盖 sleep；未来增加 IO 任务时，`task_cancel` 和
  `task_ready` 的"未知 id"约定保持不变。脚本级 task status API 的重启条件见
  [0016 - 暂缓 async task 状态 API](decisions/0016-defer-async-task-status-api.md)。
