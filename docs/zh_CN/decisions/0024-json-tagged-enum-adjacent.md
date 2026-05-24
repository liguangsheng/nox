# 0024 - JSON tagged enum 使用 adjacent 表示

- 状态：已采纳
- 日期：2026-05-24
- 涉及：语言 / 标准库 / CLI JSON

## 背景

`std/json.nox` 已提供 `to_json<T>(value: T) -> json`，需要把 Nox 的 record、enum、
option、result 和容器值转换成 RFC 8259 JSON。record 可以自然映射为 object，tuple
和 array 可以自然映射为 array；用户 enum 带 payload 时必须选择 tagged enum
表示，否则 variant 名称和 payload 无法稳定保留。

阶段 42 的目标要求明确 record / enum 与 JSON object 的映射规则，以及 tagged enum
JSON 表示取舍。这个决定最初只约束 `to_json` 的单向序列化；后续实现已经复用同一
adjacent 契约落地 `from_json<T>` 自动反序列化。

## 决策

Nox 采用 adjacent tagged enum 表示：

- record 序列化为 JSON object，key 为 record 字段名。
- 无 payload 的用户 enum variant 序列化为 JSON string：`"VariantName"`。
- 带 payload 的用户 enum variant 序列化为 JSON object：
  `{"_variant":"VariantName","payload":...}`。
- `result` 复用同一 adjacent 形状，variant 为 `"ok"` 或 `"err"`。
- `option` 不使用 tag：`some(value)` 序列化为 payload，`none` 序列化为 `null`。

`"_variant"` 和 `"payload"` 是标准库序列化契约的一部分。未来如果支持外部标签或内部
标签，只能作为显式新 helper 或新 option 入口追加，不能改变 `to_json` 的默认输出。

## 后果

adjacent 表示让 payload 类型不受限制，object / array / scalar payload 都能保持同一
形状；解析方也可以先读 `"_variant"` 再按 variant 读取 `"payload"`。代价是带 payload
variant 会多一层 object，且用户数据中如果也使用 `"_variant"` / `"payload"` 字段，
需要由调用方在业务 schema 中避免歧义。

后续 `from_json<T>` 已提供自动反序列化入口，并复用本 ADR 固定的 adjacent enum 形状。
调用点需要 expected `result[T, str]` 类型，编译器把目标类型传给 VM；错误以 path-aware
`result.err` 返回。手工 `object_get`、`as_*` 和 match 映射仍可用于需要自定义容错的脚本。

## 备选方案

- 外部标签：如 `{"VariantName": payload}`。不选择，因为 payload 为 object 时仍可用，
  但缺少固定字段名会让 schema validation 和错误路径不稳定。
- 内部标签：如 `{"type":"VariantName", ...payloadFields}`。不选择，因为只适合 object
  payload，不能一致表示 scalar、array 或 tuple payload。
- 始终用字符串或数组：不选择，因为会丢失 payload 结构或降低可读性。
