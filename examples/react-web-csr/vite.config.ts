import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import { dirname, resolve } from 'path';
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'fs';

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

const graphvizWasmSibling = {
  name: 'graphviz-wasm-sibling',
  apply: 'build' as const,
  async writeBundle(options: { dir?: string }) {
    if (!options.dir) return;
    const target = resolve(options.dir, 'assets/viz.wasm');
    mkdirSync(dirname(target), { recursive: true });

    const source = findGraphvizWasmSource(options.dir);
    if (source) {
      if (source !== target) copyFileSync(source, target);
      return;
    }

    await downloadGraphvizWasm(target);
  },
};

function findGraphvizWasmSource(outDir: string) {
  const workspaceDist = resolve(
    __dirname,
    '../../crates/graphviz-anywhere/packages/web/dist/viz.wasm'
  );
  if (existsSync(workspaceDist)) return workspaceDist;

  const assetsDir = resolve(outDir, 'assets');
  if (!existsSync(assetsDir)) return undefined;

  const emitted = readdirSync(assetsDir).find(name => /^viz-[\w-]+\.wasm$/.test(name));
  return emitted ? resolve(assetsDir, emitted) : undefined;
}

async function downloadGraphvizWasm(target: string) {
  const response = await fetch(graphvizWasmCdnUrl());
  if (!response.ok) {
    throw new Error(
      `Unable to download Graphviz wasm asset: ${response.status} ${response.statusText}`
    );
  }
  writeFileSync(target, new Uint8Array(await response.arrayBuffer()));
}

function graphvizWasmCdnUrl() {
  const packageJsonPath = resolve(
    __dirname,
    '../../crates/graphviz-anywhere/packages/web/package.json'
  );
  const { version } = JSON.parse(readFileSync(packageJsonPath, 'utf8')) as { version?: string };
  // Build-time fallback that fetches a real published tarball from unpkg when
  // the workspace wasm hasn't been built locally (e.g. CI). This stays on the
  // @kookyleo scope on purpose: the monorepo moved to @actrium, but
  // @actrium/graphviz-anywhere-web@0.2.1 is not published yet, so an @actrium
  // URL 404s. Same deliberate exception as
  // crates/plantuml-little/tests/support/package.json. Switch to @actrium only
  // once an identical build is published there.
  return `https://unpkg.com/@kookyleo/graphviz-anywhere-web@${version ?? '0.2.1'}/dist/viz.wasm`;
}

export default defineConfig({
  // `vite-plugin-wasm` + `vite-plugin-top-level-await` let us consume
  // plantuml-little-web's default wasm-bindgen shape (`import * as wasm from
  // "./plantuml_little_web_bg.wasm"`) without a custom loader.
  plugins: [jsToTsResolver, react(), wasm(), topLevelAwait(), graphvizWasmSibling],
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
      '@actrium/graphviz-anywhere-web',
      // plantuml-little-web ships a sibling .wasm blob resolved via
      // `import * as wasm from "./plantuml_little_web_bg.wasm"`. Prebundling
      // breaks that relative import.
      '@actrium/plantuml-little-web',
      // d2-little-web is the same story — wasm-bindgen sibling .wasm blob
      // resolved as a relative module import. Prebundling would strip it.
      '@actrium/d2-little-web',
    ],
  },
  build: {
    chunkSizeWarningLimit: 3500,
  },
});
