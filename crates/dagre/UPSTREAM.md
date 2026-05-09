# Upstream Tracking — dagre

## Source repo
- **Upstream:** https://github.com/kookyleo/dagre-rs
- **Pinned commit (at merge time):** `ad891113d80f5d0305962a0e45e88715e15873f4`
- **Pinned tag:** `v0.1.1`
- **Merged into supramark on:** 2026-05-09 (step 2 of the super-monorepo plan)

## License relationship
- **dagre-rs license:** Apache-2.0 (see `LICENSE`)
- **dagre.js (port source):** MIT — see `Cargo.toml#package.metadata.upstream` for pinned commit
- Apache-2.0 is compatible with MIT in the upstream → port direction. dagre-rs is a re-implementation, not a fork.
- **No upstream source files were copied.** Cross-validation reference data
  in `cross-validate/reference_data.json` is generated from running dagre.js
  against fixtures; it's data, not source.

## Relationship
- [x] reimplementation (Rust port of dagre.js)
- [ ] fork
- [ ] bindings

## Sync cadence
- **Upstream activity:** Active, kookyleo's own port of dagre.js.
- **Sync strategy:** subtree pull when a new tagged release lands at upstream. No CLA — kookyleo owns both.
  ```
  git fetch subtree-dagre
  git subtree pull --prefix=crates/dagre subtree-dagre main
  ```

## Workspace integration notes
- The supramark root `Cargo.toml` lists `crates/dagre` as a workspace member directly. `crates/dagre/Cargo.toml` has only `[package]` (no `[workspace]`), so there is no nested-workspace conflict.
- The original repo's `Cargo.lock` (at `crates/dagre/Cargo.lock`) was removed during merge — supramark workspace uses a single root `Cargo.lock`. Restore it only when developing dagre in isolation against upstream.

## Outstanding
- **dagre.js upstream pinned at:** `4713b59` (`v3.0.1-pre`). When dagre.js cuts a stable v3 release, decide whether to bump cross-validation data.
- No known security advisories.
