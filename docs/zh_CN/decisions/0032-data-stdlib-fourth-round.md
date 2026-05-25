# 0032 - 数据脚本标准库第四轮路线

- 状态：已采纳
- 日期：2026-05-25
- 涉及：标准库 / 数据格式 / 权限 / 发布

## 背景

Nox 已有一组面向脚本和配置处理的纯计算数据模块：`std/json.nox`、`std/jsonl.nox`、
`std/csv.nox`、`std/tsv.nox`、`std/hash.nox`、`std/dotenv.nox`、`std/ini.nox`、
`std/toml.nox`、`std/yaml.nox` 和 `std/xml.nox`。其中 YAML/XML 仍标记为 experimental：

- `std/yaml.nox` 是最小配置 reader，只支持单文档、缩进 mapping、标量 sequence、inline array、
  quoted string、bool、有限数字、null 和注释。
- `std/xml.nox` 是安全文本生成 helper，只校验 XML name 并转义 text/attribute，不解析 XML 文档。

阶段 91 需要决定下一步是把 YAML/XML 推向更稳定边界，还是新增压缩/归档、protobuf、SQLite/database
driver、TLS/HTTPS 等能力。

## 决策

第四轮继续稳态化现有 YAML/XML，不引入新的重依赖或 runtime capability。原因：

- YAML 完整规范包含 anchor、alias、tag、schema coercion、多文档 stream、block scalar、flow
  mapping 和复杂错误路径；一次性承诺完整 YAML 会显著扩大 parser、docs 和回滚成本。
- XML 完整 parser、namespace、schema validation 和 streaming 也会扩大内存、错误模型和文档承诺；
  当前更有价值的是继续强化“安全生成 XML 文本”的小表面。
- 压缩/归档、protobuf、SQLite/database driver 和 TLS/HTTPS 都需要依赖、权限、mock、资源上限或
  网络/文件安全边界设计，不适合作为本轮顺手引入的 stdlib helper。

阶段 92 的实现切片选择 `std/xml.nox`：

- 增加 `attrs(values: map[str, str]) -> result[str, str]`，批量校验并转义 attribute，返回带前导空格
  的 attribute fragment。
- 增加 `empty_element(name: str, attrs: map[str, str]) -> result[str, str]`，生成安全 self-closing tag。
- 增加 `text_element_attrs(name: str, attrs: map[str, str], value: str) -> result[str, str]`，生成带属性的
  text element。

这些 helper 只组合现有 `validate_name`、`escape_attr`、`escape_text` 和 `std/map.nox`，不新增 host
function、不新增 runtime permission、不新增第三方 runtime dependency，也不把 XML 宣传成完整 parser。

## 非目标

- 不把 `std/yaml.nox` 升级成完整 YAML 1.2 reader。
- 不支持 YAML multi-document stream、anchor/alias/tag、block scalar、flow mapping 或 schema-specific
  coercion。
- 不实现 XML parser、namespace resolver、DTD、schema validation、XPath 或 streaming writer。
- 不新增 gzip/zip/tar、protobuf、SQLite/database driver、TLS/HTTPS。
- 不默认授权 filesystem、network、database 或 TLS 能力。

## 阶段 103 复审

阶段 103 在 YAML/XML 第四轮和 codegen/LSP 工具切片后重新评估数据脚本标准库第五轮。结论是继续
稳态化现有纯计算数据 helper，不引入重依赖、隐式权限或完整格式实现。

当前新增证据：

- `std/yaml.nox` 仍适合保持最小配置 reader。完整 YAML 的 anchor、alias、tag、multi-document、
  block scalar、flow mapping 和 schema coercion 会扩大 parser 与错误路径承诺；当前项目样本没有
  证明这些能力优先级高于现有配置子集。
- `std/xml.nox` 已覆盖 name validation、text/attribute escaping、attribute fragment、
  self-closing tag 和 text element with attrs。真实缺口更接近 namespace-qualified name /
  namespace declaration 的安全文本生成，而不是完整 XML parser。
- 压缩/归档需要 binary buffer 边界、streaming/eager 取舍、文件能力组合和资产体积评估；protobuf
  需要 schema compiler 或 codegen 边界；SQLite/database driver 需要连接生命周期、权限、mock 和
  资源上限；TLS/HTTPS 需要网络安全、证书、依赖和 release rollback。它们都不适合作为阶段 104
  的顺手 stdlib helper。

阶段 103 决策：

- 阶段 104 选择 `std/xml.nox` namespace 文本生成增强。
- 允许新增纯源码 helper，例如 `qname(prefix, local) -> result[str, str]`、
  `xmlns(prefix, uri) -> result[str, str]`、`xmlns_default(uri) -> result[str, str]` 或
  `text_element_ns(prefix, local, attrs, value) -> result[str, str]`。
- 这些 helper 只校验 XML name / namespace prefix，并复用现有 escaping / attrs 组合；不解析 XML、
  不解析 namespace scope、不做 schema validation、不提供 XPath 或 streaming writer。
- YAML 继续保持 experimental 最小配置 reader，本轮不扩展为 YAML 1.2 完整实现。
- gzip/zip/tar、protobuf、SQLite/database driver、TLS/HTTPS 继续暂缓；如重启，必须先单独写
  ADR，明确依赖、权限、mock、资源上限、CLI/LSP/docs 和 rollback。

阶段 104 的完成标准：

- `std/xml.nox` 新 namespace helper 有正向和负向 runtime tests。
- `tests/fixtures/stdlib-surface.nox` 覆盖新 helper。
- `examples/data-formats.nox` 展示一个 namespace element 或 namespace attribute。
- 中英文 runtime / stdlib index / CHANGELOG 同步，且不把 helper 描述成完整 XML namespace
  resolver。
- 跑 focused stdlib tests、stdlib surface check、example run、Markdown link check、`git diff --check`
  和 `scripts/release-gate.sh`。

## 阶段 115 复审

阶段 115 在 XML namespace 文本生成 helper、codegen source-map 审计和 async/resource 回归补强后，
重新评估数据脚本标准库第六轮。结论仍是继续稳态化现有纯计算数据 helper，不引入重依赖、隐式权限
或完整格式实现。

当前新增证据：

- `std/xml.nox` 已覆盖安全 name/qname、attribute、namespace declaration、empty element 和 text
  element 生成。剩余低风险缺口更接近“生成 XML 文档片段时的安全注释/文本辅助”，而不是 parser、
  namespace scope resolver、schema validation 或 streaming writer。
- `std/yaml.nox` 仍只适合最小配置 reader。完整 YAML 的 block scalar、anchor、alias、tag、
  multi-document stream 和 schema coercion 仍会扩大 parser、错误路径和兼容承诺。
- codegen source-map 元数据已经补齐“生成源码可审计”的工具边界；protobuf/schema compiler 一类需求
  应继续走外部 codegen，而不是把 schema compiler 放入标准库。
- 压缩/归档、SQLite/database driver 和 TLS/HTTPS 仍需要第三方依赖、权限模型、mock、资源上限、
  release asset 体积和 rollback 设计，不适合作为第六轮标准库小切片。

阶段 115 决策：

- 阶段 116 选择 `std/xml.nox` 安全 XML comment helper，例如
  `comment(value: str) -> result[str, str]`。
- helper 必须拒绝 XML comment 中不允许的 `--`，以及以 `-` 结尾的内容；返回值是完整
  `<!--...-->` 片段。
- helper 只做纯字符串校验和拼接，不解析 XML、不维护 namespace scope、不做 schema validation、
  不提供 CDATA、processing instruction 或 streaming writer。
- YAML 完整化、压缩/归档、protobuf、SQLite/database driver 和 TLS/HTTPS 继续暂缓；若重启，必须
  先单独写 ADR 明确依赖、权限、mock、资源上限、CLI/LSP/docs、size cap 和 rollback。

阶段 116 的完成标准：

- `std/xml.nox` 新 comment helper 有正向和负向 runtime tests。
- `tests/fixtures/stdlib-surface.nox` 覆盖新 helper。
- 中英文 runtime / stdlib index / CHANGELOG 同步，且不把 helper 描述成 XML writer/parser。
- 跑 focused stdlib tests、stdlib surface check、Markdown link check、`git diff --check` 和
  `scripts/release-gate.sh`。

## 后果

这个路线让数据标准库继续服务配置读取和安全文本生成，同时保持 Nox 的零第三方 runtime dependency
和保守权限模型。XML 属性组合是高频需求，且可以通过现有 Nox stdlib 直接实现，验证成本低。

代价是 YAML/XML 仍保持 experimental，用户需要按文档理解它们是有限子集。后续如果真实项目需要
完整 YAML、XML parser、压缩或数据库能力，必须另开 ADR，先定义依赖、权限、mock、资源上限、
错误模型和 release rollback。

## 验证要求

阶段 92 至少覆盖：

- `std/xml.nox` 新 helper 的正向和负向测试。
- `tests/fixtures/stdlib-surface.nox`。
- 中英文 runtime / stdlib index / CHANGELOG。
- `examples/data-formats.nox`。
- `cargo fmt --all --check`、相关 `cargo test -p nox ... --lib`、`target/debug/nox check
  tests/fixtures/stdlib-surface.nox`、示例 run、Markdown link check 和 `git diff --check`。

公共标准库表面变化后，完整批次仍应跑 `scripts/release-gate.sh`。
