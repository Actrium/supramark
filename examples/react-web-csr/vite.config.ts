import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import { resolve } from 'path';
import { existsSync } from 'fs';

// TS NodeNext source style: `import from './foo.js'` actually points to `foo.ts`.
// Vite's default resolver doesn't fall back from .js → .ts, so we do it here.
const jsToTsResolver = {
  name: 'js-to-ts-fallback',
  enforce: 'pre' as const,
  resolveId(source: string, importer?: string) {
    if (!importer || !source.endsWith('.js') || !source.startsWith('.')) return null;
    const abs = resolve(importer, '..', source);
    if (existsSync(abs)) return null;
    for (const ext of ['.ts', '.tsx']) {
      const candidate = abs.replace(/\.js$/, ext);
      if (existsSync(candidate)) return candidate;
    }
    return null;
  },
};

export default defineConfig({
  // `vite-plugin-wasm` + `vite-plugin-top-level-await` let us consume
  // plantuml-little-web's default wasm-bindgen shape (`import * as wasm from
  // "./plantuml_little_web_bg.wasm"`) without a custom loader.
  plugins: [jsToTsResolver, react(), wasm(), topLevelAwait()],
  worker: {
    format: 'es',
  },
  // `vega` references a bare `global` and pulls in node-only modules
  // (`stream`, `url`) that Vite externalizes. Polyfill `global` and
  // provide empty shims for the node modules so bundling succeeds.
  define: {
    global: 'globalThis',
  },
  resolve: {
    alias: {
      'react-native': resolve(__dirname, 'src/__mocks__/react-native.ts'),
      '@react-native': resolve(__dirname, 'src/__mocks__/react-native.ts'),
      'node:module': resolve(__dirname, 'src/__mocks__/node-module.ts'),
    },
    // `browser` must come before `module`/`main` so packages like node-fetch
    // (pulled in by vega-loader) resolve to their browser entry instead of
    // Node code paths that pull in `stream`/`url`.
    mainFields: ['browser', 'module', 'main', 'types'],
  },
  optimizeDeps: {
    // Workspace packages must NOT be prebundled — prebundling inlines a private
    // copy of @supramark/core, which desyncs `customContainerHooks` between
    // Supramark (prebundled) and the feature packages (loaded from source).
    // See: https://vitejs.dev/guide/dep-pre-bundling.html
    exclude: [
      'react-native',
      '@react-native',
      '@react-native/virtualized-lists',
      '@supramark/core',
      '@supramark/web',
      '@supramark/web/client',
      '@supramark/engines',
      '@supramark/engines/web',
      // Pre-bundling would strip viz.wasm away from viz.js's sibling
      // directory; emscripten's runtime resolves wasm relative to viz.js
      // via import.meta.url, so the file has to stay in node_modules.
      '@kookyleo/graphviz-anywhere-web',
      // plantuml-little-web ships a sibling .wasm blob resolved via
      // `import * as wasm from "./plantuml_little_web_bg.wasm"`. Prebundling
      // breaks that relative import.
      '@kookyleo/plantuml-little-web',
      // d2-little-web is the same story — wasm-bindgen sibling .wasm blob
      // resolved as a relative module import. Prebundling would strip it.
      '@kookyleo/d2-little-web',
    ],
  },
  build: {
    chunkSizeWarningLimit: 3500,
  },
});
