# d2-little

Lightweight D2 → SVG converter: a pure-Rust port of the
[d2lang](https://d2lang.com) pipeline (parser → IR → graph → dagre
layout → SVG). Builds to native or `wasm32-unknown-unknown`, has no C
dependencies, and ships its own dagre layout engine so no JS bridge is
required.

## Status

`d2-little` targets byte-exact SVG parity with the upstream Go `d2`
reference for the "dagre + sketch" rendering configuration. The full
e2e corpus (several hundred cases from the Go test suite) is exercised
under `cargo test --workspace`.

## Rust

```rust
let svg_bytes: Vec<u8> = d2_little::d2_to_svg("a -> b")?;
```

The crate is a single-crate library: its public surface is the
top-level `d2_to_svg`, `parse`, `compile`, and `set_dimensions`
entry points, with sub-modules (`ast`, `ir`, `graph`, `compiler`,
`dagre_layout`, `svg_render`, etc.) exposed for consumers who want to
drive the pipeline phase-by-phase.

## Web / WebAssembly

The sibling crate `packages/web` is published to npm as
[`@kookyleo/d2-little-web`](https://www.npmjs.com/package/@kookyleo/d2-little-web).
It exposes the same `d2_to_svg` entry point as a wasm-bindgen
`convert(source: string) => string`:

```ts
import { convert } from '@kookyleo/d2-little-web';
const svg = convert('a -> b');
```

## Attribution

This project is a Rust port and is built on top of three upstream
works. Each is preserved with its original license; see the referenced
files for the full text.

- **[terrastruct/d2](https://github.com/terrastruct/d2)** — the Go
  reference implementation that this crate ports file-by-file
  (parser, IR, compiler, graph, dagre-layout glue, svg_render, and
  more). Copyright 2022 Terrastruct Inc.; licensed under
  [MPL-2.0](https://mozilla.org/MPL/2.0/). See the root `LICENSE`.
  The byte-level test fixtures under `tests/e2e_testdata/` and
  `tests/unit_testdata/` are likewise derived from the upstream Go
  test corpus and remain MPL-2.0; see the `LICENSE` files in those
  directories.
- **[dagre.js](https://github.com/dagrejs/dagre)** — a Rust port
  lives at `src/dagre/`. Copyright (c) 2012-2014 Chris Pettitt;
  MIT License. See `src/dagre/LICENSE` and `src/dagre/README.md`.
- **[rough.js](https://github.com/rough-stuff/rough)** — the
  sketch/hand-drawn rendering at `src/sketch/rough.rs` is a Rust port
  of the subset of rough.js used by d2's sketch renderer.
  Copyright (c) 2019 Preet Shihn; MIT License. See
  `src/sketch/LICENSE`.

## License

The crate as a whole is distributed under the
[Mozilla Public License, v. 2.0](https://mozilla.org/MPL/2.0/) (see
`LICENSE`), because the majority of its source files are direct ports
of MPL-2.0 Go code from `terrastruct/d2` and MPL-2.0 is a file-level
copyleft. Two vendored subtrees retain their upstream MIT licenses as
a Larger Work under MPL §3.3:

- `src/dagre/` — MIT (dagre.js, Chris Pettitt)
- `src/sketch/rough.rs` — MIT (rough.js, Preet Shihn)
