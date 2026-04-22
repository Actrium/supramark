#!/usr/bin/env node
/**
 * generate_ref.mjs — Deterministic reference SVG generator for mermaid-little.
 *
 * Two modes:
 *   1. single-file:  node generate_ref.mjs <input.mmd> [-o <out.svg>]
 *        Writes to stdout if -o is omitted. Exit 0 on success.
 *   2. batch:        node generate_ref.mjs --batch
 *        Walks ../fixtures and ../ext_fixtures under the project's tests/
 *        dir, renders every *.mmd, mirrors the relative path into
 *        ../reference/ with a .svg suffix. Prints per-file status.
 *
 * Determinism notes:
 *   - We feed a stable id (derived from the fixture path in batch mode,
 *     or the basename in single-file mode) to mermaid.render, so element
 *     id's do not drift across runs.
 *   - Text measurement is currently an 8px-per-char / 14px-per-line
 *     placeholder. Phase 2 will replace this with a DejaVu-Sans-baked
 *     shim matching plantuml-little's font_data.rs. Until then the
 *     reference SVGs are locked to the current placeholder geometry —
 *     regenerate after the shim lands.
 */

import { JSDOM } from 'jsdom';
import { readFileSync, writeFileSync, existsSync, mkdirSync, readdirSync, statSync } from 'node:fs';
import { dirname, basename, extname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// ---------- DOM globals ----------
const dom = new JSDOM(
  `<!DOCTYPE html><html><body><div id="container"></div></body></html>`,
  { pretendToBeVisual: true },
);
const W = dom.window;
for (const k of [
  'window',
  'document',
  'navigator',
  'HTMLElement',
  'SVGElement',
  'Element',
  'Node',
  'DOMParser',
  'XMLSerializer',
  'getComputedStyle',
]) {
  globalThis[k] = W[k];
}

// Placeholder text-metric shim. Will be replaced in Phase 2 with a
// DejaVu Sans baked table that matches the Rust side byte-for-byte.
W.SVGElement.prototype.getBBox = function () {
  const text = this.textContent ?? '';
  const lines = text.split('\n');
  const longest = lines.reduce((m, l) => Math.max(m, l.length), 0);
  return { x: 0, y: 0, width: longest * 8, height: lines.length * 14 };
};
W.SVGElement.prototype.getComputedTextLength = function () {
  return (this.textContent ?? '').length * 8;
};

const mermaid = (await import('mermaid')).default;
mermaid.initialize({ startOnLoad: false, securityLevel: 'loose' });

// ---------- render one source ----------
async function renderOne(source, id) {
  const { svg } = await mermaid.render(id, source);
  return svg;
}

// ---------- mode dispatch ----------
const argv = process.argv.slice(2);
if (argv.includes('-h') || argv.includes('--help')) {
  console.log(
    'usage:\n  generate_ref.mjs <input.mmd> [-o <out.svg>]\n  generate_ref.mjs --batch',
  );
  process.exit(0);
}

const HERE = dirname(fileURLToPath(import.meta.url));
const TESTS_DIR = resolve(HERE, '..');

function idForPath(mmdPath) {
  // Stable id derived from relative path under tests/. No chars that
  // would confuse mermaid's id-as-css-selector usage.
  const rel = relative(TESTS_DIR, mmdPath).replace(extname(mmdPath), '');
  return 'ref-' + rel.replace(/[^a-zA-Z0-9]+/g, '-');
}

if (argv[0] === '--batch') {
  const sources = [join(TESTS_DIR, 'fixtures'), join(TESTS_DIR, 'ext_fixtures')];
  const refRoot = join(TESTS_DIR, 'reference');

  let total = 0,
    ok = 0,
    fail = 0;
  const failures = [];

  for (const root of sources) {
    if (!existsSync(root)) continue;
    for (const mmdPath of walk(root)) {
      if (!mmdPath.endsWith('.mmd')) continue;
      total++;
      const relPath = relative(root, mmdPath);
      const outPath = join(refRoot, relative(TESTS_DIR, root).replace(/^.*?\//, ''), relPath)
        .replace(/\.mmd$/, '.svg');
      // refRoot/<sources_dirname>/<relPath>.svg
      const outFixed = join(
        refRoot,
        basename(root),
        relPath,
      ).replace(/\.mmd$/, '.svg');
      mkdirSync(dirname(outFixed), { recursive: true });
      try {
        const src = readFileSync(mmdPath, 'utf8');
        const svg = await renderOne(src, idForPath(mmdPath));
        writeFileSync(outFixed, svg);
        ok++;
        console.log(`OK   ${relative(TESTS_DIR, mmdPath)} -> ${relative(TESTS_DIR, outFixed)}`);
      } catch (err) {
        fail++;
        failures.push({ mmdPath, err: String(err.message ?? err) });
        console.error(`FAIL ${relative(TESTS_DIR, mmdPath)} — ${err.message}`);
      }
    }
  }
  console.log(`summary: ${ok}/${total} ok, ${fail} fail`);
  if (fail > 0) process.exit(1);
} else {
  const inputPath = argv[0];
  if (!inputPath) {
    console.error('error: missing input path. Use --help.');
    process.exit(2);
  }
  const outIdx = argv.indexOf('-o');
  const outPath = outIdx !== -1 ? argv[outIdx + 1] : null;
  const src = readFileSync(inputPath, 'utf8');
  const id = 'ref-' + basename(inputPath, extname(inputPath)).replace(/[^a-zA-Z0-9]+/g, '-');
  const svg = await renderOne(src, id);
  if (outPath) {
    mkdirSync(dirname(outPath), { recursive: true });
    writeFileSync(outPath, svg);
  } else {
    process.stdout.write(svg);
  }
}

function* walk(dir) {
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    const s = statSync(p);
    if (s.isDirectory()) yield* walk(p);
    else yield p;
  }
}
