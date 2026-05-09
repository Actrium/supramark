# mermaid-little 功能规划

对齐上游 **mermaid@11.14.0**（`2b9d054d`，2026-04-01 发布）。

本文档记录依赖分析与分期计划。随着 diagram 逐一上线，会演化成支持矩阵。

## 当前状态（2026-05-02）

25 种 diagram 全部接通，已进入**收敛阶段**。`cargo test` 绿；sweep_all **1200 / 1328 ≈ 90.4% byte-exact**。

| | |
|---|---|
| 上游版本 | `mermaid@11.14.0`（`2b9d054d`） |
| 已接通 diagram | **25 / 25**（含 sequence / mindmap / c4 / gitGraph 等全部） |
| 已 byte-exact（≥99% pass） | 22 / 25 |
| Reference 测试 | 1328 条（cypress 1126 + demos 202），known_ignored 已清空 |
| Lib unit tests | 666 / 0 / 0 |
| Layout 后端 | [`dagre-rs`](https://github.com/kookyleo/dagre-rs)（pinned，完整 dagre.js port） |
| 详细进展 | 见 [PROGRESS.zh.md](PROGRESS.zh.md) |

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

## Diagram 支持矩阵（2026-05-02 实测）

按 byte-exact 通过率分组。所有 25 种 diagram 都已 parser/layout/render 接通；下面括号 `cypress/demos` 数字来自最新 `sweep_all`。

### 已 100% byte-exact（17）

| 图表 | cypress | demos |
|---|---:|---:|
| pie | 10/10 | 3/3 |
| packet | 5/5 | — |
| radar | 6/6 | 1/1 |
| ishikawa（含 `look:handDrawn`） | 13/13 | 5/5 |
| user-journey | 10/10 | 1/1 |
| timeline | 14/14 | 3/3 |
| quadrant | 14/14 | 2/2 |
| xychart | 37/37 | 19/19 |
| wardley | 6/6 | 6/6 |
| sankey | 1/1 | 2/2 |
| treemap | 28/28 | 2/2 |
| kanban | 11/11 | — |
| c4 | — | 5/5 |
| er | 73/73 | 7/7 |
| block | 33/33 | — |
| requirement | 43/43 | 1/1 |
| state | 72/72 | 10/10 |
| class | 225/225 | 12/12 |
| gitGraph | 105/105 | 24/24 |

注：上面表格 17 个 diagram 已 100%。

### 已 ≥95% byte-exact（5）

| 图表 | 通过 | 阻塞 |
|---|---:|---|
| flowchart | 191/192 cy + 59/65 dm | KaTeX × 6, icon shape × 1, ELK opt-in × 1 |
| gantt | 41/43 cy + 8/10 dm | V8 `new Date()` 时区 quirk × 4（环境性） |
| venn | 16/16 cy + 8/12 dm | constrainedMDS × 1, handDrawn × 3 |

### 部分 byte-exact，主要工作量集中（3）

| 图表 | 通过 | 阻塞 |
|---|---:|---|
| sequence | 51/140 cy + 4/10 dm | loop/alt/par/rect/critical/break 块 × 24，activation，actor type variants，note over multi，wrap，font metrics |
| mindmap | 6/23 cy + 1/2 dm | cose-bilkent 物理引擎已落骨架（W11-D），缺 reduceTrees / FR-grid bucket / Coarsening / curveBasis edge / Base64 data-points |

### 不在范围内 / 推迟（1）

| 图表 | 起始 | Parser | 原因 |
|---|---|---|---|
| architecture | `architecture-beta` | langium | 需要 `cytoscape-cose-bilkent` / `-fcose`，本仓库 mindmap 复用一部分 cose-bilkent 但 architecture 需 fcose（更复杂的科学优化），暂不实现 |

### 辅助型（非用户可见）

`error` / `info` / `common` / `treeView` —— 上游内部辅助，此处无需 port。

## 已落地的关键能力

| 能力 | 实现状态 | 备注 |
|---|---|---|
| dagre 后端 | ✓ kookyleo/dagre-rs（vendored） | 含嵌套孤立子图 / cluster 自循环 / iso_desc edge 分区 |
| 5 套主题 | ✓ default / base / dark / forest / neutral | venn / sequence / class theme propagation 已落 |
| 字体度量（DejaVu / sans-serif jsdom shim） | ✓ | 共用 plantuml-little baked 表 |
| stylis CSS preprocessor | ✓ 用于主题段 | |
| khroma 颜色（lighten/darken/isDark） | ✓ |  |
| rough.js engine | ✓ rectangle/polygon/line/path/ellipse/circle/hachure/bbox_of_sets | ishikawa handDrawn 已端到端使用 |
| KaTeX | ✗ 6 demos/flowchart 阻塞 | 待独立 Phase |
| Icon shapes（iconify） | ✗ 3 cypress/flowchart 阻塞 | 待独立 Phase |
| ELK 后端 | ✗ 非目标 | 程序性过滤 |

## 不在范围内（v1）

- ELK layout（上游 opt-in，后期看需求再加；本仓库 `is_elk()` 程序性过滤而非 ignore 列表）
- Architecture 图（依赖 cytoscape-fcose 完整移植）
- KaTeX 公式渲染（待独立 Phase；当前 6 fixture 阻塞）
- 完整 `@iconify` 图标库（待独立 Phase；当前 3 fixture 阻塞）

## Phase 路线图（历史 → 当前）

Phase 0-4（骨架 → reference 管线 → 字体度量 → fixtures → 逐 diagram 实现）已在 11 路 wave 累计推进中全部落地。当前最新状态见 [PROGRESS.zh.md](PROGRESS.zh.md)。

收敛阶段未完成项：

- **Sequence 收尾** —— probe-driven 推进剩余 100 cypress + 6 demos sequence fixtures。
- **Mindmap 多节点收尾** —— cose-bilkent reduceTrees / FR-grid bucket / Coarsening / curveBasis edge / Base64 data-points 五大件接力（W11-D 后续）。
- **KaTeX Phase** —— port KaTeX 渲染器子集，解锁 6 demos/flowchart fixtures（独立决策）。
- **Icon shapes Phase** —— 注册 ~500 AWS / iconify SVG path，解锁 3 cypress/flowchart fixtures（独立决策）。
- **`packages/web/` wasm 构建** —— parity 收敛后，对齐 plantuml-little 的 `@kookyleo/plantuml-little-web`。

## 测试方法学

参照 plantuml-little：

- **Byte-exact reference 测试。** `tests/fixtures/` 和 `tests/ext_fixtures/` 下每个 fixture 都配有一份 `tests/reference/` 里的 SVG（由上游管线生成）。Rust 输出必须逐字节一致。
- **共享的确定性栈。** 两侧都用同一份 Node/wasm runner + 同一份 DejaVu 字体表 + 同一份字体度量 shim，剩余差异即为真正的实现 bug。
- **`native` vs `wasm` 两种测试后端。** 日常 `cargo test` 走 native 纯 Rust 管线；CI 的 `test-reference` 任务通过 `MERMAID_LITTLE_TEST_BACKEND=wasm` 启用跨平台可重放路径。

## 致谢

本项目是 [Mermaid](https://mermaid.js.org/) 的独立 Rust 重新实现，原作者为 Knut Sveidqvist。我们对 Mermaid 团队在 diagram-as-code 领域的贡献深表敬意。所有规范性内容以上游为标准。
