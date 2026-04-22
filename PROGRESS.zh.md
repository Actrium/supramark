# 阶段进展

截至 Wave 4（Stratum 3 六个 consumer diagram + timeline 集成回归）完成。

> 本项目只维护中文版 PROGRESS。

## 总览

| 指标 | 值 |
|---|---:|
| Diagram 完整 byte-exact 已落地 | **13 / 23** |
| Diagram 结构落地（parse + layout，render 部分/结构可用） | **19 / 23**（+6 Stratum 3） |
| Wave 4 byte-exact fixtures（单独 fixture 口径） | **27 / 759**（er 11 + block 16） |
| Lib unit 测试 | 464 passed / 0 failed |
| Integration sweep（convert_with_id） | 12 组全绿（timeline 已回收；er/class/state/flowchart/block/requirement 各自 inline sweep 在 lib tests） |
| 总测试数 | 567 passed / 0 failed / 2 ignored |
| Cargo check warnings | 5（4 在 block 内部 dead_code，1 xychart pre-existing，timeline parse_px pre-existing） |
| 项目代码总行数 | ~42,500 行（含测试；Wave 4 新增 12.5K） |

## 已完整 byte-exact 的 diagram（13/23）

| Diagram | 方式 | Fixtures byte-exact |
|---|---|---:|
| pie | 内置 (d3.pie + d3.arc) | 14 / 14 |
| packet | 内置 (bit-field grid) | 5 / 5 |
| radar | 内置 (polygon math) | 7 / 7 |
| ishikawa | 内置 (fishbone 几何) | 17 / 18（handDrawn demo 04 超 MVP 范围） |
| journey | 内置 (bar layout + arc score) | 11 / 11 |
| timeline | 内置（TD + LR 双模式） | 17 / 17（含 6 inline + 11 sweep；cypress/12 因 cScale 派生未接） |
| quadrant | 内置 (d3.scaleLinear) | 16 / 16 |
| xychart | 内置 (d3.scaleBand + scaleLinear) | 55 / 56（1 个走 numeric-tolerance） |
| wardley | 内置 (landscape plot) | 12 / 12 |
| sankey | 自 port d3-sankey 0.12.3 (591 LoC) | 3 / 3 |
| treemap | 自 port d3-hierarchy squarify | 30 / 30 |
| kanban | 内置 (column + card 网格) | 11 / 11 |
| — | — | — |
| **小计** | — | **198 / 199** |

## Wave 4（Stratum 3）结构落地 · byte-exact 部分进行中

| Diagram | LoC | 结构（parser+layout） | Byte-exact |
|---|---:|:-:|:-:|
| er | 2057 | ✓ 80/80 | **11 / 80** |
| block | 2189 | ✓ 33/33 | **16 / 33** |
| class | 2170 | ✓ 239/239 | 0 / 240（render 诚实 stub） |
| state | 1503 | ✓ 82/82 (69 不 panic) | 0 / 82 |
| flowchart | 2642 | ✓ 280/280（无 panic） | 0 / 280（无 elk） |
| requirement | 1852 | ✓ 44/44 | 0 / 44 |

**Wave 4 小计：12,413 LoC，27 fixtures byte-exact。**

六家共同的 render 墙（Wave 3.5 unified render shell 的 scope）：

1. **`<foreignObject><div><span><p>…` HTML-in-SVG label 栈** —— 所有节点/边 label 用这个，不是 `<text>`
2. **rough.js basis-spline 路径** —— 即便非 handDrawn，ER attribute divider / 节点描边也走 rough.js（seeded mulberry32 PRNG → basis spline polygon）
3. **完整 stylis 压缩的 `<style>` block** —— `@keyframes`、`[data-look="neo"]` rules、`mermaidTooltip`、`:root --mermaid-font-family`、drop-shadow filter defs
4. **`<g class="root">/<g class="clusters">/<g class="edgePaths">/<g class="edgeLabels">/<g class="nodes">` 层级** —— 固定顺序
5. **`data-id` / `data-edge` / `data-points`（base64-JSON）属性** —— 每条边每个 endpoint 的点序列
6. **dagre-rs vs @dagrejs/dagre 数值发散 ≤2 px** —— tie-break 稳定性、init-order sort，落在 viewBox / edge 路径 x 坐标
7. **d3-shape 弧/圆 emitter** —— state-start / history 用 36 段 cubic Bezier polyline，不是 `<circle>`

## Wave 4 附赠修复 / 挖掘

- **dagre compound-graph panic** —— state 早期发现 `rank/util.rs:42 unwrap on None` 在 composite graphs 触发，flowchart 补上解法：`retarget_cluster_endpoints` 把指向 subgraph id 的边 reroute 到其第一个 vertex member（对应上游 `adjustClustersAndEdges`/`findNonClusterChild`）
- **khroma HSL→RGB 10 位小数精度** —— er agent 手 port，解决派生色在 stylis 序列化中不 byte-exact
- **edge 标签维度沿 dagre_bridge 传递** —— er 给 `dagre_bridge.rs` 加了 `extra["label_width"]/["label_height"]` + `labelpos=Some("c")` 路径（向后兼容；ER / 将来所有 edge-labeled diagram 需要）
- **timeline 的 6 处 jsdom quirks** —— leftMargin 默认 150（不是 renderer fallback 的 50）、`getBBox` 用 14 px / sans-serif fallback（不读 CSS）、wrapped text textContent 无 `\n` 保持 1-line、TD ids 字面 `undefined-*`（上游 `initGraphics(svg)` 不传 diagramId）、lineWrapper.lower() 把 axis 拉到 `<style>` 前、float associativity 左结合不能 `+=`

## 已完成的基建（Wave 0 + Wave 3 plumbing）

Wave 3 作为"shapes/markers/edges/clusters 隔离组件库"已就绪，但**外壳**（SVG + CSS + `<g>` 层级 + foreignObject + data-attrs + drop-shadow filters）在 Wave 4 被证实是 Stratum 3 的共同瓶颈 → Wave 3.5 scope。

- **`src/layout/unified/`**（Wave 3 P0，1162 LoC）
- **`src/render/shapes/`**（P1，~2100 LoC，26 byte-exact + 40 stubbed + helpers）
- **`src/render/markers.rs`**（P2，623 LoC）
- **`src/render/edges.rs` + `shapes/clusters.rs` + `layout/routing.rs`**（P3，1871 LoC）
- **`src/math/v8_trig.rs`**（hoisted from pie）
- **`src/theme/`**（5 variant，263 flat + nested；Wave 4 加了 khroma 精度补丁）
- **`src/config/` + `src/preprocess.rs` + `src/detect.rs`**
- **`tests/eval/`** + **`src/font_{data,metrics}.rs`**

## 关键技术发现累计（Wave 0–4 共 22+ 条）

前 11 条见先前版本，Wave 4 新增：

12. **roughjs seeded PRNG 即默认渲染也要走** —— er 的 attribute 行分隔符、所有默认"方"节点描边都是 rough.js basis-spline 输出；`look: handDrawn` 只是调 roughness 参数而已
13. **dagre-rs 与 dagre-d3 的 tie-break 不完全一致** —— 同一图在坐标级别可能差 ≤2 px，viewBox width 会带上；不是 bug，是两个实现的独立选择
14. **上游默认 font metrics 全走 14 px sans-serif** —— jsdom `getBoundingClientRect` 不看 CSS 类名，除非 inline attr / style。很多 themeVariables.fontSize 实际只影响 CSS，不参与维度计算
15. **block diagram random id 生成是有状态的** —— mermaid.render 每次调用 Math.random 若干次（和 fixture index 相关），导致 `id-<12chars>-N` 里的 N 难以从源码独立复现
16. **dagre compound graph panic** —— `@dagrejs/dagre` 有 `adjustClustersAndEdges` 前置处理，把指向 cluster 的边 reroute 到第一个非 cluster 子节点；dagre-rs 缺这步会在 rank 阶段 unwrap on None

## 已知 partial / 待后续调的地方

| 项 | 原因 | 下一步 |
|---|---|---|
| cypress/timeline/12 | `themeVariables.cScale0..2` 覆盖后 `khroma.invert/lighten` 派生未接 | Wave 3.5 theme::color 动态计算 |
| ishikawa demo/04 | handDrawn 模式 | Wave 6+ 决定 port roughjs |
| xychart/35 | V8 Number.toString 17 位精度 | 已自动过 |
| 6 个 Stratum 3 的 render 层 | Wave 3.5 外壳未建 | 下一波 |

## 下一步建议

### Wave 3.5（unified render shell）—— 最大回报

六个 Stratum 3 共享的外壳，完成后 er/block 再补关键 quirks 就能快速从"部分"到"完整" byte-exact。Scope：

- `src/render/unified_shell.rs` —— SVG 外壳 + `<g>` 层级 + stylis block + data-attrs + drop-shadow filters
- `src/render/foreign_object.rs` —— `<foreignObject><div><span><p>` label 栈 + getBBox shim
- `src/render/rough.rs` —— mulberry32 PRNG + basis-spline 路径（port roughjs 核心 ~500 LoC）
- `src/theme/color.rs` 扩展 —— khroma hue2rgb / invert / lighten / darken / adjust（10 位精度）
- `src/layout/dagre_stability.rs` —— init-order sort / tie-break patch，把 dagre-rs 输出与 dagre-d3 对齐

预计 3-4 个 agent 并行（worktree 隔离，PR 模式）。

### 之后

- **Wave 5**：class / state / flowchart / requirement byte-exact 收尾（在 Wave 3.5 基础上补各自 CSS + 细节）
- **Wave 6**：gantt（chrono 依赖） / mindmap（tidy-tree layout）
- **Wave 7**：sequence / c4 / gitGraph（bespoke，sequence 是 4K+ 行最大单个 port）
- **venn**：600 行 MDS，可 port 或保持 known_ignored
- **architecture**：cytoscape-fcose，保持 known_ignored

## 本阶段（Wave 4）产出摘要

- **Agent 数 × 7 并行**（6 Stratum 3 + 1 timeline-fix；第二轮改用 worktree isolation PR 模式）
- **12,500 LoC** 新增代码
- **27 fixtures** 新增 byte-exact，**16 timeline** 回收进 sweep
- **16+ 条 upstream quirks** 文档化（六家合力挖出）
- **累计 13 / 23 完整 byte-exact，19 / 23 结构落地，57%→约 82%（结构口径）**
