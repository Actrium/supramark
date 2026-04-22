# mermaid-little Feature Plan

Aligned with upstream **mermaid@11.14.0** (`2b9d054d`, tagged 2026-04-01).

This document records the dependency analysis and phased plan. It will
evolve into the support matrix as diagram types come online.

## Status

This project is in the **scaffolding** phase. No diagram types are
rendered yet. Run `cargo check` to confirm the workspace builds.

| | |
|---|---|
| Upstream version | `mermaid@11.14.0` (`2b9d054d`) |
| Supported diagrams | 0 / 25 |
| Reference tests | 0 |
| Layout backend | [`dagre-rs`](https://github.com/kookyleo/dagre-rs) (pinned, complete dagre.js port) |

## Upstream Dependency Survey

`packages/mermaid/package.json` runtime dependencies, mapped to our
Rust side:

| Upstream JS dep | Used for | mermaid-little strategy |
|---|---|---|
| `dagre-d3-es` | default flowchart / class / state / er layout | **Use [`dagre-rs`](https://github.com/kookyleo/dagre-rs)** — complete Rust port, cross-validated byte-exact against dagre.js. Plus 2 small geometric helpers (`intersectPolygon`, `intersectRect`) to port. |
| `@mermaid-js/parser` | langium grammars for 7 newer diagrams | Rewrite each grammar as a hand-written Rust parser (nom / chumsky style). |
| Jison files under `packages/mermaid/src/diagrams/*/parser/` | jison grammars for 18 legacy diagrams | Same — port each jison grammar to a hand-written Rust parser. |
| `d3` + submodules | generic SVG primitives, drag / zoom | **Not needed** — we emit SVG strings directly, no runtime DOM. |
| `d3-sankey` | sankey only | Port the algorithm (~600 LoC). |
| `@upsetjs/venn.js` | venn only | Port the algorithm. |
| `cytoscape` + `cose-bilkent` + `fcose` | architecture only | **Unsupported in MVP.** No Rust equivalent; revisit after core is stable. |
| `elkjs` (via separate `@mermaid-js/layout-elk`) | optional ELK layout, opt-in | **Unsupported in MVP.** ELK is an opt-in package in upstream too — the default path does not require it. |
| `katex` | `$...$` math in labels | **Unsupported in MVP** (placeholder). |
| `roughjs` | hand-drawn look | Defer. Port later if demanded (plantuml-little has a similar hand-written jiggle RNG). |
| `khroma` | color manipulation | Replace with small Rust helpers. |
| `marked` | markdown in labels | Port a minimal subset (bold / italic / code / links). |
| `stylis` | CSS preprocessing | Not needed — we bake styles. |
| `dompurify` | XSS sanitization of label HTML | Not needed — no DOM surface. |
| `lodash-es` | utility helpers | Replace with stdlib. |
| `dayjs` | gantt date handling | Replace with `chrono` or `time`. |
| `uuid` | unique SVG IDs | Replace with deterministic source-seeded IDs (same approach as plantuml-little). |
| `ts-dedent` | string literal dedent | Replace with stdlib. |
| `@braintree/sanitize-url` / `@iconify/utils` | URL / icon helpers | Port minimal subset as needed. |

## Diagram Type Matrix (v11.14.0, 25 user-facing types)

"Parser" column is whether the upstream grammar is jison (18) or
langium (7). All parsers will be rewritten in Rust; the column only
notes whether it is grammar A or grammar B on the upstream side.

### Tier 1 — built-in layout, simplest first (11)

No external layout engine. Pure geometry + text placement.

| Diagram | Start | Parser | Notes |
|---|---|---|---|
| pie | `pie` | langium | Single ring, percentage labels. |
| xychart | `xychart-beta` | jison | 2D axis + bars / lines. |
| sankey | `sankey-beta` | jison | Needs `d3-sankey` algo port. |
| sequence | `sequenceDiagram` | jison | Participant lanes + message routing. |
| gantt | `gantt` | jison | Date axis + task bars. Uses `dayjs`. |
| gitGraph | `gitGraph` | langium | Branch / commit dot layout. |
| user-journey | `journey` | jison | Task + emoji / score per column. |
| timeline | `timeline` | jison | Horizontal band w/ events. |
| quadrant-chart | `quadrantChart` | jison | Fixed 2×2 grid + point placement. |
| requirement | `requirementDiagram` | jison | Blocks + typed relationships. |
| packet | `packet-beta` | langium | Bit-field grid. |

### Tier 2 — built-in layout, moderate complexity (9)

| Diagram | Start | Parser | Notes |
|---|---|---|---|
| mindmap | `mindmap` | jison | Tidy-tree-ish built-in layout. |
| kanban | `kanban` | jison | Column + card grid. |
| block | `block-beta` | jison | Nested block grid with spans. |
| treemap | `treemap` | langium | Rectangular partitioning. |
| radar | `radar-beta` | langium | Polar chart w/ axes. |
| wardley | `wardley` | langium | (beta) 2D canvas + evolution axis. |
| ishikawa | `ishikawa` | jison | (a.k.a. fishbone) diagonal branches. |
| venn | `venn` | jison | Needs `venn.js` algo port. |
| c4 | `C4Context`/`C4Container`/`C4Component` | jison | Overlays on top of class / component rendering. |

### Tier 3 — dagre-driven (4)

Uses `dagre-rs` as layout backend. Needs `intersectPolygon` /
`intersectRect` helpers (small, portable from upstream).

| Diagram | Start | Parser | Notes |
|---|---|---|---|
| flowchart | `flowchart`/`graph` | jison | Most-used diagram. |
| class | `classDiagram` | jison | Boxes w/ member rows. |
| state | `stateDiagram`/`stateDiagram-v2` | jison | Composite states, fork/join. |
| er | `erDiagram` | jison | Entity tables w/ typed edges. |

### Tier 4 — deferred / unsupported in MVP (1)

| Diagram | Start | Parser | Reason |
|---|---|---|---|
| architecture | `architecture-beta` | langium | Requires `cytoscape-cose-bilkent`/`-fcose`, no Rust port. Revisit after Tier 1-3 stable. |

### Ancillary (not user-facing)

`error` / `info` / `common` / `treeView` — internal helpers in upstream;
nothing to port here.

## Phased Execution Plan

1. **Phase 0 — scaffolding (done)**: Cargo.toml, lib / main skeleton,
   LICENSE, this plan.

2. **Phase 1 — reference pipeline**: build the deterministic ref-SVG
   generator under `tests/support/` using the aggressive path chosen
   upstream: Node + QuickJS/wasm + upstream mermaid + minimal DOM shim
   sharing the same font-metric table as the Rust side. Document the
   `MERMAID_LITTLE_TEST_BACKEND` env knob, mirroring
   `PLANTUML_LITTLE_TEST_BACKEND`.

3. **Phase 2 — font metrics**: bake DejaVu Sans / DejaVu Sans Mono
   glyph advance tables into `src/font_data.rs`. Both sides consume the
   same table so text `textLength` matches exactly.

4. **Phase 3 — fixtures, three layers**:
   - `tests/fixtures/<diagram>/*.mmd` — hand-written minimal cases, 1–3 per type.
   - `tests/ext_fixtures/<diagram>/*.mmd` — mined from upstream
     `demos/*.html`.
   - `tests/ext_fixtures/e2e/<diagram>/*.mmd` — mined from upstream
     `cypress/integration/rendering/*.spec.*`.

5. **Phase 4 — implementation, diagram by diagram**:
   Tier 1 first (lowest layout risk), then Tier 2, then Tier 3 (dagre
   path), defer Tier 4. Each diagram lands with:
   - parser + AST
   - layout (built-in or dagre-backed)
   - renderer emitting SVG bytes
   - ref-tests green against the Phase-1 pipeline

6. **Phase 5 — `packages/web/` wasm build**: mirror plantuml-little's
   `@kookyleo/plantuml-little-web` — expose a wasm-bindgen surface so
   the Rust core can run in browsers / Node, for people who want
   in-browser rendering without the upstream mermaid.js weight.

## Out of Scope (MVP)

- ELK layout (opt-in upstream; add later if demand warrants)
- Architecture diagram (cytoscape dependency)
- KaTeX formula rendering (placeholder)
- rough.js hand-drawn look (placeholder)
- Full `@iconify` icon library (on-demand only)

## Testing Methodology

Mirrors plantuml-little:

- **Byte-exact reference tests.** Every fixture under `tests/fixtures/`
  and `tests/ext_fixtures/` has a paired SVG under `tests/reference/`
  produced by the upstream pipeline. Rust output must match byte-for-byte.
- **Shared deterministic stack.** Both sides use the same Node/wasm
  runner + the same DejaVu font table + the same font-metric shim, so
  remaining divergence is a real implementation bug.
- **`native` vs `wasm` test backends.** Day-to-day `cargo test` runs
  against a native pure-Rust pipeline; CI's `test-reference` job opts
  in to `MERMAID_LITTLE_TEST_BACKEND=wasm` for cross-platform
  determinism.

## Acknowledgments

This project is an independent Rust reimplementation of
[Mermaid](https://mermaid.js.org/), created by Knut Sveidqvist. We
deeply appreciate the Mermaid team's work in making diagram-as-code
accessible. All specification-level behavior follows the upstream
standard.
