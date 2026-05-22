# 0013 - std/* 静态模块加载

- 状态：已采纳
- 日期：2026-05-21
- 涉及：运行时 / 模块 / 工具链

## 背景

ADR 0012 已经决定：现有全局标准库函数在 v0.0.3 作为兼容表面保留，新增标准库能力优先
走静态 namespace import，而不是继续扩大全局命名空间。阶段 24.1 需要把这个方向细化
成可实现的 module loader 规则，否则阶段 24.2 容易在路径、权限、诊断和 LSP 行为上各自
发散。

当前普通 import 解析文件系统路径，并在 manifest 下使用 `modules.source_dirs` 作为
搜索根。`std/*` 模块不能被普通文件偶然覆盖，也不能让 import 本身绕过 runtime
permission。它还必须保持 ADR 0008 的静态 namespace import 模型：`fs.read_text` 是
静态模块成员访问，不是动态 object field。

## 决策

`std/*` 是 `nox` runtime 提供的虚拟内置模块命名空间，不属于 `nox_core`。`nox_core`
继续只提供语言核心、模块图、类型检查、VM、C ABI 和核心纯 intrinsic；默认 CLI/runtime、
LSP 和项目命令负责安装 `std/*` module loader。

路径命名固定使用带 `.nox` 后缀的 specifier：

```nox
import "std/fs.nox" as fs;
import "std/env.nox" as env;
import "std/time.nox" as time;
```

`std/fs`、`std/env` 这类无后缀短名暂不作为规范写法。实现可以在后续为了诊断友好而识别
短名并提示 `std/fs.nox`，但不把短名加入推荐表面。这样 formatter、docs、LSP completion
和示例只有一种路径格式。

解析优先级：

1. specifier 以 `std/` 开头时，先进入 runtime 内置 std module 表。
2. 命中内置表时，不访问文件系统，也不使用当前文件目录或 manifest `modules.source_dirs`。
3. 未命中内置表时，返回标准模块诊断，不回退到普通文件 import。
4. specifier 不以 `std/` 开头时，完全保持现有相对路径和 manifest 搜索规则。

第一批内置表只覆盖已有全局能力，不新增 I/O 或系统能力：

| 模块 | 成员 | 绑定到现有全局函数 |
| --- | --- | --- |
| `std/fs.nox` | `read_text(path: str) -> str` | `read_text` |
| `std/fs.nox` | `try_read_text(path: str) -> result[str, str]` | runtime-only recoverable read helper |
| `std/fs.nox` | `exists(path: str) -> bool` | `exists` |
| `std/fs.nox` | `write_text(path: str, contents: str) -> null` | `write_text` |
| `std/env.nox` | `get(name: str) -> str` | `env_get` |
| `std/env.nox` | `try_get(name: str) -> option[str]` | runtime-only optional env helper |
| `std/env.nox` | `list() -> map[str, str]` | `env_list` |
| `std/time.nox` | `sleep_ms(ms: int) -> null` | `sleep_ms` |

`std/math.nox`、`std/net.nox`、`std/task.nox` 可以在后续阶段用同一机制加入，但不作为
24.2 的最小范围。`to_float` / `to_int` 继续属于核心 intrinsic，不迁入 `std/*`。

实现策略采用虚拟源码或等价的预注册模块表面，而不是把真实 `.nox` 文件放进用户可搜索的
目录。虚拟模块源码可以只服务 parser/type checker/module surface，例如：

```nox
export fn read_text(path: str) -> str {
    return read_text(path);
}
```

实际编译时需要避免 wrapper 自递归。实现可以在 resolver 阶段把 std module 成员静态绑定
到对应 host function symbol，或给虚拟源码使用内部不可导出的 host symbol。无论采用哪种
内部表示，对脚本可见的表面都是 `fs.read_text(...)` 这类 namespace member。

权限规则不变：import `std/fs.nox` 不授予 filesystem permission；调用
`fs.read_text`、`fs.try_read_text`、`fs.exists`、`fs.write_text` 时仍由现有 runtime
permission 检查返回诊断。`fs.try_read_text` 只把普通读取失败包装为 `err(message)`；
权限不足、allowlist 越界和无效路径仍是 diagnostic。`std/env.nox` 和 `std/time.nox`
同理。`nox check`、LSP diagnostics 和 formatter 可以在无权限环境下解析和检查这些调用的
类型，但运行时权限错误仍只在执行相关函数时产生。

工具链行为：

- formatter 保留用户写的 `import "std/fs.nox" as fs;`，不把全局函数自动改写成模块形式。
- LSP completion 在 `fs.` 后返回内置模块导出成员；路径 completion 可以暂缓。
- LSP diagnostics 使用同一 std module 表，不要求磁盘上存在 `std/fs.nox`。
- CLI `check` / `test` / `run`、project check 和 sample project 都通过同一个 runtime
  std module loader，不做每个命令一份规则。

诊断规则：

- 缺失内置模块返回稳定 code `module.not-found`，message 应说明 `std/...` 是内置模块且
  当前未提供该模块。
- 缺失成员继续使用 ADR 0008 的 `module.member-not-found`。
- 普通文件系统中存在 `std/fs.nox` 时也不会覆盖内置模块；如需导入用户文件，必须使用
  相对路径，例如 `import "./std/fs.nox" as local_fs;`。

## 后果

标准库模块化有了一个单一入口：`nox` runtime 负责安装虚拟 std module loader，
`nox_core` 不感知 runtime 标准库。这样嵌入式核心保持小而稳定，CLI/LSP/project check
又能给用户一致的模块体验。

代价是实现需要一个新的“内置模块表面”层，不能简单把 wrapper `.nox` 文件塞进 source
dirs。这个复杂度是有意的：它避免用户文件覆盖标准库，也避免 import 规则和 runtime
permission 纠缠。

旧全局函数至少保留到 v0.0.4 完成。迁移文档可以推荐模块形式，但不引入 warning 机制；
是否对旧全局函数提示迁移，需要单独设计诊断等级。

## 备选方案

- 真实 bundled `.nox` 文件：直观、可复用普通 loader，但需要把运行时文件路径加入搜索根，
  也容易和用户项目里的 `std/` 目录产生覆盖或优先级歧义。
- 无 `.nox` 后缀的 `std/fs`：更短，但和当前 import specifier 习惯不一致，也让 docs、
  formatter 和未来路径 completion 同时维护两套写法。
- 动态 `std` object：调用形式接近其他语言，但违反 ADR 0008 的静态 namespace import
  约束，并会把标准库设计拖向 object/value 模型。
- 让 `nox_core` 内置 std module：能让所有宿主天然可用，但会把文件、环境、时间等
  runtime policy 推进核心 crate，破坏 engine/runtime 分层。
