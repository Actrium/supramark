# `@supramark/feature-card-vison`

Renders `:::vison` container blocks in Markdown as
[Vison](https://github.com/Actrium/vison) cards — a JSON visual
description spec for AI chat UIs.

## Install

```bash
bun add @supramark/feature-card-vison @actrium/vison-web
# RN side also needs @actrium/vison-rn
```

## Usage

````md
:::vison
{
  "version": "1",
  "type": "container",
  "style": { "padding": 12, "backgroundColor": "#F5F5F5", "borderRadius": 8 },
  "children": [
    { "type": "text",
      "props": { "text": "Hello Vison" },
      "style": { "fontSize": 16, "fontWeight": "bold" } }
  ]
}
:::
````

```tsx
import { visonFeature } from '@supramark/feature-card-vison';
import { renderVisonContainerWeb } from '@supramark/feature-card-vison/runtime.web';
import { Supramark } from '@supramark/web';

<Supramark
  markdown={md}
  config={{ features: [{ id: '@supramark/feature-card-vison', enabled: true }] }}
  containerRenderers={{ vison: renderVisonContainerWeb }}
/>
```

For React Native, swap the runtime import to
`@supramark/feature-card-vison/runtime.rn` and pair with `@supramark/rn`.

## How it differs from a fence

The fence form (` ```vison ... ``` `) was considered but containers
fit better:

- supramark already has a renderer-side `containerRenderers` hook,
  so wiring is one line of host config.
- Vison cards are not source code; the body happens to be JSON but
  the rendered surface is a UI tree, not text.
- Containers can be nested inside other containers (e.g. an admonition
  containing a Vison card) without escaping woes.

## License

Apache-2.0. The underlying Vison spec + renderers are MIT — see
`crates/vison/UPSTREAM.md` for provenance.
