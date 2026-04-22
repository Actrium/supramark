# tests/support — reference SVG generator

Private helper package. Mirrors `plantuml-little/tests/support/`.

## Why

The mermaid-little reference tests compare its Rust-produced SVG against
an upstream-generated reference per fixture. To keep that comparison
byte-exact across machines, we pin:

- **`mermaid@11.14.0`** (see `package.json` — do not bump without also
  updating `tests/reference/VERSION`).
- **`jsdom@25.0.1`** as the DOM host — lighter than chromium, enough for
  mermaid's rendering path.
- A placeholder text-metric shim today (8px × char width, 14px line
  height). Phase 2 replaces this with a DejaVu Sans baked table so the
  Rust side and this generator agree byte-for-byte.

## Setup

```bash
cd tests/support
npm ci         # honours package-lock.json
```

Requires Node 20+ (the transitive `chevrotain@12` ideally wants Node
22+; it emits a warning on 20 but works).

## Usage

Single fixture:

```bash
node generate_ref.mjs ../fixtures/pie/01.mmd -o ../reference/fixtures/pie/01.svg
node generate_ref.mjs ../fixtures/pie/01.mmd              # -> stdout
```

Batch, mirrors `fixtures/` and `ext_fixtures/` into `reference/`:

```bash
node generate_ref.mjs --batch
```

The output tree:

```
tests/reference/
├── fixtures/<type>/<case>.svg
└── ext_fixtures/<subsource>/<type>/<case>.svg
```

## Determinism

Confirmed: same `.mmd` + same fixture path → identical SVG bytes across
runs on Node 20.19.4 + jsdom 25.0.1 + mermaid 11.14.0.

Remaining divergence sources (still to harden in later phases):

- **Text metrics**: placeholder geometry for now. Any environment that
  resolves fonts differently would diverge if we switched to a real
  font probe — hence the baked-table approach in Phase 2.
- **Node minor version drift**: chevrotain parser behaviour could
  theoretically differ; pin exact Node in CI once Phase 1 CI lands.
