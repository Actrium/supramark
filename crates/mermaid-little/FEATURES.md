# mermaid-little Feature Plan

Aligned with upstream **mermaid@11.14.0** (`2b9d054d`, tagged 2026-04-01).

This document records the dependency analysis and phased plan. It will
evolve into the support matrix as diagram types come online.

## Status (2026-05-02)

All 25 diagram types are wired through `convert_with_id`; the project
is in the **convergence phase**. `cargo test` is green; sweep_all
reports **1200 / 1328 byte-exact ≈ 90.4%**.

| | |
|---|---|
| Upstream version | `mermaid@11.14.0` (`2b9d054d`) |
| Wired diagrams | **25 / 25** (incl. sequence / mindmap / c4 / gitGraph) |
| Byte-exact (≥99% pass) | 22 / 25 |
| Reference tests | 1328 (cypress 1126 + demos 202); `known_ignored.txt` is now empty |
| Lib unit tests | 666 / 0 / 0 |
| Layout backend | [`dagre-rs`](https://github.com/kookyleo/dagre-rs) (pinned, complete dagre.js port) |
| Tracking doc | [PROGRESS.zh.md](PROGRESS.zh.md) (Chinese only, by project rule) |

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

## Diagram Support Matrix (2026-05-02 sweep_all)

All 25 user-facing diagrams are wired. Numbers below are
`cypress/demos` byte-exact pass counts from the latest sweep.

### 100% byte-exact (17)

| Diagram | cypress | demos |
|---|---:|---:|
| pie | 10/10 | 3/3 |
| packet | 5/5 | — |
| radar | 6/6 | 1/1 |
| ishikawa (incl. `look:handDrawn`) | 13/13 | 5/5 |
| user-journey | 10/10 | 1/1 |
| timeline | 14/14 | 3/3 |
| quadrant | 14/14 | 2/2 |
| xychart | 37/37 | 19/19 |
| wardley | 6/6 | 6/6 |
| sankey | 1/1 | 2/2 |
| treemap | 28/28 | 2/2 |
| kanban | 11/11 | — |
| c4 | — | 5/5 |
| er | 73/73 | 7/7 |
| block | 33/33 | — |
| requirement | 43/43 | 1/1 |
| state | 72/72 | 10/10 |
| class | 225/225 | 12/12 |
| gitGraph | 105/105 | 24/24 |

### ≥95% byte-exact (3)

| Diagram | Pass | Remaining |
|---|---:|---|
| flowchart | 191/192 cy + 59/65 dm | KaTeX × 6, icon shape × 1, ELK opt-in × 1 |
| gantt | 41/43 cy + 8/10 dm | V8 `new Date()` timezone quirks × 4 (environmental) |
| venn | 16/16 cy + 8/12 dm | constrainedMDS × 1, handDrawn × 3 |

### Partial — major work concentrated (2)

| Diagram | Pass | Remaining |
|---|---:|---|
| sequence | 51/140 cy + 4/10 dm | Loop/alt/par/rect/critical/break blocks × 24, activation, actor type variants, note over multi, wrap, font metrics |
| mindmap | 6/23 cy + 1/2 dm | cose-bilkent physics scaffold landed (W11-D); reduceTrees / FR-grid bucket / Coarsening / curveBasis edge / Base64 data-points still missing |

### Out of scope / deferred (1)

| Diagram | Start | Parser | Reason |
|---|---|---|---|
| architecture | `architecture-beta` | langium | Requires `cytoscape-fcose`; deferred until/unless we port the scientific optimisation code |

### Ancillary (not user-facing)

`error` / `info` / `common` / `treeView` — internal helpers in upstream;
nothing to port here.

## Phase Roadmap (historical → current)

Phases 0–4 (scaffolding → reference pipeline → font metrics → fixtures →
per-diagram porting) all landed across 11 wave iterations between
project start and 2026-05-02. The current state is recorded in
[PROGRESS.zh.md](PROGRESS.zh.md).

Open execution items:

- **Sequence finishing pass** — probe-driven port of remaining
  100 cypress + 6 demos sequence fixtures (W11 onward).
- **mindmap multi-node finishing** — complete cose-bilkent
  reduceTrees / FR-grid bucket / Coarsening / curveBasis edge / Base64
  data-points (W11-D follow-ups).
- **KaTeX phase** — port enough of KaTeX renderer to unlock 6
  demos/flowchart fixtures (independent decision).
- **Icon shapes phase** — register ~500 AWS / iconify SVG paths to
  unlock 3 cypress/flowchart fixtures (independent decision).
- **`packages/web/` wasm build** — mirror plantuml-little's
  `@kookyleo/plantuml-little-web` once the parity work converges.

## Out of Scope (v1)

- ELK layout (opt-in upstream; programmatically filtered via
  `is_elk()` rather than the ignore list)
- Architecture diagram (depends on full `cytoscape-fcose` port)
- KaTeX formula rendering (deferred to its own phase; 6 fixtures
  blocked)
- Full `@iconify` icon library (deferred to its own phase; 3 fixtures
  blocked)

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
