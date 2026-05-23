# 0017 - 字符字面量与 byte 字面量边界

- 状态：已采纳
- 日期：2026-05-22
- 涉及：语言 / ABI / 工具链

## 背景

阶段 11 补齐字面量人体工学后，剩余评估项是字符和 byte 字面量：

- `'A'` 这类单字符写法能减少只需要一个字符的字符串样板。
- `b"..."` 这类 byte 序列通常需要独立 `bytes` 类型、索引规则、长度语义和 ABI 传递规则。

当前 Nox 已有 `str`、array、map、record、`json`、`option` 和 `result`。C ABI 的复合值只读
handle、CLI JSON 诊断、formatter 和 LSP 都已经把这些类型作为公共契约。直接引入 `char` 或
`bytes` 会扩大类型系统、VM value kind、C ABI enum 和文档矩阵。

## 决策

阶段 11 只实现单引号字符字面量，并把它降为现有 `str` 值：

- `'A'`、`'界'` 和 `'\n'` 的类型都是 `str`。
- 字面量内容必须是一个 Unicode scalar，不能为空，也不能包含多个字符。
- 支持的 escape 仅包括 `\n`、`\t`、`\'` 和 `\\`。
- malformed 字符字面量使用稳定诊断 code `lex.invalid-character`。
- 不新增 `char` 类型、`bytes` 类型、byte string token、VM value kind、Rust API 或 C ABI enum。

`b"..."` 和 `bytes` 相关能力继续暂缓。重新启动必须先给出这些设计：

- `bytes` 的源码类型名、字面量语法、长度和索引返回类型。
- UTF-8 `str` 与 bytes 的显式转换 API，以及失败路径是否为 diagnostic、`option` 还是 `result`。
- Rust `Value` 与 C ABI value kind / owning handle 的表示。
- formatter、LSP hover/completion、CLI JSON 诊断和 release-gate fixture。

## 后果

脚本可以用更自然的写法表达单字符字符串，不需要写 `"A"` 或 `"\n"` 来表达字符意图。
因为仍然复用 `str`，不会影响类型检查、bytecode、VM、C ABI 或 host callback 边界。

代价是 Nox 暂时不能表达原始 byte buffer，也不能区分字符串长度和 byte 长度。需要处理二进制
协议、文件字节或编码转换的宿主仍应在 Rust 侧处理，或等待后续 `bytes` 设计。

## 备选方案

- 引入独立 `char` 类型：表达更精确，但会扩展类型系统、`Value`、C ABI、formatter 和 LSP。
  当前没有必须区分 `char` 与单字符 `str` 的用例。
- 立即引入 `bytes` / `b"..."`：对二进制 I/O 有价值，但会牵动 runtime fs/net、ABI handle、
  JSON/diagnostic 表示和索引语义，超出阶段 11 词法增强边界。
- 完全不做字符字面量：最保守，但会让阶段 11 在常见字面量人体工学上留下一个低成本缺口。
