# Example Projects

Supramark's examples fall into two categories: a Feature example gallery you can browse directly on the docs site, and full host projects that need to run locally.

## In-Site Examples

### [Live Playground](/playground/)

The homepage hosts this same interactive playground: edit Markdown on the left, see the actual rendered output on the right, and switch between Features and examples. Stable deep links such as [Mermaid](/playground/mermaid/) and [D2](/playground/d2/) open a Feature directly.

To open it locally for debugging, run:

```bash
bun run feature:preview:web
bun run feature:preview:web mermaid
bun run feature:preview:web d2
bun run feature:preview:web plantuml
bun run feature:preview:web diagram-dot
bun run feature:preview:web diagram-echarts
bun run feature:preview:web diagram-vega-lite
```

### [Feature Example Gallery](./gallery)

Automatically aggregated from each Feature package's `src/examples.ts`, showing Markdown input that exercises the currently built-in syntax, container, and diagram capabilities.

## Runnable Projects

### [React Web CSR Example](./react-web-csr)

Browser-based live Markdown editor example built with Vite + React.

### [React Native Example](./react-native)

Markdown and diagram rendering example for the Expo / React Native environment.

### [Build Configuration Examples](./config-examples)

Configuration reference for integrating Supramark into build tools such as Vite / Webpack.

## Running the Examples

All example projects can be cloned and run directly:

```bash
git clone https://github.com/Actrium/supramark.git
cd supramark
bun install
cd examples/react-web-csr
bun run dev
```

## Related Resources

- [Getting Started](/guide/getting-started.zh)
- [API Reference](/api/)
- [Features](/features/)
