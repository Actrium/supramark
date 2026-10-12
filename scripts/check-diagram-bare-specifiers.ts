#!/usr/bin/env node
/**
 * Build-artifact smoke test: catches diagram-engine dynamic import specifiers
 * that have degraded into a variable.
 *
 * Background: if the echarts/vega-lite loader turns `import('echarts/core')`
 * into `const spec = 'echarts/core'; import(spec)`, Vite/Rollup can no longer
 * statically analyze it, the build output retains the bare specifier string,
 * and the browser's native ESM loader throws
 * `TypeError: Failed to resolve module specifier "echarts/core"`
 * (upstream issue #80 / #79).
 *
 * Once the specifier is back to a string literal, Rollup splits the echarts
 * submodules into separate chunks (core-*.js / renderers-*.js / charts-*.js /
 * components-*.js), and the bare subpath string no longer shows up in the
 * build output.
 *
 * Fingerprint choice: `echarts/renderers` / `echarts/charts` /
 * `echarts/components` — these subpaths can only come from a leftover bare
 * specifier, they never show up as ordinary data
 * (`echarts/core` can legitimately appear once in a valid build, so it's
 * excluded). vega / vega-lite show up frequently as data / package names, so
 * a string fingerprint isn't reliable for them; that case is instead covered
 * at the specifier-shape level by the source contract test
 * (packages/engines/__tests__/dynamic-import-contract.test.ts).
 *
 * Usage: bun scripts/check-diagram-bare-specifiers.ts
 *   Skips (exit 0) when dist hasn't been built yet; exits 1 if a bare
 *   specifier remains in the build output.
 */
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const distAssets = resolve(import.meta.dir, '..', 'examples', 'react-web-csr', 'dist', 'assets');

// These echarts subpaths can only come from a bare specifier that escaped static analysis.
const FINGERPRINTS = ['echarts/renderers', 'echarts/charts', 'echarts/components'];

if (!existsSync(distAssets)) {
  console.warn('[check:diagram-specifiers] skip: examples/react-web-csr/dist has not been built.');
  console.warn('  This check only runs after the build output exists. Run: bun run docs:playground:build');
  process.exit(0);
}

const jsFiles = readdirSync(distAssets).filter(name => name.endsWith('.js'));

const leaks: Array<{ file: string; specifier: string; count: number }> = [];
for (const name of jsFiles) {
  const content = readFileSync(join(distAssets, name), 'utf8');
  for (const specifier of FINGERPRINTS) {
    const count = content.split(specifier).length - 1;
    if (count > 0) leaks.push({ file: name, specifier, count });
  }
}

if (leaks.length > 0) {
  console.error('[check:diagram-specifiers] FAIL: bare specifier leftovers detected in build output:');
  for (const leak of leaks) {
    console.error(`  ${leak.file}: "${leak.specifier}" x${leak.count}`);
  }
  console.error('');
  console.error('Some dynamic import specifier has degraded into a variable, so Vite/Rollup');
  console.error('could not statically analyze it; the build output kept the bare specifier,');
  console.error('and the browser will throw Failed to resolve module specifier.');
  console.error('Check the import(...) usage in packages/engines/src/js-chart-loaders.ts and web.ts:');
  console.error('  the specifier must be a string literal (an `as string` assertion is fine),');
  console.error('  never extract it into a variable.');
  console.error('See the source contract test: packages/engines/__tests__/dynamic-import-contract.test.ts.');
  process.exit(1);
}

console.log(
  `[check:diagram-specifiers] OK: scanned ${jsFiles.length} build file(s), no bare specifier leftovers.`
);
