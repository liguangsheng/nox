# Cookbook

本页按常见脚本任务组织 Nox 已有能力。链接到的示例都是仓库里的可运行文件；短代码片段只展示
API 形状，避免为每个小组合都增加重复示例。

## 创建项目

创建最小项目：

```sh
nox new demo_app
cd demo_app
nox project check
nox run
nox test
nox fmt --check
```

脚手架包含 `nox.toml`、`src/main.nox`、`tests/main_test.nox` 和简短 README。目录名需要
和 package name 不同时，使用 `nox new demo_app --dir path/to/project`。

## CLI 输入输出

命令行脚本使用 `std/process.nox`：

```nox
import "std/process.nox" as process;

let argv: [str] = process.argv();
let input: str = process.read_stdin();

if (len(argv) > 0 && input != "") {
    print("processed " + argv[0]);
    process.print_err("ok");
    process.exit(0);
} else {
    process.print_err("missing input");
    process.exit(2);
}
```

可运行示例：[`../../examples/process-stdio.nox`](../../examples/process-stdio.nox)。

## JSON / TOML 配置

用 `std/toml.nox` 或 `std/json.nox` 把配置解析成 `json`，需要严格 schema 时再用
`json.from_json<T>` 解码到 typed record：

```nox
import "std/json.nox" as json;
import "std/toml.nox" as toml;

record Config {
    name: str,
    retries: int,
}

let raw: result[json, str] = toml.parse("name = \"nox\"\nretries = 3");
match (raw) {
    ok(value) => {
        let decoded: result[Config, str] = json.from_json(value);
        match (decoded) {
            ok(config) => {
                config.name;
            }
            err(message) => {
                "bad config: " + message;
            }
        }
    }
    err(message) => {
        "bad TOML: " + message;
    }
}
```

JSON object 访问示例：[`../../examples/json.nox`](../../examples/json.nox)。

## 文件和权限

运行时文件系统 helper 需要宿主或 CLI 集成显式授予 capability。项目 manifest 可以声明期望权限，
但 manifest 本身不会自动授权。建议把 capability-bound 调用和纯逻辑分层，这样单元测试可以不授予
文件系统权限。

参考项目：[`../../examples/projects/health-check`](../../examples/projects/health-check)。

## HTTP 请求

`std/http.nox` 支持明文 `http://` GET/POST helper、带自定义 header 和 response headers
的通用 request helper，以及二进制 body 版本。脚本应同时处理传输错误和非 2xx 状态码：

```nox
import "std/http.nox" as http;

let response: result[(int, map[str, str], str), str] = http.request(
    "GET",
    "http://example.test/data",
    {"accept": "application/json"},
    "",
    1000,
);
match (response) {
    ok(pair) => {
        let (status, headers, body) = pair;
        if (status >= 200 && status < 300) {
            body;
        } else {
            "unexpected status";
        }
    }
    err(message) => {
        "request failed: " + message;
    }
}
```

HTTP 需要 `network` capability。嵌入测试优先用 `MockNetwork`；见
[`embedding.md`](embedding.md)。返回的 header name 会统一小写；重复 response header
用 `", "` 折叠。

## 分隔文本和 JSONL 类数据

`std/csv.nox` 与 `std/tsv.nox` 提供单行和 eager 多行 parse / format helper。可运行示例：
[`../../examples/delimited-text.nox`](../../examples/delimited-text.nox)。

JSON Lines 输入使用 `std/jsonl.nox`。当前 helper 是 eager 处理，不是 streaming
parser；parse 错误包含 1-based 行号：

```nox
import "std/json.nox" as json;
import "std/jsonl.nox" as jsonl;

let values: result[[json], str] = jsonl.parse_lines("{\"ok\":true}\n{\"ok\":false}");
```

可运行 JSONL 示例：[`../../examples/jsonl.nox`](../../examples/jsonl.nox)。

确定性 digest 使用 `std/hash.nox`：

```nox
import "std/hash.nox" as hash;

let digest: str = hash.sha256_text("abc");
```

可运行 hash 示例：[`../../examples/hash.nox`](../../examples/hash.nox)。

## 传播可恢复错误

脚本可处理的失败用 `result` 或 `option` 表达；权限、资源、类型或宿主边界错误仍走
runtime diagnostic：

```nox
import "std/json.nox" as json;

fn normalize_config(source: str) -> result[str, str] {
    let value: json = json.parse(source)?;
    return ok(json.stringify(value));
}
```

Nox 当前没有 `try/catch/finally` 异常通道，也没有 `try {}` block。局部链路需要 `?`
时，优先提取小函数；小型值转换可以用 `std/result.nox` / `std/option.nox` 的 `map`
和 `and_then`。

## 自定义值去重

`std/array.nox` 保留旧的 `Equatable` helper，同时通过 `Eq` 暴露第一批 trait-bound
helper。record 的相等性需要按业务键判断，而不是比较每个字段时，可以为 record 实现 `Eq`：

```nox
import "std/array.nox";

record User {
    id: int,
    name: str,
}

impl Eq for User {
    fn equals(self: User, other: User) -> bool {
        return self.id == other.id;
    }
}

let users: [User] = [
    User { id: 1, name: "ada" },
    User { id: 1, name: "ada-lovelace" },
];
let unique: [User] = dedupe_equal(users);
let found: bool = contains_equal(users, User { id: 1, name: "alias" });
```

## 测试、snapshot 和 property

普通测试用 `fn test_*() -> bool`。需要更明确的失败诊断时导入 `std/test.nox`：

```nox
import "std/test.nox" as test;

fn test_snapshot() -> bool {
    test.assert_snapshot("greeting", "hello", "hello");
    return true;
}

fn test_property() -> bool {
    test.assert_property_int("non-negative square", 1, 8, 0, 10, fn(value: int) -> bool {
        return value * value >= 0;
    });
    return true;
}
```

stdlib surface fixture 覆盖了这些 helper：
[`../../tests/fixtures/stdlib-surface.nox`](../../tests/fixtures/stdlib-surface.nox)。

## 嵌入 host function

C 嵌入使用 `crates/nox_core/include/nox_core.h`，在 eval 前注册 host callback。最小 smoke
保存在可运行 C 示例中：[`../../examples/embed/c_embedding.c`](../../examples/embed/c_embedding.c)。

Rust 嵌入只需要语言求值时使用 `nox_core::Engine`；需要默认运行时模块、权限和 mock 时使用
`nox::Runtime`。ownership、diagnostic、mock filesystem/network 示例见
[`embedding.md`](embedding.md)。
