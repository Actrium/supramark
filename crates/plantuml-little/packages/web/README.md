# @actrium/plantuml-little-web

Browser/Node wasm-bindgen wrapper around the
[`plantuml-little`](https://crates.io/crates/plantuml-little) Rust crate —
converts PlantUML source to SVG strings without a JVM, with byte-exact parity to
the upstream Java PlantUML for the supported diagram set.

The wasm itself delegates Graphviz layout to
[`@actrium/graphviz-anywhere-web`](https://www.npmjs.com/package/@actrium/graphviz-anywhere-web)
via a small JS bridge, so you install both:

```bash
npm install @actrium/plantuml-little-web @actrium/graphviz-anywhere-web
```

## Usage

```ts
import { Graphviz } from '@actrium/graphviz-anywhere-web';
import { setup, convert, version } from '@actrium/plantuml-little-web';

// Load the Graphviz wasm and wire it into the plantuml-little-web bridge.
const graphviz = await Graphviz.load();
setup({ graphviz });

console.log('plantuml-little-web', version());

const svg = convert(`@startuml
class A { x: int }
class B { y: string }
A --> B
@enduml`);

document.body.insertAdjacentHTML('beforeend', svg);
```

`convert(puml)` is synchronous once the Graphviz wasm is ready; it returns the
raw SVG string. Any error from the Rust converter surfaces as a thrown JS
`Error` whose message is the Rust `Display` text.

## API

- `convert(puml: string): string` — render a `.puml` source to an SVG string.
- `version(): string` — the crate version embedded into the wasm.
- `setup({ graphviz })` — install a Graphviz engine (anything exposing
  `.layout(dot, format, engine)`) as the backing renderer. Returns a disposer.
- `installGraphvizBridge((dot, engine, format) => svg)` — install a raw
  Graphviz bridge function directly. Useful if you're not using
  `@actrium/graphviz-anywhere-web`.
- `hasGraphvizBridge(): boolean` — check whether a bridge is installed.

## How it works

Internally, `plantuml-little-web` contains the `plantuml-little` Rust converter
compiled to `wasm32-unknown-unknown`. The Rust code depends on
`graphviz-anywhere`, whose wasm32 target delegates all layout calls to a global
JS function `globalThis.__graphviz_anywhere_render(dot, engine, format)`. The
`setup()` / `installGraphvizBridge()` helpers are what install that global.

This keeps the wasm small (no C++ Graphviz linked in) and lets you share the
Graphviz wasm runtime between multiple PlantUML-like libraries.

## Node.js usage

Native ESM wasm imports currently require the `--experimental-wasm-modules`
flag:

```bash
node --experimental-wasm-modules app.mjs
```

Bundlers (webpack 5, Vite, Rollup, esbuild, etc.) handle the wasm import
natively — the package ships with `"type": "module"` and points `exports["."]`
at the ESM entrypoint.

## License

Same as `plantuml-little`: `GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0
OR EPL-2.0 OR MIT`.
