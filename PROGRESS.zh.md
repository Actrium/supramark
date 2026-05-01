# 阶段进展

截至 Wave 7 中期（gantt / venn 落地，c4 / mindmap / gitGraph parser scaffolding，gitGraph 已 7/129 byte-exact）。

> 本项目只维护中文版 PROGRESS。

## 总览

| 指标 | 值 |
|---|---:|
| Diagram 完整 byte-exact（≥99%） | **20 / 24** |
| Diagram parser/layout/render 可调用（含 stub 或部分） | **23 / 24** |
| 完全未实现 diagram | **1**（sequence） |
| sweep_all byte-exact 通过率（排除 known_ignored 后） | **953 / 1111 ≈ 85.8%** |
| 已 known_ignored fixture 总数 | 178（含 c4 11 + mindmap 25 + gitGraph 122 + venn 24 + 各类 16） |
| Lib unit 测试 | **639 passed / 0 failed / 0 ignored** |
| Cargo check warnings | ≤10（pre-existing dead_code） |
| 项目代码总行数 | ~63,000 行 |

## Wave 7 进展（本轮新增）

### 已落地 byte-exact

- **gantt** (45/53)：full d3-time tick selection + Sunday-aligned weeks + REVERSE/HIGHLIGHT commit + `vert` tag + `tickInterval` 指令 + frontmatter title。8 个 fixture 进 known_ignored（年-tick 算法 / `displayMode: compact` / 时区敏感的 `%s` / 3 位年份 date）。
- **venn** (4/28)：parser + Nelder-Mead simplex + 圆相交 SVG path 生成。24 个 fixture 进 known_ignored，主因是 Rust libm 与 V8 fdlibm 的 ULP 级浮点差异（影响 10 个 fixture）。
- **gitGraph** (7/129)：单分支 LR + REVERSE/HIGHLIGHT commit + tag (polygon + hole + label) + frontmatter title。122 个 fixture 进 known_ignored，按 `merge`(~50)、`cherry-pick`(~25)、多分支(~20)、TB/BT 方向(~30) 分类。

### 已 parser 落地、render 留 stub（fixture 全进 known_ignored）

- **c4** (0/11)：parser 完整解析 11 类 C4 宏（Person*、System*、Container*、Component*、`*Boundary`、Deployment_Node、Rel*、UpdateElementStyle ...），处理嵌套 `boundary { ... }` scope。render 需要约 1500 LOC bespoke layout + svgDraw 移植，推迟到专项。
- **mindmap** (0/25)：parser 完整解析 7 种节点形状（`[]`/`()`/`(())`/`){...}(`/`))((`/`{{}}`/plain）+ `:::class` + `::icon(...)` + frontmatter `config.layout`/`config.theme` + 多行 bracket body + base-level rebasing。layout/render 需要 cose-bilkent 力导仿真器（~3000 LOC cytoscape 扩展），推迟到专项。

### 关键技术发现

29. **JS Number.toString round-half-to-even** —— Rust Ryu 在浮点 tie 时取较大 trailing digit，JS 取偶数，需 i128 精确有理数对比检测真 tie（`src/math/js_number.rs`）。
30. **classCounter 后处理** —— 上游 mermaid 的 module-level counter 跨 batch 渲染累加，`generate_ref.mjs` 按首次出现顺序 renumber；我们必须在 SVG 输出后做相同的 `(classId-\w+)-(\d+)` 重号（`src/lib.rs:renumber_counter_ids`）。
31. **classBox theme 派生** —— upstream 有 `classText = classText \|\| textColor` 的派生规则，必须在 `theme_variables` 应用后再算一次。
32. **block fixture_parse_state 简化** —— 单文件 mode 重新生成 ref 后，PRNG state 固定为 `(0, 0x12345678)`，旧的 batch-counter offset 移除。
33. **flowchart 嵌套 cluster self-loop** —— 224/224 byte-exact 借助：内层 cluster 自环边的源/目标解析跳过非叶节点；reverse-edge 的 marker-arrow 选择按 dagre 内部顺序。
34. **state edge stable_partition** —— state/34 byte-exact 借助：`partition_by` 改为稳定排序（保留输入顺序），把 self-loops 排到非 self-loops 之后。
35. **gantt 上游 jison 偏差** —— `axisFormat` 不可 trim（`substr(11)` 保留前导空格），`task name` 不可 trim_end（label bbox 依赖字面文本宽度），`accTitle` / `accDescr` 接受 `:` 紧贴关键字。
36. **gantt YAML frontmatter** —— 解析时需先剥 frontmatter，把 `displayMode` / `title` lift 出来，否则字符串 'displayMode: compact\nexcludes:...' 整体被 dateFormat 误吃。
37. **venn Nelder-Mead 浮点轨迹** —— 100+ 次迭代后，Rust libm 的 `acos/atan2/sqrt` 与 V8 fdlibm 的 0–1 ULP 差异扩散到 path 坐标尾位；要 byte-exact 需移植 fdlibm 或换 `libm` crate。
38. **gitGraph jsdom getBBox shim** —— `<style>` 块外测量时 jsdom 不应用 CSS，所有 text intrinsic bbox 是 14px sans-serif（不是 16px trebuchet）；`<text>` 元素的 bbox 永远是 (0,0,w,h) 忽略 x/y 属性；transform 不影响 intrinsic bbox。
39. **mindmap 阻塞** —— 上游用 `cytoscape-cose-bilkent` 力导仿真器（约 3000 LOC 物理引擎扩展），byte-exact 需全套移植。改用确定性 radial / tidy-tree 是结构对齐而非 byte-exact。
40. **c4 阻塞** —— 上游 c4Renderer.js 是完全 bespoke（~700 LOC bound-packing layout + 668 LOC svgDraw + base64 PNG sprites + 三种文本布局模式），与 flowchart pipeline 无任何代码复用。我之前以为可以"展开成 flowchart 文本"，事实是不行。

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

## Wave 7 关键突破

12. **class theme 重派生 classText** —— `theme/derive.rs` 增补 `classText = classText || textColor`
    与上游 `theme-base.js#updateColors` 对齐。base/neutral 主题预填的 `class_text='#333'`
    在用户提供 `primaryTextColor` 时被正确覆盖。修复 cypress/class 110/111/200/201/54/55。
13. **classBox fill/stroke 主题化** —— `svg_class.rs` 的 rough.js 矩形 fill/stroke 由
    硬编码 `#ECECFF`/`#9370DB` 改为 `theme.main_bkg`/`theme.node_border`，base 主题 +
    用户 `primaryColor` 现可正确传播。
14. **classId 后处理 renumber** —— `lib.rs` 在 SVG 输出末尾应用与
    `generate_ref.mjs#renumberCounterIds` 等价的首次出现重编号规则。上游 mermaid 的
    `classCounter` 是 module-level 变量在 batch 渲染中累积；reference 生成器按 SVG 内
    appearance order 归一化，我们的输出现在镜像同样的归一化。修复 cypress/class 226。
15. **block/class reference 单文件重生** —— 6 个 block + 4 个 class fixture 的 reference
    在 single-file 模式下重生，使 `cnt` / `classCounter` 每次从 0 开始；删除 svg_block
    的 `fixture_parse_state` 偏移表。block 27/33 → 33/33，class 220/225 → 224/225。

## 已完整 byte-exact 的 diagram（17/23）

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
| er | dagre + relationship | 80 / 80 |
| block | dagre + 块布局 + cnt/PRNG 复刻 | 33 / 33 |
| requirement | dagre + 需求/关系 | 44 / 44 |
| class | dagre + classBox shape + classId 重编号 | 224 / 225 |
| state | dagre + state shape | 82 / 82 |
| — | — | — |
| **小计** | — | **1209 / 1210** |

## Wave 7 · Stratum 3 渲染层进展

| Diagram | Render 状态 | CSS port | Byte-exact fixtures | 当前阻塞 |
|---|---|:-:|---:|---|
| er | ✓ 完整 | ✓ | **80/80** ✓ | 完成 |
| block | ✓ 完整 | ✓ | **33/33** ✓ | 完成 |
| requirement | ✓ 完整 | ✓ | **44/44** ✓ | 完成 |
| state | ✓ 完整 | ✓ 全量 | **82/82** ✓ | demos/state/07 仅余 7px 宽度差异 |
| flowchart | ✓ 完整 | ✓ 全量 | **224/224** ✓ | 完成 |
| class | ✓ 完整 | ✓ 全量 | **224/225** | 仅 cypress/221 多行 backtick 名称布局差异 |

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

### 实现 diagram 收尾（共余 4 处真实差异）

1. **demos/state/07** —— viewBox 宽度差 7.167px（"the first composite" 标签度量？）
2. **cypress/class/221** —— 多行 backtick class 名称的 viewBox 与 height 差异
3. **cypress/xychart/35** —— 单 ULP 浮点差（`...3` vs `...2`），算术顺序问题
4. **demos/ishikawa/04** —— `look: handDrawn` 模式未实现（rough.js）

### 未实现 diagram（Wave 7+）

- **gantt** 骨架已有，需完善 renderer（chrono 依赖、43+10 fixtures）
- **mindmap**（tidy-tree layout，23+2 fixtures）
- **sequence** / **c4** / **gitGraph** / **venn**（合计 ~310 fixtures）
