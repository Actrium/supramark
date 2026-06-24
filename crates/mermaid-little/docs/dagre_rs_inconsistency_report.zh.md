# dagre-rs vs dagre.js 不一致性分析报告

> 基于 mermaid-little 项目测试结果，截至 Wave 6 初期。
> 上游参照：mermaid@11.14.0 使用 dagre-d3-es@7.0.14（dagre.js 的特定封装版本）。
> 本项目使用：[dagre-rs](https://github.com/Actrium/dagre-rs)（dagre.js 的 Rust port）。

---

## 0. 2026-04-30 Update — 复核与重分类

经 dagre-rs `cross-validate/` 矩阵（30 个 case，含 10 个新增复合图组合）双向核对，本报告若干项已被推上游或重新归类。原 §2 的 20 项分类不再完整准确，以下表为当前权威状态：

| 项 | 原分类 | 现状 |
|---|---|---|
| ① tie_keep_first | CRITICAL（建议暴露字段） | ✅ **dagre-rs 已落地**：`LayoutOptions::tie_keep_first` 是正式字段（`src/layout/types.rs:21`），文档亦说明 v3.0.x vs v0.8.5 历史分叉；mermaid-little 的 `build_layout_options()` 应保留 `tie_keep_first: true` |
| ② intersect_polygon/ellipse | CRITICAL（建议暴露 helper） | ✅ **dagre-rs 已落地**：`pub mod intersect` 暴露 `intersect_rect/intersect_ellipse/intersect_polygon`（`src/layout/intersect.rs`）；mermaid-little 已删除 vendored 实现，改为 `dagre::intersect::*` 适配层 |
| ⑥ compound 边界 panic | HIGH（dagre-rs 算法差异） | ✅ **dagre-rs 已落地**：rank 路径（`slack` / `longest_path::dfs` / `shift_ranks`）改为 unrank 节点 0-fallback，不再 panic；mermaid-little 已**删除 `catch_unwind` + flat-mode 兜底**及 `synthesise_cluster_bounds` |
| ⑩ 空 cluster panic | MEDIUM（dagre-rs 算法差异） | ✅ **已验证不再 panic**：dagre-rs 新增 `layout_does_not_panic_on_empty_subgraph_nodes` 覆盖 5 种异常拓扑（孤立空 cluster、嵌套空 inner、childless cluster、cluster-as-edge-endpoint、cross-cluster edge），全部不 panic |
| ⑰ HashMap 非确定性 | LOW（两者皆有） | ✅ **dagre-rs 已落地**：layout pipeline 全 `HashMap → BTreeMap`；新增 `layout_output_is_run_to_run_deterministic` 回归测试。mermaid-little 侧 `sub_isolated` HashMap 与 `stabilise_divider_positions` 是独立责任，**保留** |
| ⑱ 零长度 vertex | LOW（两者皆有） | ✅ **dagre-rs 已落地**：layout pipeline step 25b 加 `dedupe_adjacent_edge_points`（1e-4 epsilon）；mermaid-little 已**删除 `dedupe_collinear`** |
| **④⑤⑧⑨** 复合图 5px / 2px 偏差 | HIGH/MEDIUM（dagre-rs 算法差异） | ❌ **重新归类为"上游 mermaid-js 管线行为"**——见下 |

### ④⑤⑧⑨ 重要订正

dagre-rs 新增 cross-validate 复合图矩阵（cases 21–30）覆盖：

- single-leaf TB / LR
- 三叶链 TB / LR
- nested compound TB / LR
- cross-cluster edge TB / LR
- empty inner cluster
- fork/join inside cluster

**全部 30/30 byte-exact 对齐 @dagrejs/dagre 3.0.1-pre。** 也就是说 dagre-rs 与 dagre.js 在所有这些复合图形态上输出**完全一致**。

> 报告原本认定的"5×5 LR swap"、"flowchart +5 width"、"compound height +2×padding"、"fork/join +2 width" 都**不是 dagre-rs port 引入的偏差**，而是 mermaid-js 在 dagre 输出之后施加的 post-process（详见 mermaid-js 的 `setupGraphViewbox` / `findCommonEdges` / cluster bbox 重算路径）。

mermaid-little 现存的 `apply_flowchart_cluster_correction` 等"经验补偿"代码**不是临时 hack，是忠实复刻 mermaid-js 的后处理管线**——不应被当成技术债清除。后续维护者读到这些常数（+5 / -2 / 2×max_leaf_padding）时，应该把它们当作 mermaid-js 行为的引用表，而不是 dagre-rs 偏差的补偿。

### 当前权威分类（精简版）

| 类别 | 项 | 状态 |
|---|---|---|
| dagre-rs 已上推完成 | ①②⑥⑩⑰⑱ | 不再是不一致来源 |
| 上游 mermaid-js 管线行为（mermaid-little 必须复刻） | ③④⑤⑦⑧⑨⑪–⑯ | 必要适配，非技术债 |
| 渲染层缺失（roughjs / KaTeX / icon） | ⑲ | 与 dagre 无关 |
| 测试手段 | ⑳ | 与 dagre 无关 |

§2 起的内容仍可用于历史溯源，但"建议"段（特别是 §5.1）已基本完成或被证伪，以本节为准。

---

## 1. 总览

mermaid-little 依赖 dagre-rs 作为所有 dagre 家族 diagram（ER、state、flowchart、class、requirement、block）的布局后端。测试显示 Wave 1/2 的 13 种非 dagre diagram 已 100% byte-exact，但 Stratum 3 dagre 家族的 byte-exact 通过率仍有差距，**核心瓶颈正是 dagre-rs 与 dagre.js 的行为不一致**。

当前 Stratum 3 byte-exact 进度：

| Diagram | Fixtures | Byte-exact | Pass % | dagre-rs 布局问题数 |
|---|---:|---:|---:|---:|
| block | 33 | 33 | 100% | 0 |
| requirement | 44 | 44 | 100% | 0 |
| class | 113 | 112 | 99.1% | 0 |
| ER | 80 | 75 | 93.8% | 4 |
| state | 82 | 70 | 85.4% | 5 |
| flowchart | 318 | 268 | 84.3% | ~17 |

---

## 2. 已确认的不一致性（20 项）

### 2.1 CRITICAL 级别

#### ① Tie-breaking 行为差异（平局保留 vs 替换）

**现象**：dagre-d3-es@7.0.14（mermaid 上游实际使用）在 NetworkSimplex layering 遍历中，当 crossing count 与当前 best 相等时**保留首个 best**，不替换；@dagrejs/dagre@3.0.1-pre 及 dagre-rs 则**替换 best**。

**后果**：多 rank 图中同 rank 多节点时，节点坐标发生翻转（x/y 互换），导致 viewBox 和所有节点位置偏离上游。

**位置**：`src/layout/dagre_bridge.rs:113-121`

**补偿**：`build_layout_options()` 强制设置 `tie_keep_first: true`。这是**单个最大的坐标分歧来源**。

**影响 fixture**：ER/03、所有多 rank 分支 flowchart。

**根本原因**：dagre.js 源码 `network-simplex.js` 中 `buildLayerGraph` → `findFeasibleLayering` 的 best-keeping 逻辑在 dagre-d3-es 封装中被保留，但 @dagrejs/dagre 新版改变了这一行为。dagre-rs port 了新版行为。

**建议**：dagre-rs 应将 `tie_keep_first` 作为 `LayoutOptions` 的正式字段而非本项目的临时 patch，并在文档中说明此差异的历史来源。

---

#### ② intersect_rect vs 形状专属交集计算

**现象**：dagre-rs 的 `assign_node_intersects` 对所有节点一律使用 `intersect_rect`（AABB 矩形裁剪）。上游 mermaid-js 为每个形状注册了 `node.intersect()` 回调：diamond/question/trapezoid/lean/hexagon/subroutine 使用 `intersect.polygon`，circle/ellipse/doublecircle 使用 `intersect.ellipse/intersect.circle`。

**后果**：边端点落在节点 AABB 而非实际形状边界上，偏离量可达形状剪切量的一半（如 trapezoid 偏差 ≈ h/2 px）。

**位置**：
- `src/layout/flowchart.rs:72-85`（总述 + fix_polygon + fix_ellipse）
- `src/layout/state.rs:787-792, 1169-1176`（stateEnd / stateStart fix）
- `src/layout/dagre_bridge.rs:2627-2636`（subroutine reclip）
- `src/layout/intersect.rs:1-7`（vendored intersection 模块）

**补偿**：5 个 post-layout fix 函数在 dagre 输出之后重新计算边端点：
- `fix_polygon_edge_endpoints()` — 7 种 polygon 形状
- `fix_ellipse_edge_endpoints()` — circle/ellipse/doublecircle/cylinder
- `fix_state_end_edge_endpoints()` — stateEnd（rough circle r≈7.009 vs dagre 7）
- `fix_state_start_edge_endpoints()` — stateStart
- `reclip_polygon_intersect_endpoints()` — subroutine

**根本原因**：dagre.js 原版本身也只有 `intersectRect`，但 mermaid-js 在 dagre layout 之后调用形状的 `intersect` 回调覆盖端点。dagre-rs 作为 dagre.js 的 port，正确地只实现了 `intersectRect`；但 mermaid-little 作为 mermaid.js 的 port，需要模拟上游的形状 intersect 回调。

**建议**：dagre-rs 应暴露 `intersectPolygon` 和 `intersectEllipse` helper 函数（PLAN.md 已提及此需求），让下游项目可以在 dagre layout 之后调用这些函数而非自行 vendored 重复实现。

---

#### ③ 字体度量使用 14px vs 16px

**现象**：上游 `labelHelper` 使用 `div.getBoundingClientRect()` 在 jsdom 中量测标签，继承 SVG 根的 **14px sans-serif** 默认字体大小。dagre-rs 接收的节点 width/height 应基于 14px 度量，而非 theme.fontSize（16px）。

**后果**：使用 16px 度量时，节点宽高偏大，所有坐标偏移，viewBox 变宽/高。

**位置**：
- `src/layout/flowchart.rs:49-52`（`LABEL_FONT_SIZE = 14.0`）
- `src/layout/state.rs:83-88`（`DEFAULT_FONT_SIZE: f64 = 14.0`）
- `src/layout/er.rs:37`（`LABEL_FONT_SIZE: f64 = 14.0`）
- `PROGRESS.zh.md:86`（发现 #19）

**补偿**：所有 dagre-facing 标签度量统一使用 14px。这不是 dagre-rs 的 bug，而是上游 jsdom 环境的隐式行为，dagre-rs 本身无需修改。记录在此是因为它影响所有 Stratum 3 diagram 的 dagre 度量输入。

---

### 2.2 HIGH 级别

#### ④ 5×5 Swap：复合节点 bbox LR 内部方向翻转

**现象**：当一个 isolated cluster 仅含叶节点（无子集群）且 `inner_rankdir == LR` 时，dagre-rs 报告的 compound node 宽度比上游宽 **5px**、高度比上游矮 **5px**（宽高差值恰好对调 5px）。cluster center 同步漂移 (-2.5, +2.5)，所有子节点继承此偏移。

**后果**：state diagram 的 LR 方向复合状态 viewBox 和节点位置系统性偏离 5px。

**位置**：`src/layout/dagre_bridge.rs:1900-1998`

**补偿**：条件性 post-layout 修正（`cluster_width -= 5, cluster_height += 5`），仅对 stateDiagram + LR + leaf-only + zero-padding 生效。**故意不对 class diagram 应用**（会引入 cypress/class/38, /94, /222 和 demos/class/09, /11 的回归）。

**影响 fixture**：cypress/state/30, /68, /25, /67

**根本原因**：dagre-rs compound node finalization 的 BK 位置算法与 dagre.js 在 LR 方向下的 bbox 计算存在系统性 5px 差异。可能源于 compound node 初始 width=0（dagre-rs）vs 非零初始 width（dagre.js）对 BK positioning 的影响。

**建议**：需要深入对比 dagre-rs 和 dagre.js 的 `compound-node-finalization` 模块，定位 5px 差异的精确算法源头。当前仅做 empirical patch，不够健壮。

---

#### ⑤ Flowchart 复合节点宽度窄 5px

**现象**：dagre-rs 给 flowchart 单子节点 TB 方向 isolated cluster 的 compound node 宽度比上游窄 **5px**。

**后果**：flowchart 子图 bbox 和 viewBox 偏窄。

**位置**：`src/layout/dagre_bridge.rs:1179-1186`

**补偿**：`apply_flowchart_cluster_correction()` 加 `+5.0` empirical correction。源头不明确（"possibly related to how the compound node's initial width=0 in our code vs non-zero initial width in upstream affects the BK position algorithm"）。

**建议**：同 ④，需在 dagre-rs 内部定位 BK 算法的 5px 偏差源头。

---

#### ⑥ dagre-rs 在复合图 edge case 上 panic

**现象**：dagre-rs 在以下情况 panic：
- 复合（cluster）节点直接作为边端点
- cross-cluster 边跨越不同子树

上游 dagre.js 优雅处理这些情况。

**后果**：~13 个 state fixture 在 compound-mode layout 下崩溃。

**位置**：`src/layout/state.rs:690-729`

**补偿**：两阶段策略：
1. `catch_unwind` 尝试 compound layout
2. panic 时回退 flat mode（剥离所有 `parent_id`），再用 `synthesise_cluster_bounds()` 从子节点 bbox 合成 cluster 位置

**影响 fixture**：约 13 个 state fixture

**根本原因**：dagre-rs 的 compound graph 实现对某些拓扑结构不健壮。dagre.js 原版虽然也有 compound graph 的各种 edge case，但不会 panic。

**建议**：dagre-rs 需增加 compound graph 的 robustness test case，确保 composite 端点和 cross-cluster 边不会触发 panic。

---

#### ⑦ viewBox 计算依赖 getBBox 而非 dagre 输出

**现象**：上游 mermaid 计算 viewBox 的方式是：先渲染完整 SVG 到 DOM，再调用 `getBBox()` 取根元素的 bbox。这不是 dagre 输出的简单 AABB。jsdom 的 `getBBox` shim 忽略 `transform` 属性，union 所有子元素的 local bbox，产生比 dagre AABB 更宽的结果。

**后果**：直接从 dagre 坐标算 AABB 会产生与上游不同的 viewBox。ER 27/80 和 flowchart 8 个 fixture 的 viewBox 偏差部分源于此。

**位置**：`src/layout/er.rs:724-900`（完整 jsdom shim 复现）、`src/layout/dagre_bridge.rs:989-1028`（`compute_bounds`）

**补偿**：各 diagram layout 模块复现 jsdom 的 getBBox shim 逻辑：union rect intrinsic boxes + path parsed bbox + foreignObject label extents。

**建议**：这不是 dagre-rs 的 bug（dagre 本身不负责 viewBox），而是 mermaid 渲染管线与 dagre 的衔接方式问题。无需 dagre-rs 修改。

---

### 2.3 MEDIUM 级别

#### ⑧ 复合节点高度多含 2 × max_leaf_padding

**现象**：dagre-rs compound node 高度比上游多 `2 × max_leaf_padding`。

**位置**：`src/layout/dagre_bridge.rs:1152-1156`

**补偿**：`apply_flowchart_cluster_correction()` 减去 `2 × max_leaf_padding` 并调整 inner_y。

---

#### ⑨ Cluster + fork/join bar 宽度差 +2px

**现象**：当一个 composite state cluster 包裹 fork/join 水平条（70-wide），上游内层 dagre pass 给 wrapper cluster 的宽度比 dagre-rs 多 **2px**。

**位置**：`src/layout/state.rs:780-785`、`src/layout/dagre_bridge.rs:1658-1674`

**补偿**：内层 pass 和 post-layout 各加 +2 width / +1 x。

**影响 fixture**：cypress/state/22

---

#### ⑩ 空 subgraph 降级为普通节点

**现象**：dagre-rs 在 compound node 无子节点时 panic。上游 mermaid 在 dagre 之前将空 subgraph 降级为 regular node。

**位置**：`src/layout/dagre_bridge.rs:2309-2320`

**补偿**：空 cluster 的 `is_group` 设为 `false`，`parent_id` 设为 `None`。

**影响 fixture**：cypress/flowchart/139

---

#### ⑪ Cluster anchor rewrite 产生的 self-loop 不扩展

**现象**：`adjustClustersAndEdges` 将集群端点 rewrite 为锚点叶节点后，可能产生 self-loop（如 `Sub→In` 变 `In→In`）。上游不对 rewrite self-loop 做 helper-node expansion。dagre-rs 会尝试扩展所有 self-edge。

**位置**：`src/layout/dagre_bridge.rs:490-516`

**补偿**：检测 `is_rewrite_self_loop`，跳过 expansion，用 `rewrite_self_loop_points()` 合成路径。

**影响 fixture**：cypress/flowchart/168

---

#### ⑫ 非孤立集群子节点 bbox 用绝对坐标

**现象**：非孤立集群子节点在 jsdom getBBox shim 中贡献绝对坐标（cx-w/2, cy-h/2），而非对称半宽/半高偏移。

**位置**：`src/layout/dagre_bridge.rs:1828-1865`

**补偿**：追踪 `cluster_child_min_x/max_x/min_y/max_y` 绝对内层坐标。

---

#### ⑬ Asymmetric polygon 形状在 cluster bbox 中偏移

**现象**：hexagon/subroutine/lean_left/lean_right/trapezoid/diamond 等不对称形状的 polygon vertices 在 `[0, w]` 而非 `[-w/2, w/2]`（jsdom shim 忽略 transform 时），导致 cluster bbox 计算偏差。

**位置**：`src/layout/dagre_bridge.rs:75-101`

**补偿**：`shape_is_asymmetric_x()` 标识不对称形状，bbox 计算用 full width 而非 half-width。

**影响 fixture**：cypress/flowchart/176, /181

---

#### ⑭ 嵌套孤立集群 rankdir 继承差异

**现象**：上游 extractor 对所有嵌套孤立子图使用**顶层 rankdir**做方向翻转，而非父级内部方向。初始实现用了父级 inner_rankdir，导致多嵌套时翻转一次过多。

**位置**：`src/layout/dagre_bridge.rs:2128-2134`

**补偿**：flowchart 用顶层 rankdir，其他 diagram 家族保留 parent-inner 行为。

---

#### ⑮ 内层集群边缺少 spline points

**现象**：当边的两端都在同一个 isolated cluster 内，内层 dagre pass 的 spline points 不被外层 dagre graph 传播。外层 `collect_edges` 看不到这些点。

**位置**：`src/layout/flowchart.rs:86-92`、`src/layout/dagre_bridge.rs:2499-2583`

**补偿**：内层 pass edge routing merge + `synthesize_missing_intra_cluster_edge_points()` 合成 fallback 3 点路径。

---

#### ⑯ ER viewBox foreignObject 贡献

**现象**：ER viewBox 必须包含 foreignObject 标签的 local 坐标（0, 0, label_w, label_h），否则右/下边缘塌缩到 rect/path 范围。

**位置**：`src/layout/er.rs:724-735`

**补偿**：union rect + path parsed bbox + foreignObject label extents。

---

### 2.4 LOW 级别

#### ⑰ HashMap 非确定性（divider 位置分配）

**现象**：`sub_isolated` HashMap 迭代顺序不确定，导致 divider cluster 位置跨 run 随机分配。

**位置**：`src/layout/state.rs:756-769`、`src/layout/dagre_bridge.rs:1519-1522`

**补偿**：排序 sub_isolated IDs + `stabilise_divider_positions()` 按声明顺序重绑定。

---

#### ⑱ Dagre edge point 重复（零长度线段）

**现象**：dagre 有时在同 rank 的相邻节点处输出连续相同 vertex，产生零长度 SVG 线段。

**位置**：`src/layout/routing.rs:60-63`

**补偿**：`dedupe_collinear()` 在 1e-4 epsilon 内去重。

---

### 2.5 N/A（非 dagre-rs 问题，但影响 byte-exact）

#### ⑲ Stadium/roughjs/KaTeX/icon fixture

34 个 fixture 因渲染层缺失（roughjs path 生成、KaTeX 数学渲染、icon SVG registry）无法 byte-exact。不是 dagre-rs 布局问题。见 `tests/known_ignored.txt`。

---

#### ⑳ ER reference position 测试

`er03_node_positions_match_reference`（`src/layout/er.rs:1001-1067`）用硬编码上游坐标验证 `tie_keep_first: true` 的正确性。这是验证手段而非不一致性本身。

---

## 3. 不一致性分类汇总

| 类别 | 项目数 | 根因归属 |
|---|---|---|
| dagre-rs 算法差异（需 dagre-rs 修改） | ①②④⑤⑥⑧⑨⑩⑪ | dagre-rs crate |
| 上游隐式行为（需下游适配） | ③⑦⑫⑬⑭⑮⑯ | mermaid 管线衔接 |
| 代码健壮性（需 dagre-rs 修改） | ⑥⑩ | dagre-rs crate |
| 渲染层缺失（非 dagre 问题） | ⑲ | mermaid-little renderer |
| 确定性保证（需 dagre-rs 或下游排序） | ⑰⑱ | 两者皆有 |

**核心结论**：dagre-rs 与 dagre.js 的不一致性中，**约 9 项根因在 dagre-rs crate**（①②④⑤⑥⑧⑨⑩⑪），其中 ①（tie-breaking）和 ②（intersect 只用 rect）是影响面最广的两项；④⑤（compound node bbox 5px 系列偏差）影响面次广但根因尚未精确定位。

---

## 4. 诊断工具

| 工具 | 位置 | 用途 |
|---|---|---|
| `dagre_debug.mjs` | `tests/support/dagre_debug.mjs` | 在上游 JS 端渲染 fixture，dump dagre 中间数据（节点坐标/边路径/viewBox）为 JSON |
| Rust diff probe | 各 `render/svg_*` 模块 | 渲染同一 fixture，找到第一个字节差异 |
| 逐层对照流程 | CSS → viewBox → 节点位置 → 形状 → 边路径 → 标签格式 | 系统性定位差异层级 |

---

## 5. 建议

### 5.1 dagre-rs 侧（优先级排序）

1. **P0 — 暴露 intersectPolygon / intersectEllipse helper**：当前 mermaid-little vendored 了自己的 intersect 实现，如果 dagre-rs 暴露这些 helper，下游可直接调用而非重复实现。
2. **P0 — compound graph robustness**：增加 compound graph edge case 的 panic-free 保证（composite 端点、cross-cluster 边、空 cluster）。
3. **P1 — tie_keep_first 选项正式化**：已在 mermaid-little 中验证有效，应成为 dagre-rs LayoutOptions 的正式字段，附文档说明历史差异。
4. **P1 — 定位 5px compound bbox 偏差**：对比 BK position 算法，找到 TB/LR 方向下 compound node width/height 的 5px 偏差精确源头。
5. **P2 — compound node padding 含量对齐**：确保 compound node height 不多含 `2 × max_leaf_padding`。

### 5.2 mermaid-little 侧

1. 继续 ER/state/flowchart 的 byte-exact 收敛，当前已有大量 post-layout 补偿逻辑。
2. 随 dagre-rs 修复逐步移除 empirical patch（5×5 swap、+5 width 等），回归 upstream-aligned 直接输出。
3. 34 个 known_ignored fixture 待 roughjs/KaTeX/icon port 后再处理。

---

## 6. 附录：Stratum 3 各 diagram 失败 fixture 详表

### ER（5/80 失败）

| Fixture | 原因 | 归类 |
|---|---|---|
| cypress/er/04, /44（重复） | dagre-rs 自引用实体宽度/viewBox 偏差 | Layout |
| cypress/er/10, /51（重复） | dagre-rs 自引用实体坐标/viewBox 偏差 | Layout |
| demos/er/02 | forest theme CSS + dagre 度量偏差 | Layout + Rendering |

### State（12/82 失败）

| Fixture | 原因 | 归类 |
|---|---|---|
| 07 | dagre-rs aliased state name 宽度偏差 | Layout |
| 08 | state description 文本未渲染 | Rendering |
| 24, /66（v1/v2 重复） | dagre-rs fork separator 布局偏差 | Layout |
| 27 | dagre-rs 复合状态 + 跨边界 transition | Layout + Rendering |
| 05 | degenerate diagram 处理 | Rendering |
| 20, /62（重复） | edge label 定位漂移 | Rendering |
| 26 | choice annotation 渲染 | Rendering |
| 34 | cross-composite edge routing | Rendering |
| 45, /46 | 大量 edge path/label 定位漂移 | Rendering |

### Flowchart（50/318 失败，其中 32 known_ignored）

| 类别 | 数量 | 说明 |
|---|---|---|
| ELK layout fallback | ~17 | dagre 与 ELK 布局差异 |
| Stadium/roughjs | ~24 | roughjs 路径未 port |
| KaTeX | 6 | 数学渲染未 port |
| Icon | 3 | icon registry 未 port |

### Class（1/113 失败）

demos/class/08 — 上游 parser bug，与 dagre-rs 无关。