# mermaid-little

中文 | [English](README.md)

[Mermaid](https://mermaid.js.org/) 的轻量级 Rust 重新实现，目标是与上游 `mermaid@11.14.0` 产生 **逐字节一致的 SVG 输出**。

## 这是什么

mermaid-little 读取 `.mmd` 源文本，输出 `.svg` —— 与 Mermaid 功能相同，但以原生 Rust 库 + CLI 形态运行，**运行时零 JS / DOM 依赖**。姊妹项目是 [plantuml-little](https://github.com/kookyleo/plantuml-little)，布局后端构建在完整的 dagre.js port [dagre-rs](https://github.com/kookyleo/dagre-rs) 之上。

## 当前状态

**收敛阶段。** 25 种 diagram 全部已 parser/layout/render 接通，12 路 wave 累计推进；`cargo test` 全绿（666 lib tests）。截至 2026-05-03，sweep_all **1200 / 1328 byte-exact ≈ 90.4%**。

| | |
|---|---|
| 上游版本 | `mermaid@11.14.0`（`2b9d054d`，2026-04-01 发布） |
| `convert_with_id` 已接线 | **25 / 25** diagram |
| 已 byte-exact (≥99%) | 22 / 25（pie / packet / radar / ishikawa / journey / timeline / quadrant / xychart / wardley / sankey / treemap / kanban / c4 / er / block / requirement / class / state / gitGraph / gantt / venn / flowchart） |
| 主要剩余阻塞 | sequence (51/150)、mindmap multi-node (7/25)、KaTeX × 6、icon shapes × 1、handDrawn venn × 3 |
| Layout 后端 | [`dagre-rs`](https://github.com/kookyleo/dagre-rs) |
| Reference 测试 | `cargo run --bin sweep_all` 单文件全量 sweep；known_ignored 列表已清空，所有失败均暴露 |
| 跟踪文档 | [PROGRESS.zh.md](PROGRESS.zh.md)、[docs/stratum3_execution_guide.zh.md](docs/stratum3_execution_guide.zh.md) |

## 不在范围内

- ELK layout（上游也是 opt-in，后期看需求再加）
- Architecture 图（需要 cytoscape，无 Rust 对应物）
- KaTeX 公式、rough.js 手绘风（MVP 占位）
- 运行时 DOM、JS 互操作、headless chromium

## 致谢

本项目是 [Mermaid](https://mermaid.js.org/) 的独立 Rust 重新实现，原作者为 Knut Sveidqvist。我们对 Mermaid 团队在 diagram-as-code 领域的贡献深表敬意。所有规范性内容以上游为标准。

布局后端使用 [`dagre-rs`](https://github.com/kookyleo/dagre-rs)——dagre.js 的完整 Rust port。字体度量管线（`src/font_data.rs`、`src/font_metrics.rs`）vendor 自姊妹项目 [plantuml-little](https://github.com/kookyleo/plantuml-little)——两个项目共用同一张 DejaVu Sans glyph advance 表，保证两处输出的 byte-exact 一致。

同时感谢社区已有的 Rust mermaid port 作为 prior art：[mermaid-rs-renderer (mmdr)](https://github.com/1jehuang/mermaid-rs-renderer)、[selkie](https://github.com/btucker/selkie)、[mmdflux](https://github.com/kevinswiber/mmdflux)。mermaid-little 选的 trade-off 点不同（先追求与上游 byte-exact 一致，再谈性能），但具体 diagram 卡壳时我们会参考他们的源码实现，参考时会在相关 commit message 里明确声明。

## 许可证

MIT，与上游 Mermaid 一致。参见 [LICENSE](LICENSE)。
