# Supramark

[![CI](https://github.com/Actrium/supramark/actions/workflows/ci.yml/badge.svg)](https://github.com/Actrium/supramark/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/Actrium/supramark/branch/main/graph/badge.svg)](https://codecov.io/gh/Actrium/supramark)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

An integration library for Markdown extensions and diagram rendering, aimed at React Native and mini-program hosts.

**[Try the unified Markdown and diagram playground](https://actrium.github.io/supramark/playground/)** — edit Markdown and switch between GFM, math, Mermaid, D2, PlantUML, DOT, ECharts, Vega-Lite, and container features directly in the browser, without cloning or installing the repository.

A Chinese version of this document is available at [README.zh.md](README.zh.md).

What it sets out to do:

- gather the common Markdown extensions (GFM, math, Mermaid and friends) behind one parse-and-render contract;
- ship those capabilities into a host app as built-in plugins, so a mini-program or a conversation only declares the features it wants;
- route every diagram through `@supramark/engines`, which returns an SVG string for the Web or RN renderer to display.

## Layout

- `packages/core` (`@supramark/core`)
  - defines Supramark's AST v2, the feature interface, and the parse facade;
  - produces AST v2 with source maps via the Rust `supramark-markdown` canonical parser;
  - the public entry point is just `parse(source) -> AST v2`; the Web, Node and RN render layers all consume that one AST contract.
- `packages/renderers/rn` (`@supramark/rn`)
  - the React Native render layer: maps a Supramark AST onto an RN component tree, with default rendering for basic Markdown, math, footnotes, definition lists, admonitions, emoji and the diagram types.
- `packages/renderers/web` (`@supramark/web`)
  - the React Web render layer, with separate server and client entry points.
- `packages/engines` (`@supramark/engines`)
  - the single exit for diagram and formula rendering. Web and RN both consume `render({ engine, code }) -> SVG | error`.

Other directories:

- `packages/features/*` — capability packages built on the feature interface, grouped by syntax family: `main/` (GFM, math, footnote, definition list, emoji, code highlighting), `containers/` (admonition, html-page, map, weather), `diagrams/` (Mermaid, PlantUML, D2, DOT, ECharts, Vega-Lite), plus `core-markdown/` and `cards/`;
- `crates/` — the Rust side: the `supramark-markdown` parser, and the `*-little` diagram engines (`mermaid-little`, `plantuml-little`, `d2-little`) with their supporting crates;
- `examples/react-native` — a React Native sample app, laid out as a catalogue plus per-example detail pages;
- `examples/react-web-csr` — a React Web sample showing `<Supramark />` with diagrams and math;
- `examples/config-examples` — Vite and webpack build-configuration samples.

## Documentation

The documentation site covers the user guide, the feature list and the API reference:

```bash
cd docs
npm install
npm run docs:dev     # http://localhost:5173/supramark/ (the port may shift)
npm run docs:build
```

The site is VitePress-driven, with search and navigation. Feature documentation is generated from each feature's `documentation.api` field rather than written by hand, and the API reference comes from TypeDoc — so after changing a feature, run `bun run features:sync` to keep the generated pages in step.

Design notes and per-plugin write-ups live under `docs/`. Most of them are currently written in Chinese and carry a `.zh.md` suffix.

## How diagrams are rendered

- `@supramark/engines` is the only exit for diagrams and formulas. Its interface is `render({ engine, code }) => Promise<{ format: 'svg' | 'error', payload }>`.
- Web and RN renderers consume SVG and nothing else.
- wasm, native FFI and JS SVG-string engines are all adapter details internal to `@supramark/engines`.
- A renderer only displays the SVG it is handed — RN through `react-native-svg` — and never holds a reference to a diagram library itself.

The repository already contains the core parse and render pipeline along with the RN and Web sample apps. It is still moving, but it is far enough along to trial in a real project.

## Feature quality assurance

Feature quality rests on three layers:

**TypeScript.** Strict `FeatureMetadata`, `ASTNodeDefinition` and `NodeInterface` types, checked at compile time with no runtime cost, and documented field by field.

**Runtime validation.** `validateFeature()` applies 14 or more rules across critical, warning and info levels, in basic, strict or production mode, and reports structured errors (`code` + `message` + `severity`).

**Static analysis and CI.** The feature linter checks code quality and file structure and assigns a score out of 100. GitHub Actions runs type checking, the linter, tests and coverage on every pull request.

```bash
bun run features:lint          # every feature
bun run feature:lint <name>    # one feature, strict mode
bun run lint                   # eslint + feature linter + English-only check
```

```ts
import { validateFeature } from '@supramark/core';
const result = validateFeature(myFeature, { production: true });
```

Further reading: [Feature quality assurance](./docs/guide/FEATURE_QUALITY_ASSURANCE.zh.md) (Chinese).

## Source language

Source files are English-only, enforced in CI by `scripts/check-cjk.mjs`. Chinese belongs in `*.zh.md` documents and in localisation and fixture directories. Where CJK is genuinely load-bearing — a character-width table, a multi-byte parser test input — an inline `cjk-allow: <reason>` pragma exempts it, and the reason is required.

```bash
bun run lint:cjk           # gate
bun run lint:cjk:report    # inventory, never fails
```

## Roadmap

Short term (0.1.x):

- [x] wire the Rust `supramark-markdown` AST v2 engine into `@supramark/core`, including the parse facade and plugin post-processing;
- [x] model diagram nodes (Mermaid, PlantUML, Vega and so on) in the AST, with parsing and placeholder rendering;
- [x] basic Markdown rendering in `@supramark/rn` — paragraphs, headings, lists, code blocks;
- [x] unify the diagram exit in `@supramark/engines` so Web and RN both receive SVG strings;
- [x] a runnable React Native demo app wired to current capabilities, using the catalogue-plus-detail layout;
- [x] usage docs for the React Native and React Web samples;
- [x] give both sample projects the same two-page interaction structure;
- [x] aggregate example data from the feature packages for all sample projects to share.

Medium term:

- [x] more diagram engines (Mermaid, PlantUML, D2, DOT, Graphviz, Vega-Lite, ECharts), each enabled by configuration;
- [x] LaTeX math, inline `$...$` and block `$$...$$`, displayed through the same SVG path on Web and RN;
- [x] footnotes (`[^1]`), modelled in the AST with back-references and default rendering on RN and Web;
- [x] definition lists, with their own AST nodes and worked examples;
- [x] admonition and callout container blocks with one syntax and default styling on RN and Web;
- [x] emoji shortcodes such as `:smile:` and `:rocket:`, emitted as Unicode directly into `text.value` by the AST v2 parser;
- [x] React Web samples demonstrating `<Supramark />` in a React app;
- [ ] a feature registry, permission model and configuration format spanning platform and mini-program;
- [ ] a set of recommended plugin presets — documentation-oriented, data-visualisation-oriented, and so on.

Long term:

- [ ] better rendering performance: diagram result caching, virtualised lists, lazy loading;
- [ ] full Web and Node support, so `@supramark/core` and the plugin system are reusable in a browser or under SSR;
- [ ] fuller documentation and best practices.

## License

Apache-2.0. See [LICENSE](LICENSE).
