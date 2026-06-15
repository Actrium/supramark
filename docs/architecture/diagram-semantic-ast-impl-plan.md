# Diagram 语义 AST 实施方案（落地计划）

> 状态：实施中。关联设计：`diagram-semantic-ast.md`（§0 决策记录）。
> 本文是该设计的工程落地拆解，仅承载「怎么做、谁先谁后、怎么并行」。

## 0. 本轮已拍板（在设计 §0 三项之外补充）

- **依赖方向（设计 §7-7）**：四个引擎 crate 直接依赖 trait crate 并 `impl DiagramEngine`。
- **交付边界**：本轮并行铺到阶段 0-3（骨架 + d2/mermaid/plantuml 三引擎语义）；
  graphviz（阶段 4）后置，本轮 `semantic` 返回 `None`。
- **Markon**：不在本仓（`~/Workspace` 仅 supramark），属仓外下游。本轮只把
  `{engine, kind, data}` JSON 契约暴露好，结构化 diff 留待 Markon 侧后续接入。

## 1. crate 分层（避免循环依赖）

「引擎 crate 直接 impl」要求 trait crate 零引擎依赖，否则引擎依赖它即成环。
故拆两层：

- `crates/supramark-diagram-core`：仅 `DiagramEngine` trait、`EngineAst`（对外
  `{engine, kind, data: serde_json::Value}` 信封）、`DiagramError`、`RenderOutput`、
  `DiagramRegistry`。依赖仅 `serde`/`serde_json`/`thiserror`。四引擎依赖它来 impl。
- `crates/supramark-diagram`（facade）：依赖 core + 四引擎，提供 `default_registry()`
  组装全部引擎实例。供 CLI / FFI / 下游消费；`supramark-markdown` **不**依赖它。

`semantic()` 在 trait 边界返回 `EngineAst` 信封（object-safe、可 `Box<dyn>` 注册）；
各引擎在自己 crate 内用强类型语义结构，impl 里 `serde_json::to_value` 收口为 `data`。
这就是设计 §0 决策 1「内部强类型 + 对外统一 JSON」的落地。

## 2. 任务拆解与并行划分

依赖顺序：T0a 是地基（串行最先）→ 其余按下表并行。

| 任务 | 范围 | 依赖 | 可并行组 |
|---|---|---|---|
| T0a core | trait + 信封类型 + registry，入 workspace，编译 | — | 串行地基 |
| T1 d2 | `compile→graph::Graph` 语义投影（剔 top_left/width/height/box_），强类型 `D2Semantic`，impl render+semantic | T0a | A |
| T2 mermaid | feature-gated serde derive（26 文件）；按 `DiagramKind` 聚合 `MermaidAst`；impl render+semantic（er/flowchart/sequence/class 优先） | T0a | A |
| T3 plantuml | 加 `serde` feature；feature-gated derive（36 文件）；复用 `parser::parse→Diagram`；impl render+semantic | T0a | A |
| T0b graphviz | impl render 薄封装，semantic 返回 None | T0a | A |
| Tfacade | facade crate + `default_registry()` | T0a+四引擎 impl | B |
| Tast | AST v2 `Diagram` 加 `semantic: Option<Value>`；更新 `ast-spec.md` | — | 独立 |
| Ttest | 全 workspace 编译 + clippy + 契约快照测试 | 全部 | 收尾 |

并行约束：同一 cargo workspace 并发编译抢同一 target 锁，故各引擎可并行**编写**，
编译验证用 `cargo build -p <crate>` 串行化（抢锁自动排队，不会出错只是变慢）。

## 3. 各引擎实现要点

- **d2**：crate 已无条件依赖 serde。不给 `graph::Object` 直接 derive（含布局几何），
  而是定义投影结构 `D2Semantic { nodes: Vec<D2Node>, edges: Vec<D2Edge> }`，
  只取 id/label/shape/style/parent/children/edges。kind = "d2"。
- **mermaid**：crate 已有 serde dep。新增内部 feature `semantic-serde`，model 的 derive 用
  `#[cfg_attr(feature="semantic-serde", derive(Serialize))]`；聚合枚举按
  `detect::DiagramKind`（~27 变体）分派各 `parser::*::parse`。kind = 图类型名。
- **plantuml**：crate 仅有 serde_json，需加 `serde = { features=["derive"], optional }` +
  feature gate。复用已有统一入口 `parser::parse → model::diagram::Diagram`（34 变体）。
  kind = 变体名。
- **graphviz**：Rust 侧无语义结构（DOT 解析在 C FFI 内），本轮 render-only。

## 4. AST v2 集成（懒解析，默认不内嵌）

`SupramarkNode::Diagram` 加 `#[serde(skip_serializing_if="Option::is_none")] semantic: Option<serde_json::Value>`。
parser 主流程**默认不填**（设计 §4.2 推荐路径 2，避免只渲染场景付解析成本）；
markdown crate 因此不引入任何引擎依赖。下游持 facade 按需解析填充。
同步更新 `ast-spec.md` 增补 `semantic` 字段与 `{engine,kind,data}` 形状。

## 5. 测试与契约（设计 §7-4）

- 各引擎 semantic 的序列化结果纳入 fixture 快照，锁定 `{engine,kind,data}` 形状，
  防引擎升级导致契约漂移。
- registry 分派测试：`dot`/`graphviz` 两 key 命中同一引擎实例。

## 6. 暂不做（设计仍待定项）

- 语义解析缓存（§7-5）：阶段一不做。
- graphviz DOT parser / 改 C ABI（§7-8）：阶段四，需单独 spike。
