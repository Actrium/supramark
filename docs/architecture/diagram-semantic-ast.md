# Diagram 语义 AST 与统一引擎接口设计

> 状态：设计草案（仅设计，未实施）。
> 关联文档：`DIAGRAM_ENGINE_TARGET.md`、`ENGINES_AND_CLI_PLAN.md`、`ast-spec.md`。
> 关键区分：上述两份文档讨论的是 **TS 侧 `@supramark/engines`** 的
> `source -> svg` 渲染收口；本文讨论的是 **Rust 侧四个引擎 crate** 如何在
> `source -> svg` 之外，再额外暴露一层 **结构化语义（semantic AST）**，
> 供 diff / 查询 / 下游 Markon 消费。两者是不同层、互不冲突。

---

## 0. 决策记录（Decisions）

以下三项已拍板，本文设计以此为准；§7 中第 1、3、6 条已据此收敛。

1. **EngineAst 形态**：内部强类型 + 对外统一 `{ engine, kind, data }` JSON 契约。各引擎内部各用其强类型语义结构；跨边界（AST v2 序列化 / 提供给 TS）统一序列化为 `{ engine, kind, data }` 信封（见 §3、§4）。
2. **serde 补法**：在各引擎 crate 内加 `serde` feature，对内部语义模型 feature-gated `#[derive(Serialize)]`（§7-1 选项 a）；接受对引擎 crate 的轻度侵入。
3. **d2 语义源**：采用 `graph::Graph`（含布局后几何）+ 一步「语义投影」剔除 `top_left/width/height/box_` 等布局坐标，只留布局无关子集（id/label/shape/style/parent/children/edges），避免坐标微变产生伪 diff（§7-3）。

仍待定：trait crate 依赖方向（§7-7）、语义解析缓存（§7-5）、graphviz 路线 spike（§7-8）、版本漂移契约快照（§7-4）。

## 1. 目标与非目标

### 1.1 背景（务必先读）

四个引擎 crate 当前的对外能力都是「源码进、SVG 出」：

- `mermaid-little`：`convert_with_id(source, id) -> Result<String>`（SVG 文本）。
- `plantuml-little`：`convert(source) -> Result<String>`（SVG 文本）。
- `d2-little`：`d2_to_svg(input) -> Result<Vec<u8>>`（SVG 字节），并且
  **已额外公开** `parse(input) -> Result<ast::Map>` 与
  `compile(...)`（产出语义图 `graph::Graph`）。
- `graphviz-anywhere`：`GraphvizContext::render(dot, engine, format) -> Result<Vec<u8>>` /
  `render_to_string(...)`，底层是 **C 库 FFI**，DOT 解析发生在 C 侧。

`supramark-markdown` 的 AST v2 里，图表块被映射为：

```rust
SupramarkNode::Diagram {
    engine: String,                  // "mermaid" / "plantuml" / "d2" / "dot" / ...
    code: String,                    // 原始源码，未做任何结构化
    meta: Option<serde_json::Value>, // 围栏 info 字符串里解析出的 key=value
    position: Option<SourcePosition>,
}
```

即下游拿到的只有「引擎名 + 原始源码」，没有任何节点 / 边 / 属性级别的语义结构。
因此下游 Markon 对图表只能走 **source fallback**：比对原始源码字符串，
源码变了就整块当作「变更」，无法做「这条边的 label 改了」这种结构化 diff。

### 1.2 目标

1. **结构化 diff**：让下游能在「节点 / 边 / 属性」粒度比较两个版本的图，
   而不是整块源码字符串比较。
2. **语义查询**：让下游能问「这张图有哪些节点」「A 到 B 有没有边」
   「某节点的 label / shape / class 是什么」，无需自己再写一遍解析器。
3. **统一入口**：把四个引擎收敛到一个共同的 Rust trait 后面，让 Markon /
   CLI / 其他消费方按统一形态调用 `render` 与 `semantic`，新增引擎不波及调用方。

### 1.3 非目标

- **不统一各引擎的语义模型**。Mermaid 的 ER、PlantUML 的时序、D2 的盒子图、
  Graphviz 的有向图，语义结构天然不同，本设计 **不强行抹平成同一套节点/边模型**。
- **不追求跨引擎语义可比**。「Mermaid 流程图」与「Graphviz DOT」即使画的是同一张图，
  也不保证语义 AST 能互相 diff，diff 只在 **同引擎、同图类型** 内有意义。
- **不改变 SVG 渲染的字节级 parity 目标**。语义 AST 是旁路新增能力，
  不触碰既有 `convert / render` 的输出。
- **不在第一阶段引入懒解析以外的缓存 / 持久化机制**。

---

## 2. 各引擎成熟度调研（带证据）

### 2.1 d2-little —— 现成（半个语义 AST 已公开）

- 顶层 API：
  - `d2_to_svg(input: &str) -> Result<Vec<u8>, String>`（`src/lib.rs:1512`）。
  - `parse(input: &str) -> Result<ast::Map, String>`（`src/lib.rs:143`）——
    语法 AST，含 `Range`/`Position` 源码定位（`src/ast.rs:15,83`）。
  - `compile(...)`（`src/lib.rs:160`）——产出语义图 `graph::Graph`。
- 结构化中间表示：`graph::Graph`（`src/graph/mod.rs:1613`），承载
  `Object`（节点，`:491`，含 id/abs_id/label/shape/style/parent/children）、
  `Edge`（边，`:1429`）、`Style`、`Label`、`GraphLegend` 等，是 **完整语义**
  （节点 + 边 + 属性 + 层级）。`pipeline: source -> AST -> IR -> Graph`
  在 `src/lib.rs` 头部注释中明确写出。
- 复用度：`source -> 结构` 解析步骤 **已存在且已 pub**。
- 结论：**现成可做**。只需在 `Graph`（语义）或 `ast::Map`（语法）之上包一层
  trait 即可。唯一缺口：`Graph` 未 derive `Serialize`（见 §7）。

### 2.2 mermaid-little —— 需少量包装（每图类型已有 typed 模型 + pub parser）

- 顶层 API：`convert_with_id(source, id) -> Result<String, MermaidError>`
  （`src/lib.rs`）。
- 结构化中间表示：`pub mod model` + `pub mod parser`。每个图类型一个
  `parse(source) -> Result<XxxDiagram>`（如 `parser/er.rs:19`、
  `parser/flowchart.rs:22`、`parser/sequence.rs:105`、`parser/class.rs:44`
  等 **20+ 个**），对应 `model/*.rs` 里的纯数据结构（如 `ErDiagram`：
  entities/relationships/classes/direction，`model/er.rs:135`；
  字段全 `pub`，承载完整语义）。`model/mod.rs` 注释明确「plain-data struct —
  no logic, no rendering」。图类型由 `detect::DiagramKind`
  （`src/detect.rs:36`，约 27 个变体）识别。
- 复用度：`source -> 结构` 已存在、已 pub、按图类型分派。
- 缺口：没有统一的「`source -> 某个聚合枚举`」单入口（`convert_with_id` 内部
  `match kind` 后各走各的 parser，但未把结果聚合成一个对外枚举）；
  model 未 derive `Serialize`。
- 结论：**需少量包装**。包一个 `MermaidAst` 聚合枚举（按 `DiagramKind` 分派到
  各 `XxxDiagram`），并补 serde derive。

### 2.3 plantuml-little —— 需少量包装（已有统一 Diagram 枚举 + 统一 parse 入口）

- 顶层 API：`convert(source) -> Result<String>`（`src/lib.rs`），及
  `convert_with_base_dir` / `convert_with_input_path` 变体。
- 结构化中间表示：`pub mod parser` + `pub mod model`。**已有统一入口**
  `parser::parse(source) -> Result<Diagram>`（`src/parser/mod.rs:40`）和
  `parse_with_original`（`:44`）；返回的 `model::diagram::Diagram` 是一个
  **34 变体的统一枚举**（`src/model/diagram.rs:111`：Sequence/Class/Activity/
  State/Component/Gantt/Mindmap/...），每个变体包对应 typed 子模型。
- 复用度：`source -> 统一枚举` 已存在、已 pub，比 mermaid 还现成（已自带聚合枚举）。
- 缺口：仅 serde derive 缺失；部分子模型语义完整度需逐一核（PlantUML 图种类极多，
  某些可能偏渲染导向）。
- 结论：**需少量包装**（接近现成）。直接复用 `parser::parse -> Diagram`，
  补 serde 即可对外。

### 2.4 graphviz-anywhere —— 基本只有渲染（无 Rust 侧语义结构）

- 顶层 API（`packages/rust/src/lib.rs`）：`GraphvizContext::render(dot, Engine, Format)
  -> Result<Vec<u8>>`（`:246/293`）、`render_to_string(...)`（`:306`）、
  `Engine` 枚举（dot/neato/fdp/...，`:108`）、`Format`、`GraphvizError`（`:42`）。
- 结构化中间表示：**无**。DOT 解析与布局都在被 FFI 包裹的 **C 库** 内部
  （`mod ffi`，`:31`；wasm 走 `pub mod wasm` 委托 JS）。Rust 侧没有任何
  DOT AST / 图模型类型，`render` 直接 source -> SVG 字节。
- 复用度：**无可复用的 source -> 结构 步骤**。
- 结论：**需大改 / 另起炉灶**。要么在 Rust 侧写一个独立的 DOT 解析器
  （DOT 语法相对简单，节点 + 边 + 属性表，工作量中等但确实是新代码），
  要么让 C 侧额外导出解析结构（改 C ABI，成本更高）。短期可只实现
  `render`、`semantic` 返回 `None`。

> 现状交叉印证：`supramark-markdown` 的 `diagram_engine()`（`supramark.rs:954`）
> 把 `dot`/`graphviz` 都归一到图表块；`ENGINES_AND_CLI_PLAN.md` §1 也提到
> renderer 层对未接入引擎「fallback 为原样代码块——默默失败」，与本文要解决的
> 「source fallback」是同一痛点的两个层面。

---

## 3. 统一接口设计

### 3.1 核心 trait（Rust 伪代码）

放在一个新的轻量 crate（建议 `crates/diagram-engine` 或
`crates/supramark-diagram`）里，只定义 trait 与通用类型，**不反向依赖** 四个引擎；
各引擎 crate 反过来 impl 这个 trait（或由新 crate 以 adapter 形式 impl，避免给
引擎 crate 增加依赖）。

```rust
/// 渲染输出：统一成字节，SVG 文本用 UTF-8 字节装；预留未来 png 等。
pub struct RenderOutput {
    pub mime: &'static str,   // "image/svg+xml"
    pub bytes: Vec<u8>,
}

/// 统一错误：包引擎名 + 离散错误类别 + 原始错误。
#[derive(Debug, thiserror::Error)]
pub enum DiagramError {
    #[error("{engine}: parse failed: {message}")]
    Parse { engine: &'static str, message: String },
    #[error("{engine}: render failed: {message}")]
    Render { engine: &'static str, message: String },
    #[error("{engine}: semantic AST not supported")]
    SemanticUnsupported { engine: &'static str },
}

/// 一个 diagram 引擎的统一形态。
pub trait DiagramEngine {
    /// 引擎稳定标识，对齐 markdown 的 `engine` 字段（"mermaid"/"plantuml"/"d2"/"graphviz"）。
    fn id(&self) -> &'static str;

    /// source -> 渲染产物（既有能力的薄封装，永远要实现）。
    fn render(&self, source: &str) -> Result<RenderOutput, DiagramError>;

    /// source -> 语义 AST。
    /// 返回 None = 该引擎/该图类型当前不支持语义（如 graphviz 阶段一）。
    /// 返回 Err = 支持但解析失败（语法错误）。
    fn semantic(&self, source: &str) -> Result<Option<EngineAst>, DiagramError> {
        let _ = source;
        Ok(None) // 默认不支持，新引擎可零成本接入 render-only
    }
}
```

### 3.2 `EngineAst` 的两种设计取舍

**核心问题**：`semantic()` 返回的东西，是「每引擎各自的语义类型」还是
「一个带 engine 标签的通用枚举」？

#### 方案 A：带 engine 标签的通用枚举（推荐）

```rust
/// 顶层 newtype + 标签，按引擎分派到各自原生语义类型。
#[derive(serde::Serialize)]
#[serde(tag = "engine", rename_all = "lowercase")]
pub enum EngineAst {
    Mermaid(MermaidAst),     // 再按 DiagramKind 分派
    Plantuml(PlantumlAst),   // 复用 plantuml model::diagram::Diagram
    D2(D2Ast),               // 包 d2 graph::Graph 或 ast::Map
    // Graphviz(GraphvizAst), // 阶段四
}
```

- 优点：调用方（Markon）拿到 **单一类型**，`match` 一层就知道是哪个引擎，
  序列化到 TS 时有稳定的 `engine` discriminant，AST v2 的
  `Diagram { engine, .. }` 天然对应。开闭：加引擎 = 加变体。
- 缺点：这个枚举落在 trait crate 里，会让 trait crate **依赖全部四个引擎 crate**
  （编译期耦合、编译变重）。缓解：用 feature gate，每个引擎一个 feature，
  下游按需开启；或各变体内放 `serde_json::Value`（牺牲类型，见方案 C）。

#### 方案 B：每引擎各自语义类型（trait 关联类型）

```rust
pub trait DiagramEngine {
    type Ast: serde::Serialize;
    fn semantic(&self, source: &str) -> Result<Option<Self::Ast>, DiagramError>;
}
```

- 优点：trait crate **零引擎依赖**，每个引擎只暴露自己的类型，最干净、编译最轻。
- 缺点：`DiagramEngine` 不再是 **object-safe**（关联类型 + 泛型返回），
  无法 `Box<dyn DiagramEngine>` 放进一个注册表里统一调度，调用方要静态知道引擎类型。
  与「统一入口、运行时按 engine 字符串分派」的诉求冲突。

#### 方案 C：通用枚举但 payload 用 `serde_json::Value`

`EngineAst { engine: String, kind: String, data: serde_json::Value }`。

- 优点：trait crate 零引擎依赖且 object-safe；序列化到 TS 极简单。
- 缺点：Rust 侧丢失类型安全，diff / 查询逻辑只能在 JSON 上做（但其实下游 Markon
  / TS 本来就在 JSON 上做 diff，这个缺点对 **跨语言契约** 反而不是问题）。

**推荐**：**方案 A（强类型枚举）+ feature gate** 作为 Rust 内部强类型表示，
**对外序列化统一走方案 C 的 JSON 形状**（即 `EngineAst` 序列化后就是
`{ engine, kind, data }`）。这样：Rust 内部消费方享受类型安全，
TS / Markon 侧拿到稳定 JSON 契约，trait 通过「返回 `EngineAst` 枚举 +
`semantic` 在 trait 层仍 object-safe（不用关联类型）」保持可注册、可动态分派。

### 3.3 注册与分派

```rust
pub struct DiagramRegistry { /* HashMap<&str, Box<dyn DiagramEngine>> */ }
impl DiagramRegistry {
    pub fn get(&self, engine_id: &str) -> Option<&dyn DiagramEngine>;
}
```

`supramark-markdown` 的 `engine` 字段（"mermaid"/"plantuml"/"d2"/"dot"/"graphviz"）
直接作为注册表 key。注意现状里 `dot` 与 `graphviz` 是两个 key（`supramark.rs:954`），
需统一映射到同一个 graphviz 引擎实例。

---

## 4. AST v2 集成

### 4.1 现状

`SupramarkNode::Diagram { engine, code, meta, position }`
（`supramark-markdown/src/supramark.rs:139`），整个 `SupramarkNode` 已
`#[derive(Serialize, Deserialize)]` 且用 `#[serde(tag = "type")]`（`:62-63`），
`Diagram` 节点已能序列化为 v2 JSON 给 TS。

### 4.2 设计：新增可选 `semantic` 字段 + 懒解析

```rust
Diagram {
    engine: String,
    code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<serde_json::Value>,
    /// 新增：语义 AST。None = 未解析或不支持。
    /// 始终序列化为 { engine, kind, data } 形状（见 §3.2 方案 C 契约）。
    #[serde(skip_serializing_if = "Option::is_none")]
    semantic: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<SourcePosition>,
}
```

关键点：

- **保留 `code` 字段**。语义 AST 是 **附加**，不是替代——graphviz 阶段一没有语义，
  仍靠 `code`；diff 失败时也要能回退到 source fallback。`semantic` 与
  `{engine, code, meta}` 是「同一节点的两种视图」，`code` 永远是事实源。
- **懒解析（lazy）**。parser 主流程 **默认不** 调用引擎做语义解析（解析一张大图可能
  很贵，见 §7 性能）。提供两种触发方式：
  1. parser 配置项 `parse_diagram_semantics: bool`（默认 false），开启后在 mapping
     阶段调用 `DiagramRegistry::get(engine).semantic(code)` 填充字段；
  2. 或保持 AST 轻量，由下游（Markon）在需要 diff 时才按 `engine + code` 现场解析。
     **推荐默认走 (2)**：AST v2 不内嵌 semantic，Markon 真正要做结构 diff 时才解析，
     避免给所有消费方（哪怕只渲染）付解析成本。`semantic` 字段保留为「(1) 模式的可选产物」。
- **序列化契约**：`semantic` 一律是 `{ "engine": "...", "kind": "...", "data": {...} }`。
  TS 侧只依赖 `engine`/`kind` 做分派，`data` 的形状随引擎/版本演进（见 §7 版本漂移）。

### 4.3 与 `ast-spec.md` 的衔接

需在 `ast-spec.md` 增补 `Diagram` 节点的 `semantic` 可选字段定义与
`{engine,kind,data}` JSON 形状（本设计落地时同步更新，避免契约漂移）。

---

## 5. 分阶段路线（按成熟度排序）

| 阶段 | 范围 | 风险 | 可独立交付点 |
|---|---|---|---|
| **0. 接口骨架** | 新建 trait crate：`DiagramEngine` trait、`EngineAst`/`DiagramError`/`RenderOutput` 类型、`DiagramRegistry`。四引擎各 impl `render`（薄封装既有 `convert/d2_to_svg/render`），`semantic` 全返回 `None`。 | 低。纯新增，不碰渲染。 | 统一 `render` 入口可用；Markon 可改走 registry 调 render（行为不变）。 |
| **1. d2 语义** | impl `D2::semantic`：复用已 pub 的 `d2::parse`/`compile`，把 `graph::Graph`（或 `ast::Map`）包成 `D2Ast`；补 serde（见 §7）。 | 低-中。`Graph` 字段多、含布局后坐标，需筛「语义子集」（节点/边/label/shape，剔除 top_left/width 等几何）。 | d2 图可结构化查询 + diff（最先落地的真实价值）。 |
| **2. mermaid 语义** | impl `Mermaid::semantic`：按 `DiagramKind` 分派到各 `parser::xxx::parse`，聚合成 `MermaidAst` 枚举；补 serde；逐图类型铺开（er/flowchart/sequence/class 优先）。 | 中。图类型多（27），需逐一确认 model 语义完整度并加 serde；可分图类型增量交付。 | 主流 mermaid 图（er/flow/seq/class）可 diff。 |
| **3. plantuml 语义** | impl `Plantuml::semantic`：复用 `parser::parse -> model::diagram::Diagram`（已是聚合枚举）；补 serde；按子图类型确认语义完整度。 | 中。34 个变体，部分子模型可能偏渲染；serde 量大。 | 主流 plantuml 图（sequence/class/activity）可 diff。 |
| **4. graphviz 语义** | 在 Rust 侧实现 DOT 解析器产出图模型；或推动 C 侧导出结构。 | 高。新代码 / 改 C ABI；DOT 属性继承语义繁琐。 | DOT 图可 diff（最后做，前期 `semantic=None` 不阻塞）。 |

> 排序依据：d2（解析已 pub 且有完整 Graph）→ plantuml（已有统一 parse+聚合枚举，
> 仅缺 serde，工程量其实可能小于 mermaid）→ mermaid（typed model 齐全但需聚合层 +
> 逐类型 serde）→ graphviz（零基础）。
> 说明：原预估「plantuml 难于 mermaid」，但调研发现 plantuml 已自带统一 `parse -> Diagram`
> 聚合枚举入口，mermaid 反而要自己写聚合层——**阶段 2 与 3 的先后可在落地时按 serde
> 工作量再定**，二者都属「需少量包装」。

每个阶段都 **可独立 revert / 独立交付**：trait crate 在阶段 0 稳定后，后续阶段只是
把对应引擎的 `semantic` 从 `None` 替换为真实实现，不影响其他引擎与渲染路径。

---

## 6. 对 Markon 的影响

### 6.1 现状

Markon 对图表走 **source fallback**：拿 `Diagram.code` 字符串整块比对，
源码任意改动（哪怕只是空格 / 重排）都视为「整图变更」，无法定位到具体节点/边。

### 6.2 升级路径

- **阶段 0 之后**：Markon 改用 `DiagramRegistry` 调 `render`，统一入口，行为不变
  （仍 source fallback）。这是无风险的前置整理。
- **阶段 1（d2）之后**：Markon 对 `engine == "d2"` 的图：两版 `code` 都解析成语义 AST
  → 做结构 diff（节点增删、边增删、label/shape 改动）。解析失败或
  `semantic == None` → 自动回退 source fallback。
- **阶段 2/3 之后**：mermaid / plantuml 的已支持图类型同样升级；未支持的图类型继续
  source fallback。
- **graphviz**：阶段 4 前一直 source fallback。

### 6.3 过渡策略（关键）

- **能力探测而非假设**：Markon 对每个图都先尝试 `semantic`，`None`/`Err` 即回退，
  **永不假定** 某引擎一定有语义。这样阶段推进时 Markon 无需逐版本改判断逻辑。
- **diff 结果分层**：结构 diff 命中时给「节点级」变更摘要；回退时给「整块变更」。
  UI / 输出格式需同时支持两种粒度。
- **契约版本**：`semantic` JSON 带 `kind`（以及未来可加 `schema_version`），
  Markon 按 `engine + kind` 选 diff 策略，未知 kind 回退。

---

## 7. 风险与开放问题

1. **serde 缺失（必须解决的工程前提）**：调研确认 mermaid `model/*`、
   plantuml `model/*`、d2 `graph`/`ast` **均未 derive `Serialize`**。
   要对外 / 跨语言，必须补。选项：(a) 在各引擎 crate 加 `serde` feature +
   `#[derive(Serialize)]`（最直接，但给引擎 crate 增依赖）；
   (b) 在 trait crate 写 `From<XxxDiagram> for serde_json::Value` 的手工映射
   （引擎 crate 零侵入，但映射代码量大、易漂移）。**已决：(a) 各引擎 crate 加 `serde` feature + feature-gated derive（见 §0 决策记录）。**

2. **引擎语义模型差异**：四套模型互不相同，`EngineAst` 只能做「带标签的并集」，
   **跨引擎 diff 无意义**，须在文档与 API 注释中讲清，避免下游误用。

3. **d2 Graph 含布局产物**：`graph::Object` 里混了 `top_left/width/height/box_`
   等 **布局后几何**（`graph/mod.rs:491+`）。语义 AST 应只取「布局无关子集」
   （id/label/shape/style/parent/children/edges），否则同一图换字体/版本会因坐标
   微变产生伪 diff。需定义清晰的「语义投影」。是否改用 `ast::Map`（纯语法、无几何）
   作为 d2 语义源 —— **已决：采用 `graph::Graph` + 「语义投影」剔布局坐标，不改用 `ast::Map`（见 §0 决策记录）。**

4. **版本漂移**：引擎升级（mermaid 跟 upstream、d2 跟 Go）可能改 model 字段，
   导致 `semantic` JSON 形状变化，破坏 TS 契约。缓解：`schema_version` 字段 +
   契约快照测试（序列化结果纳入测试 fixture）。

5. **性能（懒解析的理由）**：语义解析对大图可能不便宜（mermaid/plantuml/d2 都要跑
   完整 parser）。若 parser 主流程对每个 diagram 都同步解析，会拖慢「只渲染不 diff」
   的常见场景。故默认 **不在 AST v2 内嵌 semantic**，由 Markon 按需解析（§4.2）。
   开放问题：是否需要解析结果缓存（按 `code` 内容 hash），以及缓存放在哪一层。

6. **object-safety vs 类型安全的最终取舍**（§3.2）：方案 A+C 混合需确认
   `EngineAst` 枚举 + feature gate 的编译耦合可接受；若引擎 crate 编译成本敏感，
   可能要退到方案 C（纯 JSON payload）。**已决：内部强类型（方案 A）+ 对外统一 JSON 契约（方案 C）（见 §0 决策记录）。**

7. **trait crate 依赖方向**：是「引擎 crate 依赖 trait crate 并 impl」还是
   「trait crate 依赖引擎 crate 写 adapter」？前者更干净但要改四个引擎 crate；
   后者集中但 trait crate 变重。**需拍板**。

8. **graphviz 路线**：Rust 侧自写 DOT parser vs 改 C ABI 导出结构，
   是阶段四的主要不确定点，可能需要单独 spike。

---

## 8. 小结

- **现成可做**：d2（解析与语义图已 pub）、统一 trait 骨架。
- **需少量包装**：mermaid（typed model 齐全，缺聚合层 + serde）、
  plantuml（已有统一 parse + 聚合枚举，缺 serde）。
- **大工程**：graphviz（Rust 侧无任何语义结构）。
- **共同前置**：补 serde derive，是所有语义对外的硬门槛。
- **最小可交付**：阶段 0（trait 骨架，render 统一）+ 阶段 1（d2 semantic + Markon 对
  d2 做结构 diff），即可端到端验证「source fallback → 结构 diff」的价值闭环。
