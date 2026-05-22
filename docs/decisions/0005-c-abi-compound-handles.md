# 0005 - C ABI 复合值只读 handle

- 状态：已采纳
- 日期：2026-05-20
- 涉及：ABI / 嵌入

## 背景

早期 C ABI 可以返回 `null`、`bool`、`int`、`float` 和 owned string。脚本返回
array、map 或 record 时，C 宿主只能看到 value kind，无法读取内容。PLAN.md 要求
v0.0.2 让 C 宿主能读取这些复合值，同时保持所有权规则清楚。

## 决策

为 array、map 和 record 引入只读 owning handle：

- `NoxCoreArrayHandle`
- `NoxCoreMapHandle`
- `NoxCoreRecordHandle`

`NoxCoreValue` 在末尾追加 `array_handle`、`map_handle` 和 `record_handle` 字段。
当 `nox_core_engine_eval` 返回对应 kind 时，宿主获得一个必须释放的 handle：

- `nox_core_array_free`
- `nox_core_map_free`
- `nox_core_record_free`

读取 API 第一批只覆盖必要操作：

- array：`nox_core_array_len`、`nox_core_array_get`
- map：`nox_core_map_len`、`nox_core_map_keys`、`nox_core_map_get`
- record：`nox_core_record_field`

读取函数写出的 `NoxCoreValue` 继续遵守相同释放规则：owned string 用
`nox_core_string_free`，复合 handle 用对应 free 函数。

## 后果

C 宿主可以读取脚本返回的容器和 record，而无需引入长期借用或 iterator 状态机。代价是
宿主必须正确释放每个返回的 string 或 handle。`NoxCoreValue` 结构体在 v0.0.x 本地开发阶段追加字段，
因此动态加载宿主仍应使用匹配版本的 header 编译并检查 `nox_core_version`。

## 备选方案

- 只暴露 `kind`：被拒绝，宿主无法实际消费复合值。
- iterator API：暂不选择，第一批读取需求可由 len/get/keys/field 覆盖，iterator 状态会扩大 ABI 面。
- C 端构造/mutate 复合值：暂不选择，当前 v0.0.2 目标是读取 eval 结果，不是把 C 变成完整 Value 构造器。
