# mermaid-little

[中文](README.zh.md) | English

A lightweight Rust reimplementation of [Mermaid](https://mermaid.js.org/),
targeting byte-exact SVG output parity with upstream `mermaid@11.14.0`.

## What Is This

mermaid-little takes `.mmd` source text and produces `.svg` output —
the same as Mermaid, but as a native Rust library + CLI with **zero
JS / DOM dependency at runtime**. Sibling project to
[plantuml-little](https://github.com/kookyleo/plantuml-little) and built
on top of the complete dagre.js port at
[dagre-rs](https://github.com/kookyleo/dagre-rs).

## Status

**Convergence phase.** All 25 diagram types now parse, layout, and
render through `convert_with_id`; 12 waves of byte-exact work have
landed. `cargo test` is green (666 lib tests). As of 2026-05-03
sweep_all reports **1200 / 1328 byte-exact ≈ 90.4%**.

| | |
|---|---|
| Upstream version | `mermaid@11.14.0` (`2b9d054d`, tagged 2026-04-01) |
| Wired in `convert_with_id` | **25 / 25** diagrams |
| Byte-exact (≥99%) | 22 / 25 (pie, packet, radar, ishikawa, journey, timeline, quadrant, xychart, wardley, sankey, treemap, kanban, c4, er, block, requirement, class, state, gitGraph, gantt, venn, flowchart) |
| Remaining frontier | sequence (51/150), mindmap multi-node (7/25), KaTeX ×6, icon shapes ×1, handDrawn venn ×3 |
| Reference tests | `cargo run --bin sweep_all` runs the full sweep; `known_ignored.txt` is now empty so every gap is surfaced |
| Tracking doc | [PROGRESS.zh.md](PROGRESS.zh.md) (Chinese only, by project rule) |

## Non-Goals

- ELK layout (opt-in upstream; add later if demand warrants)
- Architecture diagram (requires cytoscape; no Rust equivalent)
- KaTeX formulas, rough.js hand-drawn look (placeholders for MVP)
- Runtime DOM, JS interop, headless chromium

## Acknowledgments

This project is an independent Rust reimplementation of
[Mermaid](https://mermaid.js.org/), created by Knut Sveidqvist. We
deeply appreciate the Mermaid team's work in making diagram-as-code
accessible to everyone. All specification-level behavior follows the
upstream standard.

The layout backend is [`dagre-rs`](https://github.com/kookyleo/dagre-rs),
a complete Rust port of dagre.js. The font metric pipeline
(`src/font_data.rs`, `src/font_metrics.rs`) is vendored from the sister
project [plantuml-little](https://github.com/kookyleo/plantuml-little) —
the same DejaVu Sans glyph advance tables anchor both projects, which
keeps byte-exact output consistent across the two codebases.

Thanks also to the prior-art community Rust mermaid ports —
[mermaid-rs-renderer (mmdr)](https://github.com/1jehuang/mermaid-rs-renderer),
[selkie](https://github.com/btucker/selkie),
[mmdflux](https://github.com/kevinswiber/mmdflux) — for charting this
design space. mermaid-little aims at a different point of the tradeoff
(byte-exact parity with upstream first, performance second) but we
expect to consult their source when stuck on specific diagram types
and will cite such references in commit messages.

## License

MIT, same as upstream Mermaid. See [LICENSE](LICENSE).
