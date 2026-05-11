# Upstream Tracking — graphviz-anywhere

## Source repo
- **Upstream:** https://github.com/kookyleo/graphviz-anywhere
- **Pinned commit (last subtree pull):** `631e871c726041241252fc874c98725db5f308a2`
- **Pinned tag:** `test-v0.2.0-rc1` (release candidate; promote to `v0.2.0` once CI green)
- **First subtree merge into supramark:** 2026-05-09 at `436fe2f` / `v0.1.7`
- **Last subtree pull:** 2026-05-11 — 0.2.0 brings full iOS / Android-x86 / Linux-aarch64 / Windows-arm64 coverage in `build.rs`, scripts, and CI; also static `.a` alongside shared libs so `prebuilt/<target-triple>/` actually has content. Unblocks `plantuml-little` cross-compile for iOS + 4 Android ABI (see `docs/architecture/native-ffi-blockers.md` 阻塞 #1 + #4).

## License relationship
- **graphviz-anywhere wrapper code:** Apache-2.0 (see `LICENSE`).
  - Applies to: `packages/rust/`, `packages/web/`, `packages/react-native/`, `capi/`, `examples/`, `scripts/`.
  - This wrapper does not include any Graphviz source code in this repo — it consumes Graphviz via the `graphviz/` git submodule (see below) at build time.
- **Graphviz upstream (the C library being wrapped):** EPL-1.0 / CPL-1.0
  - Lives at https://gitlab.com/graphviz/graphviz, registered as a git submodule at `graphviz/`.
  - Built artifacts (in `packages/rust/prebuilt/` after a release build, and embedded in `packages/web/dist/viz.js` wasm) are derivative works of Graphviz and inherit EPL-1.0 — distributed under EPL-1.0 only.
  - Per ADR-002 in `docs/architecture/LICENSE_COMPATIBILITY.md`, EPL-1.0 boundaries are isolated to `graphviz/` and built artifact paths; never source-mixed with Apache files.

## Relationship
- [ ] reimplementation
- [ ] fork
- [x] bindings (Rust + wasm + RN bridges over Graphviz C ABI)

## Sub-tree contents
| Path | Purpose | License |
|---|---|---|
| `crates/graphviz-anywhere/Cargo.toml` | Inner workspace root (now shadowed by supramark root; see below) | Apache-2.0 |
| `crates/graphviz-anywhere/packages/rust/` | Crate `graphviz-anywhere` — safe Rust wrapper + wasm32 bridge | Apache-2.0 |
| `crates/graphviz-anywhere/packages/web/` | npm `@kookyleo/graphviz-anywhere-web` — wasm + JS bindings | Apache-2.0 (built artifacts inherit EPL-1.0 from bundled Graphviz) |
| `crates/graphviz-anywhere/packages/react-native/` | npm `@kookyleo/graphviz-anywhere-rn` — RN bridge | Apache-2.0 |
| `crates/graphviz-anywhere/capi/` | C API wrapper around Graphviz | Apache-2.0 |
| `crates/graphviz-anywhere/examples/` | Rust + Web + RN demos | Apache-2.0 |
| `crates/graphviz-anywhere/graphviz/` | (empty) submodule placeholder for upstream Graphviz | EPL-1.0 (when populated) |
| `crates/graphviz-anywhere/packages/rust/prebuilt/` | (empty until built) compiled Graphviz binaries | EPL-1.0 derivatives |
| `crates/graphviz-anywhere/packages/web/dist/viz.js` | Pre-committed wasm build of Graphviz + JS glue | EPL-1.0 derivative + Apache (JS glue) |

## Submodule note
- The original `crates/graphviz-anywhere/.gitmodules` declares
  `path = graphviz` relative to that sub-tree. Subtree merge does not
  carry this through to supramark's root `.gitmodules`; the directory
  is empty.
- To populate Graphviz source for native builds, run from supramark root:
  ```
  git submodule add https://gitlab.com/graphviz/graphviz.git crates/graphviz-anywhere/graphviz
  ```
  (defer until the first native build is actually wired up — not needed
  for cargo-deny activation in step 2.)

## Workspace integration notes
- The original repo's `Cargo.toml` (at `crates/graphviz-anywhere/Cargo.toml`) declares an inner virtual `[workspace]`. That declaration is **shadowed** when running cargo from the supramark root: cargo resolves the supramark root workspace first because root `Cargo.toml` explicitly lists the leaf crates.
- Cargo behaviour split:
  - From supramark root → 3-crate workspace (dagre + graphviz-anywhere + example), single root `Cargo.lock`.
  - From inside `crates/graphviz-anywhere/` → 2-crate inner workspace (legacy upstream behaviour). Useful only when syncing upstream / standalone debugging — produces a separate `Cargo.lock` that should not be committed.
- Decision: leave the inner Cargo.toml as-is to keep upstream sync clean. Future `cargo` workflows in supramark should always run from repo root.

## Sync cadence
- **Upstream activity:** Active, kookyleo's own project. Releases tagged.
- **Sync strategy:** subtree pull on each tagged release. Special care for the `graphviz` submodule pointer when upstream updates Graphviz version.
  ```
  git fetch subtree-graphviz
  git subtree pull --prefix=crates/graphviz-anywhere subtree-graphviz main
  ```
- **No CLA** — kookyleo owns it.

## Outstanding
- Wire the `graphviz` git submodule to supramark root `.gitmodules` before any native build runs (step 4+ when mermaid-little needs it for layout).
- Decide whether to publish `crates/graphviz-anywhere/packages/web/dist/viz.js` from this monorepo or keep building inside the upstream repo.
- The committed `wasm-build.log` at the sub-tree root is build-time noise; safe to delete or `.gitignore` in a later cleanup.

## Patches we'd like to land upstream
Small drift items the supramark license-check skips today via
`SKIP_PATTERNS` in `scripts/license-check.ts`. When upstream accepts
these, the corresponding skip entry can be removed here.

| Path | Issue | Suggested upstream patch |
|---|---|---|
| `examples/react-native/package.json` | missing `"license"` field | add `"license": "Apache-2.0"` (matches repo `LICENSE`) |
