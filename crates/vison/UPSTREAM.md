# Upstream Tracking — vison

## Source repo
- **Upstream:** https://github.com/Actrium/vison
- **Pinned commit (at merge time):** `7fb2a910e22e6c4a3eb5cb34158b8d0c77e2dbf9`
- **Pinned tag:** (HEAD of `main`; no tag at merge time)
- **Merged into supramark on:** 2026-05-09 (step 5 of the super-monorepo plan)

## License relationship
- **vison upstream license:** MIT (declared in README; no `LICENSE` file
  shipped at merge time — supramark adds one in this sub-tree as a
  patch, see below).
- MIT ⇆ Apache-2.0 (supramark default) is fully compatible. supramark
  consumes vison under MIT.

## Relationship
- [ ] reimplementation
- [ ] fork
- [x] integration (vison is the upstream's primary form; supramark
      embeds it as a card-family extension to Markdown)

## Sub-tree contents
| Path | Purpose | License |
|---|---|---|
| `crates/vison/SPEC.md` / `ARCHITECTURE.md` / `README.md` | Spec docs | MIT |
| `crates/vison/example.vison.json` / `playground.html` | Reference example + standalone HTML demo | MIT |
| `crates/vison/verify.sh` | Helper script | MIT |
| `crates/vison/vison-core/` | Rust crate `vison-core` — validator + parser + CLI | MIT |
| `crates/vison/vison-renderers/web/VisonWebRenderer.tsx` | React Web renderer | MIT |
| `crates/vison/vison-renderers/rn/VisonRNRenderer.tsx` | React Native renderer | MIT |
| `crates/vison/vison-renderers/shared/types.ts` | TypeScript types shared between platforms | MIT |

## Local patches (a.k.a. things to upstream)
The merged tree carries a small number of supramark-side additions to
make vison consumable as workspace packages. All are MIT-clean; we
intend to upstream them.

| Path | Status | Notes |
|---|---|---|
| `LICENSE` | added | upstream README declares MIT but ships no LICENSE file. Added the canonical SPDX MIT text. |
| `vison-core/Cargo.toml` | edited | added `license = "MIT"`, `description`, `authors`, `repository` so cargo metadata is complete (cargo-deny would otherwise reject) |
| `vison-renderers/web/package.json` | added | makes the renderer a proper `@actrium/vison-web` npm workspace package |
| `vison-renderers/rn/package.json` | added | same as above for `@actrium/vison-rn` |
| `vison-renderers/{web,rn}/index.ts` | added | barrel exports so consumers `import { VisonWebRenderer }` instead of pointing at the `.tsx` directly |
| `vison-renderers/{web,rn}/tsconfig.json` | added | per-package tsconfig so `tsc -p` works in workspace context |

The `vison-core/target/` directory committed upstream was removed during
merge (build cache; gitignored at supramark root). The lock file
`vison-core/Cargo.lock` was likewise removed in favour of the unified
workspace `Cargo.lock`.

## Workspace integration notes
- `vison-core` joins the supramark Rust workspace at the top of root
  `/Cargo.toml#workspace.members`. No nested-workspace conflict
  (vison-core has only `[package]`).
- `vison-renderers/{web,rn}` are wrapped as bun workspace packages via
  patches above; `pnpm-workspace.yaml` already includes
  `crates/*/packages/{web,react-native}` from step 4, but vison's
  shape is `crates/vison/vison-renderers/{web,rn}/`, so we add
  explicit globs for those paths.
- The upstream-side `vison-renderers/shared/types.ts` is consumed by
  both renderer packages via relative imports (`../shared/types`);
  preserved verbatim.
- Supramark's host feature `@supramark/feature-card-vison` (at
  `packages/features/cards/vison/`) consumes the wrapped npm packages
  and wires them into the AST node → renderer pipeline.

## Sync cadence
- **Upstream activity:** Stable. vison v1 spec is locked; renderer
  components are reference implementations.
- **Sync strategy:** subtree pull when upstream changes. Expect to
  re-resolve the patches above; submit them upstream first to remove
  the drift.
  ```
  git fetch subtree-vison
  git subtree pull --prefix=crates/vison subtree-vison main
  ```
- **No CLA** — kookyleo owns it.

## Outstanding
- Land the patches (LICENSE + Cargo.toml metadata + per-renderer
  package.json + tsconfig) upstream.
- Decide whether `@supramark/feature-card-vison` should also expose a
  RN-side bridge for vison-renderers/rn (currently only Web rendered
  through `@supramark/web`; RN side needs follow-up wiring once
  `@supramark/rn` gains a Card renderer slot).
