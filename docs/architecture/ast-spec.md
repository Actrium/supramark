# Supramark AST v2 规范

本文档是 Supramark Markdown 解析层的规范合同。目标不是描述当前某个 parser 已经做到什么，而是定义 `supramark-markdown`、`@supramark/core`、Web/RN renderer、协作/批注系统共同依赖的 AST v2 标准。

一句话版本：

> 输入 Markdown source，输出一个可序列化、可渲染、带可靠 source map、可承载有限扩展语义的 Supramark AST v2。

## 1. 范围

AST v2 负责：

- 表达 Markdown 与 Supramark 扩展语法的语义结构。
- 为每个源码派生节点提供源码位置，用于选区、批注、协作锚点、错误定位。
- 给 Web/RN renderer 一个稳定、平台无关的输入。
- 给后续 feature 一个稳定扩展面，而不是暴露 parser 内部 rule/plugin API。

AST v2 不负责：

- 保存 CRDT 文档状态。
- 保存批注本身。
- 保存渲染后的 React tree、SVG、HTML、布局信息。
- 保证节点 ID 跨编辑稳定。
- 暴露 parser rule、token、internal node、plugin registry 等内部对象。

## 2. 公开 API

规范公开入口只有一个：

```ts
parse(source: string): SupramarkRootNode
```

Rust 入口等价为：

```rust
parse(source: &str) -> SupramarkNode
```

要求：

- `parse` 必须是确定性的：相同 source 与相同 parser 版本产生相同 AST。
- `parse` 不执行网络、文件系统、渲染引擎、宿主回调。
- parser 可以在内部使用 rule、scanner、adapter、feature registry，但这些都不是 public API。
- 合法但暂不支持的语法不得静默丢弃；必须降级为 `raw` / `unsupported` 节点，或在 `diagnostics` 中记录。

## 3. JSON 数据模型

AST v2 必须可以无损序列化为 JSON。

### 3.1 命名

- 节点用 `type` 字段判别。
- `type` 使用 `snake_case`。
- 字段名使用 `snake_case`，但 `data` 保持通用 JSON 扩展字段名。
- 内置节点类型由本规范保留；自定义语义必须通过 `container`、`input`、`raw`、`unsupported` 或未来明确加入规范的节点表达。

### 3.2 字段存在规则

- 必填字段必须总是出现。
- 可选字段在不适用时应该省略。
- `null` 只用于数组占位或明确三态语义；普通“不适用”不要用 `null`。
- `data` 必须是 JSON object，不得包含函数、class instance、平台对象。

示例：

```json
{
  "type": "list_item",
  "children": [{ "type": "paragraph", "children": [] }]
}
```

普通列表项省略 `checked`。任务列表项才写：

```json
{ "type": "list_item", "checked": false, "children": [] }
```

## 4. 基础类型

### 4.1 Point

```ts
interface SourcePoint {
  line: number;
  column: number;
  byte_offset: number;
  utf16_offset: number;
}
```

语义：

- `line` 从 1 开始。
- `column` 从 1 开始，按源码中的 Unicode scalar/display-near 字符位置计数；它只用于展示与调试。
- `byte_offset` 是 UTF-8 源码中的 0-based byte offset。
- `utf16_offset` 是 JS/RN string 中的 0-based UTF-16 code unit offset。
- 编辑器选区、CRDT range、批注锚点必须优先使用 `utf16_offset` 或 CRDT relative position；Rust/native 索引必须优先使用 `byte_offset`。

### 4.2 Position

```ts
interface SourcePosition {
  start: SourcePoint;
  end: SourcePoint;
}
```

语义：

- `position` 表示半开区间 `[start, end)`。
- `start` 指向节点源码范围第一个 code unit/byte。
- `end` 指向节点源码范围之后的位置。
- 对源码派生节点，`source.slice(start.byte_offset, end.byte_offset)` 必须是该节点对应的原始源码范围。
- 节点的 `position` 包含 Markdown 标记本身：例如 `**strong**` 的 strong 节点范围包含两侧 `**`；其 text 子节点只覆盖 `strong` 文字部分。
- `value` 不要求等于源码切片，因为实体、转义、代码围栏、数学分隔符等会被语义化。

### 4.3 Diagnostic

```ts
type DiagnosticSeverity = 'info' | 'warning' | 'error';

interface Diagnostic {
  code: string;
  severity: DiagnosticSeverity;
  message: string;
  position?: SourcePosition;
  data?: Record<string, unknown>;
}
```

要求：

- recoverable parse 问题进入 `root.diagnostics`。
- 单个节点的局部问题可以放在 `node.diagnostics`，同时也可以汇总到 `root.diagnostics`。
- `error` 表示 AST 可产出但某段源码无法被完整语义化；不是 throw。

### 4.4 BaseNode

```ts
interface SupramarkBaseNode {
  type: string;
  position?: SourcePosition;
  data?: Record<string, unknown>;
  diagnostics?: Diagnostic[];
}

interface SupramarkParentNode extends SupramarkBaseNode {
  children: SupramarkNode[];
}
```

要求：

- 源码派生节点必须有 `position`。
- 解析器生成的纯派生节点可以省略 `position`，但应该尽量映射回最接近的源码范围。
- `children` 必须按源码顺序排列。
- parser 不应该产生空 `text` 节点。

## 5. Root 节点

```ts
interface SupramarkRootNode extends SupramarkParentNode {
  type: 'root';
  ast_version: 2;
  children: SupramarkBlockNode[];
  diagnostics: Diagnostic[];
  parser?: {
    name: string;
    version?: string;
  };
}
```

要求：

- `root.ast_version` 必须为数字 `2`。
- `root.children` 只能包含 block-level 节点。
- `root.position` 覆盖整个 source：`[0, source.length)`；空文档时可以是 `[0, 0)`。
- `root.diagnostics` 必须存在；没有诊断时为空数组。

## 6. 节点集合

AST v2 节点分为四类：

- Core Markdown：CommonMark 基础语义。
- GFM：表格、删除线、任务列表。
- Supramark 扩展：diagram、math、footnote、definition list、container、input。
- Recovery：raw、unsupported。

### 6.1 Core Markdown 节点

| type | children | 必填字段 | 说明 |
| --- | --- | --- | --- |
| `paragraph` | inline | `children` | 段落 |
| `heading` | inline | `depth`, `children` | `depth` 必须是 1-6 |
| `text` | none | `value` | 纯文本；不得为空字符串 |
| `strong` | inline | `children` | 加粗 |
| `emphasis` | inline | `children` | 强调 |
| `inline_code` | none | `value` | 行内代码；`value` 不含反引号 |
| `link` | inline | `url`, `children` | `title` 可选 |
| `image` | none | `url`, `alt` | `alt` 必须存在，可为空字符串；`title` 可选 |
| `break` | none | - | 强制换行 |
| `code` | none | `value` | 代码块；`lang`、`meta` 可选 |
| `list` | block | `ordered`, `children` | 有序列表必须有 `start` |
| `list_item` | block | `children` | 任务项才有 `checked` |
| `blockquote` | block | `children` | 引用块 |
| `thematic_break` | none | - | 分隔线 |

参考类型：

```ts
interface HeadingNode extends SupramarkParentNode {
  type: 'heading';
  depth: 1 | 2 | 3 | 4 | 5 | 6;
}

interface CodeNode extends SupramarkBaseNode {
  type: 'code';
  value: string;
  lang?: string;
  meta?: string;
}

interface ListNode extends SupramarkParentNode {
  type: 'list';
  ordered: boolean;
  start?: number;
  tight?: boolean;
}

interface ListItemNode extends SupramarkParentNode {
  type: 'list_item';
  checked?: boolean;
}
```

代码块规则：

- `value` 不包含 opening/closing fence。
- `lang` 是 info string 的第一个非空字段。
- `meta` 是 info string 中 `lang` 后剩余的原始字符串。
- `position` 覆盖整个代码块源码，包括 fence 或缩进。

列表规则：

- `ordered: false` 时省略 `start`。
- `ordered: true` 时 `start` 必须存在；如果源码没有显式不同起点，则为 `1`。
- 普通列表项省略 `checked`；任务项使用 `checked: true | false`。

### 6.2 GFM 节点

```ts
type TableAlign = 'left' | 'right' | 'center';

interface DeleteNode extends SupramarkParentNode {
  type: 'delete';
}

interface TableNode extends SupramarkParentNode {
  type: 'table';
  align: Array<TableAlign | null>;
  children: TableRowNode[];
}

interface TableRowNode extends SupramarkParentNode {
  type: 'table_row';
  children: TableCellNode[];
}

interface TableCellNode extends SupramarkParentNode {
  type: 'table_cell';
  header: boolean;
  align?: TableAlign;
}
```

要求：

- `table.align.length` 必须等于表格列数。
- `table.align` 中没有对齐信息的位置使用 `null`，因为数组需要列占位。
- `table_cell.header` 必须存在。
- `table_cell.align` 只在该列有对齐信息时出现。

### 6.3 Diagram

```ts
interface DiagramNode extends SupramarkBaseNode {
  type: 'diagram';
  engine: string;
  code: string;
  meta?: Record<string, unknown>;
  semantic?: { engine: string; kind: string; data: Record<string, unknown> };
}
```

要求：

- diagram 来自 fenced code block。
- `engine` 是 canonical engine id，必须小写。
- `code` 不包含 opening/closing fence。
- `meta` 由 info string 中 engine 之后剩余部分解析为结构化对象：按空白拆项，每项以第一个 `=` 切 key/value（value 可用双引号包裹，去引号后保存）；无 `=` 的裸 token 记为 `key=true`。剩余部分为空时省略 `meta`（不输出空对象）。
- renderer 根据 `engine` 分发；parser 不调用 Mermaid、PlantUML、Vega、D2 等渲染引擎。
- `semantic` 是可选的语义 AST 信封，形状为 `{ "engine": "...", "kind": "...", "data": {...} }`：`engine` 为引擎 id，`kind` 标识图形语义类别，`data` 为该类别的结构化语义数据。默认懒解析——parser 主流程不填充，缺省为 `None` 且不序列化（无该字段即等同未解析）；由下游按需解析后填充。详见 `docs/architecture/diagram-semantic-ast.md`。

内置 engine id：

- `mermaid`
- `plantuml`
- `vega`
- `vega-lite`
- `echarts`
- `chart`
- `chartjs`
- `dot`
- `graphviz`
- `d2`

### 6.4 Math

```ts
interface MathBlockNode extends SupramarkBaseNode {
  type: 'math_block';
  value: string;
  meta?: string;
}

interface MathInlineNode extends SupramarkBaseNode {
  type: 'math_inline';
  value: string;
}
```

要求：

- `value` 是原始 TeX 内容，不包含 `$` / `$$` 分隔符。
- parser 不渲染数学公式。
- renderer 可以走 MathJax、KaTeX 或宿主提供的 math engine。

### 6.5 Footnote

```ts
interface FootnoteReferenceNode extends SupramarkBaseNode {
  type: 'footnote_reference';
  label: string;
  identifier: string;
}

interface FootnoteDefinitionNode extends SupramarkParentNode {
  type: 'footnote_definition';
  label: string;
  identifier: string;
}
```

要求：

- `label` 保留用户源码中的 label 文本。
- `identifier` 是规范化后用于引用匹配的 key：去首尾空白、内部连续空白折叠为单空格、转小写。ref 与 def 按 `identifier` 关联。
- AST 保持源码顺序；footnote definition 不在 parser 层移动到文末。
- renderer 可以自行收集 footnote definition 并在视觉上渲染到文末。
- 同一 `identifier` 多次定义时，parser 必须产生 diagnostic。

编号规则：

- AST v2 不在 parser 层写死用户可见脚注编号。
- 编号是 renderer/processor 根据引用出现顺序派生的展示数据。
- 如果宿主需要持久化编号，应放在外部 view model，不写回 AST。

### 6.6 Definition List

Definition list 使用显式子节点，避免 `term` / `descriptions` 这种非标准 child collection 破坏统一遍历。

```ts
interface DefinitionListNode extends SupramarkParentNode {
  type: 'definition_list';
  children: DefinitionItemNode[];
}

interface DefinitionItemNode extends SupramarkParentNode {
  type: 'definition_item';
  children: Array<DefinitionTermNode | DefinitionDescriptionNode>;
}

interface DefinitionTermNode extends SupramarkParentNode {
  type: 'definition_term';
  children: SupramarkInlineNode[];
}

interface DefinitionDescriptionNode extends SupramarkParentNode {
  type: 'definition_description';
  children: SupramarkBlockNode[];
}
```

要求：

- 一个 `definition_item` 必须至少有一个 `definition_term` 和一个 `definition_description`。
- 多个 term 或 description 必须按源码顺序保留。

### 6.7 Container

`container` 是所有 `:::` 块的统一承载节点。

```ts
type ExtensionMode = 'transparent' | 'opaque';

interface ContainerNode extends SupramarkParentNode {
  type: 'container';
  name: string;
  mode: ExtensionMode;
  children: SupramarkBlockNode[];
  params?: string;
  value?: string;
  data?: Record<string, unknown>;
}
```

语法：

```md
:::name params
content
:::
```

要求：

- `name` 必须是规范化小写名称。
- `params` 是 opening marker 中 name 后面的原始参数字符串。
- `mode: 'transparent'` 表示内部 Markdown 已解析到 `children`。
- `mode: 'opaque'` 表示内部内容不按普通 Markdown 解析，原始内部文本放入 `value`，`children` 为空数组。
- 未识别的 container name 仍然产生 `container` 节点，不应该降级为普通 paragraph。
- 具体 feature 通过 `name` 与 `data` 承载语义，例如 `name: 'map'`、`name: 'html'`、`name: 'warning'`。

名称规则：

- 内置名称使用 `[a-z][a-z0-9-]*`。
- 自定义名称也应该使用同一格式。
- parser 可以接受旧语法中的 `_`，但输出前应该规范化或给出 diagnostic。

#### 6.7.1 `:::map` 输出

`map` 不引入独立 `type: 'map'`。它是一个命名 container：

```ts
interface MapContainerNode extends ContainerNode {
  type: 'container';
  name: 'map';
  mode: 'opaque';
  children: [];
  value: string;
  data: {
    center?: [number, number];
    zoom?: number;
    markers?: Array<{
      lat: number;
      lng: number;
      label?: string;
      id?: string;
      data?: Record<string, unknown>;
    }>;
    meta?: Record<string, unknown>;
  };
}
```

输入：

```md
:::map
center: [34.05, -118.24]
zoom: 12
marker:
  lat: 34.05
  lng: -118.24
:::
```

输出：

```json
{
  "type": "container",
  "name": "map",
  "mode": "opaque",
  "value": "center: [34.05, -118.24]\nzoom: 12\nmarker:\n  lat: 34.05\n  lng: -118.24",
  "data": {
    "center": [34.05, -118.24],
    "zoom": 12,
    "markers": [{ "lat": 34.05, "lng": -118.24 }]
  },
  "children": [],
  "position": {
    "start": { "line": 1, "column": 1, "byte_offset": 0, "utf16_offset": 0 },
    "end": { "line": 7, "column": 1, "byte_offset": 82, "utf16_offset": 82 }
  }
}
```

要求：

- `value` 保留内部原文，不包含 opening/closing marker。
- `data` 是 feature 对 `value` 的结构化解析结果。
- `markers` 使用数组；单个旧式 `marker:` 输入也规范化为单元素 `markers`。
- 经纬度顺序固定为 `[lat, lng]`，与 marker 字段一致。
- 如果 `center` 缺省但存在第一个 marker，renderer 可以把第一个 marker 作为视觉中心；parser 不应该伪造 `center`，除非 feature 文档明确声明默认策略。

### 6.8 Input

`input` 是所有 `%%%` 块的统一承载节点。

```ts
interface InputNode extends SupramarkParentNode {
  type: 'input';
  name: string;
  mode: ExtensionMode;
  children: SupramarkBlockNode[];
  params?: string;
  value?: string;
  data?: Record<string, unknown>;
}
```

语法：

```md
%%%name params
content
%%%
```

要求：

- 默认 `input` 是 `opaque`，因为表单/调查/配置类内容通常有自己的解析规则。
- `value` 保存 opening/closing marker 之间的原始内部文本。
- 如果某个 input feature 明确声明透明解析，可以使用 `mode: 'transparent'` 并填充 `children`。

### 6.9 Raw

`raw` 表示合法源码片段被保留，但 Supramark 不在 AST v2 中赋予更细语义。

```ts
interface RawNode extends SupramarkBaseNode {
  type: 'raw';
  format: string;
  value: string;
  block: boolean;
}
```

用途：

- HTML block / inline HTML。
- 未来可选 MDX、directive、frontmatter 等未标准化语法。
- parser 能识别边界但不应该强行语义化的内容。

要求：

- renderer 默认必须转义或跳过 `raw`。
- 只有宿主显式启用并承担安全策略时，才能把 `format: 'html'` 作为 HTML 渲染。

### 6.10 Unsupported

`unsupported` 表示 parser 知道这里有一个结构，但当前 AST v2 尚不能表达其完整语义。

```ts
interface UnsupportedNode extends SupramarkParentNode {
  type: 'unsupported';
  syntax: string;
  reason: string;
  value?: string;
  children: SupramarkNode[];
}
```

要求：

- `unsupported` 必须带 diagnostic。
- 如果能解析出安全的子结构，放入 `children`。
- 如果不能解析，保留 `value`。
- renderer 默认渲染 `children`；没有 children 时以纯文本/code fallback 渲染。

## 7. 子节点约束

### 7.1 Block-level children

以下节点的 `children` 是 block-level：

- `root`
- `blockquote`
- `list_item`
- `definition_description`
- transparent `container`
- transparent `input`

### 7.2 Inline-level children

以下节点的 `children` 是 inline-level：

- `paragraph`
- `heading`
- `strong`
- `emphasis`
- `delete`
- `link`
- `table_cell`
- `definition_term`

### 7.3 特殊结构 children

- `list.children` 只能是 `list_item`。
- `table.children` 只能是 `table_row`。
- `table_row.children` 只能是 `table_cell`。
- `definition_list.children` 只能是 `definition_item`。
- `definition_item.children` 只能是 `definition_term` 或 `definition_description`。

## 8. Source Map 合同

source map 是 AST v2 的核心，不是附加功能。

### 8.1 必须保证的性质

- 每个源码派生节点必须有 `position`。
- parent range 必须覆盖所有 child ranges。
- siblings 必须按 `position.start` 非递减排序。
- node range 使用半开区间 `[start, end)`。
- `byte_offset` 与 `utf16_offset` 是锚点用字段；`line` / `column` 是展示用字段。
- 对同一 source，所有 position 必须落在 `[0, source.length]` 内。

### 8.2 标记范围

结构节点的范围包含 Markdown 标记：

- heading 包含 `#` 或 setext underline。
- strong/emphasis/delete 包含两侧 delimiter。
- link 包含 `[label](url)` 全部源码。
- list item 包含 marker。
- code/diagram/math block 包含 opening/closing fence 或 delimiter。
- container/input 包含 opening/closing marker。

子节点范围只覆盖自己的源码片段。

### 8.3 转义与实体

`value` 是语义值，不是 raw source。

示例：

```md
\* &amp;
```

对应 text 的 `value` 可以是：

```txt
* &
```

但 `position` 仍然覆盖原始源码中的 `\* &amp;` 范围。

### 8.4 CRLF 与换行

- parser 不应该为了构建 AST 改写原始 source。
- `byte_offset` / `utf16_offset` 必须基于原始 source。
- CRLF 在 offset 中占两个 bytes / 两个 UTF-16 code units。
- `line` 应把 CRLF 视为一个换行展示单元。

## 9. 协作与批注模型

AST v2 支持协作/批注，但不存储协作/批注状态。

### 9.0 AST 需要承担什么

协作/批注语义需要 AST 有所考虑，但不能把 AST 变成批注数据库。

AST 必须承担：

- 为源码派生节点提供精确 `position`。
- 保留合法但暂不支持的源码结构，避免批注锚点落到“被 parser 吞掉”的区域。
- 让宿主可以从 source range 找到语义节点，例如“这段选区属于 paragraph / table_cell / math_inline”。
- 区分 source-derived 节点和 generated/derived 节点：源码派生节点必须有 position，纯生成节点不能伪造精确 position。
- 保持 children 源码顺序，方便构造临时 node path。

AST 不应该承担：

- 存储 comment/thread/reaction/resolution 状态。
- 存储 CRDT relative position。
- 承诺节点 ID 跨编辑稳定。
- 记录用户、时间、权限、已读状态等协作元数据。

换句话说：AST v2 是批注系统的“语义索引”和“source map”，不是批注系统本身。

### 9.1 批注锚点应该存在哪里

批注系统应该把主锚点存到 CRDT 文档层，例如：

```ts
interface CommentAnchor {
  start: unknown; // CRDT relative position
  end: unknown;   // CRDT relative position
  quote?: string;
  snapshot?: {
    utf16_start: number;
    utf16_end: number;
    byte_start?: number;
    byte_end?: number;
    node_path?: number[];
  };
}
```

AST 提供的是把当前 source range 映射到语义节点的能力。

### 9.2 为什么 AST 不承诺稳定 node id

Markdown AST 的节点边界会随着编辑改变：

- 一行文字加 `#` 会从 paragraph 变成 heading。
- 插入一个空行会改变列表/段落结构。
- 改一个 delimiter 会让 inline tree 重组。

因此 AST v2 不内置“跨编辑稳定 node id”。如果宿主要缓存渲染结果，可以在 AST 外部基于 `type + position + content hash` 派生临时 key；这个 key 不能作为批注持久锚点。

### 9.3 AST 查询职责

协作/批注系统通常需要这些派生能力：

- 给定 UTF-16 range，找到最小覆盖节点。
- 给定 point，找到所在 inline text/code/math 节点。
- 给定 range，提取跨节点 selection fragments。
- 给定节点，回到原始 source slice。

这些应该作为 helper 构建在 AST v2 之上，不改变 AST 数据结构。

## 10. 扩展模型

### 10.1 不暴露 parser plugin API

AST v2 的扩展策略不是把 rule / token / internal node 暴露给宿主。parser core 可以内部拥有扩展 rule，但 public contract 仍然是：

```ts
source -> AST v2
```

### 10.2 有限扩展承载点

扩展优先使用已有承载点：

- fenced diagram：`diagram`
- `:::`：`container`
- `%%%`：`input`
- raw syntax：`raw`
- 暂不支持语义：`unsupported`
- 渲染/处理元信息：`data`

只有当一个语法具有稳定、跨平台、长期存在的语义时，才加入新的 core node type。

### 10.3 data 字段

`data` 是扩展自定义 JSON，但有边界：

- core parser 不解释未知 `data` key。
- renderer 不应该依赖未声明 feature 的 `data` key。
- feature 应该在自己的文档中声明 `data` schema。
- 不得把大体积二进制、渲染结果、宿主对象塞进 `data`。

## 11. Renderer 合同

Renderer 消费 AST v2，不调用 parser 内部 API。

要求：

- Renderer 必须能安全跳过未知或 unsupported 节点。
- `container` / `input` 的具体渲染由 `name` 匹配 feature。
- `diagram` 的具体渲染由 `engine` 匹配 renderer capability。
- `raw` 默认不作为 HTML 执行。
- source position 不应该默认渲染到 UI；调试模式可以展示。

推荐 fallback：

| 节点 | fallback |
| --- | --- |
| unknown `container` transparent | 渲染 children |
| unknown `container` opaque | 渲染为 code block 或安全占位 |
| unknown `input` | 渲染安全占位 |
| unknown `diagram.engine` | 渲染 code block，显示 engine |
| `raw` | escape 后纯文本或跳过 |
| `unsupported` | children fallback，否则纯文本 |

## 12. 安全策略

- parser 保存 source 语义，不负责信任 source。
- renderer 对 URL、HTML、raw、diagram engine 输入必须有安全策略。
- `link.url` / `image.url` 必须保留源码语义；是否允许 `javascript:`、`data:` 等由 renderer/host policy 决定。
- `raw.format === 'html'` 不等于可以执行 HTML。
- diagram/math 渲染应该运行在受控 engine 中，不能由 AST 直接触发任意代码执行。

## 13. 与 mdast 的关系

Supramark AST v2 借鉴 mdast，但不是 mdast 的别名。

保持一致的地方：

- CommonMark 基础节点语义。
- GFM table/delete/task list 概念。
- math/footnote 可与 mdast 生态互相映射。

主动不同的地方：

- `type` 使用 snake_case。
- source map 同时要求 byte offset 与 UTF-16 offset。
- `diagram` 是一等节点，不只是 code fence。
- `container` / `input` 是 Supramark 扩展承载点。
- definition list 使用显式 `definition_term` / `definition_description` 子节点。
- parser 不移动 footnote definition；视觉重排由 renderer/processor 做。
- recovery 使用 `raw` / `unsupported`，避免静默丢语义。

## 14. 完整节点类型清单

Block-level：

- `root`
- `paragraph`
- `heading`
- `code`
- `diagram`
- `math_block`
- `list`
- `list_item`
- `blockquote`
- `thematic_break`
- `table`
- `table_row`
- `table_cell`
- `footnote_definition`
- `definition_list`
- `definition_item`
- `definition_term`
- `definition_description`
- `container`
- `input`
- `raw`
- `unsupported`

Inline-level：

- `text`
- `strong`
- `emphasis`
- `inline_code`
- `math_inline`
- `link`
- `image`
- `break`
- `delete`
- `footnote_reference`
- `raw`
- `unsupported`

`raw` 与 `unsupported` 可以是 block 或 inline，通过 `block` 字段或上下文判断。

## 15. Conformance 测试要求

实现 AST v2 的 parser 至少需要这些测试：

### 15.1 Public API

- crate/package 只暴露 `parse` 与 AST v2 类型。
- 不暴露 rule、token、internal node、plugin registry。

### 15.2 JSON shape

- 所有节点 `type` 为 snake_case。
- `root.ast_version === 2`。
- `root.diagnostics` 存在。
- 可选字段按规则省略。

### 15.3 Source map

必须覆盖：

- ASCII。
- 中文。
- emoji / surrogate pair。
- CRLF。
- escaped characters。
- entities。
- block + inline 嵌套。
- table cell。
- container/input。

### 15.4 Syntax coverage

必须覆盖：

- CommonMark core。
- GFM table/delete/task list。
- diagram fence。
- math inline/block。
- footnote reference/definition。
- definition list。
- transparent/opaque container。
- opaque input。
- raw HTML。
- unsupported recovery。

### 15.5 Renderer fallback

必须覆盖：

- unknown diagram engine。
- unknown container name。
- raw HTML disabled。
- unsupported node。

## 16. 当前实现状态

截至当前 Rust `crates/supramark-markdown` MVP：

| 能力 | 状态 |
| --- | --- |
| 单一 public `parse` | 已做 |
| parser internals 内部化 | 已做 |
| CommonMark 基础映射 | 基础已做 |
| byte offset + UTF-16 offset | 已做基础版 |
| GFM table | 已做基础版 |
| GFM delete | 已做基础版 |
| diagram fence | 已做基础版 |
| `root.ast_version` / `diagnostics` | Rust 已做；TS parser facade 已对齐 |
| 可选字段省略策略 | Rust 已做；TS 类型已对齐 |
| task list `checked` | Rust 已做基础版 |
| math | Rust 已做 block/inline 基础版 |
| footnote | Rust 已做 reference/definition 基础版 |
| definition list v2 结构 | Rust/TS/Web/RN 已做基础版 |
| container/input parser | Rust 已做 top-level opaque 基础版 |
| `:::map` canonical output | Rust/TS 已做基础版，旧 `marker:` 输入规范化为 `data.markers[]` |
| `:::html` / `:::vison` data | Rust 已做基础版 |
| raw/unsupported recovery | Rust 已做基础版 |
| CRLF source map conformance | 未做完整测试 |

这个表不是规范的一部分，只是迁移清单。规范以本文档前文为准。

## 17. 迁移原则

从旧 TS AST / legacy parser pipeline 迁到 AST v2 时：

1. 先让 Rust `supramark-markdown` 输出符合本文档的 AST v2 JSON。
2. 再让 `@supramark/core` 的 TS 类型与本文档对齐。
3. Web/RN renderer 只消费 AST v2，不消费 parser token。
4. 旧 feature 的 parser hook 逐步改造成 Rust parser rule 或 AST post-process。
5. 每迁移一个节点类型，补齐 source map + JSON shape + renderer fallback 测试。

## 18. 最小合格 AST v2 示例

Markdown：

```md
# 标题

Hello **世界**.
```

AST：

```json
{
  "type": "root",
  "ast_version": 2,
  "diagnostics": [],
  "position": {
    "start": { "line": 1, "column": 1, "byte_offset": 0, "utf16_offset": 0 },
    "end": { "line": 4, "column": 1, "byte_offset": 27, "utf16_offset": 23 }
  },
  "children": [
    {
      "type": "heading",
      "depth": 1,
      "position": {
        "start": { "line": 1, "column": 1, "byte_offset": 0, "utf16_offset": 0 },
        "end": { "line": 1, "column": 5, "byte_offset": 8, "utf16_offset": 4 }
      },
      "children": [{ "type": "text", "value": "标题" }]
    },
    {
      "type": "paragraph",
      "children": [
        { "type": "text", "value": "Hello " },
        {
          "type": "strong",
          "children": [{ "type": "text", "value": "世界" }]
        },
        { "type": "text", "value": "." }
      ]
    }
  ]
}
```

示例中省略了部分子节点 `position` 以便阅读；真实 AST v2 输出必须为源码派生节点填充。
