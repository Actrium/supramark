# mermaid-little 功能规划

对齐上游 **mermaid@11.14.0**（`2b9d054d`，2026-04-01 发布）。

本文档记录依赖分析与分期计划。随着 diagram 逐一上线，会演化成支持矩阵。

## 当前状态

本项目处于**骨架阶段**，尚无任何 diagram 类型落地。执行 `cargo check` 可确认 workspace 构建通过。

| | |
|---|---|
| 上游版本 | `mermaid@11.14.0`（`2b9d054d`） |
| 已支持图表 | 0 / 25 |
| Reference 测试 | 0 |
| Layout 后端 | [`dagre-rs`](https://github.com/kookyleo/dagre-rs)（pinned，完整 dagre.js port） |

## 上游依赖勘察

`packages/mermaid/package.json` 的运行时依赖 → 我们 Rust 侧的策略：

| 上游 JS 依赖 | 用途 | mermaid-little 策略 |
|---|---|---|
| `dagre-d3-es` | flowchart / class / state / er 默认 layout | **使用 [`dagre-rs`](https://github.com/kookyleo/dagre-rs)** —— 完整 Rust port，已对 dagre.js byte-exact 交叉验证通过。外加两个小几何辅助函数（`intersectPolygon`、`intersectRect`）需要补齐。 |
| `@mermaid-js/parser` | 7 种较新 diagram 的 langium grammar | 每个 grammar 重写成手写 Rust parser（nom / chumsky 风格）。 |
| `packages/mermaid/src/diagrams/*/parser/*.jison` | 18 种老 diagram 的 jison grammar | 同上，每个 jison 规则都手动 port。 |
| `d3` 及子模块 | 通用 SVG 原语、拖拽、缩放 | **不需要** —— 我们直接拼 SVG 字符串，零运行时 DOM。 |
| `d3-sankey` | 仅 sankey 用 | 直接 port 算法（约 600 行）。 |
| `@upsetjs/venn.js` | 仅 venn 用 | 直接 port 算法。 |
| `cytoscape` + `cose-bilkent` + `fcose` | 仅 architecture 用 | **MVP 不支持**。无 Rust 对应物，核心稳定后再评估。 |
| `elkjs`（通过 `@mermaid-js/layout-elk`） | 可选 ELK layout，opt-in | **MVP 不支持**。上游本身就是独立子包、用户主动切换；默认路径不依赖它。 |
| `katex` | label 里的 `$…$` 公式 | **MVP 不支持**（占位）。 |
| `roughjs` | 手绘风格 | 推迟。有需求再 port（plantuml-little 自己写过类似 jiggle RNG）。 |
| `khroma` | 颜色处理 | 用少量 Rust 辅助函数替代。 |
| `marked` | label 里的 markdown | port 最小子集（粗体 / 斜体 / code / 链接）。 |
| `stylis` | CSS 预处理 | 不需要，我们 bake 样式。 |
| `dompurify` | label HTML 的 XSS 过滤 | 不需要，不暴露 DOM。 |
| `lodash-es` | 工具函数 | 用 stdlib 替代。 |
| `dayjs` | gantt 的日期处理 | 用 `chrono` 或 `time`。 |
| `uuid` | 唯一 SVG ID | 用确定性的 source-seeded ID（plantuml-little 同款做法）。 |
| `ts-dedent` | 字符串字面量缩进处理 | 用 stdlib。 |
| `@braintree/sanitize-url` / `@iconify/utils` | URL / 图标辅助 | 按需 port 最小子集。 |

## Diagram 类型矩阵（v11.14.0，25 种面向用户的类型）

"Parser" 列标注上游用的是 jison（18 种）还是 langium（7 种）。所有 parser 我们都会重写为 Rust，这列只是说明上游是哪种 grammar。

### Tier 1 —— 内置 layout、最简单优先（11）

无外部 layout 引擎，纯几何 + 文字摆放。

| 图表 | 起始 | Parser | 备注 |
|---|---|---|---|
| pie | `pie` | langium | 单环、百分比 label。 |
| xychart | `xychart-beta` | jison | 2D 坐标系 + 柱/折线。 |
| sankey | `sankey-beta` | jison | 需要 port `d3-sankey` 算法。 |
| sequence | `sequenceDiagram` | jison | 参与者泳道 + 消息路由。 |
| gantt | `gantt` | jison | 日期轴 + 任务条，用到 `dayjs`。 |
| gitGraph | `gitGraph` | langium | 分支 / 提交点布局。 |
| user-journey | `journey` | jison | 任务 + emoji / 分数列。 |
| timeline | `timeline` | jison | 横向带状 + 事件。 |
| quadrant-chart | `quadrantChart` | jison | 固定 2×2 网格 + 点放置。 |
| requirement | `requirementDiagram` | jison | 块 + 类型化关系。 |
| packet | `packet-beta` | langium | 位域网格。 |

### Tier 2 —— 内置 layout、中等复杂度（9）

| 图表 | 起始 | Parser | 备注 |
|---|---|---|---|
| mindmap | `mindmap` | jison | 类 tidy-tree 内置布局。 |
| kanban | `kanban` | jison | 列 + 卡片网格。 |
| block | `block-beta` | jison | 嵌套 block 网格 + 跨列。 |
| treemap | `treemap` | langium | 矩形分割。 |
| radar | `radar-beta` | langium | 极坐标 + 多轴。 |
| wardley | `wardley` | langium | （beta）2D 画布 + 演化轴。 |
| ishikawa | `ishikawa` | jison | （鱼骨图）对角分支。 |
| venn | `venn` | jison | 需 port `venn.js` 算法。 |
| c4 | `C4Context`/`C4Container`/`C4Component` | jison | 叠加在 class / component 之上的渲染覆盖。 |

### Tier 3 —— dagre 驱动（4）

使用 `dagre-rs` 作为 layout 后端。需要 `intersectPolygon` / `intersectRect` 辅助（小，直接从上游 port）。

| 图表 | 起始 | Parser | 备注 |
|---|---|---|---|
| flowchart | `flowchart`/`graph` | jison | 使用率最高的类型。 |
| class | `classDiagram` | jison | 带成员行的矩形。 |
| state | `stateDiagram`/`stateDiagram-v2` | jison | 组合状态、fork/join。 |
| er | `erDiagram` | jison | 实体表 + 类型化连线。 |

### Tier 4 —— MVP 推迟 / 不支持（1）

| 图表 | 起始 | Parser | 原因 |
|---|---|---|---|
| architecture | `architecture-beta` | langium | 需要 `cytoscape-cose-bilkent`/`-fcose`，无 Rust port。Tier 1-3 稳定后再评估。 |

### 辅助型（非用户可见）

`error` / `info` / `common` / `treeView` —— 上游内部辅助，此处无需 port。

## 分期执行计划

1. **Phase 0 —— 骨架（已完成）**：Cargo.toml、lib / main 空壳、LICENSE、本计划。

2. **Phase 1 —— reference 管线**：在 `tests/support/` 下搭确定性 ref-SVG 生成器，走你选的激进路径：Node + QuickJS/wasm + 上游 mermaid + 极简 DOM shim，共享和 Rust 侧一致的字体度量表。定义 `MERMAID_LITTLE_TEST_BACKEND` 环境变量，模仿 `PLANTUML_LITTLE_TEST_BACKEND`。

3. **Phase 2 —— 字体度量**：把 DejaVu Sans / DejaVu Sans Mono 的 glyph advance 烘焙进 `src/font_data.rs`，两边共用同一张表，`textLength` 逐字一致。

4. **Phase 3 —— 三层 fixtures**：
   - `tests/fixtures/<diagram>/*.mmd` —— 手写最小样例，每种 1~3 份。
   - `tests/ext_fixtures/<diagram>/*.mmd` —— 从上游 `demos/*.html` 抽取。
   - `tests/ext_fixtures/e2e/<diagram>/*.mmd` —— 从上游 `cypress/integration/rendering/*.spec.*` 抽取。

5. **Phase 4 —— 按 diagram 逐类实现**：
   先 Tier 1（layout 风险最低），再 Tier 2，再 Tier 3（dagre 路径），Tier 4 推迟。每个 diagram 的落地包含：
   - parser + AST
   - layout（built-in 或 dagre-backed）
   - renderer 输出 SVG 字节
   - 对 Phase-1 管线的 ref 测试全绿

6. **Phase 5 —— `packages/web/` wasm 构建**：
   镜像 plantuml-little 的 `@kookyleo/plantuml-little-web` —— 暴露 wasm-bindgen 接口，让 Rust 核心能在浏览器 / Node 里跑，供想在浏览器内渲染、又不想带上游 mermaid.js 整包体积的用户使用。

## 不在范围内（MVP）

- ELK layout（上游 opt-in，后期看需求再加）
- Architecture 图（依赖 cytoscape）
- KaTeX 公式渲染（占位）
- rough.js 手绘风格（占位）
- 完整 `@iconify` 图标库（仅按需 port）

## 测试方法学

参照 plantuml-little：

- **Byte-exact reference 测试。** `tests/fixtures/` 和 `tests/ext_fixtures/` 下每个 fixture 都配有一份 `tests/reference/` 里的 SVG（由上游管线生成）。Rust 输出必须逐字节一致。
- **共享的确定性栈。** 两侧都用同一份 Node/wasm runner + 同一份 DejaVu 字体表 + 同一份字体度量 shim，剩余差异即为真正的实现 bug。
- **`native` vs `wasm` 两种测试后端。** 日常 `cargo test` 走 native 纯 Rust 管线；CI 的 `test-reference` 任务通过 `MERMAID_LITTLE_TEST_BACKEND=wasm` 启用跨平台可重放路径。

## 致谢

本项目是 [Mermaid](https://mermaid.js.org/) 的独立 Rust 重新实现，原作者为 Knut Sveidqvist。我们对 Mermaid 团队在 diagram-as-code 领域的贡献深表敬意。所有规范性内容以上游为标准。
