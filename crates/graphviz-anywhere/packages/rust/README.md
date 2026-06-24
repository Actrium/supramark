# graphviz-anywhere (Rust crate)

Rust crate published as `graphviz-anywhere` on crates.io.

## Status

This crate currently builds against the legacy C-ABI wrapper
(`capi/graphviz_api.{h,c}`). A follow-up PR will switch the FFI surface
to the `CGraphviz` C++ class via the `cxx` crate so all platforms share
one Graphviz implementation.

## Usage (native)

```rust
use graphviz_anywhere::{Engine, Format, GraphvizContext};

let ctx = GraphvizContext::new()?;
let svg = ctx.render_to_string(
    r#"digraph G { a -> b; }"#,
    Engine::Dot,
    Format::Svg,
)?;
println!("{svg}");
```

Ship native binaries via one of:

- Environment: `GRAPHVIZ_ANYWHERE_DIR=/path/with/lib+include`
- Prebuilt: drop `libgraphviz_api.a` under `packages/rust/prebuilt/<os>/`
- Repo build: `./scripts/build-<os>.sh` populates `output/<os>*/lib/`

## Usage (wasm32)

When compiled for `wasm32-unknown-unknown` the crate **does not link the
native C library**. Instead, `GraphvizContext::render` delegates to a
JavaScript function the host environment must install on `globalThis`:

```ts
// Contract (TypeScript):
declare global {
  function __graphviz_anywhere_render(
    dot: string,
    engine: string,   // "dot" | "neato" | "fdp" | "sfdp" | "circo" |
                      // "twopi" | "osage" | "patchwork"
    format: string,   // "svg" | "png" | "pdf" | "ps" | "json" |
                      // "dot" | "xdot" | "plain"
  ): string;
}
```

The function must return the rendered output as a string (for `svg`, the
SVG source; for binary formats the consumer is expected to interpret the
returned string as raw bytes). On failure, **throw** a JavaScript `Error`;
the thrown error is reported to Rust as
[`GraphvizError::RenderFailed`](crate::GraphvizError::RenderFailed).

### Example wire-up with `@actrium/graphviz-anywhere-web`

```js
import { Graphviz } from "@actrium/graphviz-anywhere-web";

const graphviz = await Graphviz.load();
globalThis.__graphviz_anywhere_render = (dot, engine, format) =>
  graphviz.layout(dot, format, engine);

// Now any wasm-bindgen consumer that depends on `graphviz-anywhere`
// (e.g. `plantuml-little-web`) can call .render(...) normally.
```

### Context lifetime on wasm32

`GraphvizContext::new()` on wasm32 is a zero-cost no-op that returns a
marker value; the real Graphviz context is owned by the JavaScript side.
Dropping a `GraphvizContext` is likewise a no-op on wasm32.
