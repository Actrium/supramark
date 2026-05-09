# Cross-validation setup

`tests/cross_validate.rs` runs dagre-rs against a baseline JSON
(`cross-validate/reference_data.json`) that was generated from a specific
upstream `dagre.js` commit. The `_meta` block at the top of that JSON is
authoritative — keep it in sync with whatever you actually ran.

## What is checked in

| Path | Tracked |
|------|---------|
| `cross-validate/generate_reference.mjs` | yes |
| `cross-validate/reference_data.json` | yes (the frozen baseline used by `cargo test`) |
| `ref/dagre-js/` | **no** (in `.gitignore`) — you must clone it locally |

`cargo test` does **not** rebuild the baseline; it deserializes the committed
JSON. You only need a working `ref/dagre-js` if you want to *regenerate* the
baseline.

## Reproducing the baseline

The current baseline (see `_meta` in `reference_data.json`) was produced from:

- upstream: `@dagrejs/dagre`
- version: `3.0.1-pre`
- commit: `4713b59bfa05af56cf58aa01e2027adf5d2dcf88`

To regenerate against the same commit:

```bash
git clone https://github.com/dagrejs/dagre.git ref/dagre-js
cd ref/dagre-js
git checkout 4713b59bfa05af56cf58aa01e2027adf5d2dcf88
npm ci
npm run build              # produces dist/dagre.esm.js
cd ../..
node cross-validate/generate_reference.mjs
```

The script reads `ref/dagre-js/package.json` and runs `git rev-parse HEAD`
inside `ref/dagre-js`, then writes the resulting version + commit into
`_meta` automatically — so the baseline always self-documents which upstream
it came from.

## Bumping to a new upstream

1. `cd ref/dagre-js && git fetch && git checkout <new-commit> && npm ci && npm run build`
2. From the repo root: `node cross-validate/generate_reference.mjs`
3. Update three places to match the new commit:
   - `Cargo.toml` → `[package.metadata.upstream]`
   - `README.md` → "Compatibility" section
   - This file → "Reproducing the baseline" block above
4. `cargo test` — investigate any new divergences before committing.

## Why `ref/` is not a submodule

`ref/dagre-js` is treated as a developer convenience, not a build dependency:
the only consumer is `generate_reference.mjs`, the JSON output is checked in,
and end users of the crate never need it. A submodule would force every
`cargo` user to fetch a JS repo for no reason. The `_meta` block + this
SETUP.md keep the version pin explicit without that cost.
