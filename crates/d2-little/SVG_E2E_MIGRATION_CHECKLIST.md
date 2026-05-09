# D2 Little Dagre SVG E2E 全量迁移清单

日期：2026-04-13

## 目标

把上游 Go `e2etests` 中所有对 `dagre + SVG` 适用的 case 全量迁移到 `d2-little` 的 Rust runner 中，并以测试驱动方式把差异逐项打平。

本轮明确不做：

- `elk` 路径
- `asciitxtar` 的 ASCII 文本快照校验
- 非 `dagre` 的 layout parity

本轮明确要做：

- 所有 `dagre` 适用 case 都能在 Rust runner 中执行
- 有 SVG fixture 的 case 做 `sketch.exp.svg` 字节级比对
- 预期为 `dagreFeatureError` 的 case 做错误字符串精确比对
- 预期为 `expErr` 的 case 做编译错误字符串精确比对
- `TIMEOUT 0`

## 基线

- 当前主 gate 只覆盖 `110` 个 case
- 当前主 gate 只包含 `sanity 5` + `stable 105`
- 以 Go `e2etests` 源码为准，当前 dagre 适用总数是 `322`
- 其中：
  `315` 个是 SVG fixture-backed case
  `5` 个是 `dagreFeatureError`
  `2` 个是 `expErr`
- 本地另有 `7` 个 dagre SVG fixture 暂未被当前 Go 源码引用，不能直接算入主 corpus

## 适用范围判定

一个 Go E2E case 只要满足下面任一条件，就纳入本轮迁移范围：

1. 本地存在该 case 的 dagre SVG fixture
2. Go 测试中该 case 对 dagre 预期的是 `dagreFeatureError`
3. Go 测试中该 case 对 dagre 预期的是 `expErr`

`asciitxtar` family 特殊处理：

- 保留 `sketch.exp.svg` 比对
- 暂不迁移 `extended.exp.txt` / `standard.exp.txt`

## Family 总表

| family | dagre 适用 case |
| --- | ---: |
| `sanity` | 5 |
| `stable` | 152 |
| `regression` | 63 |
| `patterns` | 9 |
| `todo` | 10 |
| `measured` | 3 |
| `unicode` | 9 |
| `root` | 7 |
| `themes` | 5 |
| `txtar` | 49 |
| `asciitxtar` | 10 |
| 合计 | 322 |

## Error-only Case

这些 case 没有 dagre SVG fixture，但仍属于 dagre 适用范围，必须迁进 Rust runner：

- `todo/container_child_edge`：`dagreFeatureError`
- `todo/child_parent_edges`：`dagreFeatureError`
- `todo/container_label_loop`：`dagreFeatureError`
- `stable/chaos1`：`dagreFeatureError`
- `stable/container_dimensions`：`dagreFeatureError`
- `regression/undeclared_nested_sequence`：`expErr`
- `regression/sequence-panic`：`expErr`

## Runner 能力补齐清单

- [ ] 把 case 数据模型从 `name + script + category` 扩成接近 Go `testCase`
- [ ] 支持 `fixture_kind` 区分普通 dagre fixture 与 `asciitxtar/sketch.exp.svg`
- [ ] 支持 `theme_id`
- [ ] 支持 `exp_err`
- [ ] 支持 `dagre_feature_error`
- [ ] 支持 `measured` family 的 `mtexts`
- [ ] 支持 duplicate case 名的 `#01/#02` fixture 解析
- [ ] 支持多 board SVG 包装流程，保持与 Go `RenderMultiboard + Wrap` 对齐
- [ ] 支持按 case 输出 dashboard 分类：`MATCH / DIFF / COMPILE / FEATURE / TIMEOUT`
- [ ] 新 gate 不再硬编码只跑 `sanity + stable`

## Case 数据迁移清单

- [ ] `sanity` 5 个 case 全接入 runner
- [ ] `stable` 152 个 case 全接入 runner
- [ ] `regression` 63 个 case 全接入 runner
- [ ] `patterns` 9 个 case 全接入 runner
- [ ] `todo` 10 个 case 全接入 runner
- [ ] `measured` 3 个 case 全接入 runner
- [ ] `unicode` 9 个 case 全接入 runner
- [ ] `root` 7 个 case 全接入 runner
- [ ] `themes` 5 个 case 全接入 runner
- [ ] `txtar` 49 个 case 全接入 runner
- [ ] `asciitxtar` 10 个 case 的 SVG 检查接入 runner
- [ ] `7` 个 error-only case 接入 runner

## 实现推进顺序

- [ ] 第 1 步：重写 Rust E2E manifest / runner，使其能表达 Go `testCase` 的 dagre 子集
- [ ] 第 2 步：把全部 `322` 个 dagre 适用 case 接线到 Rust runner
- [ ] 第 3 步：跑第一版全量 dashboard，按失败类型分桶
- [ ] 第 4 步：先修 runner/配置语义问题，再修渲染与布局差异
- [ ] 第 5 步：修完后把新全量 dashboard 升级为主 gate

## 差异修复分桶

- [ ] Case 提取/命名错误
- [ ] 主题/配置回灌错误
- [ ] `measured text` 语义缺失
- [ ] rich text 渲染差异
- [ ] sequence diagram 布局差异
- [ ] dagre 后处理差异
- [ ] shape/export/render 细节差异
- [ ] 纯字节级序列化差异
- [ ] 超时/死循环/栈溢出

## 完成定义

满足下面条件，才可以对外宣称 dagre/SVG 全量迁移完成：

- [ ] Rust runner 已覆盖全部 `322` 个 dagre 适用 case
- [ ] `315` 个 SVG fixture-backed case 全部 `MATCH`
- [ ] `7` 个 error-only case 全部按预期报错
- [ ] `DIFF 0`
- [ ] `COMPILE 0`（不含预期 `expErr`）
- [ ] `FEATURE 0`（不含预期 `dagreFeatureError`）
- [ ] `TIMEOUT 0`
- [ ] 可以稳定复跑

## 备注

- 本文件是执行清单；后续以勾选状态和 dashboard 结果为准推进。
- 当前 generator 识别出 `7` 个本地 dagre fixture 未被上游 Go 源码引用：
  `measured/empty-markdown`
  `regression/empty_md_measurement`
  `stable/3d_sketch_mode`
  `stable/tooltips`
  `txtar/c4-theme`
  `txtar/dark-theme-shape`
  `txtar/sequence-bounding-box#01`
