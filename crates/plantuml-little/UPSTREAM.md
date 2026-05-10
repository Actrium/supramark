# Upstream Tracking — plantuml-little

## Source repo
- **Upstream:** https://github.com/kookyleo/plantuml-little
- **Pinned commit (at merge time):** `c6d01ba4a7c30cd8c4289f5e1d772e2e18fb1d3b`
- **Pinned tag:** `v1.2026.2-3`
- **Merged into supramark on:** 2026-05-09 (step 3 of the super-monorepo plan)

## License relationship
- **plantuml-little license:** **multi-license disjunction** —
  `GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0 OR EPL-2.0 OR MIT`.
  Downstream consumers may pick whichever option they prefer.
- supramark consumes plantuml-little under **Apache-2.0** to match the
  monorepo's default licence; per-file headers in this sub-tree retain
  the upstream multi-license SPDX expression.
- **PlantUML upstream (the project being reimplemented):** GPL-3 / LGPL-3.
  plantuml-little is a **reimplementation, not a fork**. byte-exact SVG
  parity targets v1.2026.2 of upstream PlantUML — the parity is over
  output bytes, not source code. No upstream Java source files were
  copied.

## Relationship
- [x] reimplementation (Rust rewrite of Java PlantUML)
- [ ] fork
- [ ] bindings

## Sub-tree contents
| Path | Purpose | License |
|---|---|---|
| `crates/plantuml-little/Cargo.toml` | Root crate manifest (was a workspace + package; inner `[workspace]` removed during merge — see below) | multi (5-way OR) |
| `crates/plantuml-little/src/` | Rust library + CLI binary | multi (5-way OR) |
| `crates/plantuml-little/packages/web/` | crate `plantuml-little-web` — wasm-bindgen wrapper, published as npm `@kookyleo/plantuml-little-web` | multi (5-way OR) |
| `crates/plantuml-little/stdlib/` | PlantUML stdlib (sprites, includes) — vendored | multi (5-way OR), or upstream-PlantUML licence for original sprites |
| `crates/plantuml-little/tests/` | Test fixtures (PUML inputs + expected SVG references) | multi (5-way OR) |
| `crates/plantuml-little/examples/` | dump_parse / dump_preproc / dump_svg debug binaries | multi (5-way OR) |

## Workspace integration notes
- The original `crates/plantuml-little/Cargo.toml` declared an inner
  `[workspace] members = [".", "packages/web"]`. That declaration was
  **removed during merge** because cargo does not allow nested workspaces.
  The supramark root `/Cargo.toml` lists both members directly.
- The original repo's `Cargo.lock` was removed; supramark workspace uses
  a single root `Cargo.lock`.
- Cargo emits a harmless warning:
  `profiles for the non root package will be ignored, specify profiles at the workspace root`
  This is from the upstream `[profile.*]` overrides in
  `crates/plantuml-little/Cargo.toml`. Move them to root only if we
  decide they're worth aligning monorepo-wide; otherwise leave as-is.
- plantuml-little depends on `graphviz-anywhere = "^0.1.8"` from
  crates.io. Root `[patch.crates-io]` redirects it to the in-tree copy
  at `crates/graphviz-anywhere/packages/rust` so only one native build
  (`links = "graphviz_api"`) is linked into the final binary.

## Sync cadence
- **Upstream activity:** Active, kookyleo's project. Tagged releases.
- **Sync strategy:** subtree pull on each tagged release.
  ```
  git fetch subtree-plantuml
  git subtree pull --prefix=crates/plantuml-little subtree-plantuml main
  ```
  After pull, expect to re-resolve the inner `[workspace]` deletion
  conflict — keep ours (no inner workspace).
- **No CLA** — kookyleo owns it.

## supramark-side metrics-* feature flags

The crate exposes a `metrics-{ttf-parser, host-callback, ffi-callback}`
family — the `Metrics` impl `crate::font_metrics` routes through is
selected at compile time and the family is mutually exclusive. The
crate's own default feature set deliberately includes NONE of them:
production consumers (e.g. `plantuml-little-web`) must
`default-features = false` and explicitly opt into one platform impl,
so the choice is visible in the consumer's `Cargo.toml`. A
`[dev-dependencies]` self-cycle flips on `metrics-ttf-parser` so
`cargo test` keeps running the 268+ byte-equal-with-Java reference SVG
suite unchanged. `metrics-ttf-parser` doubles as both the byte-equal-
Java path AND the production native fallback: a 2026-05-10 measurement
spike confirmed raw `TtfParserMetrics::default_latin()` (the embedded
DejaVu Latin subset parsed via ttf-parser) matches Java FontMetrics to
sub-0.0001 px on the discriminating italic test (`«archimate-node»`
italic = 128.385742 px vs Java 128.3857 px, delta = 0.000042 px), so
no italic-skew wrapper is needed. `metrics-ffi-callback` is reserved
for the planned React-Native native-FFI wrapper; enabling it today
fires a `compile_error!` because no impl ships yet.

Historical note: prior to 2026-05-10 this slot was `metrics-static-dejavu`,
gated by ~22K LOC of offline-baked Java-FontMetrics-equivalent range
tables. A first cleanup pass replaced those with a wrapper named
`TtfParserJavaCompatMetrics` (TtfParser + Java AWT italic-skew
adjustment) under a `metrics-java-compat` feature. The follow-up spike
above showed the italic-skew adjustment was based on a wrong AWT
assumption and over-corrected widths, while raw `TtfParserMetrics`
already matched Java byte-for-byte — both the wrapper and the
`metrics-java-compat` feature were therefore deleted in favour of
`metrics-ttf-parser`.

## Outstanding
- Once `mermaid-little` lands in step 4, both will share the in-tree
  `graphviz-anywhere` (mermaid-little uses it for layout via the same
  global bridge that plantuml-little installs in wasm).
- Decide whether to publish `plantuml-little-web` from this monorepo or
  keep cutting npm releases from the upstream repo.
- **Engine switchover deferred to step 4.** supramark's
  `packages/engines/src/web.ts` still loads `@kookyleo/plantuml-little-web`
  from npm. The vendored switchover (workspace-resolve to
  `crates/plantuml-little/packages/web` after a `wasm-pack` build) lands
  together with mermaid-little + a CI wasm-build job, so we don't ship
  half-built local symlinks that break `bun install` flows.
