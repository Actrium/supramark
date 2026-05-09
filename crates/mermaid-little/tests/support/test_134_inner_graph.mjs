// Quick test: trace inner graph node order for fixture 134
// by patching graphlib's children() to log calls

// Load the full generate_ref environment first
import { JSDOM } from 'jsdom';

const dom = new JSDOM('<!DOCTYPE html><html><body><div id="container"></div></body></html>', { pretendToBeVisual: true });
const W = dom.window;
for (const k of ['window','document','navigator','HTMLElement','SVGElement','Element','Node','DOMParser','XMLSerializer','getComputedStyle','screen','location','requestAnimationFrame','cancelAnimationFrame','MutationObserver','Image','CSS']) {
  if (W[k] !== undefined) globalThis[k] = W[k];
}
if (!globalThis.screen) globalThis.screen = { availWidth: 1024, availHeight: 768, width: 1024, height: 768 };
if (!globalThis.CSS) globalThis.CSS = {};
if (!globalThis.CSS.supports) globalThis.CSS.supports = () => true;
if (!globalThis.Option && W.document) {
  globalThis.Option = function OptionShim(text = '', value = '') {
    const el = W.document.createElement('option');
    el.text = text; el.value = value; return el;
  };
}

const __origGetComputedStyle = W.getComputedStyle.bind(W);
W.getComputedStyle = function patchedGetComputedStyle(el, pseudo) {
  const cs = __origGetComputedStyle(el, pseudo);
  if (!cs || cs.__paddingShimmed) return cs;
  const origGet = cs.getPropertyValue.bind(cs);
  cs.getPropertyValue = (name) => { const v = origGet(name); return v === '' || v == null ? '0px' : v; };
  try { Object.defineProperty(cs, '__paddingShimmed', { value: true }); } catch {}
  return cs;
};
globalThis.getComputedStyle = W.getComputedStyle;

if (W.HTMLCanvasElement) {
  const noop = () => {};
  const measureText = () => ({ width: 0 });
  const makeCtx = () => ({
    canvas: { width: 0, height: 0 }, fillStyle: '', strokeStyle: '', lineWidth: 1, font: '',
    textAlign: 'start', textBaseline: 'alphabetic', globalAlpha: 1, save: noop, restore: noop,
    translate: noop, rotate: noop, scale: noop, setTransform: noop, beginPath: noop, closePath: noop,
    moveTo: noop, lineTo: noop, arc: noop, rect: noop, fill: noop, stroke: noop, clip: noop,
    fillText: noop, strokeText: noop, fillRect: noop, strokeRect: noop, clearRect: noop,
    measureText, createLinearGradient: () => ({ addColorStop: noop }),
    createRadialGradient: () => ({ addColorStop: noop }), drawImage: noop,
    getImageData: () => ({ data: new Uint8ClampedArray(0) }), putImageData: noop,
  });
  W.HTMLCanvasElement.prototype.getContext = function () { return makeCtx(); };
}

globalThis.fetch = async () => ({ ok: true, status: 200, text: async () => '', json: async () => ({}), arrayBuffer: async () => new ArrayBuffer(0), blob: async () => new W.Blob([], { type: 'text/plain' }) });

if (W.HTMLImageElement) {
  Object.defineProperty(W.HTMLImageElement.prototype, 'src', {
    configurable: true, set(_v) { setTimeout(() => { this.dispatchEvent && this.dispatchEvent(new W.Event('error')); }, 0); }, get() { return ''; },
  });
  Object.defineProperty(W.HTMLImageElement.prototype, 'complete', { configurable: true, get() { return true; } });
  if (typeof W.HTMLImageElement.prototype.decode !== 'function') {
    W.HTMLImageElement.prototype.decode = function () { return Promise.resolve(); };
  }
}

// Font metrics shims (simplified)
function resolveFont(el) { return { size: 14, family: 'sans-serif', bold: false }; }
function textWidth(text, family, size, bold) { return text.length * 8; }
function measureTextBlock(text, family, size, bold) { return { width: text.length * 8, height: 14 }; }

const NON_VISIBLE_TAGS = new Set(['style','defs','metadata','title','desc','script','marker','pattern','mask','clippath','symbol','lineargradient','radialgradient','filter']);
function intrinsicBox(el) { return { x: 0, y: 0, width: 0, height: 0 }; }
function elementBBox() { return intrinsicBox(this); }
function textLen() { const { size, family, bold } = resolveFont(this); return textWidth(this.textContent ?? '', family, size, bold); }

W.SVGElement.prototype.getBBox = elementBBox;
W.SVGElement.prototype.getComputedTextLength = textLen;
if (W.HTMLElement) W.HTMLElement.prototype.getBBox = elementBBox;
if (W.Element && !W.Element.prototype.getBBox) W.Element.prototype.getBBox = elementBBox;
function boundingClientRectShim() { return { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0, toJSON() { return this; } }; }
if (W.HTMLElement) W.HTMLElement.prototype.getBoundingClientRect = boundingClientRectShim;
if (W.Element) W.Element.prototype.getBoundingClientRect = boundingClientRectShim;

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

const FROZEN_EPOCH_MS = Date.parse('2024-01-01T00:00:00Z');
Date.now = () => FROZEN_EPOCH_MS;
W.Date.now = () => FROZEN_EPOCH_MS;

const mermaid = (await import('mermaid')).default;
mermaid.initialize({ startOnLoad: false, securityLevel: 'loose', handDrawnSeed: 1 });

// Now patch the graphlib to log graph.nodes() calls
const dagreMod = await import('mermaid/src/rendering-util/layout-algorithms/dagre/index.js');

// We need to intercept the recursiveRender to log inner graph node order
// Let's monkey-patch graphlib.Graph.prototype.nodes to log when called on inner graphs
const graphlib = await import('dagre-d3-es/src/graphlib/index.js');
const origNodes = graphlib.Graph.prototype.nodes;
let callCount = 0;
graphlib.Graph.prototype.nodes = function() {
  const result = origNodes.call(this);
  if (result.length > 0 && result.includes('c') && result.includes('b') && result.includes('a')) {
    console.log(`graph.nodes() call #${++callCount}:`, result);
  }
  return result;
};

const source = `flowchart TB
    b-->B
    a-->c
    subgraph O
      A
    end
    subgraph B
      c
    end
    subgraph A
        a
        b
        B
    end`;

const { svg } = await mermaid.render('test-134', source);
console.log('\nDone. Total graph.nodes() calls intercepted:', callCount);
