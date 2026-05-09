import { JSDOM } from 'jsdom';
import { readFileSync } from 'node:fs';

const dom = new JSDOM(`<!DOCTYPE html><html><body><div id="d"></div></body></html>`, { pretendToBeVisual: true });
const W = dom.window;
for (const k of ['window','document','navigator','HTMLElement','SVGElement','Element','Node','DOMParser','XMLSerializer','getComputedStyle','screen','location','requestAnimationFrame','cancelAnimationFrame','MutationObserver','Image','CSS']) {
  if (W[k] !== undefined) globalThis[k] = W[k];
}
if (!globalThis.screen) globalThis.screen = {availWidth:1024,availHeight:768,width:1024,height:768};
const FROZEN = Date.parse('2024-01-01T00:00:00Z');
Date.now = () => FROZEN;
W.Date.now = () => FROZEN;

import mermaid from '/ext/mermaid/tests/support/node_modules/mermaid/dist/mermaid.core.mjs';

const src = readFileSync('/ext/mermaid/tests/ext_fixtures/cypress/flowchart/134.mmd', 'utf8');

mermaid.initialize({ startOnLoad: false, theme: 'default', securityLevel: 'loose' });

const diagram = mermaid.getDiagramFromText(src);
const db = diagram.db;
const vertices = db.getVertices();
console.log('getVertices order:');
for (const [key, val] of vertices) {
  console.log(`  ${key}: domId=${val.domId}`);
}
const data = db.getData();
console.log('getData nodes:');
for (const n of data.nodes) {
  console.log(`  id=${n.id} parentId=${n.parentId} isGroup=${n.isGroup}`);
}
