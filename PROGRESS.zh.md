# 阶段进展

截至 Wave 5 中期（CSS 全量 port + class 渲染器实现 + ER classDef 支持）。

> 本项目只维护中文版 PROGRESS。

## 总览

| 指标 | 值 |
|---|---:|
| Diagram 完整 byte-exact 已落地 | **13 / 23** |
| Diagram 结构落地（parse + layout，render 可用） | **19 / 23** |
| 结构中 Stratum 3 byte-exact fixtures | **57 / 759**（er **41** + block 16） |
| Lib unit 测试 | 522 passed / 0 failed / 6 ignored |
| Cargo check warnings | ≤8（pre-existing dead_code） |
| 项目代码总行数 | ~52,000 行（Wave 5 新增 ~5K） |

## 已完整 byte-exact 的 diagram（13/23）

| Diagram | 方式 | Fixtures byte-exact |
|---|---|---:|
| pie | 内置 (d3.pie + d3.arc) | 14 / 14 |
| packet | 内置 (bit-field grid) | 5 / 5 |
| radar | 内置 (polygon math) | 7 / 7 |
| ishikawa | 内置 (fishbone 几何) | 17 / 18（handDrawn demo 04 超 MVP 范围） |
| journey | 内置 (bar layout + arc score) | 11 / 11 |
| timeline | 内置（TD + LR 双模式） | 17 / 17 |
| quadrant | 内置 (d3.scaleLinear) | 16 / 16 |
| xychart | 内置 (d3.scaleBand + scaleLinear) | 55 / 56 |
| wardley | 内置 (landscape plot) | 12 / 12 |
| sankey | 自 port d3-sankey 0.12.3 | 3 / 3 |
| treemap | 自 port d3-hierarchy squarify | 30 / 30 |
| kanban | 内置 (column + card 网格) | 11 / 11 |
| — | — | — |
| **小计** | — | **198 / 199** |

## Wave 5 进行中 · Stratum 3 渲染层

| Diagram | Render 状态 | CSS port | Byte-exact fixtures | 当前阻塞 |
|---|---|:-:|---:|---|
| er | ✓ 完整 | ✓ | 41/80 | dagre 坐标差、classDef data-color-id、markdown emoji |
| block | ✓ 完整 | ✓ | 16/33 | random ID 有状态、具体形状差异 |
| class | ✓ 新实现 | ✓ 全量 | 0/113 | classBox shape 未 port、dagre 坐标差 |
| state | ✓ 结构改进 | ✓ 全量 | 0/82 | dagre 坐标差、d3-shape arc、state-end rough.js |
| flowchart | ✓ 结构改进 | ✓ 全量 | 0/318 | dagre 坐标差、节点形状、edge label 格式 |
| requirement | ✓ 结构改进 | ✓ 全量（CSS byte-exact 4429/4429） | 0/44 | dagre 坐标差、节点形状 |

### Wave 5 关键进展

1. **CSS 全量 port 完成** —— 所有 6 个 Stratum 3 diagram 的 upstream styles.js/styles.ts 已完整移植到 Rust。requirement 的 CSS 已确认 byte-exact（4429/4429 bytes）。
2. **class diagram 渲染器实现** —— 从 `Unsupported` stub 到完整工作渲染器（148 → 855 行），包括 SVG shell、CSS、edge paths with data-attrs、foreignObject labels、markers。
3. **ER classDef/style 支持** —— `collect_entity_styles()` + `EntityStyles` 结构体，支持 fill/stroke → rect、color/font → label/span 的分流。`div_style_prefix` 支持 classDef 文本属性在 `<div>` 上的优先输出。
4. **state diagram 边和节点改进** —— data-edge/et/id/points/look 属性、foreignObject edge labels、state-start circle r=7、data-look=classic。
5. **flowchart 边属性** —— data-points base64、duplicated thickness/pattern classes、edge style=";"。
6. **theme 扩展** —— bkgColorArray、borderColorArray、requirementEdgeLabelBackground。

### 核心阻塞：dagre 坐标差异

CSS 已全量 byte-exact，但 **dagre-rs 与 @dagrejs/dagre 的布局坐标不同** 是当前 0 byte-exact 的根本原因。具体表现：

- 上游 viewBox 使用负坐标（如 `-73.29 -142.58`），我们的可能是 `0 0`
- 上游 `setupViewPortForSVG` 用 `svg.getBBox()` 获取实际渲染边界
- 节点位置、边路径、标签位置全部受影响
- ER 的 41/80 能通过说明简单图 dagre-rs 坐标正确，复杂图有 tie-break 差异

## 关键技术发现累计（Wave 0–5 共 25+ 条）

Wave 4 及之前的 16 条见先前版本。Wave 5 新增：

17. **上游 setupViewPortForSVG 用 getBBox() 计算 viewBox** —— 不是从 dagre 输出直接算，而是先渲染到 DOM 再量 bbox。这意味着 dagre 坐标必须匹配上游才能得到相同 viewBox。
18. **CSS 全量 port 后仍 0 byte-exact** —— CSS 只是前提条件，坐标差异是更根本的阻塞。CSS byte-exact 验证方法：提取 `<style>...</style>` 区间做独立对比。
19. **classDef style 分流规则** —— fill/stroke → rect style (加 `!important`)；color/font → label span style (加 `!important`) + div style prefix。hex 色值在 div 上需 normalize 为 `rgb()` 格式。
20. **class edge style 用 `;;;`** —— 上游 class diagram 的 edge path style 属性是 `style=";;;"` 而非 ER 的 `style="undefined;;;undefined"`。
21. **flowchart edge class 重复** —— upstream `insertEdge` 重复 thickness/pattern classes：`edge-thickness-normal edge-pattern-solid edge-thickness-normal edge-pattern-solid flowchart-link`。
22. **genColor CSS 只在 borderColorArray 非空时输出** —— 默认主题无 `borderColorArray`，因此 requirement 的 genColor 段为空。

## 下一步

### Wave 5 剩余（优先级排序）

1. **dagre 坐标对齐** —— 分析 dagre-rs 与 @dagrejs/dagre 的 tie-break 差异，尝试修复或 workaround
2. **class classBox shape port** —— 上游 classBox.ts 的 rough.js 8-segment basis-spline outline + header/members/methods 结构
3. **state d3-shape arc emitter** —— state-start 的 36 段 cubic bezier polyline
4. **ER data-color-id** —— 给节点加 `data-color-id="color-N"` 属性
5. **block diagram** —— 16/33，需具体分析每个失败 fixture

### Wave 6

- **gantt**（chrono 依赖） / **mindmap**（tidy-tree layout）

### Wave 7

- **sequence** / **c4** / **gitGraph**（bespoke，sequence 是 4K+ 行最大单个 port）
- **venn**：600 行 MDS，可 port 或保持 known_ignored
- **architecture**：cytoscape-fcose，保持 known_ignored
