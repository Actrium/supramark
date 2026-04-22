#!/usr/bin/env node
// Probe script: render a few different diagram types via mermaid+jsdom
// to confirm the pipeline survives the hard paths (dagre-backed flowchart,
// sequence diagram, class diagram). Exits non-zero on any failure.

import { JSDOM } from 'jsdom';

const dom = new JSDOM(
  `<!DOCTYPE html><html><body><div id="container"></div></body></html>`,
  { pretendToBeVisual: true },
);
globalThis.window = dom.window;
globalThis.document = dom.window.document;
globalThis.navigator = dom.window.navigator;
globalThis.HTMLElement = dom.window.HTMLElement;
globalThis.SVGElement = dom.window.SVGElement;
globalThis.Element = dom.window.Element;
globalThis.Node = dom.window.Node;
globalThis.DOMParser = dom.window.DOMParser;
globalThis.XMLSerializer = dom.window.XMLSerializer;
globalThis.getComputedStyle = dom.window.getComputedStyle;

dom.window.SVGElement.prototype.getBBox = function () {
  const text = this.textContent ?? '';
  const lines = text.split('\n');
  const longest = lines.reduce((m, l) => Math.max(m, l.length), 0);
  return { x: 0, y: 0, width: longest * 8, height: lines.length * 14 };
};
dom.window.SVGElement.prototype.getComputedTextLength = function () {
  return (this.textContent ?? '').length * 8;
};

const mermaid = (await import('mermaid')).default;
mermaid.initialize({ startOnLoad: false, securityLevel: 'loose' });

const cases = [
  {
    name: 'pie',
    source: `pie title Sports in Sweden
  "Bandy": 40
  "Ice-Hockey": 80
  "Football": 90
`,
  },
  {
    name: 'flowchart',
    source: `flowchart LR
    A[Start] --> B{Decision}
    B -->|Yes| C[OK]
    B -->|No| D[Stop]
`,
  },
  {
    name: 'sequence',
    source: `sequenceDiagram
    Alice->>+John: Hello John, how are you?
    John-->>-Alice: Great!
`,
  },
  {
    name: 'class',
    source: `classDiagram
    Animal <|-- Duck
    Animal : +int age
    Animal : +String gender
    Duck : +String beakColor
    Duck : +swim()
`,
  },
  {
    name: 'state',
    source: `stateDiagram-v2
    [*] --> Still
    Still --> [*]
    Still --> Moving
    Moving --> Still
    Moving --> Crash
    Crash --> [*]
`,
  },
];

let ok = 0,
  fail = 0;
for (const c of cases) {
  try {
    const { svg } = await mermaid.render(`probe-${c.name}-1`, c.source);
    console.log(`[probe] ${c.name}: ${svg.length} bytes`);
    ok++;
  } catch (err) {
    console.error(`[probe] ${c.name}: FAILED — ${err.message}`);
    fail++;
  }
}
console.log(`[probe] summary: ${ok} ok, ${fail} fail`);
process.exit(fail === 0 ? 0 : 1);
