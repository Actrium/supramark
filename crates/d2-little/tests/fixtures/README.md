# Cross-validation fixtures

This directory holds reference data produced by running upstream
implementations, used to validate byte-level parity of the ports in
this repository.

- `dagre_cross_validate/reference_data.json` — layout output produced
  by [dagre.js](https://github.com/dagrejs/dagre) (MIT,
  Copyright (c) 2012-2014 Chris Pettitt) via the
  `generate_reference.mjs` driver in this directory. The driver script
  itself is original and distributed under MPL-2.0 alongside the rest
  of the crate.
- `font_byte_match/*.hex` — TTF/WOFF byte dumps produced by the
  upstream Go [`terrastruct/d2`](https://github.com/terrastruct/d2)
  font pipeline (MPL-2.0, Copyright 2022 Terrastruct Inc.).

These outputs are distributed here under the same licenses as the
tools that produced them (MIT and MPL-2.0 respectively), consistent
with MPL-2.0 §3.3 (Larger Work). The full license texts are in the
root `LICENSE`, `src/dagre/LICENSE`, and `src/sketch/LICENSE`.
