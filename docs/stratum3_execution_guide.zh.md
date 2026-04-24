# Stratum 3 执行指南

更新日期：2026-04-23

这份文档只处理当前最重要的推进面：Stratum 3 dagre 家族
`er / requirement / state / flowchart / block / class`，以及它们之后的
最优接续顺序。目标不是重复 `FEATURES` 或 `PLAN`，而是给出一份可以直接照着执行的工作手册。

## 1. 当前判断

以当前仓库实测为准，不再使用旧的 “scaffolding / 0 tests” 口径。

- `cargo test --quiet` 全绿。
- `convert_with_id` 已接线 19 种 diagram；`gantt` 仍是 renderer stub。
- Wave 1/2 几何类 diagram 已有稳定的 byte-exact sweep。
- 当前最高杠杆工作是把 Stratum 3 的“能 render”继续推进到“可量化收敛的 byte-exact parity”。

2026-04-23 的实测快照：

- `flowchart`：`319` 个 fixture 全部 parse/layout/render 成功；去掉 `39` 个 ELK fixture 后，`280` 个可比对 case 中 byte-exact `0 / 280`。
- `er`：byte-exact `53 / 80`。
- `requirement`：byte-exact `0 / 44`。
- `state`：`82` 个 fixture 中 `69` 个可完整 render，byte-exact `0`。
- `block`：已锁定 `16 / 33` 个 byte-exact fixture。
- `class`：byte-exact `0 / 113`。

这组数字的来源不再靠手工维护，而是靠 sweep 命令和状态脚本。

## 2. Source Of Truth

当前 source of truth 只认两类结果：

- `cargo test --quiet`
- `bash scripts/stratum3_status.sh`

说明：

- `PROGRESS.zh.md` 是叙述性进展文档，不应再单独作为数字真相源。
- 任何新的计数、百分比、阶段判断，都应先跑状态脚本再回写文档。

## 3. 每次开工的固定动作

### 基线

```bash
cargo test --quiet
```

### Stratum 3 状态

```bash
bash scripts/stratum3_status.sh
```

这条脚本会汇总：

- `flowchart` 的 parse/layout/render 稳定性
- `flowchart` 的 byte-exact 比例
- `er / requirement / state / block / class` 的当前 sweep 结果

### 单 diagram 聚焦

```bash
cargo test render::svg_er::tests::byte_exact_sweep -- --exact --nocapture
cargo test render::svg_requirement::tests::byte_exact_sweep_reports_progress -- --exact --nocapture
cargo test render::svg_state::tests::reports_byte_exact_pass_count -- --exact --nocapture
cargo test flowchart_parser_roundtrips_all_fixtures -- --exact --nocapture
cargo test flowchart_byte_exact_sweep -- --exact --nocapture
cargo test render::svg_block::tests::byte_exact_sweep -- --exact --nocapture
cargo test render::svg_class::tests::byte_exact_sweep -- --exact --nocapture
```

### 诊断 diff

```bash
cargo test dump_er_shell_alignment -- --nocapture
cargo test dump_requirement_01_diff -- --nocapture
cargo test dump_state_01_diff -- --nocapture
cargo test flowchart_single_diff_report -- --nocapture
```

## 4. 固定修复顺序

对 Stratum 3 的任一 diagram，都按下面顺序排查，不要跳层：

1. CSS / style block
2. viewBox / bbox / padding
3. 节点几何和 shape
4. edge path / marker / points
5. label HTML / foreignObject / class 名 / 空标签高度
6. DOM id / data-* / attribute 顺序

原因：

- CSS 和 viewBox 错了，后面的 diff 会被噪声淹没。
- 节点外形没对齐时，edge path 往往也不成立。
- label stack 往往是最后一段长尾，但对 byte-exact 影响巨大。

## 5. 最优推进顺序

### 第一梯队：继续只打 Stratum 3

#### 1. ER

优先级最高。

- 它已经到 `53 / 80`，离闭环最近。
- 它能最快验证共享问题是否真的被修掉。
- 一旦 ER 上升，通常说明统一 dagre 路径、rough.js 路径、label stack 中至少有一层被做对了。

退出标准：

- `byte_exact_sweep` 比 `53 / 80` 更高。
- `byte_exact_locked_set` 不能回退。
- 失败集开始从“整类模式失败”变成“少量离散 fixture”。

#### 2. requirement

第二优先。

- CSS 已经接近/达到可比状态。
- 现在主要是节点/边 SVG 结构与 viewBox fidelity。
- 工作量远小于 class 和 flowchart。

退出标准：

- byte-exact 从 `0 / 44` 开始出现稳定正数。
- `foreignObject` label 栈继续保持，不回退成纯 `<text>`。

#### 3. state

第三优先。

- 坐标层面已经有可复用发现。
- 但当前还有一批 fixture 在 layout 阶段触发 dagre panic。
- 它是最能暴露 dagre 边界 case 的收敛面。

退出标准：

- `rendered` 明显高于 `69 / 82`。
- render-failure 列表收缩。
- byte-exact 开始从 `0` 向上爬。

#### 4. flowchart

第四优先。

- 这是数量最大的收益面，但不应最先打。
- 当前已经证明 parser/layout/render 很稳，问题集中在 fidelity，而不是可运行性。
- 共享问题在 ER / requirement / state 上收敛后，再吃 flowchart 的 `280` 个 case，收益最大。

退出标准：

- `parse-fail=0 layout-fail=0 render-fail=0` 持续保持。
- byte-exact 从 `0 / 280` 开始出现连续增长。
- `flowchart/02` 这类 canary fixture 的首个 diff 持续后移。

#### 5. block

第五优先。

- 它已经有 `16 / 33` 锁定集，不是零起点。
- 但它不是主共享瓶颈，排在 flowchart 后更合理。

退出标准：

- `byte_exact_sweep` 高于 `16 / 33`。
- 已锁定的 16 个 fixture 不回退。

#### 6. class

最后处理。

- 工作量最大，长尾最多。
- `classBox`、sections、shape fidelity、ID 细节都更重。
- 太早投入会把主战线打散。

退出标准：

- byte-exact 从 `0 / 113` 脱离零。
- 先出现一批可锁定的 passing fixtures，再考虑全量拉升。

### 第二梯队：打通新的图种

#### 7. gantt

- 只在 Stratum 3 第一梯队稳定后再做。
- 原因是 `gantt` 现在不是共享瓶颈，而是独立 renderer 缺口。

#### 8. mindmap

- 在 `gantt` 之后。
- 先用 deterministic tidy-tree 思路，不要重新开新的布局不确定性。

### 第三梯队：重图种

#### 9. sequence / c4 / gitGraph

- 这三项都不该抢占当前主线。
- 它们体量大、共享收益低、容易打断 Stratum 3 收敛。

## 6. 文档更新纪律

只有在重新跑完状态脚本后，才更新这些文件里的数字或阶段判断：

- `README.md`
- `README.zh.md`
- `PROGRESS.zh.md`

更新原则：

- `README*` 只写高层状态，不写脆弱数字。
- `PROGRESS.zh.md` 写阶段性结论、关键发现和当前快照。
- 精确计数优先放在脚本输出和测试输出里，不要手工抄来抄去。

## 7. 提交前检查

至少执行：

```bash
cargo test --quiet
bash scripts/stratum3_status.sh
```

如果改的是某个单一 diagram，再补一条对应的 diff/diagnostic 测试。

## 8. 一句话准则

当前阶段不要分兵开新图；先把 Stratum 3 的进度量化、回归面固定，再按
`ER -> requirement -> state -> flowchart -> block -> class` 的顺序收敛。
