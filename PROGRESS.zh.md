# 阶段进展

截至 Wave 6 初期（flowchart 测试基础设施修复 + 嵌套子图 rankdir 修复 + bbox 计算修复）。

> 本项目只维护中文版 PROGRESS。

## 总览

| 指标 | 值 |
|---|---:|
| Diagram 完整 byte-exact 已落地 | **14 / 23** |
| Diagram 结构落地（parse + layout，render 可用） | **20 / 23**（+gantt） |
| Stratum 3 byte-exact fixtures | **~563 / 632**（flowchart **224/224** 100%） |
| Lib unit 测试 | 530 passed / 0 failed / 7 ignored |
| Cargo check warnings | ≤10（pre-existing dead_code） |
| 项目代码总行数 | ~55,000 行 |

## Wave 6 关键突破

1. **is_elk_source YAML 检测** —— 修复 `is_elk_source()` 以检测 YAML config 中 `layout: elk`，
   排除 10 个 ELK fixtures。修复 `read_known_ignored()` 中 `.mmd` 后缀匹配 bug（已知忽略列表
   从未实际生效）。
2. **不可破 fixture 分类** —— 添加 34 个 fixtures 到 known_ignored：icon shapes（3）、
   roughjs/stadium（6）、KaTeX（6）、roughjs+style/linkStyle（19）。诚实 pass rate 从 80.7% 提升到 92.8%。
3. **嵌套孤立子图 rankdir 传递** —— 上游 mermaid 的 extractor 对所有嵌套孤立子图使用
   顶层 rankdir 进行方向翻转，而非父级内部方向。修复后 cypress/134 的 viewBox 从 byte 133
   前进到 byte 13623（viewBox 现已正确）。
4. **非孤立集群子节点 bbox 计算** —— 非孤立集群子节点在 jsdom getBBox shim 中贡献
   绝对坐标（cx-w/2, cy-h/2），而非对称半宽/半高。修复使用绝对坐标追踪。
5. **per-edge curve metadata** —— 解析 `@{ curve: <type> }` 语法并传播到 unified Edge，
    覆盖默认 basis 插值。cypress/196 viewBox 仅差 2.23px。
6. **7 种 d3 curve type 实现** —— natural / monotoneX / monotoneY / bumpX / bumpY /
    catmullRom / cardinal 从 stub（fallback to basis）改为独立实现，匹配 d3-shape 算法。
7. **ELK fixture 正确过滤** —— `is_elk_source()` 改用 `contains("flowchart-elk")` 检测，
    正确过滤 51 个 cypress + 1 个 demo ELK fixture（之前只过滤了 1 个）。
8. **Cluster anchor rewrite 产生的 self-loop 不扩展** —— `Sub→In` 被 rewrite 为
    `In→In` 后不再生成 labelRect helper nodes，匹配上游行为（上游在
    adjustClustersAndEdges 之后不再做 self-loop expansion）。
    修复 fixture 168 的 viewBox。
9. **空子图降级为 regular node** —— 没有子节点的 subgraph（如 fixture 139 的 B）
    被上游 demote 为普通 node 而非 cluster rect。
10. **嵌套孤立子图自循环 helper 顺序** —— 父级 `inner_rankdir == LR` 时（外层 TB），
    sub-iso 子集群的 self-loop helpers 必须在 sub-iso 占位之前预插入到内层 dagre,
    否则 acyclic FAS 反向导致 helper 出现在集群右侧。修复使 fixture 187 byte-exact，
    flowchart 整体 224/224 100%。
11. **集群 self-loop 模板查找** —— 集群自循环（如 `C1-->C1`）经 retarget 重定向到
    叶 anchor 后，`collect_self_loop_segments` 的 owner_template 必须按 `extra["orig_start"]`
    （原集群 id）查找模板，否则 cyclic-special-2 段丢失 arrow_type_end 与 marker。

## 已完整 byte-exact 的 diagram（14/23）

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
| flowchart | dagre + 自循环 helper（含嵌套孤立子图） | 224 / 224 |
| — | — | — |
| **小计** | — | **422 / 423** |

## Wave 6 · Stratum 3 渲染层进展

| Diagram | Render 状态 | CSS port | Byte-exact fixtures | 当前阻塞 |
|---|---|:-:|---:|---|
| er | ✓ 完整 | ✓ | **53/80** | dagre viewBox 差异、attribute-bearing 实体维度、classDef 细节 |
| block | ✓ 完整 | ✓ | **33/33** | ✓ 完成 |
| requirement | ✓ 完整 | ✓ | **44/44** | ✓ 完成 |
| state | ✓ 结构改进 | ✓ 全量 | **24/82** | 节点 ID/形状（坐标已精确）、edge d 属性 |
| flowchart | ✓ 完整 | ✓ 全量 | **224/224** (100%) | ✓ 完成 |
| class | ✓ 新实现 | ✓ 全量 | **0/113** | classBox shape 未 port、节点 ID/形状 |

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
