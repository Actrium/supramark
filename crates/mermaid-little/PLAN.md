# mermaid-little Phase 4 Plan

Execution plan for porting `mermaid@11.14.0` to pure Rust, targeting
byte-exact SVG parity. Follow-up to `FEATURES.md` (what we port) with
*how* and *in what order*.

This file synthesises three research passes:

- **Upstream structure survey** — every diagram's parser / DB /
  renderer / layout, LoC tallies, and shared plumbing
- **Community Rust ports review** — `mmdr`, `selkie`, `mmdflux`; which
  pieces are worth borrowing per diagram, and what clashes with
  byte-exact parity
- **Sister-project architecture review** — patterns to adopt from
  `plantuml-little` and `d2-little`

---

## Part I — Upstream mermaid inventory

### Render pipeline (applies to every diagram)

```
source
  → preprocess         (CRLF, YAML frontmatter, %%{init}%% directives, strip comments)
  → detectType         (linear regex scan — v2 detectors before v1)
  → lazy-load module   (loader registered in diagram-orchestration.ts)
  → db.clear() + init(config) + db.setDiagramTitle(frontmatter.title)
  → parser.parse(text) (jison: yy=db mutation; langium: AST return)
  → createUserStyles   (theme + per-diagram styles() + classDef → stylis → <style>)
  → renderer.draw      (per-diagram entry; modern diagrams dispatch to rendering-util/render.ts)
  → cleanUpSvgCode     (a11y title/desc, marker-URL scrub, optional DOMPurify)
  → output SVG string
```

### Per-diagram matrix (25 user-facing types)

Sizing: S (<500 total LoC), M (500-1500), L (1500-3500), XL (3500-6000),
XXL (>6000). "Tier" column renumbers Agent A's five strata to match our
existing FEATURES.md wave language. Grouped by stratum.

#### Stratum 0 — foundations (port before any diagram)

| Subsystem | Upstream path | LoC | Port impact |
|---|---|---:|---|
| config + defaults + schema | `config.ts` / `defaultConfig.ts` / `schemas/config.schema.yaml` | 288+303+2527 | Merge-order (default ← site ← frontmatter ← directive) is load-bearing for byte parity |
| preprocess + frontmatter | `preprocess.ts` + `diagram-api/frontmatter.ts` + `comments.ts` | 64+60+8 | Small |
| detectType dispatch | `diagram-api/diagramAPI.ts` + `detectType.ts` + `diagram-orchestration.ts` | 83+82+114 | Small |
| common DB helpers | `diagrams/common/common.ts` + `commonDb.ts` + `populateCommonDb.ts` | 388+32+14 | sanitize/evaluate/lineBreakRegex/katex stubs |
| createText label factory | `rendering-util/createText.ts` + `handle-markdown-text.ts` + `splitText.ts` | 393+147+135 | **Every label in every diagram flows through this.** |
| themes registry | `themes/*.js` (10 files) + `styles.ts` | ~4200+165 | Port 5 core themes (default/base/dark/forest/neutral) first |

#### Stratum 1 — langium-parsed, no graph layout (pick first)

Ordered by complexity; best first-port candidates at the top.

| Diagram | Parser | DB | Renderer | Layout engine | Size | Rationale |
|---|---|---:|---:|---|---|---|
| pie | langium (20) | 71 | 199 | d3.pie + d3.arc | **S** | Absolute smallest; validates foundations end-to-end |
| packet | langium (21) | 53 | 94 | own grid | **S** | Sibling of pie |
| radar | langium (55) | 128 | 231+styles | polygon math | **S** | Pure geometry |

#### Stratum 2 — own-geometry jison/langium (no graph layout)

| Diagram | Parser | DB | Renderer | Layout engine | Size |
|---|---|---:|---:|---|---|
| ishikawa | jison (56) | 96 | 513 | own | M |
| user-journey | jison (69) | 130 | 356+svgDraw | own | M |
| timeline | jison (87) | 300+ | 387+vertical | own | M |
| quadrant-chart | jison (187) | 230 | 623+175 | own (scaleLinear) | M |
| xychart | jison (172) | 234 | 262+412 | own (BaseAxis/BandAxis) | M |
| wardley | langium (150) | 165+206 | 1065 | own | L |
| gantt | jison (188) | 841 | 894 | own (dayjs, d3 axes) | L |
| sankey | jison (67) | 87 | 208 | **d3-sankey** (port ~600 LoC) | M |
| treemap | langium (90) | 95 | 526 | **d3-hierarchy + squarify** (port) | M |
| kanban | jison (166) | 255 | 102 | own column + kanbanItem shape | M |

#### Stratum 3 — unified-pipeline + dagre (port once, 5-6 diagrams fall out)

The big shared investment: port `rendering-util/render.ts` + whole
`rendering-util/rendering-elements/` (shapes/edges/clusters/markers/
nodes, ~3.2K LoC) + `rendering-util/layout-algorithms/dagre/` (791
LoC wrapping our in-house `dagre-rs`).

| Diagram | Parser | DB | Renderer | Size | Notes |
|---|---|---:|---:|---|---|
| er | jison (303) | 255 | 77 | M | Smallest — ideal first unified port |
| requirement | jison (267) | 352 | 39 | M | Tiny renderer, one new shape |
| class | jison (435) | 772 | 82 | L | Type modelling is heaviest DB work |
| state (v2 only) | jison (336) | 731+395 | 144 | L | Composite/nested → clusters |
| flowchart | jison (634) | 1186 | 62 | **XXL** | Largest parser & DB in repo; pays off once plumbing exists |
| block | jison (290) | 343 | 74+363+257 | L | Custom layout **on top of** unified render |
| mindmap | jison (127) | 414 | 120 | L | Needs cose-bilkent OR tidy-tree (recommend tidy-tree for determinism) |

#### Stratum 4 — bespoke / hard-parity (leave for late or relax parity)

| Diagram | Upstream LoC | Size | Parity risk |
|---|---:|---|---|
| sequence | 420+729+2129+2037 svgDraw | **XXL** | Bespoke 2-state-machine renderer, zero reuse from unified path. Biggest single renderer in repo |
| c4 | 322+834+685 | L | Built on legacy dagre-wrapper; DB entangles concepts with shapes |
| gitGraph | langium (56) + 525 AST + 1489 renderer | L | Pure hand-drawn commit/branch lanes |
| architecture | langium (52) + 394+553 | XL | **cytoscape-fcose force layout** — byte-exact parity essentially impossible without porting scientific numerical optimisation code |
| venn | jison (135) + 133+367 | XL | **@upsetjs/venn.js MDS circle packing** — same parity problem as fcose |

### Cross-cutting abstractions (LoC on upstream side)

Ranked by port priority:

1. **`rendering-util/render.ts`** (146) + **`rendering-util/types.ts`**
   (209) — the layout-algorithm registry. Unlocks all of Stratum 3.
2. **`rendering-util/rendering-elements/`** (~3200) — 72 shape files,
   edges (968), clusters (526), markers (976). Biggest "shape
   parity" lift in the project. Porting once covers every
   unified-pipeline diagram.
3. **`rendering-util/layout-algorithms/dagre/`** (791) — wraps our
   `dagre` crate. Handles sub-graph nesting, cluster insertion,
   edge routing.
4. **`rendering-util/createText.ts`** (393) + markdown / katex /
   splitText helpers — every label goes through this.
5. **`diagrams/common/common.ts`** (388) + `commonDb.ts` (32) +
   `populateCommonDb.ts` (14) + `svgDrawCommon.ts` (159).
6. **`config.ts`** (288) + `defaultConfig.ts` (303) + schema YAML +
   directive parsing — byte parity depends on exact merge order.
7. **Themes + per-diagram styles** — 10 theme files × ~420 LoC +
   per-diagram styles.ts (50-200 each). Total ~6K LoC.
8. **`diagram-api/`** (~500) — orchestration: diagramAPI, detectType,
   diagram-orchestration, frontmatter, comments, loadDiagram.

### External layouts (separate packages — MVP defers)

| Package | LoC | Used by | MVP stance |
|---|---:|---|---|
| `@mermaid-js/layout-elk` | 1094+209 | flowchart with `layout:elk` | **Defer** (already ignored) |
| `@mermaid-js/layout-tidy-tree` | 629 | mindmap alternative | Consider making default for mindmap to avoid cose-bilkent |
| `cytoscape` + `-cose-bilkent` + `-fcose` | — | mindmap (cose-bilkent), architecture (fcose) | **Defer architecture; mindmap via tidy-tree** |
| `@upsetjs/venn.js` | ~600 | venn | Port as pure math if we want venn in v1 |

---

## Part II — Community prior art: borrow matrix

All three community ports are MIT, license-compatible. Any code we
lift gets a file-header attribution block **and** a README credits
line per project.

### Project snapshots

| Project | Stars | Coverage | Parser | Layout | Fonts | Relevance |
|---|---:|---|---|---|---|---|
| [`mmdr`](https://github.com/1jehuang/mermaid-rs-renderer) | 1161 | 4 diagrams (flow/class/state/sequence) + 12 geometry ports | hand-rolled regex | custom Sugiyama-lite | `fontdb` + `ttf-parser` runtime | **Skeleton reference for Tier-1/2 geometry** |
| [`selkie`](https://github.com/btucker/selkie) | 20 | ~22 diagrams | pest grammars | own dagre port (7.5K LoC) | `fontdue` runtime | **Grammars as reading aid; eval framework is gold** |
| [`mmdflux`](https://github.com/kevinswiber/mmdflux) | 45 | 4 diagrams (flow/class/state/sequence) | pest + pair-walker | own dagre port + post-routing | char-ratio heuristic | **Post-dagre routing + arrow markers** |

### What we DON'T use (critical to understand)

- **None of their layout engines.** All three independently ported
  dagre (or invented heuristic substitutes); we already have
  `/ext/dagre` byte-cross-validated against dagre.js.
- **None of their text metrics.** All three pick runtime font
  loading or char-ratio heuristics — incompatible with our
  baked-DejaVu determinism target.
- **None of their parsers verbatim.** Pest / regex substitutes
  diverge from jison semantics; byte parity requires hand-rolled
  per-diagram parsers matching jison's stateful lexer modes.
- **None of their SVG emitters verbatim.** Attribute order and
  element ordering differ from upstream mermaid; we re-emit
  upstream's exact output.

### Borrow-per-diagram table

`mmdr:path#fn` means "read as reference"; `selkie:path` means "grammar
/ type-shape reference only"; `mmdflux:path` means "edge/marker emit
reference".

| Diagram | Stratum | Best source | What to borrow |
|---|---|---|---|
| pie | 1 | mmdr `src/layout/pie.rs` (188) | Skeleton; reimplement |
| packet | 1 | selkie `src/diagrams/packet/` | Grammar; reimplement |
| radar | 1 | selkie `src/diagrams/radar/` | Grammar reference |
| ishikawa | 2 | **none** | Original implementation |
| user-journey | 2 | mmdr `src/layout/journey.rs` (319) | Skeleton |
| timeline | 2 | mmdflux `src/render/timeline/` | Fullest timeline port |
| quadrant-chart | 2 | selkie grammar | Grammar + geometry reimpl |
| xychart | 2 | mmdr `layout/xychart.rs` (179) | Thin — upstream is primary ref |
| wardley | 2 | **none** | Original — 1065 LoC upstream renderer |
| gantt | 2 | selkie grammar + mmdr `layout/gantt.rs` (376) | Date handling: `chrono` replace `dayjs` |
| sankey | 2 | mmdr `src/layout/sankey.rs` (350) | Has d3-sankey port already |
| treemap | 2 | mmdr `src/layout/treemap.rs` (247) | Squarified treemap |
| kanban | 2 | selkie grammar | Thin layouts — reimpl on flex-grid |
| er | 3 | selkie `src/diagrams/er/` | Grammar + types.rs |
| requirement | 3 | selkie `src/diagrams/requirement/` | Grammar + types |
| class | 3 | selkie `src/diagrams/class/` + mmdr `src/parser.rs:1127` | Grammar + types (819 LoC) |
| state | 3 | selkie grammar (most complete v2) | Grammar + types (1114 LoC) |
| flowchart | 3 | selkie grammar + mmdflux `src/graph/routing/` | Grammar; **mmdflux** post-dagre routing |
| block | 3 | mmdr `src/layout/block.rs` (310) | Grid-pack geometry |
| mindmap | 3 | mmdr `src/layout/mindmap.rs` (487) | Tidy-tree geometry |
| sequence | 4 | selkie grammar + mmdr `src/layout/sequence.rs` (1447) | Lane/activation geometry skeleton |
| c4 | 4 | mmdr `src/layout/c4.rs` (975) | Largest substantial port available |
| gitGraph | 4 | mmdr `src/layout/gitgraph.rs` (1137) | Largest substantial port |
| architecture | 4 (deferred) | selkie `src/diagrams/architecture/` parser | Only if we revisit |
| venn | 4 | **none** | Original — port @upsetjs/venn.js math (~600 LoC) |

### High-leverage borrows (not per-diagram)

1. **selkie eval framework** — `/ext/selkie/src/eval/{checks,ssim,
   runner,cache,report}.rs`. Structural SVG diff + SSIM + reference
   cache + HTML/JSON reports. **Port the structure to our
   `tests/eval/` for Phase 4 feedback loops.** Highest-leverage
   single borrow.
2. **mmdr intersection helpers** — `/ext/mermaid-rs-renderer/src/
   layout/routing.rs:699 ray_polygon_intersection` + `:735
   ray_ellipse_intersection`. These ARE the `intersectPolygon` /
   `intersectRect` upstream helpers we flagged. Straight borrow
   with header attribution.
3. **mmdr theme color math** — `/ext/mermaid-rs-renderer/src/
   theme.rs:adjust_color` + `parse_color_to_hsl` (~287 LoC).
   Replaces upstream `khroma` JS dep.
4. **mmdflux edge markers + basis splines** — `/ext/mmdflux/src/
   render/graph/svg/edges/{basis,endpoints,markers,path_emit}.rs`
   (~2700 LoC). Reference for matching upstream arrow marker
   paths byte-exactly.
5. **mmdflux post-dagre routing** — `/ext/mmdflux/src/graph/
   routing/{orthogonal,label_lanes,label_gap,...}`. Upstream mermaid
   applies its own router on top of dagre coordinates; this is the
   fullest Rust reference available.
6. **mmdflux frontmatter + init** — `/ext/mmdflux/src/mermaid/
   theme_hint.rs` (334). Annoying parsing already solved.

---

## Part III — Internal architecture for mermaid-little

### III.1 Crate layout

**Single crate now; promote to workspace when `packages/web` lands.**

```toml
# Cargo.toml (now)
[package]
name = "mermaid-little"
# ...

# Cargo.toml (when wasm lands)
[workspace]
members = [".", "packages/web"]
```

Both sisters (plantuml-little + d2-little) converged on this. d2-little's
`lib.rs` header explicitly notes: *"24 sibling crates flattened into
sub-modules of this single crate so crates.io can publish it as one
artefact."*

Sub-crates are only justified when the user-facing API needs boundary
enforcement (e.g. separate WASM export surface). Otherwise internal
modules stay in the root crate.

### III.2 Source layout — four-parallel-trees (plantuml-little pattern)

For **each** diagram, four files in four parallel trees. No
`src/flowchart/{parser,render}.rs` bundling.

```
src/
├── lib.rs
├── main.rs                         (cli feature)
├── error.rs
├── font_data.rs                    (DONE — vendored from plantuml-little)
├── font_metrics.rs                 (DONE — vendored)
├── text.rs                         (TO VENDOR — plantuml-little CJK width)
├── config/                         (mermaid's config schema + directive + frontmatter merging)
│   ├── mod.rs
│   ├── defaults.rs
│   ├── directive.rs                (%%{init: ...}%%)
│   └── frontmatter.rs              (--- title/config ---)
├── preprocess.rs                   (CRLF / comments / etc)
├── detect.rs                       (diagram type dispatch)
├── theme/
│   ├── mod.rs                      (ThemeVariables struct)
│   ├── default.rs, base.rs, dark.rs, forest.rs, neutral.rs
│   └── color.rs                    (borrowed from mmdr theme.rs)
├── parser/
│   ├── mod.rs                      (per-diagram entry dispatch)
│   ├── common.rs                   (shared tokens, arrow syntax, directives)
│   ├── richtext.rs                 (port of plantuml creole.rs, TRIMMED to ~300 LoC)
│   ├── pie.rs, packet.rs, radar.rs, ...
│   └── (one file per diagram)
├── model/
│   ├── mod.rs                      (Diagram enum, DiagramMeta)
│   ├── richtext.rs                 (TO VENDOR — plantuml-little TextSpan)
│   └── (one file per diagram)
├── layout/
│   ├── mod.rs                      (DiagramLayout enum)
│   ├── dagre_bridge.rs             (adapter onto /ext/dagre)
│   ├── unified.rs                  (Stratum-3 rendering-util/render.ts port)
│   ├── intersect.rs                (borrowed: mmdr ray_polygon/ray_ellipse)
│   ├── routing.rs                  (post-dagre router; ref mmdflux)
│   └── (one file per diagram)
├── render/
│   ├── mod.rs                      (dispatch on DiagramLayout)
│   ├── svg.rs                      (root <svg>, defs, theme CSS)
│   ├── svg_richtext.rs             (tspan emitter; TRIMMED port from plantuml)
│   ├── shapes/                     (Stratum-3 rendering-util/rendering-elements port)
│   │   ├── mod.rs
│   │   ├── class_box.rs, er_box.rs, requirement_box.rs, ...
│   │   └── (one file per upstream shape — 72 shapes total)
│   ├── markers.rs                  (arrow/diamond/cross/...; ref mmdflux/render/graph/svg/edges/markers.rs)
│   ├── edges.rs                    (curve emission; ref mmdflux basis.rs)
│   └── svg_<diagram>.rs            (one per diagram)
└── logger.rs
```

**Enforced rule**: no diagram module > 1500 LoC. If a diagram's renderer
grows larger (likely: sequence at 4K, flowchart at 1K+), split by
concern the moment it crosses the line. This directly addresses
plantuml-little's main regret (95K-LoC monoliths from 1:1 Java port).

### III.3 Dispatch — monomorphic enum (plantuml-little pattern)

No `Box<dyn DiagramRenderer>`. Central enum + exhaustive match:

```rust
pub enum Diagram {
    Pie(model::pie::PieDiagram),
    Flowchart(model::flowchart::FlowchartDiagram),
    Sequence(model::sequence::SequenceDiagram),
    // ... one variant per diagram
}

pub enum DiagramLayout {
    Pie(layout::pie::PieLayout),
    Flowchart(layout::flowchart::FlowchartLayout),
    // ...
}

// in render/mod.rs:
pub fn render(diag: &Diagram, lay: &DiagramLayout, theme: &Theme) -> Result<String> {
    match (diag, lay) {
        (Diagram::Pie(d), DiagramLayout::Pie(l)) => svg_pie::render(d, l, theme),
        // ...
    }
}
```

Keeps the compiler as a safety net: adding a diagram variant requires
updates to every dispatch point, making partial implementations
impossible.

### III.4 Dagre strategy

**Keep git-pinned `kookyleo/dagre-rs`** for now.

- d2-little vendors its dagre into `src/dagre/` (~15K LoC in-tree).
  Justified there because they needed d2-specific modifications.
- We so far need **one** patch (`intersectPolygon` / `intersectRect`
  helpers). Small enough to stay as upstream-dagre extensions
  via `src/layout/intersect.rs`.
- Revisit if we accumulate 3+ patches — then vendor following
  d2-little's pattern.

### III.5 Richtext scope

Mermaid's label markup subset: `<b>`, `<i>`, `<u>`, `<s>`, `<br>`,
`**bold**`, `*italic*`, `#text styling`, plus katex `$...$`.

Plantuml-little's `creole.rs` is 1980 LoC with `//italic//` +
`__underline__` + 30 other inline forms; most conflict with mermaid's
syntax.

**Decision**: Hand-write a ~300 LoC `src/parser/richtext.rs` using
plantuml's `TextSpan` enum shape (vendor the enum itself from
`src/model/richtext.rs`). Katex stays a placeholder (no-render) for
v1.

### III.6 Test infrastructure

```
tests/
├── fixtures/                       (DONE — hand-written minimum)
├── ext_fixtures/                   (DONE — 1393 from demos+cypress)
├── reference/                      (DONE — 1394 byte-exact SVGs)
│   └── VERSION
├── known_ignored.txt               (DONE)
├── support/                        (DONE — Node+jsdom+DejaVu)
├── generate_test_list.py           (TO ADD — port from plantuml-little)
├── reference_tests.rs              (AUTOGEN — one #[test] per fixture)
├── integration.rs                  (xmlns/NaN/tag-balance sanity)
├── dagre_cross_validate.rs         (TO ADD — mirror d2-little's approach)
├── eval/                           (TO ADD — selkie's framework ported)
│   ├── mod.rs
│   ├── structural_diff.rs
│   ├── ssim.rs
│   └── report.rs
└── port_*.rs                       (one per subsystem as we port upstream TS)
```

---

## Part IV — Phase 4 execution waves

All timing estimates assume focused port work with the eval framework
providing tight feedback. Total ~25-30 weeks for byte-exact parity on
23/25 diagrams (architecture + venn deferred or parity-relaxed).

### Wave 0 — Foundations (2-3 weeks)

Pre-requisite for every diagram. Builds the four-parallel-tree
skeleton without any diagram-specific code.

1. `src/config/` with directive + frontmatter + defaults + schema
2. `src/preprocess.rs` + `src/detect.rs`
3. `src/theme/` with 5 core theme variants (default/base/dark/forest/neutral)
4. `src/render/svg.rs` root <svg> scaffold + defs
5. `src/render/shapes/` stubs (real shapes land in Wave 3)
6. `src/render/markers.rs` + `src/render/edges.rs` (full port from mmdflux refs)
7. `src/parser/richtext.rs` + `src/model/richtext.rs` + `src/render/svg_richtext.rs` (trim from plantuml)
8. `src/layout/intersect.rs` (port mmdr intersections with attribution)
9. `src/text.rs` (CJK width from plantuml)
10. **tests/eval/** scaffolding (port selkie's structural-diff)

Exit criteria: `src/lib.rs` compiles; `Diagram` enum has placeholder
variants for all 25 types; eval harness diffs two empty SVGs.

### Wave 1 — Pie end-to-end (3-5 days)

**Single-diagram validation of the foundation.** If anything's wrong
with Wave 0, pie surfaces it first.

- `src/parser/pie.rs` (hand-write — ~100 LoC)
- `src/model/pie.rs` (~50 LoC — PieDiagram/PieSlice structs)
- `src/layout/pie.rs` (~150 LoC — reference mmdr `src/layout/pie.rs`)
- `src/render/svg_pie.rs` (~150 LoC)

Exit criteria: `tests/fixtures/pie/01.mmd` renders byte-exactly to its
reference SVG; all 13 pie fixtures in `tests/ext_fixtures/` pass.

### Wave 2 — Remaining Stratum-1 diagrams (1 week)

Pattern established; mechanical fan-out:
- packet (langium grammar, grid geometry)
- radar (langium grammar, polygon math)

### Wave 3 — Unified-pipeline plumbing (3-4 weeks)

**Biggest single infrastructure investment.** Unlocks Stratum 3 (er,
requirement, class, state-v2, flowchart, block) in subsequent waves.

1. `src/layout/unified.rs` — port `rendering-util/render.ts` +
   `types.ts` LayoutData shape
2. `src/render/shapes/*` — port all 72 shapes from
   `rendering-util/rendering-elements/shapes/`
3. `src/layout/dagre_bridge.rs` — adapter calling `/ext/dagre`
4. `src/layout/routing.rs` — post-dagre router (ref mmdflux)
5. Edge label placement + cluster boundaries

Exit criteria: a hand-crafted minimal graph renders byte-exactly via
the unified pipeline.

### Wave 4 — Stratum 3 diagrams, in dependency order (4-6 weeks)

Each diagram is a short per-diagram add on top of Wave 3 plumbing.

1. er (M) — smallest; validates pipeline
2. requirement (M) — 39-line renderer, one new shape
3. class (L) — heaviest DB work (type modelling)
4. state-v2 (L) — composite states = clusters
5. flowchart (XXL) — biggest parser & DB, but layout is free
6. block (L) — custom layout atop unified render

### Wave 5 — Stratum 2 geometry diagrams (2-3 weeks, can parallelise with Wave 4)

ishikawa, user-journey, timeline, quadrant-chart, xychart — all
self-contained geometry. Skeleton references in mmdr/selkie.

### Wave 6 — Heavy Stratum-2 diagrams (3-4 weeks)

- wardley (1065-LoC renderer in upstream)
- gantt (dayjs → chrono; 841-LoC DB)
- sankey (port d3-sankey algorithm ~600 LoC)
- treemap (port d3-hierarchy + squarify)
- kanban (flex-grid)
- mindmap (tidy-tree as default — avoid cose-bilkent entirely)

### Wave 7 — Sequence (3-4 weeks — its own milestone)

Sequence is the biggest renderer in the whole upstream (>4K LoC).
Zero reuse from unified pipeline. Scope as its own milestone.

### Wave 8 — c4, gitGraph (2-3 weeks)

Bespoke renderers; large but self-contained.

### Wave 9 — v1 decisions for architecture + venn

At this point, decide:
- **architecture** — ship with parity-relaxed mode (compare
  structure not pixels)? Defer to v2? Port cytoscape-fcose?
- **venn** — port @upsetjs/venn.js math (~600 LoC of
  scientific numerical optimisation)? Defer?

---

## Part V — Decisions needing user sign-off

Before starting Wave 0, I need answers to these:

### V.1 Workspace promotion timing

**Recommendation**: Flip `Cargo.toml` to workspace only when
`packages/web` lands (Wave 9+). Keep root lean through Wave 8.

### V.2 Dagre fork management

**Recommendation**: Keep git-pinned `kookyleo/dagre-rs`. Revisit if we
accumulate 3+ patches (currently 1: intersect helpers).

### V.3 Wave ordering

The waves above pay off the unified-pipeline investment. Alternative
orderings exist (e.g. breadth-first Stratum-1 before plumbing). I
recommend the plumbing-first order; confirm.

### V.4 Architecture + venn handling

These two diagrams have fundamentally non-deterministic layouts
upstream (fcose force layout, MDS circle packing). Options:

- **A**: Skip both in v1, document as "upstream parity not feasible"
- **B**: Implement with parity-relaxed mode (structural diff only)
- **C**: Port the numerical optimisation code

Recommendation: **A** for architecture (it's already in
known_ignored.txt via the cytoscape dep), **C** for venn (the math
is ~600 LoC and self-contained).

### V.5 Mindmap layout

Upstream default is `cytoscape-cose-bilkent` (non-deterministic).
Upstream also ships `@mermaid-js/layout-tidy-tree` as alternative.

**Recommendation**: Make tidy-tree our default for mindmap. We'd be
deliberately diverging from upstream's default for parity reasons.
Flag in README.

### V.6 Richtext parser strategy

**Recommendation**: Hand-write focused ~300 LoC parser matching
mermaid's label markup subset. Vendor plantuml's `TextSpan` enum but
not the creole parser. Confirm before Wave 0.

### V.7 Attribution format

For borrowed code:

```rust
// Portions adapted from <project> (https://github.com/...)
// © <author>, MIT license. Specifically: <symbol>, <symbol>.
```

Plus `CREDITS.md` listing projects + what we took. Current README
already credits prior art in general terms.

Confirm this format.

---

## Appendix: effort summary

| Wave | Scope | Estimate |
|---|---|---:|
| 0 | Foundations | 2-3 w |
| 1 | pie end-to-end | 3-5 d |
| 2 | packet, radar | 1 w |
| 3 | Unified pipeline plumbing | 3-4 w |
| 4 | er → requirement → class → state-v2 → flowchart → block | 4-6 w |
| 5 | Stratum-2 geometry | 2-3 w (parallel) |
| 6 | wardley, gantt, sankey, treemap, kanban, mindmap | 3-4 w |
| 7 | sequence | 3-4 w |
| 8 | c4, gitGraph | 2-3 w |
| 9 | architecture + venn decisions | TBD |
| **Total for 23/25 diagrams** | | **~22-28 weeks** |
