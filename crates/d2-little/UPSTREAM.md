# Upstream Tracking — d2-little

## Source repo
- **Upstream:** https://github.com/Actrium/d2-little
- **Pinned commit (at merge time):** `a8db3b23f3ba2f89e031e70525ace34b1d3f0226`
- **Pinned tag:** `v0.7.2`
- **Merged into supramark on:** 2026-05-09 (step 3 of the super-monorepo plan)

## License relationship
- **d2-little license:** `MPL-2.0` (file-level copyleft).
- **terrastruct/d2 (the project being ported):** `MPL-2.0` (same).
- d2-little is a **pure-Rust port** of the d2lang pipeline (parser → IR
  → graph → dagre layout → SVG). No Go source from upstream is shipped
  here, but the port closely follows upstream's algorithms; per
  ADR-002 this is treated as an MPL-2.0 derivative.
- MPL-2.0 ⇆ Apache-2.0 (supramark default) is **link-compatible**:
  consuming d2-little from supramark does not propagate MPL to
  supramark's own files. Modifications to MPL files inside this
  sub-tree must keep the MPL header.

## Relationship
- [x] port (pure-Rust port of terrastruct/d2)
- [ ] fork
- [ ] bindings

## Sub-tree contents
| Path | Purpose | License |
|---|---|---|
| `crates/d2-little/Cargo.toml` | Root crate manifest (was a workspace + package; inner `[workspace]` removed during merge) | MPL-2.0 |
| `crates/d2-little/src/` | Parser, IR, graph, dagre layout, SVG renderer | MPL-2.0 |
| `crates/d2-little/packages/web/` | crate `d2-little-web` — wasm-bindgen wrapper, published as npm `@actrium/d2-little-web` | MPL-2.0 |
| `crates/d2-little/tests/` | E2E + unit tests with reference data | MPL-2.0; reference data files inherit terrastruct/d2 attribution |
| `crates/d2-little/examples/` | dump_basic / dump_ir / dump_layout / measure debug binaries | MPL-2.0 |
| `crates/d2-little/ttf/` | Bundled font files (for byte-exact text-metric parity) | each font has its own license — see file headers |
| `crates/d2-little/assets/`, `mathjax.js` | Vendored runtime assets | upstream-attributed |

## Workspace integration notes
- The original `crates/d2-little/Cargo.toml` declared an inner
  `[workspace] members = [".", "packages/web"] resolver = "3"`. That
  declaration was **removed during merge** because cargo does not
  allow nested workspaces. The supramark root `/Cargo.toml` lists
  both members directly.
- The original repo's `Cargo.lock` was removed; supramark workspace
  uses a single root `Cargo.lock`.
- d2-little has its own internal dagre implementation (vendored from
  the algorithm, not depending on the `dagre` crate at
  `crates/dagre`). Step 4 may consider unifying — until then, keep as
  upstream ships.
- d2-little uses `resolver = "3"` upstream; supramark root uses
  `resolver = "2"`. The root resolver wins. No observed semantic
  divergence so far; revisit if rust-version edge cases appear.

## Sync cadence
- **Upstream activity:** Active, kookyleo's project. Tagged releases.
- **Sync strategy:** subtree pull on each tagged release.
  ```
  git fetch subtree-d2
  git subtree pull --prefix=crates/d2-little subtree-d2 main
  ```
  After pull, expect to re-resolve the inner `[workspace]` deletion
  conflict — keep ours (no inner workspace).
- **No CLA** — kookyleo owns it.

## Outstanding
- Decide whether to retire d2-little's internal dagre module in favour
  of the in-tree `dagre` crate (`crates/dagre`). Step 5 candidate.
- Decide whether to publish `d2-little-web` from this monorepo or keep
  cutting npm releases from the upstream repo.
- **Engine switchover deferred to step 4.** supramark's
  `packages/engines/src/web.ts` still loads `@actrium/d2-little-web`
  from npm; the vendored switchover lands in step 4 together with
  mermaid-little and a CI wasm-build job.
