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
import { readFileSync, writeFileSync, existsSync, mkdirSync, readdirSync, statSync, appendFileSync } from 'node:fs';
import { dirname, basename, extname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { textWidth, lineHeight, measureTextBlock } from './font_metrics.mjs';

// Install process-level error handlers BEFORE any other imports that
// could throw during initialisation (mermaid's module load path can
// synchronously die deep inside its DOM globals / cytoscape setup).
// The handlers need a side-channel to record WHICH worker died and on
// WHICH fixture — that state is wired up later inside runWorker().
let __workerCrashSink = null; // set by runWorker to a fs path
let __workerCrashTag = 'pre-init';
process.on('uncaughtException', (err) => {
  const line = `${__workerCrashTag}\t__CRASH__ ${err?.message ?? err}`;
  try {
    if (__workerCrashSink) appendFileSync(__workerCrashSink, line + '\n');
  } catch {}
  process.stderr.write(`[crash ${__workerCrashTag}] ${err?.stack ?? err?.message ?? err}\n`);
  process.exit(1);
});
process.on('unhandledRejection', (err) => {
  const line = `${__workerCrashTag}\t__REJECT__ ${err?.message ?? err}`;
  try {
    if (__workerCrashSink) appendFileSync(__workerCrashSink, line + '\n');
  } catch {}
  process.stderr.write(`[reject ${__workerCrashTag}] ${err?.stack ?? err?.message ?? err}\n`);
  process.exit(1);
});

// ---------- DOM globals ----------
const dom = new JSDOM(
  `<!DOCTYPE html><html><body><div id="container"></div></body></html>`,
  { pretendToBeVisual: true },
);
const W = dom.window;
// jsdom exposes these on W but not on globalThis; mermaid and its
// runtime deps reach for them as bare globals in various code paths.
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
  'screen',
  'location',
  'requestAnimationFrame',
  'cancelAnimationFrame',
  'MutationObserver',
  'Image',
  'CSS',
]) {
  // Node >=21 ships a built-in globalThis.navigator as a read-only,
  // non-configurable getter. ESM is strict mode, so the bare assignment
  // below throws TypeError there. Guard it so the generator runs on both
  // Node 20 (assignment succeeds, jsdom's navigator wins) and Node >=21
  // (assignment skipped, Node's native navigator remains). Output parity
  // across the two is verified at reference-regeneration time.
  if (W[k] !== undefined) {
    try { globalThis[k] = W[k]; } catch {}
  }
}
if (!globalThis.screen) globalThis.screen = { availWidth: 1024, availHeight: 768, width: 1024, height: 768 };

// Pin Date.now() only. Gantt fixtures with explicit dates (the vast
// majority) are unaffected; the handful that use `today`-relative
// arithmetic stay deterministic because Date.now() is frozen. We do
// NOT override the `Date` constructor — dayjs and d3-scale rely on
// the genuine prototype chain, and a Proxy-wrapped Date breaks how
// mermaid's gantt computes its time-axis domain (seen empirically:
// replacing Date collapses gantt viewBox width to 0).
const FROZEN_EPOCH_MS = Date.parse('2024-01-01T00:00:00Z');
Date.now = () => FROZEN_EPOCH_MS;
W.Date.now = () => FROZEN_EPOCH_MS;

// mermaid's sequence parser has a color-validation path that checks
// `window?.CSS.supports('color', str)`. If CSS.supports is missing or
// false, it falls back to `new Option().style` — `Option` is a browser
// global alias for HTMLOptionElement that jsdom does NOT expose, so the
// fallback crashes with `ReferenceError: Option is not defined`, taking
// out every sequence fixture that uses `box <color> <title>`.
// Easiest fix: install a permissive CSS.supports so the first branch
// is always taken. This preserves color parsing fidelity: mermaid just
// accepts whatever CSS color string it was given.
if (!globalThis.CSS) globalThis.CSS = {};
if (!globalThis.CSS.supports) globalThis.CSS.supports = () => true;
if (!W.CSS) W.CSS = globalThis.CSS;
else if (!W.CSS.supports) W.CSS.supports = globalThis.CSS.supports;
// Also expose Option as a constructor that returns a detached
// <option> element — belt and braces for any other mermaid code path
// that reaches for it.
if (!globalThis.Option && W.document) {
  globalThis.Option = function OptionShim(text = '', value = '') {
    const el = W.document.createElement('option');
    el.text = text;
    el.value = value;
    return el;
  };
}

// cytoscape (used by mindmap-cose-bilkent) queries container geometry
// via getComputedStyle(el).getPropertyValue('padding-*') and subtracts
// those from clientWidth/clientHeight. jsdom returns '' for unset
// padding, which parseFloat turns into NaN — cytoscape then produces
// bb = makeBoundingBox({x1:0,y1:0,w:NaN,h:NaN}) = undefined, and the
// next statement crashes with "Cannot read properties of undefined
// (reading 'h')" inside GridLayout.run (cytoscape.esm.mjs:22168).
//
// Shim getComputedStyle so any blank value is reported as '0px'.
// This also forces containerBB / padding terms to parse as 0, which
// is what we want: our pipeline doesn't need cytoscape pixel
// fidelity, only non-NaN numbers so cose-bilkent can run.
const __origGetComputedStyle = W.getComputedStyle.bind(W);
W.getComputedStyle = function patchedGetComputedStyle(el, pseudo) {
  const cs = __origGetComputedStyle(el, pseudo);
  if (!cs || cs.__paddingShimmed) return cs;
  const origGet = cs.getPropertyValue.bind(cs);
  cs.getPropertyValue = (name) => {
    const v = origGet(name);
    return v === '' || v == null ? '0px' : v;
  };
  try { Object.defineProperty(cs, '__paddingShimmed', { value: true }); } catch {}
  return cs;
};
globalThis.getComputedStyle = W.getComputedStyle;

// jsdom ships HTMLCanvasElement but throws on getContext unless the
// native `canvas` npm package is installed. cytoscape (used by
// architecture + mindmap-cose-bilkent + some flowchart code paths)
// initialises a canvas renderer eagerly — returning null makes it
// throw "Could not create canvas of type 2d". So return a silent
// no-op 2d context good enough to let cytoscape's renderer
// initialise. We don't actually draw via canvas; mermaid's SVG
// output comes from its own DOM tree, not canvas pixels.
// Prevent jsdom from trying to fetch remote images for label tags
// like <img src="https://..."> inside mermaid diagrams. Without this,
// mermaid.render() hangs forever on network IO; even with the
// RENDER_TIMEOUT_MS guard, it wastes real time per fixture.
if (W.HTMLImageElement) {
  Object.defineProperty(W.HTMLImageElement.prototype, 'src', {
    configurable: true,
    set(_v) {
      // Drop the URL on the floor; fire synthetic error so any
      // onload/onerror observer wakes up.
      setTimeout(() => {
        this.dispatchEvent && this.dispatchEvent(new W.Event('error'));
      }, 0);
    },
    get() {
      return '';
    },
  });
  // ! Critical for avoiding hangs on fixtures like
  // cypress/flowchart/166.mmd (`<img src="https://..."/>` inside
  // labels). mermaid's configureLabelImages() and addText2() wait on
  // `await Promise.all(images.map(img => new Promise(res => {
  //   setTimeout(() => { if (img.complete) setupImage(); });
  //   img.addEventListener('error', setupImage);
  //   img.addEventListener('load',  setupImage);
  // })))`. For <img> elements parsed from innerHTML, jsdom never
  // fires load/error (no real network fetch) AND `complete` stays
  // false — so the Promise never settles. Meanwhile, mermaid's own
  // `executionQueue` is serial: one stuck render wedges every
  // subsequent render in the same process. A single unresolved img
  // promise was the root cause of all 62 "render timeout" failures.
  //
  // Override `complete` to always return true so the setTimeout
  // branch runs immediately and resolves the promise.
  Object.defineProperty(W.HTMLImageElement.prototype, 'complete', {
    configurable: true,
    get() { return true; },
  });
  // mermaid's imageSquare (src/rendering-util/rendering-elements/
  // shapes/imageSquare.ts) calls `await img.decode()` after assigning
  // `.src`. jsdom does not implement HTMLImageElement.decode — stub
  // it to resolve immediately. Without this, any diagram using the
  // `image-shape` node type would throw synchronously.
  if (typeof W.HTMLImageElement.prototype.decode !== 'function') {
    W.HTMLImageElement.prototype.decode = function () { return Promise.resolve(); };
  }
}

// Block fetch globally in the jsdom env — another avenue mermaid can
// use to pull icon SVGs or similar. Return a resolved response that
// looks like a 200 to keep callers happy, but with empty body.
globalThis.fetch = async () => ({
  ok: true,
  status: 200,
  text: async () => '',
  json: async () => ({}),
  arrayBuffer: async () => new ArrayBuffer(0),
  blob: async () => new W.Blob([], { type: 'text/plain' }),
});

if (W.HTMLCanvasElement) {
  const noop = () => {};
  const measureText = () => ({ width: 0 });
  const makeCtx = () => ({
    canvas: { width: 0, height: 0 },
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 1,
    font: '',
    textAlign: 'start',
    textBaseline: 'alphabetic',
    globalAlpha: 1,
    save: noop,
    restore: noop,
    translate: noop,
    rotate: noop,
    scale: noop,
    setTransform: noop,
    beginPath: noop,
    closePath: noop,
    moveTo: noop,
    lineTo: noop,
    arc: noop,
    rect: noop,
    fill: noop,
    stroke: noop,
    clip: noop,
    fillText: noop,
    strokeText: noop,
    fillRect: noop,
    strokeRect: noop,
    clearRect: noop,
    measureText,
    createLinearGradient: () => ({ addColorStop: noop }),
    createRadialGradient: () => ({ addColorStop: noop }),
    drawImage: noop,
    getImageData: () => ({ data: new Uint8ClampedArray(0) }),
    putImageData: noop,
  });
  W.HTMLCanvasElement.prototype.getContext = function () {
    return makeCtx();
  };
}

// Text-metric shim backed by DejaVu Sans / DejaVu Sans Bold TTFs via
// opentype.js. Both sides of the pipeline (this generator + the Rust
// renderer) consume the same TTFs, so getBBox output here matches the
// Rust font_metrics module byte-for-byte — the anchor that makes the
// byte-exact reference tests work.
//
// Patched on SVGElement, HTMLElement, and Element (fallback) — block,
// mindmap, and treemap reach for getBBox on HTML labels inside
// <foreignObject>, not just SVG text.
function resolveFont(el) {
  // Walk up the tree looking for the closest font-family / font-size /
  // font-weight declaration. Attributes first, then inline style.
  let size = 14;
  let family = 'sans-serif';
  let bold = false;
  let sizeSet = false,
    familySet = false,
    weightSet = false;
  let node = el;
  while (node && node.nodeType === 1) {
    const g = (k) => (node.getAttribute ? node.getAttribute(k) : null);
    const style = node.style ?? {};
    if (!sizeSet) {
      const s = g('font-size') ?? style.fontSize ?? null;
      if (s) {
        const m = /([0-9.]+)/.exec(String(s));
        if (m) {
          size = parseFloat(m[1]);
          sizeSet = true;
        }
      }
    }
    if (!familySet) {
      const f = g('font-family') ?? style.fontFamily ?? null;
      if (f) {
        family = String(f);
        familySet = true;
      }
    }
    if (!weightSet) {
      const w = g('font-weight') ?? style.fontWeight ?? null;
      if (w) {
        const s = String(w).trim().toLowerCase();
        bold = s === 'bold' || s === 'bolder' || (parseInt(s, 10) >= 600);
        weightSet = true;
      }
    }
    if (sizeSet && familySet && weightSet) break;
    node = node.parentNode;
  }
  return { size, family, bold };
}

function attrNum(el, k, d = 0) {
  const v = el?.getAttribute ? el.getAttribute(k) : null;
  if (v == null || v === '') return d;
  const n = parseFloat(v);
  return Number.isFinite(n) ? n : d;
}

// Non-visible SVG elements contribute zero to a parent's bbox, even
// though they have textContent. Measuring their text as "layout width"
// inflates <svg>.getBBox by tens of thousands of pixels because id
// prefixes in <style>...</style> balloon the inner CSS source.
const NON_VISIBLE_TAGS = new Set([
  'style',
  'defs',
  'metadata',
  'title',
  'desc',
  'script',
  'marker',
  'pattern',
  'mask',
  'clippath',
  'symbol',
  'lineargradient',
  'radialgradient',
  'filter',
]);

// SVG path `d` attribute parser. Returns {x,y,width,height} as the
// axis-aligned envelope of all anchor + control points (M/L/H/V/C/S/Q/T/A/Z).
// We approximate curves by the polygon their control points span, which
// is a superset of the exact curve bbox — good enough for dagre node
// sizing (mermaid uses paths primarily for rough.js rectangles and
// createRoundedRectPathD output, where endpoints-only already gives the
// true bbox within a couple of pixels).
function pathBBox(d) {
  if (!d) return { x: 0, y: 0, width: 0, height: 0 };
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  let cx = 0, cy = 0, startX = 0, startY = 0;
  const add = (x, y) => {
    if (x < minX) minX = x;
    if (y < minY) minY = y;
    if (x > maxX) maxX = x;
    if (y > maxY) maxY = y;
  };
  const tok = d.match(/[MmLlHhVvZzCcSsQqTtAa]|-?\d*\.?\d+(?:[eE][-+]?\d+)?/g) ?? [];
  let i = 0;
  let cmd = '';
  const read = () => parseFloat(tok[i++]);
  while (i < tok.length) {
    const t = tok[i];
    if (/^[A-Za-z]$/.test(t)) {
      cmd = t;
      i++;
    }
    const rel = cmd === cmd.toLowerCase();
    switch (cmd) {
      case 'M': case 'm': {
        let x = read(), y = read();
        if (rel) { x += cx; y += cy; }
        cx = x; cy = y; startX = x; startY = y;
        add(x, y);
        cmd = rel ? 'l' : 'L'; // subsequent pairs are implicit lineto
        break;
      }
      case 'L': case 'l': {
        let x = read(), y = read();
        if (rel) { x += cx; y += cy; }
        cx = x; cy = y; add(x, y);
        break;
      }
      case 'H': case 'h': {
        let x = read();
        if (rel) x += cx;
        cx = x; add(x, cy);
        break;
      }
      case 'V': case 'v': {
        let y = read();
        if (rel) y += cy;
        cy = y; add(cx, y);
        break;
      }
      case 'C': case 'c': {
        let x1 = read(), y1 = read(), x2 = read(), y2 = read(), x = read(), y = read();
        if (rel) { x1 += cx; y1 += cy; x2 += cx; y2 += cy; x += cx; y += cy; }
        add(x1, y1); add(x2, y2); add(x, y);
        cx = x; cy = y;
        break;
      }
      case 'S': case 's': {
        let x2 = read(), y2 = read(), x = read(), y = read();
        if (rel) { x2 += cx; y2 += cy; x += cx; y += cy; }
        add(x2, y2); add(x, y);
        cx = x; cy = y;
        break;
      }
      case 'Q': case 'q': {
        let x1 = read(), y1 = read(), x = read(), y = read();
        if (rel) { x1 += cx; y1 += cy; x += cx; y += cy; }
        add(x1, y1); add(x, y);
        cx = x; cy = y;
        break;
      }
      case 'T': case 't': {
        let x = read(), y = read();
        if (rel) { x += cx; y += cy; }
        add(x, y);
        cx = x; cy = y;
        break;
      }
      case 'A': case 'a': {
        read(); read(); read(); read(); read(); // rx ry x-axis-rotation large-arc sweep
        let x = read(), y = read();
        if (rel) { x += cx; y += cy; }
        add(x, y);
        cx = x; cy = y;
        break;
      }
      case 'Z': case 'z':
        cx = startX; cy = startY;
        break;
      default:
        i++; // unknown token, skip
    }
  }
  if (!Number.isFinite(minX)) return { x: 0, y: 0, width: 0, height: 0 };
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}

function polyBBox(pointsAttr) {
  if (!pointsAttr) return { x: 0, y: 0, width: 0, height: 0 };
  const parts = pointsAttr.trim().split(/[\s,]+/).filter(Boolean).map(parseFloat);
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (let i = 0; i + 1 < parts.length; i += 2) {
    const x = parts[i], y = parts[i + 1];
    if (!Number.isFinite(x) || !Number.isFinite(y)) continue;
    if (x < minX) minX = x;
    if (y < minY) minY = y;
    if (x > maxX) maxX = x;
    if (y > maxY) maxY = y;
  }
  if (!Number.isFinite(minX)) return { x: 0, y: 0, width: 0, height: 0 };
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}

// ---------- HTML label clamp (issue #254) ----------
//
// A browser lays an HTML label <div> out as `min(content width, max-width)` —
// the content width being the *widest painted line* — and upstream
// `addHtmlSpan` reacts to `bbox.width === max-width` by re-styling that div to
// `display: table; white-space: break-spaces; width: Npx;`, which wraps the
// text inside N and grows the box to one line box per painted line. jsdom has
// no layout engine, so the shim models both steps itself.
//
// The clamp runs in the label's *rendered* font — `font-size: 16px` and
// `font-family: "trebuchet ms",…` inherited from the generated SVG root — not
// in the 14 px / sans-serif default `resolveFont` falls back to. That default
// is what measured a clamped label at its un-clamped width in the first
// place: mermaid then positioned the label for the wide box the browser never
// paints. Keep this in step with the Rust side
// (`foreign_object::clamp_html_label` + `HTML_LABEL_FONT_SIZE`); a decision
// the two disagree about shows up as a byte-exact diff.
//
// Scoped to flowchart node and edge labels: the one surface whose Rust *layout*
// and renderer both implement the clamp. Other families route labels through
// the same measurement and emission helpers but still size their boxes from
// the un-clamped text, so modelling the clamp for them here would put the
// harness at odds with the renderer — widen this gate together with them.
const LABEL_CLAMP_FAMILY = '"trebuchet ms",verdana,arial,sans-serif';
const LABEL_CLAMP_SIZE = 16;
const LABEL_CLAMP_LINE_HEIGHT_RATIO = 1.5;

// East Asian Wide / Fullwidth code points — the width-2 set
// `unicode_width::UnicodeWidthChar::width` reports on the Rust side, which is
// where a line may break inside a word and where the per-glyph width floor
// applies. Generated from the Unicode EastAsianWidth property (W + F).
const WIDE_RANGES = [
  [0x1100, 0x115F],
  [0x231A, 0x231B],
  [0x2329, 0x232A],
  [0x23E9, 0x23EC],
  [0x23F0, 0x23F0],
  [0x23F3, 0x23F3],
  [0x25FD, 0x25FE],
  [0x2614, 0x2615],
  [0x2648, 0x2653],
  [0x267F, 0x267F],
  [0x2693, 0x2693],
  [0x26A1, 0x26A1],
  [0x26AA, 0x26AB],
  [0x26BD, 0x26BE],
  [0x26C4, 0x26C5],
  [0x26CE, 0x26CE],
  [0x26D4, 0x26D4],
  [0x26EA, 0x26EA],
  [0x26F2, 0x26F3],
  [0x26F5, 0x26F5],
  [0x26FA, 0x26FA],
  [0x26FD, 0x26FD],
  [0x2705, 0x2705],
  [0x270A, 0x270B],
  [0x2728, 0x2728],
  [0x274C, 0x274C],
  [0x274E, 0x274E],
  [0x2753, 0x2755],
  [0x2757, 0x2757],
  [0x2795, 0x2797],
  [0x27B0, 0x27B0],
  [0x27BF, 0x27BF],
  [0x2B1B, 0x2B1C],
  [0x2B50, 0x2B50],
  [0x2B55, 0x2B55],
  [0x2E80, 0x2E99],
  [0x2E9B, 0x2EF3],
  [0x2F00, 0x2FD5],
  [0x2FF0, 0x2FFB],
  [0x3000, 0x303E],
  [0x3041, 0x3096],
  [0x3099, 0x30FF],
  [0x3105, 0x312F],
  [0x3131, 0x318E],
  [0x3190, 0x31E3],
  [0x31F0, 0x321E],
  [0x3220, 0x3247],
  [0x3250, 0x4DBF],
  [0x4E00, 0xA48C],
  [0xA490, 0xA4C6],
  [0xA960, 0xA97C],
  [0xAC00, 0xD7A3],
  [0xF900, 0xFAFF],
  [0xFE10, 0xFE19],
  [0xFE30, 0xFE52],
  [0xFE54, 0xFE66],
  [0xFE68, 0xFE6B],
  [0xFF01, 0xFF60],
  [0xFFE0, 0xFFE6],
  [0x16FE0, 0x16FE4],
  [0x16FF0, 0x16FF1],
  [0x17000, 0x187F7],
  [0x18800, 0x18CD5],
  [0x18D00, 0x18D08],
  [0x1AFF0, 0x1AFF3],
  [0x1AFF5, 0x1AFFB],
  [0x1AFFD, 0x1AFFE],
  [0x1B000, 0x1B122],
  [0x1B132, 0x1B132],
  [0x1B150, 0x1B152],
  [0x1B155, 0x1B155],
  [0x1B164, 0x1B167],
  [0x1B170, 0x1B2FB],
  [0x1F004, 0x1F004],
  [0x1F0CF, 0x1F0CF],
  [0x1F18E, 0x1F18E],
  [0x1F191, 0x1F19A],
  [0x1F200, 0x1F202],
  [0x1F210, 0x1F23B],
  [0x1F240, 0x1F248],
  [0x1F250, 0x1F251],
  [0x1F260, 0x1F265],
  [0x1F300, 0x1F320],
  [0x1F32D, 0x1F335],
  [0x1F337, 0x1F37C],
  [0x1F37E, 0x1F393],
  [0x1F3A0, 0x1F3CA],
  [0x1F3CF, 0x1F3D3],
  [0x1F3E0, 0x1F3F0],
  [0x1F3F4, 0x1F3F4],
  [0x1F3F8, 0x1F43E],
  [0x1F440, 0x1F440],
  [0x1F442, 0x1F4FC],
  [0x1F4FF, 0x1F53D],
  [0x1F54B, 0x1F54E],
  [0x1F550, 0x1F567],
  [0x1F57A, 0x1F57A],
  [0x1F595, 0x1F596],
  [0x1F5A4, 0x1F5A4],
  [0x1F5FB, 0x1F64F],
  [0x1F680, 0x1F6C5],
  [0x1F6CC, 0x1F6CC],
  [0x1F6D0, 0x1F6D2],
  [0x1F6D5, 0x1F6D7],
  [0x1F6DC, 0x1F6DF],
  [0x1F6EB, 0x1F6EC],
  [0x1F6F4, 0x1F6FC],
  [0x1F7E0, 0x1F7EB],
  [0x1F7F0, 0x1F7F0],
  [0x1F90C, 0x1F93A],
  [0x1F93C, 0x1F945],
  [0x1F947, 0x1F9FF],
  [0x1FA70, 0x1FA7C],
  [0x1FA80, 0x1FA88],
  [0x1FA90, 0x1FABD],
  [0x1FABF, 0x1FAC5],
  [0x1FACE, 0x1FADB],
  [0x1FAE0, 0x1FAE8],
  [0x1FAF0, 0x1FAF8],
  [0x20000, 0x2FFFD],
  [0x30000, 0x3FFFD],
];
const WIDE_GLYPH = new RegExp(
  // A character class: `\u{4e00}-\u{a48c}` outside one is three characters,
  // not a range.
  `[${WIDE_RANGES.map(([a, b]) =>
    a === b ? `\\u{${a.toString(16)}}` : `\\u{${a.toString(16)}}-\\u{${b.toString(16)}}`,
  ).join('')}]`,
  'u',
);
const isWideGlyph = (ch) => WIDE_GLYPH.test(ch);
const hasWideGlyph = (text) => {
  for (const ch of text) if (isWideGlyph(ch)) return true;
  return false;
};

// Rust `char::is_whitespace` (the White_Space property) minus the no-break
// spaces, which `break-spaces` never breaks at. Not JS `\s`: that also matches
// U+FEFF, which Rust treats as an ordinary painted glyph.
const RUST_WHITESPACE =
  /[\t\n\x0B\f\r \u0085\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]/;
const isBreakableSpace = (ch) => RUST_WHITESPACE.test(ch) && ch !== '\u00A0' && ch !== '\u202F';

/// `s` without its trailing breakable spaces — mirror of
/// `label_metrics::trim_trailing_breakable_spaces`.
function trimTrailingBreakableSpaces(s) {
  let end = s.length;
  while (end > 0) {
    const ch = String.fromCodePoint(s.codePointAt(end - 1));
    if (!isBreakableSpace(ch)) break;
    end -= ch.length;
  }
  return s.slice(0, end);
}

/// Width estimate the clamp decides on — mirror of
/// `foreign_object::clamp_html_label`'s `width_of`: the shim width, floored per
/// glyph (1em for wide glyphs, 0.56em otherwise) when the text holds any wide
/// glyph. The shim under-measures CJK to ~40 %, so without the floor a CJK
/// label a browser clamps would look like it fits and stay mis-aligned.
const labelClampWidthOf = (text, size = LABEL_CLAMP_SIZE, bold = false) => {
  const shim = textWidth(text, LABEL_CLAMP_FAMILY, size, bold);
  if (!hasWideGlyph(text)) return shim;
  // Top up each *wide* glyph to the 1em a browser paints it at; narrow glyphs
  // keep the shim's own width (billing them a flat 0.56em over-estimates `i`,
  // `l`, `.` and would clamp labels a browser leaves alone). Mirror of
  // `label_metrics::painted_line_width`.
  let deficit = 0;
  for (const ch of text) {
    if (!isWideGlyph(ch)) continue;
    const glyphShim = textWidth(ch, LABEL_CLAMP_FAMILY, size, bold);
    if (size > glyphShim) deficit += size - glyphShim;
  }
  return shim + deficit;
};

/// The font a label is finally painted in: the inline `font-size` /
/// `font-weight` declared on it or an ancestor, on top of the browser default
/// the generated SVG root sets (`16px`). Mirrors the Rust side, where the clamp
/// and the `font_size` / `bold` post-process passes all resolve the node's
/// styles — deciding at any other size reports a different line count than the
/// label the browser wraps.
function clampFont(el) {
  let size = LABEL_CLAMP_SIZE;
  let bold = false;
  let sizeSet = false;
  let weightSet = false;
  for (let node = el; node && node.nodeType === 1; node = node.parentNode) {
    const styleSize = node.style?.fontSize ?? node.getAttribute?.('font-size') ?? null;
    if (!sizeSet && styleSize) {
      const m = /([0-9.]+)/.exec(String(styleSize));
      if (m) {
        size = parseFloat(m[1]);
        sizeSet = true;
      }
    }
    const weight = node.style?.fontWeight ?? node.getAttribute?.('font-weight') ?? null;
    if (!weightSet && weight) {
      const v = String(weight).trim().toLowerCase();
      bold = v === 'bold' || v === 'bolder' || (parseInt(v, 10) >= 700);
      weightSet = true;
    }
    if (sizeSet && weightSet) break;
  }
  return { size, bold };
}

/// Greedy `white-space: break-spaces` line breaking — the mirror of
/// `label_metrics::wrap_line_to_width`: segments are a word, one wide glyph
/// or a whitespace run (plus the whitespace that follows it), and a word
/// wider than the budget keeps a line to itself instead of being broken.
function wrapLineToWidth(line, maxWidth, size = LABEL_CLAMP_SIZE, bold = false) {
  const segments = [];
  let i = 0;
  while (i < line.length) {
    const start = i;
    const ch = String.fromCodePoint(line.codePointAt(i));
    i += ch.length;
    if (!isBreakableSpace(ch) && !isWideGlyph(ch)) {
      while (i < line.length) {
        const c = String.fromCodePoint(line.codePointAt(i));
        if (isBreakableSpace(c) || isWideGlyph(c)) break;
        i += c.length;
      }
    }
    while (i < line.length && isBreakableSpace(line[i])) i += 1;
    segments.push([start, i]);
  }
  const lines = [];
  let lineStart = 0;
  let width = 0;
  for (const [start, end] of segments) {
    const w = labelClampWidthOf(line.slice(start, end), size, bold);
    if (width > 0 && width + w > maxWidth) {
      lines.push(line.slice(lineStart, start));
      lineStart = start;
      width = 0;
    }
    width += w;
  }
  lines.push(line.slice(lineStart));
  return lines;
}

/// Painted lines of a label element: `<br>` (any spelling) and `<p>` block
/// boundaries end a line, and a break at the end of a block opens no new one
/// — the DOM-side mirror of `label_metrics::split_label_lines`. A no-break
/// space is a painted glyph, so `a<br/>&nbsp;` is two lines.
function paintedLabelLines(el) {
  const lines = [];
  let block = [];
  let current = '';
  const isBlank = (s) => s.replace(/\u00A0/g, '\u0000').trim() === '';
  const endBlock = () => {
    if (block.length > 1 && isBlank(block[block.length - 1])) block.pop();
    if (!(block.length === 1 && isBlank(block[0]))) lines.push(...block);
    block = [];
  };
  const walk = (node) => {
    for (const child of node.childNodes ?? []) {
      if (child.nodeType === 3) {
        current += child.nodeValue ?? '';
        continue;
      }
      if (child.nodeType !== 1) continue;
      const tag = (child.tagName ?? '').toLowerCase();
      if (tag === 'br') {
        block.push(current);
        current = '';
        continue;
      }
      if (tag === 'p') {
        block.push(current);
        current = '';
        endBlock();
        walk(child);
        block.push(current);
        current = '';
        endBlock();
        continue;
      }
      walk(child);
    }
  };
  walk(el);
  block.push(current);
  endBlock();
  return lines.length ? lines : [''];
}

/// Whether the fixture being rendered is a flowchart — the family whose Rust
/// renderer implements the clamp. Mermaid selects the flowchart loader for
/// `flowchart` and for its `graph` alias, on the first line that is not a
/// directive comment (frontmatter blocks are stripped first).
function isFlowchartSource(source) {
  const body = String(source).replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, '');
  for (const raw of body.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith('%%')) continue;
    return /^(flowchart|graph)\b/.test(line);
  }
  return false;
}

/// Set per render by `renderOne` — see `labelClampBox`.
let __labelClampEnabled = false;

/// Whether the browser clamps this label, and the box it paints once it does.
/// `null` means the label fits inside its `max-width` and nothing changes.
function labelClampBox(el) {
  if (!__labelClampEnabled) return null;
  const style = el.getAttribute?.('style') ?? '';
  const m = /(?:^|;)\s*max-width:\s*([\d.]+)px/.exec(style);
  if (!m) return null;
  const maxWidth = parseFloat(m[1]);
  if (!(maxWidth > 0)) return null;
  const { size, bold } = clampFont(el);
  const painted = paintedLabelLines(el);
  if (!painted.some((line) => labelClampWidthOf(line, size, bold) >= maxWidth)) return null;
  const lineHeight = size * LABEL_CLAMP_LINE_HEIGHT_RATIO;
  // Upstream measures the div twice. Before the style switch it is
  // `table-cell; nowrap; max-width: N`, so the browser reports
  // `min(widest line, N)` — and it is that equality with `N` that makes
  // upstream switch the style. Model both steps: the second measurement is
  // what ends up in the SVG, but a shim that only reported it would never let
  // the switch happen (the equal-width test fails as soon as an unbreakable run
  // pushes the final width past the budget).
  const alreadyWrapped = /display:\s*table\s*;/.test(style);
  if (!alreadyWrapped) {
    const widest = painted.reduce((w, line) => Math.max(w, labelClampWidthOf(line, size, bold)), 0);
    return {
      x: 0,
      y: 0,
      width: Math.min(widest, maxWidth),
      height: lineHeight * painted.length,
    };
  }
  let wrapped = 0;
  let widestRun = 0;
  for (const line of painted) {
    const wrappedLines = wrapLineToWidth(line, maxWidth, size, bold);
    wrapped += wrappedLines.length;
    for (const segment of wrappedLines) {
      // `display: table` never shrinks below its min-content width, so a run
      // with no break opportunity inside it widens the whole label — and the
      // foreignObject upstream measures from it — past the budget.
      widestRun = Math.max(
        widestRun,
        labelClampWidthOf(trimTrailingBreakableSpaces(segment), size, bold),
      );
    }
  }
  return {
    x: 0,
    y: 0,
    width: Math.max(maxWidth, widestRun),
    height: lineHeight * wrapped,
  };
}

function intrinsicBox(el) {
  // Per-tag bbox without descending. For <g>, returns null so the
  // caller recurses into children.
  const tag = (el.tagName ?? '').toLowerCase();
  if (NON_VISIBLE_TAGS.has(tag)) {
    return { x: 0, y: 0, width: 0, height: 0 };
  }
  if (tag === 'rect') {
    return {
      x: attrNum(el, 'x'),
      y: attrNum(el, 'y'),
      width: attrNum(el, 'width'),
      height: attrNum(el, 'height'),
    };
  }
  if (tag === 'circle') {
    const r = attrNum(el, 'r');
    return { x: attrNum(el, 'cx') - r, y: attrNum(el, 'cy') - r, width: r * 2, height: r * 2 };
  }
  if (tag === 'ellipse') {
    const rx = attrNum(el, 'rx'), ry = attrNum(el, 'ry');
    return { x: attrNum(el, 'cx') - rx, y: attrNum(el, 'cy') - ry, width: rx * 2, height: ry * 2 };
  }
  if (tag === 'line') {
    const x1 = attrNum(el, 'x1'), y1 = attrNum(el, 'y1');
    const x2 = attrNum(el, 'x2'), y2 = attrNum(el, 'y2');
    return {
      x: Math.min(x1, x2), y: Math.min(y1, y2),
      width: Math.abs(x2 - x1), height: Math.abs(y2 - y1),
    };
  }
  if (tag === 'polygon' || tag === 'polyline') {
    return polyBBox(el.getAttribute?.('points') ?? '');
  }
  if (tag === 'path') {
    return pathBBox(el.getAttribute?.('d') ?? '');
  }
  if (tag === 'foreignobject') {
    // Use explicit width/height if set, else fall back to the HTML
    // content's measurement.
    const w = attrNum(el, 'width', -1), h = attrNum(el, 'height', -1);
    if (w >= 0 && h >= 0) return { x: attrNum(el, 'x'), y: attrNum(el, 'y'), width: w, height: h };
    const { size, family, bold } = resolveFont(el);
    const { width, height } = measureTextBlock(el.textContent ?? '', family, size, bold);
    return { x: 0, y: 0, width, height };
  }
  if (tag === 'text' || tag === 'tspan') {
    const { size, family, bold } = resolveFont(el);
    const { width, height } = measureTextBlock(el.textContent ?? '', family, size, bold);
    return { x: 0, y: 0, width, height };
  }
  if (tag === 'g' || tag === 'svg') {
    return null; // recurse
  }
  // HTML in foreignObject, or anything else measurable via textContent.
  // A label the browser would clamp to its `max-width` reports the wrapped
  // box instead (see `labelClampBox`).
  const clamped = labelClampBox(el);
  if (clamped) return clamped;
  const { size, family, bold } = resolveFont(el);
  const { width, height } = measureTextBlock(el.textContent ?? '', family, size, bold);
  return { x: 0, y: 0, width, height };
}

function unionBox(boxes) {
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  let found = false;
  for (const b of boxes) {
    if (!b) continue;
    if (b.width === 0 && b.height === 0) continue;
    found = true;
    if (b.x < minX) minX = b.x;
    if (b.y < minY) minY = b.y;
    if (b.x + b.width > maxX) maxX = b.x + b.width;
    if (b.y + b.height > maxY) maxY = b.y + b.height;
  }
  if (!found) return { x: 0, y: 0, width: 0, height: 0 };
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}

function elementBBox() {
  const intrinsic = intrinsicBox(this);
  if (intrinsic) return intrinsic;
  // <g> / <svg>: union of descendant intrinsic boxes. Skip transforms
  // for now (mermaid places geometry in local coords by default).
  const stack = [this];
  const boxes = [];
  let depth = 0;
  while (stack.length && depth++ < 5000) {
    const n = stack.pop();
    for (const c of n.children ?? []) {
      const ib = intrinsicBox(c);
      if (ib) boxes.push(ib);
      else stack.push(c); // <g>/<svg> - descend
    }
  }
  return unionBox(boxes);
}
function textLen() {
  const { size, family, bold } = resolveFont(this);
  return textWidth(this.textContent ?? '', family, size, bold);
}
const textBBox = elementBBox;

W.SVGElement.prototype.getBBox = textBBox;
W.SVGElement.prototype.getComputedTextLength = textLen;
if (W.HTMLElement) W.HTMLElement.prototype.getBBox = textBBox;
if (W.Element && !W.Element.prototype.getBBox) W.Element.prototype.getBBox = textBBox;

// jsdom's native getBoundingClientRect returns {0,0,0,0} because it
// has no layout engine. Mermaid's labelHelper / addHtmlSpan relies on
// this to measure HTML labels inside <foreignObject> — and feeds the
// width back into dagre as the node's width/height. Returning zero
// there collapses nodes and dagre then emits zero-length edges, which
// makes calcLabelPosition throw "Could not find a suitable point for
// the given distance". Delegate to our font-aware elementBBox shim so
// HTML labels carry real dimensions.
function boundingClientRectShim() {
  const b = elementBBox.call(this);
  return {
    x: b.x,
    y: b.y,
    width: b.width,
    height: b.height,
    top: b.y,
    left: b.x,
    right: b.x + b.width,
    bottom: b.y + b.height,
    toJSON() {
      return { x: b.x, y: b.y, width: b.width, height: b.height, top: b.y, left: b.x, right: b.x + b.width, bottom: b.y + b.height };
    },
  };
}
if (W.HTMLElement) W.HTMLElement.prototype.getBoundingClientRect = boundingClientRectShim;
if (W.Element) W.Element.prototype.getBoundingClientRect = boundingClientRectShim;

// Deterministic Math.random. Mermaid's shape renderers call rough.js
// even when look != "handDrawn" (with roughness:0 + fillStyle:'solid'),
// and rough.js falls back to Math.random whenever `seed` is missing
// from its options. Without a fixed PRNG the emitted bezier control
// points drift on every invocation — breaking byte-exact references
// on every class / er / requirement / flowchart shape that routes
// through rc.path() instead of <rect>. Reseed per render inside
// renderOne() so cross-fixture ordering does not leak between
// successive renders in the same process.
let __rngState = 0x12345678;
function __mulberry32() {
  __rngState = (__rngState + 0x6d2b79f5) | 0;
  let t = __rngState;
  t = Math.imul(t ^ (t >>> 15), 1 | t);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
Math.random = __mulberry32;
W.Math.random = __mulberry32;

const mermaid = (await import('mermaid')).default;
// `forceLegacyMathML: true` makes mermaid call the real KaTeX renderer
// (output: 'htmlAndMathml') even in jsdom — without it, the jsdom env
// (`window.MathMLElement === undefined`) triggers mermaid's `renderKatexUnsanitized`
// fallback that replaces every `$$...$$` with the literal placeholder
// "MathML is unsupported in this environment.", which would force the
// Rust port to mirror that placeholder instead of real KaTeX output.
// We aim for byte-exact parity with what users see in real browsers.
mermaid.initialize({ startOnLoad: false, securityLevel: 'loose', handDrawnSeed: 1, forceLegacyMathML: true });

// ---------- render one source ----------
// Guarded against mermaid.render() promises that never settle (seen
// on fixtures with remote <img src>, fa: icons, or certain flowchart
// syntax variants). Also flush document.body between calls — without
// this, some renders pollute the DOM such that subsequent renders
// also hang.
const RENDER_TIMEOUT_MS = 3_000;
async function renderOne(source, id) {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`render timeout after ${RENDER_TIMEOUT_MS}ms`)), RENDER_TIMEOUT_MS);
  });
  try {
    // Clean state: mermaid leaves stray nodes (tooltips, id-scoped
    // foreignObject labels) that accumulate across renders.
    const body = W.document.body;
    while (body.firstChild) body.removeChild(body.firstChild);
    // Reseed the PRNG so cross-render order does not affect output.
    // Any fixed starting state works; using a derived-from-id seed
    // makes single-file and batch runs produce the same bytes for
    // the same fixture regardless of the surrounding batch slice.
    __rngState = 0x12345678;
    __labelClampEnabled = isFlowchartSource(source);
    const { svg } = await Promise.race([mermaid.render(id, source), timeout]);
    return normaliseSvg(svg);
  } finally {
    clearTimeout(timer);
  }
}

// Post-render normaliser: strips/rewrites parts of mermaid's SVG that
// are intrinsically non-deterministic in our Node env so repeated
// runs produce byte-identical output. The Rust-side renderer must
// apply the equivalent normalisation — or simply not emit these
// counter suffixes / wall-clock markers in the first place.
function normaliseSvg(svg) {
  // 1. Gantt "today" marker: x derived from wall-clock outside our
  // Date.now() freeze. Wipe the body.
  svg = svg.replace(/<g class="today">[\s\S]*?<\/g>/g, '<g class="today"></g>');

  // 2. Module-level counters in mermaid (`var classCounter`, etc.)
  // never reset across renders. In single-file mode they start at 0;
  // in a batch worker they start at some N. Renumber per-SVG by
  // first-appearance so the output is stable regardless of where in
  // a batch the fixture renders.
  // Each rule = (pattern, join). Pattern captures `(id)(counter)` in
  // that order; join is the separator used to splice them back.
  // classDiagram uses `classId-A-N` (sep '-'); sequenceDiagram uses
  // `actor7` (no sep). Add more tuples when new counter leaks
  // surface during Rust-side diffing.
  const counterRules = [
    [/(classId-\w+)-(\d+)/g, '-'],
    // `(?<![a-zA-Z])` avoids matching mid-word; \b would fail here
    // because `_` is a word char (e.g. `actor2_popup` must match).
    [/(?<![a-zA-Z])(actor)(\d+)/g, ''],
    [/(-msg)(\d+)/g, ''],
    [/(?<![a-zA-Z])(state)(\d+)(?![a-zA-Z])/g, ''],
    [/(?<![a-zA-Z])(root)-(\d+)/g, '-'],  // sequenceDiagram participant roots
    [/(actor-[a-z]+-[a-z]+)(\d+)/g, ''],       // sequenceDiagram svg-sprite body parts
  ];
  for (const [re, sep] of counterRules) svg = renumberCounterIds(svg, re, sep);
  return svg;
}

function renumberCounterIds(svg, pattern, sep) {
  const map = new Map();
  let next = 0;
  return svg.replace(pattern, (_m, id, counter) => {
    const uniq = id + ':' + counter;
    if (!map.has(uniq)) map.set(uniq, next++);
    return id + sep + map.get(uniq);
  });
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
  // --batch has two modes:
  //   1. Coordinator: `--batch` or `--batch --workers N`
  //      Forks N workers, each gets --chunk i/N, merges results.
  //   2. Worker:      `--batch --chunk K/N`
  //      Processes only fixtures whose index mod N === K.
  const chunkArgIdx = argv.indexOf('--chunk');
  const workersArgIdx = argv.indexOf('--workers');
  const isWorker = chunkArgIdx !== -1;

  const sources = [join(TESTS_DIR, 'fixtures'), join(TESTS_DIR, 'ext_fixtures')];
  const refRoot = join(TESTS_DIR, 'reference');

  // Enumerate all fixtures up front so workers have a stable, shared
  // indexing. Sort for determinism, then deterministically shuffle —
  // consecutive fixtures from the same diagram type tend to fail in
  // bursts (e.g. all flowchart/1*.mmd hit remote-image timeouts). The
  // shuffle spreads those hot spots across workers so no single
  // worker dominates wall-clock.
  // Fixtures listed in tests/known_ignored.txt are skipped entirely
  // (no render attempt, no failure entry). Used for fixtures that
  // upstream mermaid itself can't render (broken demo syntax,
  // opt-in subpackages we don't load, etc). Each non-empty, non-
  // comment line is `<rel path>\t<reason>`.
  const ignorePath = join(TESTS_DIR, 'known_ignored.txt');
  const ignored = new Map(); // absolute mmd path → reason
  if (existsSync(ignorePath)) {
    for (const line of readFileSync(ignorePath, 'utf8').split('\n')) {
      const trimmed = line.trimStart();
      if (!trimmed || trimmed.startsWith('#')) continue;
      const [rel, ...reasonParts] = line.split('\t');
      if (!rel || !reasonParts.length) continue;
      ignored.set(join(TESTS_DIR, rel.trim()), reasonParts.join('\t').trim());
    }
  }

  const allFixtures = [];
  for (const root of sources) {
    if (!existsSync(root)) continue;
    for (const mmdPath of walk(root)) {
      if (!mmdPath.endsWith('.mmd')) continue;
      if (ignored.has(mmdPath)) continue;
      allFixtures.push({ root, mmdPath });
    }
  }
  allFixtures.sort((a, b) => a.mmdPath.localeCompare(b.mmdPath));
  // Mulberry32 deterministic PRNG, seeded by fixture count so the
  // order is reproducible but spreads pathological bursts.
  let seed = allFixtures.length;
  const rand = () => {
    seed |= 0;
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  for (let i = allFixtures.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [allFixtures[i], allFixtures[j]] = [allFixtures[j], allFixtures[i]];
  }

  if (!isWorker) {
    // ---- Coordinator path ----
    const nWorkers = workersArgIdx !== -1 ? parseInt(argv[workersArgIdx + 1], 10) : 1;
    if (!Number.isFinite(nWorkers) || nWorkers < 1) {
      console.error('error: --workers must be a positive integer');
      process.exit(2);
    }

    const startedAt = Date.now();
    if (nWorkers === 1) {
      // Single-worker inline path: avoid fork overhead on small runs.
      await runWorker(0, 1, allFixtures, refRoot, startedAt);
      process.exit(0);
    }

    // Fan out to children. Each child writes its own failure log at
    // reference/_failures.<pid>.log; coordinator merges at the end.
    const { fork } = await import('node:child_process');
    const scriptPath = fileURLToPath(import.meta.url);
    const children = [];
    console.log(`dispatching ${allFixtures.length} fixtures across ${nWorkers} workers...`);
    for (let i = 0; i < nWorkers; i++) {
      const child = fork(scriptPath, ['--batch', '--chunk', `${i}/${nWorkers}`], {
        stdio: ['ignore', 'inherit', 'inherit', 'ipc'],
      });
      children.push(
        new Promise((res) => {
          child.on('exit', (code) => res({ i, code }));
        }),
      );
    }
    const results = await Promise.all(children);
    const bad = results.filter((r) => r.code !== 0);
    const secs = ((Date.now() - startedAt) / 1000).toFixed(1);

    // Merge per-worker failure logs into a single _failures.log
    const merged = [];
    for (let i = 0; i < nWorkers; i++) {
      const p = join(refRoot, `_failures.worker-${i}.log`);
      if (existsSync(p)) {
        const content = readFileSync(p, 'utf8').trim();
        if (content) merged.push(content);
        try {
          (await import('node:fs')).rmSync(p);
        } catch {}
      }
    }
    if (merged.length > 0) {
      const allPath = join(refRoot, '_failures.log');
      writeFileSync(allPath, merged.join('\n') + '\n');
    }
    const svgCount = countSvgs(refRoot);
    const failCount = merged.length === 0 ? 0 : merged.reduce((n, s) => n + s.split('\n').length, 0);
    console.log(
      `\ncoordinator summary: ${svgCount} svg / ${failCount} fail / ${allFixtures.length} total in ${secs}s; workers with non-zero exit: ${bad.length}`,
    );
    process.exit(0);
  }

  // ---- Worker path ----
  const spec = argv[chunkArgIdx + 1] ?? '';
  const [kStr, nStr] = spec.split('/');
  const K = parseInt(kStr, 10);
  const N = parseInt(nStr, 10);
  if (!Number.isFinite(K) || !Number.isFinite(N) || N < 1 || K < 0 || K >= N) {
    console.error(`error: --chunk expects form K/N with 0<=K<N, got ${spec}`);
    process.exit(2);
  }
  const slice = allFixtures.filter((_, idx) => idx % N === K);
  await runWorker(K, N, slice, refRoot, Date.now());
  process.exit(0);
} else {
  const inputPath = argv[0];
  if (!inputPath) {
    console.error('error: missing input path. Use --help.');
    process.exit(2);
  }
  const outIdx = argv.indexOf('-o');
  const outPath = outIdx !== -1 ? argv[outIdx + 1] : null;
  const src = readFileSync(inputPath, 'utf8');
  // Use the same path-based id scheme as --batch so single-file and
  // batch invocations produce byte-identical output for the same
  // fixture. If the input is outside tests/, fall back to basename.
  const abs = resolve(inputPath);
  const id = abs.startsWith(TESTS_DIR + '/')
    ? idForPath(abs)
    : 'ref-' + basename(inputPath, extname(inputPath)).replace(/[^a-zA-Z0-9]+/g, '-');
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

function countSvgs(root) {
  if (!existsSync(root)) return 0;
  let n = 0;
  for (const p of walk(root)) if (p.endsWith('.svg')) n++;
  return n;
}

async function runWorker(workerId, totalWorkers, slice, refRoot, startedAt) {
  // Each worker writes its own incremental failure log so a crash
  // doesn't lose the diagnostic trail we've accumulated so far.
  const failLogPath = join(refRoot, `_failures.worker-${workerId}.log`);
  mkdirSync(refRoot, { recursive: true });
  writeFileSync(failLogPath, '');
  __workerCrashSink = failLogPath;
  __workerCrashTag = `w${workerId}:<pre-loop>`;
  const appendFail = (line) => appendFileSync(failLogPath, line + '\n');

  const tag = totalWorkers === 1 ? '' : `[w${workerId}] `;
  let ok = 0, fail = 0;
  for (let i = 0; i < slice.length; i++) {
    const { root, mmdPath } = slice[i];
    const currentFixture = relative(TESTS_DIR, mmdPath);
    __workerCrashTag = `w${workerId}:${currentFixture}`;
    const relPath = relative(root, mmdPath);
    const outPath = join(refRoot, basename(root), relPath).replace(/\.mmd$/, '.svg');
    mkdirSync(dirname(outPath), { recursive: true });
    try {
      const src = readFileSync(mmdPath, 'utf8');
      let svg;
      try {
        svg = await renderOne(src, idForPath(mmdPath));
      } catch (err) {
        if ((err.message ?? '').includes('beginning with ---')) {
          const normalised = src.replace(/^\s+(---)$/gm, '$1');
          svg = await renderOne(normalised, idForPath(mmdPath));
        } else {
          throw err;
        }
      }
      writeFileSync(outPath, svg);
      ok++;
    } catch (err) {
      fail++;
      const msg = (err.message ?? String(err)).split('\n')[0];
      appendFail(`${currentFixture}\t${msg}`);
    }
    if ((ok + fail) % 50 === 0) {
      const secs = ((Date.now() - startedAt) / 1000).toFixed(1);
      console.log(`${tag}  progress: ${ok} ok / ${fail} fail / ${slice.length} total (${secs}s)`);
    }
  }
  const secs = ((Date.now() - startedAt) / 1000).toFixed(1);
  console.log(`${tag}summary: ${ok}/${slice.length} ok, ${fail} fail in ${secs}s`);
}
