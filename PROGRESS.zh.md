# 阶段进展

截至 Wave 3.5（unified render shell / foreignObject label / roughjs port 三路并行 + PR merge）完成。

> 本项目只维护中文版 PROGRESS。

## 总览

| 指标 | 值 |
|---|---:|
| Diagram 完整 byte-exact 已落地 | **13 / 23** |
| Diagram 结构落地（parse + layout，render 部分/结构可用） | **19 / 23** |
| 结构中 Stratum 3 byte-exact fixtures（单独 fixture 口径） | **57 / 759**（er **41** + block 16） |
| Lib unit 测试 | 511 passed / 0 failed / 5 ignored |
| Integration sweep（convert_with_id） | 12 组全绿 |
| 总测试数 | 614 passed / 0 failed / 7 ignored |
| Cargo check warnings | 5（全部 pre-existing 或 block 内部 dead_code） |
| 项目代码总行数 | ~47,000 行（Wave 3.5 新增 ~4.5K） |

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

| Diagram | LoC | 结构（parser+layout） | Byte-exact（Wave 4 → 3.5） |
|---|---:|:-:|:-:|
| er | 2057 | ✓ 80/80 | 11/80 → **41/80**（+30，rough.js 解 divider） |
| block | 2189 | ✓ 33/33 | **16 / 33** |
| class | 2170 | ✓ 239/239 | 0 / 240（render 诚实 stub） |
| state | 1503 | ✓ 82/82 (69 不 panic) | 0 / 82（shell 对齐 1552 B） |
| flowchart | 2642 | ✓ 280/280（无 panic） | 0 / 280（无 elk） |
| requirement | 1852 | ✓ 44/44 | 0 / 44 |

**Wave 4 结构小计：12,413 LoC。Wave 3.5 收益：ER +30 byte-exact；shell + label 管道对所有 Stratum 3 就位。**

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

### Wave 3.5 完成（3 worktree agents + 1 resolver）

三条独立落地：

- **`src/render/rough.rs`**（1165 LoC）—— mulberry32 + Lehmer LCG（Math.imul 精确 32-bit）+ RoughOptions + Op/OpSet/Drawable + RoughGenerator（rectangle / polygon / line）+ ops_to_path + path_out_to_svg。ER attribute-row divider 直接受益：**11 → 41 byte-exact fixtures**
- **`src/render/foreign_object.rs`**（523 LoC）+ 14 shape files 切换 —— `<g class="label"><foreignObject><div><span><p>` 标签栈 + `measure_html_label` 匹配 jsdom 14 px sans-serif 默认 + CSS-aware override 路径。classbox / requirement / 所有 rect-like shape 的 label emission 切换到 foreignObject
- **`src/render/unified_shell.rs`**（321 LoC）+ **`src/render/stylis.rs`**（336 LoC）+ **`src/theme/css.rs`**（287 LoC）—— SVG 外壳 open/close + 种子组 + root/clusters/edgePaths/edgeLabels/nodes 层级 helpers + drop-shadow defs + data_edge_attrs (base64 JSON) + stylis 压缩 + 共享 base_preamble / neo_look_block。state/01 post-viewBox 对齐从 0 B → **1552 B**

**Merge**：rough.js 先 landed 到 main（conflict-free），foreignObject + unified-shell 两个 worktree 分支由 resolver worktree agent 解冲突后 `--no-ff` 合入（resolver 自己 rebase + 解 svg_er.rs label 分发 + svg_block/svg_requirement 的 preamble 提取）。3 个 merge commit：2c9758b / 736074e / 30f74f1。

## 剩余 blockers（Wave 5 scope）

- state/class/flowchart/requirement 的 **diagram-specific CSS** 未进 theme::css（unified-shell 留了接口；每个需各自 port ~100-300 行 styles.ts）
- **rough.js hachure filler**（`look: handDrawn`）—— scan-line-hachure.js 未 port，ishikawa/04 仍 known_partial
- **dagre-rs vs @dagrejs/dagre 数值 tie-break** ≤2 px —— class / flowchart / state / er 仍部分受影响
- **markdown emoji / classDef style override / data-color-id** —— ER 剩 39/80 的具体 blockers
- **`d3-shape` arc 精确 emitter** —— state-start / history 的 36 段 cubic polyline

### Wave 5（下一步）

- **class / state / flowchart / requirement byte-exact 收尾** —— 每个 port 对应 styles.ts（~100-300 行）进 theme::css，补 dagre tie-break 对齐 patch，其余细节；预计 4 agent 并行 worktree
- **ER 剩 39 fixture** —— 单独 1 个 agent 负责 markdown emoji / classDef style / data-color-id

### 之后

- **Wave 6**：gantt（chrono 依赖） / mindmap（tidy-tree layout）
- **Wave 7**：sequence / c4 / gitGraph（bespoke，sequence 是 4K+ 行最大单个 port）
- **venn**：600 行 MDS，可 port 或保持 known_ignored
- **architecture**：cytoscape-fcose，保持 known_ignored

## 本阶段（Wave 4 + 3.5）产出摘要

- **Agent 数 × 11 并行**（7 Wave4 + 3 Wave3.5 worktree + 1 resolver worktree）
- **~17,000 LoC** 新增代码（Wave 4 12.5K + Wave 3.5 4.5K）
- Byte-exact fixture 增量：
  - Wave 4：27 新增 + 16 timeline 回收 = 43
  - Wave 3.5：+30 ER（11→41）
- **21+ 条 upstream quirks** 文档化
- **累计 13 / 23 完整 byte-exact，19 / 23 结构落地；总 pass 测试 614**

## Wave 3.5 协作协议演进

- **worktree isolation + PR 模式** 初次全面启用：每 subagent 在独立 worktree 分支工作，完成后 branch 成为可合并的 PR
- **主 agent ≠ 冲突解决者**：三路并行的 merge 冲突由一个额外派发的 **resolver worktree agent** 解决（资源等价于一轮子 agent），主 agent 只做 fast-forward 合并
- 一个 agent（rough.js）意外在主树工作绕过了 worktree 隔离 —— 未来 prompt 强化 `cd <worktree-path>` 指令
