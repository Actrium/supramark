# 阶段进展

截至 Wave 5 后期（dagre tie-break 修复 + 14px font metrics + SVG 结构对齐进行中）。

> 本项目只维护中文版 PROGRESS。

## 总览

| 指标 | 值 |
|---|---:|
| Diagram 完整 byte-exact 已落地 | **13 / 23** |
| Diagram 结构落地（parse + layout，render 可用） | **20 / 23**（+gantt） |
| 结构中 Stratum 3 byte-exact fixtures | **69 / 759**（er **53** + block 16） |
| Lib unit 测试 | 530 passed / 0 failed / 7 ignored |
| Cargo check warnings | ≤10（pre-existing dead_code） |
| 项目代码总行数 | ~55,000 行 |

## 已完整 byte-exact 的 diagram（13/23）

| Diagram | 方式 | Fixtures byte-exact |
|---|---|---:|
| pie | 内置 (d3.pie + d3.arc) | 14 / 14 |
| packet | 内置 (bit-field grid) | 5 / 5 |
| radar | 内置 (polygon math) | 7 / 7 |
| ishikawa | 内置 (fishbone 几何) | 17 / 18 |
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

## Wave 5 · Stratum 3 渲染层进行中

| Diagram | Render 状态 | CSS port | Byte-exact fixtures | 当前阻塞 |
|---|---|:-:|---:|---|
| er | ✓ 完整 | ✓ | **53/80** | dagre viewBox 差异、attribute-bearing 实体维度、classDef 细节 |
| block | ✓ 完整 | ✓ | 16/33 | random ID 有状态、形状差异 |
| class | ✓ 新实现 | ✓ 全量 | 0/113 | classBox shape 未 port、节点 ID/形状 |
| state | ✓ 结构改进 | ✓ 全量 | 0/82 | 节点 ID/形状（坐标已精确）、edge d 属性 |
| flowchart | ✓ 结构改进 | ✓ 全量 | 0/318 | padding/shape 修复中、dagre viewBox 差异 |
| requirement | ✓ 结构改进 | ✓ 全量（CSS byte-exact） | 0/44 | 节点/边 SVG 格式、dagre viewBox 差异 |

### Wave 5 关键突破

1. **dagre tie_keep_first 修复** —— dagre-d3-es v7.0.14 在 crossing-count 平局时保留第一个 best，@dagrejs/dagre 替换。设 `tie_keep_first: true` 后 ER 从 41→53 byte-exact。
2. **14px font metrics** —— 上游的 `labelHelper` 用 `div.getBoundingClientRect()` 测量标签，继承 SVG 根的 14px sans-serif（不是主题 fontSize 16px）。将所有 Stratum 3 diagram 的 dagre 度量字体从 16px 改为 14px 后，state/01 坐标与上游完全匹配。
3. **CSS 全量 port 完成** —— 6 个 Stratum 3 diagram 的 upstream styles.js/styles.ts 完整移植。requirement CSS 已确认 byte-exact（4429/4429 bytes）。
4. **class diagram 渲染器** —— 从 Unsupported stub 到 855 行工作渲染器。
5. **gantt diagram 骨架** —— parser + model + layout + render stub，7 个测试。
6. **flowchart 结构改进** —— padding 8→15、diagram_padding 8→20、diamond shape 重写、vertex counter 修复、FontAwesome icon 替换。

### 核心诊断方法

建立了跨管线对照诊断流程：
1. `tests/support/dagre_debug.mjs` —— 在上游 JS 端渲染 fixture 并 dump dagre 中间数据（节点坐标/边路径/viewBox）
2. Rust 端的 `dump_*_diff` 测试 —— 渲染同一 fixture 并找到第一个字节差异
3. 逐层对比：CSS → viewBox → 节点位置 → 节点形状 → 边路径 → 标签格式

## 关键技术发现累计（Wave 0–5 共 28+ 条）

前 16 条见先前版本。Wave 5 新增：

17. **上游 setupViewPortForSVG 用 getBBox() 计算 viewBox** —— 不是从 dagre 输出直接算，而是先渲染到 DOM 再量 bbox。
18. **dagre-d3-es v7.0.14 ≠ @dagrejs/dagre** —— tie-breaking 行为不同：前者保留平局首个 best，后者替换。这是多 rank 图坐标翻转的根本原因。
19. **标签度量用 14px 不是 16px** —— upstream labelHelper 的 `div.getBoundingClientRect()` 继承 SVG 根的 14px sans-serif 默认值，不是 theme.fontSize（16px）。所有 Stratum 3 的 dagre 度量必须用 14px。
20. **flowchart padding = 15，diagramPadding = 20** —— 上游 config.flowchart.padding 默认 15（不是 8），diagramPadding 默认 20（不是 8）。
21. **class edge style 用 `;;;`** —— 上游 class diagram 的 edge path style 是 `style=";;;"` 而 ER 用 `style="undefined;;;undefined"`。
22. **flowchart edge class 重复** —— upstream `insertEdge` 重复 thickness/pattern classes。
23. **genColor CSS 只在 borderColorArray 非空时输出** —— 默认主题无 borderColorArray，requirement 的 genColor 段为空。
24. **ER 不需要 data-color-id** —— 只有 `redux-color`/`redux-dark-color` 主题才触发，默认主题不生成。
25. **flowchart diamond shape** —— 上游 `question.ts` 使用 `insertPolygonShape`，polygon 点为 top-right-bottom-left 格式，class 是 `label-container`（不是 `basic label-container`）。
26. **FontAwesome icon 替换** —— flowchart 标签中 `fa:fa-car` 格式需替换为 `<i class="fa fa-car"></i>`。
27. **空 edge label 高度** —— 上游空边标签的高度是 line-height（~16.3px），不是 0。
28. **flowchart vertex counter** —— 上游 `flowDb.vertexCounter++` 在每次 `ensureVertex` 时递增，包括 start/stop 节点，影响 dom_id 后缀。

## 下一步

### Wave 5 剩余

1. **ER 53→80** —— 分析 27 个失败 fixture，逐个修复（viewBox 差异、attribute-bearing 实体）
2. **state 0→byte-exact** —— 坐标已精确，需修复节点 ID/形状/edge path d
3. **flowchart 0→byte-exact** —— padding/shape 修复已落地，需继续对齐 viewBox 和 edge path
4. **class classBox shape** —— 最大工作量，rough.js outline + sections
5. **requirement 0→byte-exact** —— CSS 已 byte-exact，需修复节点/边 SVG 格式

### Wave 6

- **gantt** 骨架已有，需完善 renderer（chrono 依赖）
- **mindmap**（tidy-tree layout）

### Wave 7

- **sequence** / **c4** / **gitGraph**
