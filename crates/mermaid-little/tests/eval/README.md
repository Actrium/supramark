# `tests/eval/` — SVG evaluation framework

A Rust-only, test-support module for comparing mermaid-little's SVG output
against the byte-exact mermaid.js references under `tests/reference/`.
Ported & trimmed from [selkie](https://github.com/btucker/selkie)'s
`src/eval/` (MIT licensed).

## Modules

| file | lines | purpose |
|---|---|---|
| `mod.rs` | ~45 | module root + re-exports |
| `structural_diff.rs` | ~620 | roxmltree-based SVG parsing + structural diff |
| `ssim.rs` | ~135 | SSIM math for pre-decoded grayscale buffers (raw-SVG path stubbed) |
| `report.rs` | ~340 | text / JSON / HTML report emitters |

## Using it from a test file

Because `tests/eval/` is a directory (not a `tests/*.rs` file), Cargo does
**not** compile it as its own integration-test binary. Pull it in from
any test file with `#[path = ...] mod eval;`:

```rust
// tests/some_diagram.rs
#[path = "eval/mod.rs"]
mod eval;

use eval::structural_diff;

#[test]
fn flowchart_01_matches_reference() {
    let source = include_str!("fixtures/flowchart/01.mmd");
    let candidate = mermaid_little::convert(source).unwrap();
    let reference =
        std::fs::read_to_string("tests/reference/fixtures/flowchart/01.svg").unwrap();

    let diff = structural_diff::compare(&candidate, &reference).unwrap();
    assert!(diff.is_empty(), "divergence:\n{}", diff.report_text());
}
```

For a full sweep, collect per-fixture results into an `EvalReport`:

```rust
#[path = "eval/mod.rs"]
mod eval;

use eval::{structural_diff, EvalReport, FixtureReport};
use std::path::PathBuf;

#[test]
#[ignore] // run explicitly: `cargo test --test sweep -- --ignored`
fn sweep_all_fixtures() {
    let mut report = EvalReport::new();
    for entry in walkdir::WalkDir::new("tests/fixtures") {
        /* ... */
        let diff = structural_diff::compare(&candidate, &reference).unwrap();
        report.push(FixtureReport::new(name, diff).with_type(dtype));
    }
    report.write_all(&PathBuf::from("target/eval")).unwrap();
}
```

The HTML report lists every fixture, its issues, and highlights
errors / warnings / matches with colour.

## What's checked

| level | check | notes |
|---|---|---|
| ERROR | `node_count`, `edge_count`, `labels_missing` | structural break |
| WARN | `dimensions` (> 20 %), `shapes` (> 20 % diff per shape), `z_order`, `colors` | noticeable visual drift |
| INFO | `dimensions` (5-20 %), `markers`, `labels_extra` | acceptable variation |

Thresholds are tweakable via `CheckConfig`.

## What's deferred

* **SSIM for raw SVGs** — `ssim::ssim_svg` currently returns `Err(..)`.
  The math (`calculate_ssim`, `rgba_to_grayscale`, `resize_grayscale`) is
  ready; wire in `resvg` + `image` decoding behind a future `eval-ssim`
  feature when visual scoring becomes worthwhile.
* **Diagram-specific checks** — selkie's per-diagram structural checks
  (timeline, architecture, composite-state centring, etc.) are not
  ported; they belong in Phase 4+ once we're actively rendering each
  diagram type.
* **CSS-aware stroke / visibility analysis** — depends on `simplecss`;
  omitted to keep dev-deps minimal.

## Dependencies

Added `roxmltree = "0.20"` under `[dev-dependencies]` in the root
`Cargo.toml`. No other new deps; SSIM decoding is stubbed.
