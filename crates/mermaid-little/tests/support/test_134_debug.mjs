import { JSDOM } from 'jsdom';
import { readFileSync } from 'node:fs';

const dom = new JSDOM('<!DOCTYPE html><html><body><div id="container"></div></body></html>', { pretendToBeVisual: true });
const W = dom.window;
for (const k of ['window','document','navigator','HTMLElement','SVGElement','Element','Node','DOMParser','XMLSerializer','getComputedStyle','screen','location','requestAnimationFrame','cancelAnimationFrame','MutationObserver','Image','CSS']) {
  if (W[k] !== undefined) globalThis[k] = W[k];
}
if (!globalThis.CSS) globalThis.CSS = {};
if (!globalThis.CSS.supports) globalThis.CSS.supports = () => true;

const FROZEN_EPOCH_MS = Date.parse('2024-01-01T00:00:00Z');
Date.now = () => FROZEN_EPOCH_MS;
W.Date.now = () => FROZEN_EPOCH_MS;

const mermaid = (await import('mermaid')).default;
mermaid.initialize({ startOnLoad: false, securityLevel: 'loose', handDrawnSeed: 1 });

const source = readFileSync('/ext/mermaid/tests/ext_fixtures/cypress/flowchart/134.mmd', 'utf8');
const id = 'ref-ext-fixtures-cypress-flowchart-134';

// Intercept mermaid.render to log the data
let capturedData = null;
const origRender = mermaid.render.bind(mermaid);
mermaid.render = async function(renderId, src) {
    // Parse first
    await mermaid.parse(src);
    return origRender(renderId, src);
};

const { svg } = await mermaid.render(id, source);

// Now parse again to inspect the DB
const diagram = await mermaid.parse(source);
console.log('Diagram type:', diagram.type);
console.log('DB keys:', Object.keys(diagram));
console.log('DB prototype methods:', Object.getOwnPropertyNames(Object.getPrototypeOf(diagram.db)));
