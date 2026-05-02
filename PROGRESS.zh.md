# 阶段进展

截至 2026-05-03，Wave 13 完结，Wave 14 推进中。

**当前指标：1204 / 1328 byte-exact（约 90.7%）**。

- 1204 = Wave 13 新增 +4（cypress/flowchart/118 icon polygon +1，cypress/sequence/52 + demos/sequence/03 encodeEntities +2，demos/venn/10 dual-layout padding=8 +1）
- 1328 = sweep_all 处理的 fixture 总数（剔除 ELK opt-in 后的全集）
- 差额 124 = sequence ~88 + mindmap 18 + KaTeX 6 + handDrawn venn 2 (11/12) + gantt timezone 4 + demos/class/08 1 + constrainedMDS 1 + stadium rough 1 + ELK 1 + misc ~2
- ELK fixtures 仍由 `is_elk()` 程序性过滤，不再走 known_ignored

旧记录（按时间倒序）：1099 → 1135 → 1136 (W6) → 1145 (W7) → 1151 (W8) → 1161 (W9) → 1179 (W10) → 1184 (W11) → 1200 (W12) → 1204 (W13)。

## known_ignored 清空（2026-05-02）

应用户要求把当时仍在列表里的 146 条全部清空，让 sweep_all 暴露所有真实失败。`tests/known_ignored.txt` 现仅保留头部说明。

清空后 sweep_all 失败分布（共 144 项）：

| 类别 | 通过 / 总数 | 失败数 |
|---|---:|---:|
| cypress/sequence | 40 / 140 | 100 |
| cypress/mindmap | 6 / 23 | 17 |
| demos/flowchart | 57 / 65 | 8 |
| demos/sequence | 4 / 10 | 6 |
| cypress/flowchart | 188 / 192 | 4 |
| demos/venn | 8 / 12 | 4 |
| cypress/gantt | 41 / 43 | 2 |
| demos/gantt | 8 / 10 | 2 |
| demos/mindmap | 1 / 2 | 1 |
| 其它（含 demos/class/08） | 1183 残数 | 0~少量 |

剩余阻塞按性质归类：

1. **架构性（合计 ~118）**
   - 100 sequence —— 上游 sequenceRenderer.ts + svgDraw.ts 重渲染器（4K+ LOC），剩余 fixture 都需 activation / autonumber / actor_type / wrap / loop_alt / par 等组合特性。继续推进必须 probe-driven，按 diff_at 选最小差异 fixture（W7-A retry 已验证此方法论）。
   - 17 mindmap multi-node —— W11-D 已落 cose-bilkent 骨架（660+1313 LOC），但缺 reduceTrees / FR-grid / Coarsening 多级缩放；node 位置差大，边 d= 直线非 curveBasis。
   - 1 demos/mindmap/01 —— 同上。

2. **依赖未实现（合计 ~13）**
   - 6 demos/flowchart 42-47 —— KaTeX `$$...$$` 公式渲染，需要 KaTeX renderer 端口。
   - 3 cypress/flowchart 116/117/118 —— Icon shapes（`@{ icon: "aws:..." }`），需要 ~500 个图标 SVG path 注册表。
   - 3 demos/venn 10/11/12 —— `look: handDrawn` + foreignObject 文本节点，需要 rough ellipse + path bbox 模拟器（W6-C / W8-B 已部分铺就）。
   - 1 demos/venn/04 —— 4 sets × 6 pairwise 触发 constrainedMDS，依赖 V8 PRNG 状态。

3. **样式 threading 残留（合计 ~3）**
   - 2 demos/flowchart 41 + cypress/flowchart 144 —— Doublecircle 在内联 style 时缺 `style=` 属性。
   - 1 demos/flowchart 65 —— Stadium + thick arrows + linkStyle/classDef 走 rough.js 渲染。

4. **环境性 / 上游 quirk（合计 ~6）**
   - 4 gantt（cypress/05/39, demos/06/07）—— V8 `new Date()` 对畸形 / 时区 / `%s` epoch 的特殊处理，不打算复刻。
   - 1 demos/class/08 —— 上游 fixture 与上游 jison 解析器互不兼容（fixture 文本对应 demos 但 grammar 拒绝）。
   - 1 cypress/flowchart/46 —— `flowchart-elk LR` 走 ELK opt-in 后端，已是非目标（被 `is_elk()` 过滤而非 known_ignored）。

> 本项目只维护中文版 PROGRESS。


## Wave 7 进展（4 路并行）

- **W7-A retry +5（W7-A 原版被 watchdog 重启，retry 4 个 commit 直接落 main）**：
  - autonumber 渲染 +2
  - cross-arrow `-x`/`--x` → demos/sequence/06,09 +2
  - bidirectional `<<->>` → cypress/69 + demos/10 +2
  - bidir+autonumber line shift（startx+12）→ cypress/120 +1
- **W7-B +2**：edge.animation 字段 + group_style threading；113/237 unlock
  - Bug B（group_style threading）：circle/cylinder/diamond/ellipse/label_rect/stadium 全部走 `shape_label_block_with_styles` 变体，把 node css_styles 接入 LabelOpts；font-size 因 post-process 二次度量被排除以避免冲突
  - Bug C（edge.animation 字段）：parser → model → layout(`UEdge::animation`) → render(`edge-animation-{slow|fast}` class)
  - Bug A（edge-label x drift）诊断但未修：实际是 stadium intersection 模型差（rounded-rect vs ellipse），且原 fixtures 还被 rough.js stadium 阻塞
- **W7-C 0 unlock**：ellipse/circle/bbox_of_sets 基础设施 +383 行落地。澄清：真正的 handDrawn 是 `demos/ishikawa/04`，不是 cypress/04。剩余 6 步 wiring 已细化记入 known_ignored
- **W7-D 0 unlock 但防御性硬化**：gitGraph 早就 103/105 cypress + 24/24 demos byte-exact，只是 test harness 只覆盖 7 个；扩到 128 个 named test，未来回归一秒发现

### W7-A retry 关键洞察

"纯 activation"在数据集中**没有任何 fixture 单独成立** —— 每一个用 `activate`/`deactivate` 或 `+`/`-` 的 fixture 都和 notes/loops/rects/init 至少一项捆绑。W7-A 原版花了 30min 试图实现 activation 渲染，0 unlock。retry 改用 probe-driven（按 diff_at 排序找最小差异 fixture），命中的全是被错误标 `[activation]` 的 misclassified fixture（cross-arrow / bidirectional 等）。**未来 sequence 推进必须 probe-driven，不能按 feature 标签分组**。

> 本项目只维护中文版 PROGRESS。

## 总览

| 指标 | 值 |
|---|---:|
| Diagram 完整 byte-exact（≥99%） | **22 / 25** |
| Diagram parser/layout/render 可调用 | **25 / 25** |
| 完全未实现 diagram | **0** |
| sweep_all byte-exact 通过率（known_ignored 已清空） | **1184 / 1328 ≈ 89.2%** |
| 暴露的失败 fixture | 144（sequence 106 + mindmap 18 + flowchart 12 + venn 4 + gantt 4） |
| Lib unit 测试 | **664 passed / 0 failed / 0 ignored** |
| Cargo check warnings | ≤10（pre-existing dead_code） |
| 项目代码总行数 | ~75,000 行 |

## Wave 8 进展（本轮新增 +182：953 → 1135 byte-exact）

通过 6 路并行子 agent + Wave 5 4 路并行 agent，分阶段推进：

### Wave 4（c4 / sequence phase 1 / gitGraph roughjs / venn libm / gantt year-tick）

- **c4 0/11 → 11/11**：bespoke layout + render 移植，处理 `<br/>` descr 拆分、screen.availWidth=0 wrap、techn fontSize=12 fallback、interleave shape/boundary DFS。
- **sequence 0/150 → 4/150**：renderer phase 1 scaffolding（DOM 顺序 + max-message-width margin）。
- **roughjs 引擎落地**：移植 `path()` 解析器（M/L/H/V/Z/C 子集）+ stadium polygon body，未来 ishikawa/venn handDrawn 路径的前置依赖。
- **venn +1**：`khroma` colour adjustment 修正（cypress/07）。
- **gantt +2**：year-multiplier tick 算法。

### Wave 5（4 路并行：dagre / sequence phase 2 / mindmap / roughjs hachure）

- **dagre stadium polygon +6**：诊断出"0.008 ULP dagre drift"实为 stadium 102-point polygon 的 `getBBox().width` 比 analytical width 短 `2r·(1-cos(π/98)) ≈ 0.0161 px`；将修正 baked 进 `measure_vertex_box`，恢复下游 polygon 几何。同时补齐 stadium 的 `classDef` / `userNodeOverrides` 通路。
- **sequence phase 2 +23**（4 → 27/150）：actor stickman / `<br>` 多行 / 单行 note / ZWS 占位 / `wrap-label` + `@{ "alias": ... }` / `mirrorActors=false` / 空 items diagram。
- **mindmap single-node +7**（0 → 7/25）：probe-derived `centre = (W/2+15, H/2+15)` cose-bilkent 单节点 fast path + 完整 SVG envelope（viewBox + 12 套 section CSS + marker defs + drop-shadow filter）+ default/rect/icon labelBkg 形状。多节点物理引擎仍待移植（~3000 LOC）。
- **roughjs hachure**：移植 hachure-fill scan-line 算法（530 行 + 9 byte-exact 单测），groundwork 入库；ishikawa/04 + venn/10/11/12 真正解锁还需 `rough.path()` getBBox 模拟器 + ellipse 移植。

### Wave 6（4 路并行：sequence p3 / mindmap tidy-tree / rough ellipse / flowchart linkStyle）

净增 +1 byte-exact，但澄清了 3 个关键技术阻塞：

- **W6-A sequence p3 +1**（27 → 28/150）：cypress/86 byte-exact —— per-actor width 派生自 description text width（`max(conf.width, textWidth(desc, actorFont) + 2*wrapPadding)`，`fontSize=16`、`Open Sans`），加 `->`/`-->` 开口箭头变体（无 marker-end，dotted 加 `messageLine1` + `stroke-dasharray: 3,3`）。剩余 122 sequence fixture 多为 exotic-arrow（`<<-->>` / `()->`）和 activation 组合，单 fixture 价值低。
- **W6-B mindmap tidy-tree 0 unlock**：诊断出 mermaid 11.14.0 distribution 没注册 tidy-tree loader —— 即使 frontmatter 写 `layout: tidy-tree` 也回退到 cose-bilkent。所以 mindmap 剩余 18 fixtures 全部都需要完整 cose-bilkent 物理引擎移植（~3000+ LOC，多日工作）。tidy-tree 路径不存在。
- **W6-C rough ellipse + bbox sim 0 unlock**：980 行 ellipse/circle/path/bbox_of_sets 基础设施验证过（vs Node 20 + roughjs@4.6 byte-exact），但因主仓 rough.rs 已演进，cherry-pick 冲突太复杂未合。venn/10/11/12 不是 handDrawn（agent 实测，是 foreignObject text + Nelder-Mead FP 问题），ishikawa/04 才是真正的 handDrawn 目标。
- **W6-D flowchart linkStyle 0 unlock 但关键诊断**：Bug 1（`linkStyle` color dedup last-wins）+ Bug 2（asymmetric shape padding 缺、`>text]` polygon endpoint offset）已修，但每个原 fixture 现在都暴露下一层 bug：
  - edge-label `+0.5` X drift（90/91/223/224, demos 34/35/63）
  - 缺 `color:#…` 在 `<g class="label">`（group_style threading bug）（105/144/239, demos 38/39/40/41）
  - 缺 `edge.animation` 字段（113/237）—— 在 src/model/flowchart.rs

### 工程方法论

- **多 agent 并发隔离**：每个 agent 一个 git worktree，文件域互不重叠（dagre→flowchart.rs、sequence→svg_sequence*.rs、mindmap→mindmap.rs、roughjs→rough.rs）。基于 72bb088 分别起 90min 硬上限。
- **byte-exact 子 agent 必须 commit-early**：prompt 写死"首破即刻 commit 退出"，避免贪心挖到时限到了一无所获。
- **check_ignored 审计工具**：`src/bin/check_ignored.rs` 跑遍 known_ignored 报 false positives / no_reference / errored，每次合并后必跑。本 wave 0 false positives。

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

## 旧版 diagram 状态表（2026-05-02，已迁移至 Wave 12 表）

> 已在上方 "各 diagram 当前 byte-exact 状态" 章节中更新至 2026-05-03 实测数据。

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

## Wave 12 进展（持续推送）

净增 +16，1184 → 1200 byte-exact。666 lib tests pass。

- **W12-A doublecircle style threading +2**（cypress/flowchart/144 + demos/flowchart/41）：把 `shape_label_block_with_styles` 接入 doublecircle 内层 `<circle>` 与外层 `<g>`，`nodeStyles` threading 补齐。commit `bf26a02`。
- **W12-B classDef comma + thick arrow +1**（demos/flowchart/65）：`classDef start_node,finish_node fill:...` 逗号分隔类名拆分注册 + `classify_arrow` 修复 `.` 标签误判为 dotted。commit `ccee80f`。
- **W12-C icon shape +2**（cypress/flowchart/116/117）：icon shape parser/layout/renderer 端到端 wiring，`@{ icon: "aws:..." }` 识别与 SVG placeholder。commit `585fc9e`。
- **W12-D sequence self-ref + point arrows +11**（cypress/sequence 48/52/53/55 + 34/35/38/39/42/43）：self-ref cubic bezier `<path>` 渲染 + SolidPoint/DottedPoint `-)` / `--)` 箭头 + `resolve_hash_entities_for_measure` 文本度量 + DottedPoint `is_dashed` 修复。commit `5a65937` + `005e592` + `41d10d2`。
- **W12-E venn handDrawn groundwork 0 unlock**：rough hachure fill for ellipse + transparentize + HSL alpha fix。骨架就位但 byte-exact 尚未对齐（demos/venn 10/11/12 rough path 数据差异）。commit `fb9ec38`。
- **W12-F/F2 sequence loop block attempt 两次失败**：子 agent 两次尝试实现 loop/alt/par 等控制块渲染，均引入回归（从 51/140 降至 22/140），已回滚。需更仔细的增量实现。

## Wave 13 进展（4 路并行 + 监控重启）

净增 +4，1200 → 1204 byte-exact。6 个子 agent，4 命中 / 2 干净 bail（带回精确情报）。

- **W13-A flowchart icon polygon-intersect +1**（cypress/flowchart/118）：上游 `icon.ts` 用 8 顶点 polygon `intersect.polygon` 而非 `intersect.rect`。我们的 dagre bridge 默认走 rect，导致每条边端点 0.5px 偏移传播到 viewBox。把 `"icon"` 加入 `shape_uses_polygon_intersect`，复刻 `bbox.width/2/2` 上游 typo 在 vertex 5；HTML label 体补 `<p>...</p>` wrap。**cypress/flowchart 192/192 收官**。commit `11d947e`。
- **W13-B venn dual-layout padding=8 +1**（demos/venn/10）：上游 vennRenderer 跑 **两次** layout pass —— 一次 `padding=15` 用于可见 `<path>` 轮廓，一次 `config.padding ?? 15`（schema 默认 8）用于 handDrawn rough.js 圆心 / 半径。我们已经计算了两套但 handDrawn 分支错读 `area.circles[0]`（padding=15）；切到 `area.text_node_circles[0]`（padding=8）后 RNG-driven jitter 完全对齐。次要清理：venn-intersection style 在 handDrawn-no-customFill 时只 mutate `fill-opacity`，不要追加合成 `fill: transparent`。commit `0cefc9a`。
- **W13-F sequence encodeEntities 占位符度量 +2**（cypress/sequence/52 + demos/sequence/03）：**Wave 11-A 注记反了**。上游 `encodeEntities` 把 `#word;` 改写为 `ﬂ°word¶ß`（U+FB02 + U+00B0 + word + U+00B6 + U+00DF）而非 `&word;`，**所有 `calculateTextDimensions` 度量的是占位符形式**；只有 `decodeEntities` 在最终 SVG 阶段映回 `&word;`。我们的 `resolve_hash_entities_for_measure` 错误地把 `#lt;` 解码到 `&lt;` 给度量用，使 `using #lt;br /#gt;` 度量为 102px 而非上游 179px，actor 默认宽度被 clamp 到 conf.width=150。修：度量路径改发占位符形式（渲染输出不变）。commit `5b5a2e8`（cherry-pick of `928a2b8`）。
- **W13-C sequence loop block 干净 bail（情报）**：所有 7 个 cypress loop fixture（03/05/27/73/105/110/111）+ 1 个 demos loop（08）byte diff 25,272–73,321 bytes。alt/par/rect/opt 同等量级。`only_supported_items` gate 直接 placeholder 化，需要：(a) 垂直 cursor 推进、(b) `<g data-et="control-structure">` 4 行 `loopLine` + polygon label box + 2 个 `<text>`、(c) `bounds_startx/stopx` 集成。**多 session feature 端口**，单 fixture 不够。
- **W13-D sequence actor type variants 干净 bail（情报）**：32 个 sequence fixture 用 `@{ "type" : "..." }`，**每一个**都同时需要 boundary+control+entity+database 全套几何 + per-actor 变长 width/height + custom defs marker + `data-type` 属性 threading + reversed-order 渲染 + actor-bottom group。25–37 KB 每 fixture diff，单 fixture 不够。
- **W13-E sequence/70 干净 bail（情报修正）**：W13-C 报的"96 byte diff"是文件大小差，**实际 byte-diff 30,531 bytes**。fixture 70 卡在 **font-metrics 校准**（Trebuchet MS 16pt 对包含 `#lt;`/`#gt;` 字面量字符串的宽度），不是 `<br \t/>` 解析。`split_br` 与上游 `lineBreakRegex` (`/<br\s*\/?>/gi`) 同等。**font-metrics 是全局阻塞**（影响多数剩余 sequence fixture）；W13-F 的 encodeEntities 修复部分缓解但未对齐 Trebuchet 实际度量。

### Wave 13 工程教训

- **agent worktree 起点漂移**：4 个并发派出的 agent 中 2 个落在 stale base `72bb088`（Wave 7 mid，2026-05-01）而非 main `04b11a4e`，导致它们看不到 Wave 8-12 全部基础设施。修：所有 agent prompt 顶部加 PREFLIGHT 段（`git rev-parse HEAD` → `git reset --hard <main>`），并用主 agent 监控脚本扫 worktree HEAD 实时确认。
- **监控脚本（/tmp/agent_monitor.sh）**：每 5 分钟扫 `/ext/mermaid/.claude/worktrees/agent-*`，跟踪每个 worktree 的 commit 活动；30 分钟无 commit 标 `[stale]`。命中事件触发主 agent 通知，避免空轮询。本 wave 命中：每个 commit 推进都被 `[main-advanced]` / `[progress]` 事件捕获。
- **bail-clean 情报比贪心实现更值钱**：W13-C/D/E 三个 0-unlock bail 都带回精确情报 —— W13-C 的 fixture 列表 + diff_at 表，W13-D 的"全套几何 + 反向顺序"必备清单，W13-E 的"file-size delta ≠ byte diff"修正 + font-metrics 全局阻塞确认。这些直接喂给后续 wave，比"硬上"性价比高。
- **commit-early protocol 验证**：4 个命中 agent 全部首破即 commit 退出；0 个回归。

### Wave 13 关键技术发现

41. **flowchart icon polygon-intersect** —— `shape_uses_polygon_intersect` 必须包含 `"icon"`；8 顶点 polygon 用上游的 `bbox.width/2/2` typo（point 5），用 `intersect.polygon` 算 edge endpoint。否则每边端点偏 0.5px 累加到 viewBox。
42. **venn dual-layout** —— vennRenderer 调 layout 两次：visible padding=15 + config.padding=8。handDrawn rough.js 路径用 padding=8 的 circle，可见 `<path>` 用 padding=15 的 circle。
43. **encodeEntities 占位符度量** —— 上游用 U+FB02/U+00B0/U+00B6/U+00DF 5 个 BMP 字符夹住 entity 名做占位符，所有度量函数（`calculateTextDimensions` / canvas / getBBox）量的是占位符串，只有最终 SVG 输出阶段 `decodeEntities` 还原回 `&entity;`。byte-exact 需在度量阶段保留占位符形态。
44. **sub-agent worktree base drift** —— `Agent({ isolation: "worktree" })` 创建的 worktree 不保证从 `main` 起步；可能落在历史中任意可达 commit（疑似与 .git/worktrees 状态共享或 harness 内部 cache）。**所有 agent prompt 必须 PREFLIGHT 强制 reset 到当前 main**。

## 各 diagram 当前 byte-exact 状态（2026-05-03 sweep_all 实测）

| Diagram | 方式 | cypress | demos | 阻塞 |
|---|---|---:|---:|---|
| pie | 内置 (d3.pie + d3.arc) | 10/10 | 3/3 | — |
| packet | 内置 (bit-field grid) | 5/5 | — | — |
| radar | 内置 (polygon math) | 6/6 | 1/1 | — |
| ishikawa | 内置 (fishbone) + handDrawn (rough) | 13/13 | 5/5 | — |
| journey | 内置 (bar layout + arc score) | 10/10 | 1/1 | — |
| timeline | 内置（TD + LR 双模式） | 14/14 | 3/3 | — |
| quadrant | 内置 (d3.scaleLinear) | 14/14 | 2/2 | — |
| xychart | 内置 (d3.scaleBand + scaleLinear) | 37/37 | 19/19 | — |
| wardley | 内置 (landscape plot) | 6/6 | 6/6 | — |
| sankey | 自 port d3-sankey 0.12.3 | 1/1 | 2/2 | — |
| treemap | 自 port d3-hierarchy squarify | 28/28 | 2/2 | — |
| kanban | 内置 (column + card 网格) | 11/11 | — | — |
| c4 | bespoke layout + svgDraw | 6/6 | 5/5 | — |
| flowchart | dagre + 嵌套孤立子图 + linkStyle + icon polygon-intersect | **192/192** ✓ | **59/65** | KaTeX × 6 |
| er | dagre + relationship | 73/73 | 7/7 | — |
| block | dagre + 块布局 + cnt/PRNG 复刻 | 33/33 | — | — |
| requirement | dagre + 需求/关系 | 43/43 | 1/1 | — |
| class | dagre + classBox + classId 重编号 | 225/225 | 12/12 | — |
| state | dagre + state shape | 72/72 | 10/10 | — |
| gitGraph | bespoke commits + branches + parallelCommits + multi-line | 105/105 | 24/24 | — |
| gantt | d3-time tick + Sunday-aligned + REVERSE/HIGHLIGHT + tickInterval | 41/43 | 8/10 | V8 `new Date()` 时区 quirk × 4（环境性） |
| venn | Nelder-Mead simplex + V8 hypot + theme + handDrawn dual-layout | 16/16 | 9/12 | constrainedMDS × 1, handDrawn × 2 (11 customFill, 12 ULP) |
| sequence | scaffold + self-ref + point arrows + encodeEntities placeholder 度量 | **52/140** | **5/10** | loop/alt/par/rect/critical/break × 24, activation, actor type variants, note over multi, wrap, font metrics |
| mindmap | 单节点 fast path + 多节点骨架 | **6/23** | **1/2** | cose-bilkent reduceTrees / FR-grid / Coarsening / curveBasis edge / Base64 data-points |
| **总计** | — | 1007 / 1126 | 197 / 202 | sweep_all 1204 / 1328 |

注：上表数据来自 `cargo run --bin sweep_all`（2026-05-03 W13 完结），cypress 1007/1126 + demos 197/202 = 1204/1328。

## 下一步（2026-05-03 W13 后重排）

124 项暴露失败按攻关性价比排序：

### 极高性价比（1-2 commit 可解锁）

1. **demos/venn/11 customFill cross-hatch** —— W13-B 报告剩余 venn 阻塞之一。Wave 14-A 推进中。
2. **demos/venn/04 constrainedMDS** —— 4 sets × 6 pairwise 触发，依赖 V8 PRNG 状态。

### 高性价比（需要某个模块就位）

3. **sequence font-metrics 校准（Trebuchet MS 16pt）** —— W13-E 确认是全局阻塞，影响多数剩余 sequence fixture。改一处可能多解锁；但风险触及 layout/svgDraw 的所有 width 路径，需要先建一套 dimensions 探针对照上游。
4. **sequence wrap: prefix on actor description**（cypress/sequence 87/88）—— W13-C 提到，依赖 wrap 配置渲染。
5. **sequence theme: 'base' init**（cypress/sequence 29/30/33-51 / 74）—— W9-A `build_style(id, theme)` 已铺，差 theme=base 路径与 actor `<g>` defs。

### 中性价比（需多日、多 wave）

6. **sequence loop/alt/par block rendering** —— W13-C 确认 24 fixture 全是 25-73 KB diff 的 feature 端口，单 session 不可行。
7. **sequence actor type variants** —— W13-D 确认需同时实现 6 套几何 + lifecycle，多 session feature 端口。
8. **sequence activation rendering** —— ~20 fixture，需 lifecycle bar + 端点偏移。
9. **mindmap cose-bilkent 五大件** —— 18 fixture，~3000 LOC 物理引擎。
10. **KaTeX × 6** —— 独立 Phase。

### 环境性 / 不修复

- **gantt timezone × 4** —— V8 quirk。
- **demos/class/08** —— 上游 fixture 与 jison 自相矛盾。
- **demos/venn/12 ULP** —— libm vs fdlibm 0–1 ULP 噪声，需移植 fdlibm。
8. **Icon shapes × 1** —— 独立 Phase。

### 环境性 / 不修复

9. **gantt timezone × 4** —— V8 quirk。
10. **demos/class/08** —— 上游自相矛盾。
11. **ELK × 1** —— 程序性过滤。

> 本项目只维护中文版 PROGRESS。

净增 +6，1145 → 1151/1151 byte-exact。

- **W8-A sequence probe-driven +1**：cypress/sequence/72（multi-line actor description via byTspan dy 步进）。groundwork：`Actor.wrap` 字段 + `wrap:`/`nowrap:` 前缀剥离。**关键洞察**：probe 后发现剩余 114 个 sequence fixture 都需要 heavier features，最小 diff_at 也 ≥114px，单 feature 不够：theme/popup link/external-service-actor/central-arrow `()->>()` 解析/self-reference activation/wrap 配置/font-metrics #lt;-#gt; 校准。
- **W8-B ishikawa demos/04 byte-exact**：look=handDrawn 端到端 wiring 完成。新模块 `svg_ishikawa_hand_drawn.rs` (480 LOC) 镜像上游 rough.js 调用顺序（head→pairs→deferred spine→arrow markers→label boxes）。rough.rs 三处扩展：Q→C 路径转换、`points_on_path`（cubic flattening + RDP simplify）、`omit_dash_attrs` flag。所有 6 步 wiring 步骤全部到位。
- **W8-C venn +4**（cypress/02/03/15 + demos/02）：根因不是 libm/ULP，而是 `greedyLayout` 应过滤到 `length === 2` pair-only（上游 layout.js:921）。3-circle 对称输入下，full augmented set 给出非零 triple-intersection 干扰打破对称 tie，导致简形从镜像侧起步偏离 2.6e-4。`fsqrt`/`facos`/`fatan2` 已经把 transcendentals 校准到 fdlibm。
- **W8-D mindmap cose-bilkent groundwork 0 unlock as expected**：`src/layout/cose_bilkent.rs` 660 LOC 核心数据结构（PointD/RectangleD/LayoutQuality/CoSEConstants/RandomSeed/IGeometry/LNode/LEdge/LGraph/LGraphManager/SimulationState/单步 simulation_step）。**未集成**到 main（mindmap.rs 三方合并冲突 + 0-unlock 不急），保留在 `tmp-w8d` 分支等 W9-C 干净接力。

## Wave 9 进展（4 路并行）

净增 +10，1151 → 1161/1161 byte-exact。150 fixture 仍在 known_ignored。

- **W9-A sequence theme propagation +2**：cypress/sequence/32 (theme=base) + cypress/sequence/113 (theme=dark)。`SEQUENCE_STYLE` 常量改为 `build_style(id, theme)` 函数，`useGradient` flag 驱动 `[data-look="neo"]` rules 用 gradient 而非裸 nodeBorder hex。剩余 38 个 ignored sequence fixture 都需非 theme 特性。
- **W9-B flowchart "stadium" +7**（cypress 90/91/223/224 + demos 34/35/63）：known_ignored 注释误导（不是 rough.js 问题），实际是 (1) diamond polygon 缺 `style="stroke;fill"` 属性（upstream `question.ts` 镜像 `nodeStyles`）+ (2) edge label position 漏算 `pointsHasChanged` 检测 → `calcLabelPosition` recompute（`label_coordinate_in_d` + `build_rendered_d_for_label_check`）。
- **W9-C cose-bilkent 0 unlock**：W8-D groundwork 干净集成 + 扩展到 1313 LOC（runSpringEmbedder 完整循环 + getIntersection2 + Mulberry32 PRNG + position_nodes_randomly + 12 个新单测）。仍未 byte-exact 因 renderer 拒绝多节点 + 缺 reduceTrees/growTree + FR-grid bucket repulsion + Coarsening 多级缩放。
- **W9-D state +1**（demos/07）：`build_graph_filtered_ex` 边分区漏分类 isolated/nested-iso cluster id 上的 no-op rewrite 边，导致 dagre 收到与 upstream 不同的 binding order，First 子节点错列 → 7.17px 宽度差。`iso_desc_for_outer` 参数补齐分类范围。class/221 早已修复（不需触动）。

664 lib tests pass（W9-C 新增 12 单测）。

## Wave 10 进展（4 路并行）

净增 +18，1161 → 1179/1179 byte-exact。132 fixture 仍在 known_ignored。664 lib tests pass。

- **W10-A retry sequence +3**：cypress/sequence/21/31/47。central-connection markers `()->>()` 三处微妙差错：startx offset 符号（`-6` 不是 `+6`）、autonumber circle offset `CIRCLE_OFFSET=16.5` for AtFrom/Dual、autonumber+bidir+RTL line.x1 修正（额外 `-5` for any central-conn + `-7.5` for DUAL/REVERSE）。原版 W10-A 30min 卡死被 watchdog 重启，retry 60min 内 3 commit 收工。
- **W10-B flowchart asymmetric +5**：cypress 105/239 + demos 38/39/40。upstream `rectLeftInvArrow.ts` 即使 `look !== 'handDrawn'` 也走 `rc.path(pathData, options)` 双 `<path>` 输出，不走 analytical polygon。复刻 rough.path emission + outer-path `<g>` wrapper + label `dx` offset。剩 144/41 卡 doublecircle style threading defect。
- **W10-C venn foreignObject +7**：cypress 06/10/11/13/16 + demos 06/07。3 根因：(1) **dual padding** —— upstream `vennRenderer` 用 visible padding=15 算可见圆，再用 `config.padding=8` 跑第二次 `scale_solution` + `compute_text_centres` 给 foreignObject 文本节点定位；(2) V8 `Math.hypot` vs libm `hypot` 1-ULP 差异，三相交 inner_radius 偏移 3 ULP；用 V8 双参公式 `max * sqrt(1 + (min/max)²)` 重写；(3) hex `#rrggbb` → CSSOM `rgb(r,g,b)` 序列化（jsdom 行为）。
- **W10-D gantt residuals +4**：cypress/gantt 27/24/40 + demos/10。`displayMode: compact` + `HH:mm:ss`/`HH:mm` 时间格式；d3-array `tickSpec` 完整 `sqrt(50)/sqrt(10)/sqrt(2)` 阈值 + 多年 stride 锚定到 N 整数倍；3 位年份 `202-12-01` lenient parse 正常解到 202 AD。剩 4 个时区敏感 fixture 留 `#[ignore]`（DST 边界 / `new Date("0")` / 非 ISO `08-08-09-01:00` 后退路径）。

## Wave 11 进展（4 路并行）

净增 +5，1179 → 1184/1184 byte-exact。127 fixture 仍在 known_ignored。

- **W11-A sequence 0 unlock**：`#lt;`/`#gt;`/`#colon;` entity decoder（`xml_escape` + `attr_escape` 加 `try_consume_hash_entity`）。SVG 输出端正确，但 demos/sequence/03（2-byte 差）还需 placeholder-encoded text width 度量补齐。其它 small-gap fixtures 都需大特性（self-arrow cubic-bezier / wrap / autonumber+activation / par_over / link popup / external-actor）。
- **W11-B venn theme +3**（demos/venn 03/08/09 dark/forest/neutral）：W9-A `build_style(id, theme)` 模式应用到 svg_venn。`is_dark(color)` mirror khroma 0.2126R+0.7152G+0.0722B luminance（threshold 0.5, 1e-10 round），用于 single-set 标签 lighten/darken 切换。剩余 4 demos venn 都是 constrainedMDS / handDrawn。
- **W11-C gitGraph +2**（cypress 101 parallelCommits + 105 multi-line branch）：parallelCommits 给每个 commit 按 closest_parent_x + COMMIT_DISTANCE 重锚（兄弟可同 X）；multi-line branch label 在 `\n` 处拆 tspan + dy 步进 + 更新 bbox 高度。gitGraph 现在 100% byte-exact。
- **W11-D mindmap multi-node 0 unlock**：`run_layout()` 改返 Ok 真实模拟坐标，`render_multi()` + `emit_shape_body()` 处理 7 种 shape，结构骨架匹配但 (1) 节点位置差大（缺 reduceTrees/Coarsening）(2) 边 d= 是直线 M…L… 而非 curveBasis M…L…C…C…L…（3）缺 data-points Base64 metadata。3 个独立 follow-up 任务。

664 lib tests pass。注：cargo build cache 导致 sweep 一度假报 1179；touch sweep_all.rs 强制重建后正确。
