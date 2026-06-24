# `@actrium/mermaid-little-web`

wasm-bindgen wrapper around the
[`mermaid-little`](https://github.com/Actrium/mermaid-little) Rust crate.
Run [Mermaid](https://mermaid.js.org/) diagrams to SVG in the browser,
Web Workers, or any wasm-capable runtime — no DOM, no headless browser,
no JS-side Mermaid bundle.

## Install

```bash
npm install @actrium/mermaid-little-web
# or: bun add / pnpm add / yarn add
```

## Usage

```ts
import { convert } from '@actrium/mermaid-little-web';

const svg = convert(`
graph TD
  A[Start] --> B{Decision}
  B -->|Yes| C[OK]
  B -->|No|  D[Stop]
`);

document.getElementById('out')!.innerHTML = svg;
```

`convert(source: string)` returns an SVG string or throws on parse /
render errors. `convertWithId(source, id)` mirrors upstream Mermaid's
`mermaid.render(id, source)` signature when stable element ids matter.

## How it differs from upstream `mermaid`

- Pure Rust → wasm — no DOM, no headless browser, no JS-side mermaid bundle.
- Self-contained dagre layout (no Graphviz bridge needed).
- Targets byte-exact SVG parity with upstream `mermaid@11.14.0`.

## Provenance

This package is part of the
[supramark](https://github.com/Actrium/supramark) super-monorepo (path:
`crates/mermaid-little/packages/web/`). It originated **inside the
super-monorepo as a downstream patch** to the
[`mermaid-little`](https://github.com/Actrium/mermaid-little) sub-tree;
see `crates/mermaid-little/UPSTREAM.md` for the contribution path back
to the standalone repo.
