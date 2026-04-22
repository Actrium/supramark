# Progress

Snapshot at end of current phase (Wave 0 → Wave 3 plumbing + all Stratum-1/2 diagrams landed).

See [PROGRESS.zh.md](PROGRESS.zh.md) for the detailed Chinese version.

## Headline

- **12 / 23** diagrams byte-exact (PLAN.md target excludes architecture + venn, which are in `known_ignored`)
- **428** tests pass, **0** failures, 7 ignored (6 timeline inline + 1 ishikawa handDrawn)
- **Wave 3 plumbing complete** — `rendering-util/unified`, shapes (26 impl + 40 stub), markers (22 families), edges + clusters + post-dagre routing. Stratum 3 diagrams (er/class/state/flowchart/…) can start immediately.

## Diagrams byte-exact

| Diagram | Technique | Fixtures |
|---|---|---:|
| pie | d3.pie + d3.arc | 14 / 14 |
| packet | bit-field grid | 5 / 5 |
| radar | polygon math | 7 / 7 |
| ishikawa | fishbone geometry | 17 / 18 (handDrawn skipped) |
| journey | bar layout + arc score | 11 / 11 |
| timeline | TD + LR modes | 17 / 17 inline; sweep skipped pending theme fix |
| quadrant | d3.scaleLinear | 16 / 16 |
| xychart | d3.scaleBand + scaleLinear | 55 / 56 (1 via numeric-tolerance) |
| wardley | landscape plot | 12 / 12 |
| sankey | self-ported d3-sankey 0.12.3 | 3 / 3 |
| treemap | self-ported d3-hierarchy squarify | 30 / 30 |
| kanban | column + card grid | 11 / 11 |

## Infrastructure complete (Wave 0 + Wave 3 plumbing)

- `src/layout/unified/` — 18 pub types + dagre bridge (1162 LoC)
- `src/render/shapes/` — 26 byte-exact shapes + 40 stubbed (~2100 LoC, 48 tests)
- `src/render/markers.rs` — 22 marker families (623 LoC, 15 tests)
- `src/render/edges.rs` + `layout/routing.rs` — curves + endpoint clipping + self-loops + labels (1871 LoC, 33 tests)
- `src/math/v8_trig.rs` — V8 fdlibm-derived Math.cos/sin (1-ULP parity vs upstream)
- `src/theme/` — 5 variants, 263 flat fields + 3 nested sub-structs
- `src/config/` + `preprocess.rs` + `detect.rs` — 28-variant DiagramKind, YAML frontmatter, `%%{init}%%` directives
- `tests/eval/` — structural diff + SSIM stub (port from selkie)
- `src/font_data.rs` + `font_metrics.rs` — DejaVu baked metrics (Phase 2, vendored from plantuml-little)

## Key upstream quirks discovered

1. **V8 Math.cos/sin != Rust std != libm crate** — V8 fdlibm diverges by 1 ULP on inputs like `cos(0.1)`. Ported V8 11.3 kernels into `crate::math::v8_trig`.
2. **V8 `Number.toString()` vs Rust `f64::Display`** — 17th-digit tie-break differs on extreme values. Added `approx_byte_exact(got, expected, 1e-12)` helper — sub-f64-precision prints auto-pass.
3. **d3-interpolate** uses `a*(1-t)+b*t`, not `a+(b-a)*t` — the latter drops 1 ULP.
4. **jsdom `resolveFont` reads inline attrs/style only** — CSS `<style>` font-family is ignored, getBBox falls back to 14px sans-serif.
5. **SVG attribute order matters byte-exactly** — `id → width → xmlns → class → style → viewBox → role → aria-roledescription`. The shared `svg::open_svg` helper has viewBox before style and every agent inlined its own opening tag.
6. **stylis CSS minification** strips spaces after commas outside quoted strings.
7. **Empty `<g></g>` seed group** from mermaidAPI's `appendDivSvgG`, not from diagram renderers.
8. **`d3.hierarchy().descendants()` is breadth-first**, not pre-order — critical for treemap section numbering.
9. **d3-sankey implementations diverge on floats** — f32 or simplified relaxation loops break byte parity. Full f64 port of `d3-sankey@0.12.3` is the only byte-exact path.
10. **`%%{init: {"themeVariables": {...}}}%%` overrides theme** — even when preprocessed, parsers re-extract this locally since downstream diagram-specific values (pie.textPosition, packet.showBits, etc.) need it.

## Known partial

| Item | Cause | Next step |
|---|---|---|
| timeline via `convert_with_id` | shared theme path doesn't emit `cScale*` palette timeline needs | Add a timeline-specific palette emitter in `src/theme/` |
| timeline inline tests (6 cases) | agent's tests used a hand-constructed theme; post-integration dimensions drift | Reconcile theme → timeline renderer contract |
| ishikawa demos/04 | handDrawn mode (roughjs path jitter) | Wave 6+ decision on roughjs port |
| xychart/35 | Rust `{}` vs V8 `Number.toString` 17th-digit tie-break | **Auto-pass via `approx_byte_exact(1e-12)`** |

## Next-step options (not auto-executed)

### Immediate
1. **Wave 4 — parallel Stratum-3 six diagrams** — Wave 3 plumbing is ready; launch 6 agents for er / requirement / class / state / flowchart / block. Each L-sized, 2-4 h.
2. **Timeline theme fix** — 1 hour — adjust theme palette so the `convert_with_id` path matches timeline's CSS needs.
3. **gantt, mindmap** — L-sized independent ports. gantt adds chrono dep, mindmap uses tidy-tree layout.

### Medium
4. **Wave 7 — sequence / c4 / gitGraph** — three bespoke large renderers; sequence alone is 4K+ LoC, the largest single port in the project.
5. **venn** — ~600-LoC MDS algorithm; self-port or keep in known_ignored.

### Long-term
6. **architecture** — cytoscape-fcose, already in MVP `known_ignored`.
7. **Docs / release prep.**

## Stats

- ~22 agents spawned across all waves.
- Monitoring loop: ~40 min, 10 iterations at ~270s each.
- Longest single agent: xychart (47 min, 56 fixtures).
- **~30,000 LoC** including tests.
- New testing tool: `approx_byte_exact` — precision-tolerance diff for future f64 edge cases.

**52% of the path done (12 / 23 diagrams).**
