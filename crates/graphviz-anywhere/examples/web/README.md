# graphviz-anywhere web example

This package intentionally keeps the Web example lightweight and focuses on the
three deployment profiles exposed by `@actrium/graphviz-anywhere-web`.

## 1. Lazy client-side rendering

Use this for docs sites, blogs, or product pages where DOT rendering happens
after the initial page load.

```ts
import { createLazyWasmRenderer } from '@actrium/graphviz-anywhere-web';

const renderer = createLazyWasmRenderer();
const svg = await renderer.render('digraph { web -> wasm }');
```

## 2. Worker-backed rendering

Use this for editors, whiteboards, or interactive knowledge tools where large
graphs must not block the main thread.

```ts
import { createWorkerWasmRenderer } from '@actrium/graphviz-anywhere-web';

const renderer = createWorkerWasmRenderer({ timeoutMs: 8000 });
const svg = await renderer.render('digraph { a -> b -> c }');
```

## 3. Server / edge warm instance

Use this for preview APIs, SSR routes, or edge workers that render many graphs
per process and want to amortize Wasm startup cost.

```ts
import { createServerWasmRenderer } from '@actrium/graphviz-anywhere-web';

const renderer = createServerWasmRenderer();
await renderer.preload();

const { svg, json } = await renderer.renderMany(
  'digraph { api -> cache -> client }',
  ['svg', 'json']
);
```
