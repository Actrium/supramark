# Upstream Tracking — mermaid-little

## Source repo
- **Upstream:** https://github.com/kookyleo/mermaid-little
- **Pinned commit (at merge time):** `7beb9ab1f737833bb4798eafe6a59da97dab1401`
- **Pinned tag:** (HEAD of `main`; no tag yet at merge time)
- **Merged into supramark on:** 2026-05-09 (step 4 of the super-monorepo plan)

## License relationship
- **mermaid-little license:** `MIT`.
- **mermaid (the project being reimplemented):** `MIT` upstream.
- mermaid-little is a **reimplementation in Rust** targeting byte-exact
  SVG parity with `mermaid@11.14.0`. Font metrics are shared with sister
  project `plantuml-little` via the in-tree crate `font-metrics`
  (extracted 2026-05-09; previously a hand-vendored copy of plantuml-
  little @ `b32d6aa` lived in `crates/mermaid-little/src/font_data.rs`).
  The shared crate's offline-baked Java-FontMetrics-equivalent range
  tables (~22K LOC) were deleted on 2026-05-10 in favour of
  `TtfParserJavaCompatMetrics` (TtfParser + Java AWT italic-skew
  adjustment) which produces byte-identical output from the embedded
  DejaVu Latin subset alone.
- MIT ⇆ Apache-2.0 (supramark default) is fully compatible.

## Relationship
- [x] reimplementation (Rust rewrite of mermaid-js)
- [ ] fork
- [ ] bindings

## Sub-tree contents
| Path | Purpose | License |
|---|---|---|
| `crates/mermaid-little/Cargo.toml` | Root crate manifest (single-crate; no inner workspace) | MIT |
| `crates/mermaid-little/src/` | Rust library + CLI binary | MIT |
| `crates/mermaid-little/packages/web/` | crate `mermaid-little-web` — wasm-bindgen wrapper | MIT |
| `crates/mermaid-little/tests/` | Reference SVG parity tests against mermaid@11.14.0 | MIT |
| `crates/mermaid-little/docs/` | Implementation notes | MIT |
| `crates/mermaid-little/dagre-d3-es-7.0.14.tgz` | Vendored upstream dagre-d3-es tarball (used as a parity reference, not as a runtime dep) | MIT |

## Workspace integration notes
- mermaid-little is a clean single-crate manifest with no inner
  `[workspace]` declaration — supramark root `/Cargo.toml` lists it
  directly without needing the workspace-strip patch we applied to
  plantuml-little / d2-little.
- The original repo's `Cargo.lock` was removed; supramark workspace
  uses a single root `Cargo.lock`.

## Local patches (a.k.a. things to upstream)
**`packages/web/` is supramark-side downstream.** mermaid-little upstream
does not yet ship a wasm wrapper. We added one inside the merged
sub-tree to bring it into structural parity with `plantuml-little` and
`d2-little`, so supramark's `packages/engines` can consume an in-tree
`@kookyleo/mermaid-little-web`. Patch contents:

| File | Status | Notes |
|---|---|---|
| `packages/web/Cargo.toml` | added | wasm-bindgen wrapper crate (mirrors d2-little-web shape) |
| `packages/web/src/lib.rs` | added | re-exports `convert` / `convert_with_id` / `version` |
| `packages/web/src/index.ts` | added | TS wrapper exporting from generated wasm bundle |
| `packages/web/package.json` | added | npm publish config for `@kookyleo/mermaid-little-web` |
| `packages/web/tsconfig.json` | added | matches plantuml-little-web tsconfig |
| `packages/web/README.md` | added | usage docs + provenance note |
| `Cargo.toml` (root crate) | edited | added `version = "0.1"` constraint to the `dagre` git dep so cargo-deny accepts it as a non-wildcard. Net behaviour identical because the supramark workspace root `[patch."https://github.com/kookyleo/dagre-rs.git"]` redirects to in-tree `crates/dagre`. |

**Upstream PR plan:** open a PR against
`https://github.com/kookyleo/mermaid-little` proposing this directory
verbatim. When upstream merges, we resolve any `subtree pull` conflict
by accepting upstream's version. If upstream takes a different shape
(e.g. different package name), align supramark to match.

## Sync cadence
- **Upstream activity:** Active, kookyleo's project. May grow tagged
  releases; not yet at the time of merge.
- **Sync strategy:** subtree pull on each upstream change, with the
  expectation that the `packages/web/` patch above will eventually be
  upstream-resolved.
  ```
  git fetch subtree-mermaid
  git subtree pull --prefix=crates/mermaid-little subtree-mermaid main
  ```
- **No CLA** — kookyleo owns it.

## supramark-side metrics-* feature flags

The crate exposes a `metrics-{ttf-parser, host-callback, ffi-callback}`
family — the `Metrics` impl `crate::font_metrics` routes through is
selected at compile time and the family is mutually exclusive. The
crate's own default feature set deliberately includes NONE of them:
production consumers (e.g. `mermaid-little-web`) must
`default-features = false` and explicitly opt into one platform impl,
so the choice is visible in the consumer's `Cargo.toml`. A
`[dev-dependencies]` self-cycle flips on `metrics-ttf-parser` so
`cargo test` keeps running the `*_byte_exact.rs` reference suites
unchanged. `metrics-ttf-parser` doubles as both the byte-equal
upstream-Mermaid path AND the production native fallback: a
2026-05-10 measurement spike confirmed raw
`TtfParserMetrics::default_latin()` (the embedded DejaVu Latin subset
parsed via ttf-parser) matches Java FontMetrics to sub-0.0001 px on
the discriminating italic test (`«archimate-node»` italic =
128.385742 px vs Java 128.3857 px, delta = 0.000042 px), so no
italic-skew wrapper is needed. `metrics-ffi-callback` is reserved
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
- Land the `packages/web/` patch upstream so this section can shrink to
  "no local patches".
- ✅ (step 4) Wire mermaid-little-web into supramark's
  `packages/engines/src/mermaid/index.ts` — replaces `beautiful-mermaid`
  on the web path. Run `bun run build:wasm` first to produce the
  in-tree dist/.
- Decide whether `katex` and `cose_bilkent` features should be enabled
  in the default wasm build — they pull `rquickjs` (~1MB) but unlock
  byte-exact LaTeX math + mindmap layout parity.
