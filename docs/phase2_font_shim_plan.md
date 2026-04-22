# Phase 2 — Font Metric Shim Plan

Source: research agent report, Phase 1 companion work.

## Summary

- 7 diagram types do **zero** runtime text measurement and can be
  ported without any DOM shim:
  `c4`, `packet`, `quadrant-chart`, `radar`, `requirement`, `sankey`,
  `venn`.
- 19 diagram types depend on some combination of `getBBox`,
  `getComputedTextLength`, `getBoundingClientRect`, or the shared
  `computeDimensionOfText` helper. They need the shim on both sides
  of the pipeline (Node generator + Rust renderer).
- Upstream's default font family is `"Open Sans", sans-serif` in 26
  theme slots, with `"trebuchet ms", verdana, arial, sans-serif` in 3
  legacy slots (c4, gantt, flowchart). Neither is deterministic
  across machines.
- plantuml-little solves the same problem by baking DejaVu Sans
  metrics into `src/font_data.rs` (4 fonts, 3458+ glyph ranges, 5918+
  codepoints) and exposing `font_metrics::text_width` /
  `char_width` / `line_height`. mermaid-little should mirror this
  exactly, sharing the generator script.

## Integration Points

### Rust side (`src/font_data.rs` + `src/font_metrics.rs`)

Copy the plantuml-little layout verbatim:

```rust
pub struct FontMetrics {
    pub units_per_em: u16,
    pub ascender: i16,
    pub descender: i16,
    pub typo_ascender: i16,
    pub ranges: &'static [(u32, u32, u16)], // (start, end, advance)
}
```

Families to bake: DejaVu Sans + DejaVu Sans Bold + DejaVu Sans Mono +
DejaVu Sans Mono Bold. Generation script `gen_font_data.py` is not
in the plantuml-little repo — we need to reconstruct one (reads TTF,
emits Rust source).

### JS/Node side (`tests/support/`)

Three entry points mermaid calls, all forwardable to the same
metric table:

| API | Implementation |
|---|---|
| `SVGElement.prototype.getBBox` | compute bbox from `textContent` via `text_width(text, family, size)` + `line_height` per line |
| `SVGElement.prototype.getComputedTextLength` | single-line `text_width(text, family, size)` |
| `HTMLElement.prototype.getBoundingClientRect` | fallback to `getBBox` for foreignObject labels |

Font choice in shim: always coerce the mermaid-configured family to
`"DejaVu Sans"` (or Mono) so the metric lookup hits our baked table.
This is a deliberate divergence from upstream defaults — we trade
visual fidelity to system "Open Sans" for perfect cross-machine
determinism, the same trade plantuml-little makes against
sans-serif fontconfig drift.

## Risks / Open Questions

- Mermaid may internally assume specific baselines from Open Sans
  (e.g., cap-height vs x-height used in label vertical centring).
  Check after the shim lands whether any diagram visibly regresses.
- Some diagrams (pie legend, mindmap) use `getBoundingClientRect`
  via foreignObject — jsdom may need an HTMLElement-prototype shim
  too, not only SVGElement.
- `getExtentOfChar` / `getSubStringLength` were NOT found by the
  research pass; if they turn up during Phase 4 diagram porting, add
  them to the shim at that point.
