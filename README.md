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

**Active porting phase.** The foundations, reference-SVG pipeline, and
Wave 1/2 geometry diagrams are already live. `cargo test` is green; the
current work is concentrated on the Stratum 3 dagre family (`er`,
`requirement`, `state`, `flowchart`, `block`, `class`), where working
renderers exist but byte-exact parity is still being closed.

| | |
|---|---|
| Upstream version | `mermaid@11.14.0` (`2b9d054d`, tagged 2026-04-01) |
| Implemented in `convert_with_id` | 19 diagram kinds (`gantt` is still renderer-stubbed) |
| Layout backend | [`dagre-rs`](https://github.com/kookyleo/dagre-rs) |
| Reference tests | Wave 1/2 byte-exact sweeps are green; Stratum 3 is tracked by progress sweeps |
| Active frontier | Stratum 3 parity, `gantt` renderer, then `mindmap` / `sequence` / `c4` / `gitGraph` |
| Tracking docs | [PROGRESS.zh.md](PROGRESS.zh.md), [docs/stratum3_execution_guide.zh.md](docs/stratum3_execution_guide.zh.md) |

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
