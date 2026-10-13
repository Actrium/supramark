import { mkdir, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const { chromium } = require(
  'C:/Users/fhink/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/node_modules/playwright-core'
);
const sharp = require(
  'C:/Users/fhink/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/sharp@0.34.5/node_modules/sharp'
);
const pixelmatch = require(
  'C:/Users/fhink/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/pixelmatch'
);
const { PNG } = require(
  'C:/Users/fhink/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/pngjs'
);

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');
const outDir = resolve(root, 'artifacts', 'mermaid-official-vs-supramark');
const OFFICIAL_MERMAID_VERSION = '11.14.0';
const CASE_FILTER = process.env.CASE_FILTER;
const ENABLE_DIRECT_MERMAID_LITTLE = process.env.DIRECT_MERMAID_LITTLE === '1';

const CASES = [
  {
    id: 'mermaid-flowchart-direction-td',
    source:
      'https://raw.githubusercontent.com/mermaid-js/mermaid/develop/packages/mermaid/src/docs/syntax/flowchart.md',
    sourceExcerpt: 'flowchart TD Start --> Stop',
    diagram: `flowchart TD
  Start --> Stop`,
    expectedTexts: ['Start', 'Stop'],
  },
  {
    id: 'mermaid-flowchart-edge-label',
    source:
      'https://raw.githubusercontent.com/mermaid-js/mermaid/develop/packages/mermaid/src/docs/syntax/flowchart.md',
    sourceExcerpt: 'flowchart LR A-->|text|B',
    diagram: `flowchart LR
  A-->|text|B`,
    expectedTexts: ['A', 'B', 'text'],
  },
  {
    id: 'mermaid-sequence-basic',
    source:
      'https://raw.githubusercontent.com/mermaid-js/mermaid/develop/packages/mermaid/src/docs/syntax/sequenceDiagram.md',
    sourceExcerpt: 'sequenceDiagram Alice->>John: Hello John, how are you?',
    diagram: `sequenceDiagram
  Alice->>John: Hello John, how are you?
  John-->>Alice: Great!`,
    expectedTexts: ['Alice', 'John', 'Hello John, how are you?', 'Great!'],
  },
  {
    id: 'mermaid-class-basic',
    source:
      'https://raw.githubusercontent.com/mermaid-js/mermaid/develop/packages/mermaid/src/docs/syntax/classDiagram.md',
    sourceExcerpt: 'classDiagram Animal <|-- Duck',
    diagram: `classDiagram
  Animal <|-- Duck
  Animal : +int age
  Animal : +isMammal()
  Duck : +String beakColor
  Duck : +swim()
  Duck : +quack()`,
    expectedTexts: ['Animal', 'Duck', 'age', 'isMammal', 'beakColor', 'swim', 'quack'],
  },
  {
    id: 'mermaid-state-basic',
    source:
      'https://raw.githubusercontent.com/mermaid-js/mermaid/develop/packages/mermaid/src/docs/syntax/stateDiagram.md',
    sourceExcerpt: 'stateDiagram-v2 [*] --> Still',
    diagram: `stateDiagram-v2
  [*] --> Still
  Still --> [*]`,
    expectedTexts: ['Still'],
  },
  {
    id: 'mermaid-pie-basic',
    source:
      'https://raw.githubusercontent.com/mermaid-js/mermaid/develop/packages/mermaid/src/docs/syntax/pie.md',
    sourceExcerpt: 'pie title Pets adopted by volunteers',
    diagram: `pie title Pets adopted by volunteers
  "Dogs" : 386
  "Cats" : 85
  "Rats" : 15`,
    expectedTexts: ['Pets adopted by volunteers', 'Dogs', 'Cats', 'Rats'],
  },
].map(testCase => ({
  ...testCase,
  markdown: `# ${testCase.id}

\`\`\`mermaid
${testCase.diagram}
\`\`\``,
})).filter(testCase => !CASE_FILTER || testCase.id.includes(CASE_FILTER));

await mkdir(outDir, { recursive: true });

const browser = await chromium.launch({
  executablePath: 'C:/Program Files/Google/Chrome/Application/chrome.exe',
});

try {
  const reports = [];
  for (const testCase of CASES) {
    console.log(`Running ${testCase.id}`);
    const reference = await renderOfficialMermaid(browser, testCase);
    const mermaidLittle = ENABLE_DIRECT_MERMAID_LITTLE
      ? await renderMermaidLittleDirect(browser, testCase)
      : skippedMermaidLittleDirect();
    const actual = await renderSupramark(browser, testCase);
    const comparison =
      reference.pngPath && actual.pngPath
        ? await comparePng(reference.pngPath, actual.pngPath, testCase.id)
        : null;

    const report = {
      case: testCase.id,
      officialSource: testCase.source,
      officialSourceExcerpt: testCase.sourceExcerpt,
      reference,
      mermaidLittle,
      actual,
      comparison,
      pass:
        reference.semantic.pass &&
        (!ENABLE_DIRECT_MERMAID_LITTLE || mermaidLittle.semantic.pass) &&
        actual.semantic.pass &&
        (comparison ? comparison.diffRatio <= comparison.threshold : false),
    };

    await writeFile(
      resolve(outDir, `${testCase.id}.report.json`),
      JSON.stringify(report, null, 2),
      'utf8'
    );
    reports.push(report);
  }

  const summary = {
    officialMermaidVersion: OFFICIAL_MERMAID_VERSION,
    supramarkMermaidLittleVersion: '11.14.0-1',
    total: reports.length,
    passed: reports.filter(r => r.pass).length,
    failed: reports.filter(r => !r.pass).length,
    reports: reports.map(r => ({
      case: r.case,
      pass: r.pass,
      semantic: {
        reference: r.reference.semantic.pass,
        mermaidLittle: r.mermaidLittle.semantic.pass,
        actual: r.actual.semantic.pass,
      },
      mermaidLittleGeometry: r.mermaidLittle.geometry,
      actualGeometry: r.actual.geometry,
      diffRatio: r.comparison?.diffRatio ?? null,
    })),
  };
  await writeFile(resolve(outDir, `summary.json`), JSON.stringify(summary, null, 2), 'utf8');
  console.log(JSON.stringify(summary, null, 2));
} finally {
  await browser.close();
}

function skippedMermaidLittleDirect() {
  return {
    renderer: '@actrium/mermaid-little-web direct CDN import',
    rendererVersion: '11.14.0-1',
    skipped: true,
    reason: 'Set DIRECT_MERMAID_LITTLE=1 to probe the package directly.',
    semantic: { pass: true, skipped: true },
    geometry: { pass: true, skipped: true },
  };
}

async function renderMermaidLittleDirect(browser, testCase) {
  const page = await browser.newPage({ viewport: { width: 900, height: 600 }, deviceScaleFactor: 1 });
  const errors = [];
  page.on('console', message => {
    if (message.type() === 'error') errors.push(message.text());
  });
  page.on('pageerror', error => errors.push(error.message));
  try {
    await page.setContent(`<!doctype html>
<html>
  <head><meta charset="utf-8" /></head>
  <body>
    <script type="module">
      import { convert } from 'https://cdn.jsdelivr.net/npm/@actrium/mermaid-little-web@11.14.0-1/dist/index.js';
      window.__SVG__ = convert(${JSON.stringify(testCase.diagram)});
    </script>
  </body>
</html>`);

    await page.waitForFunction(() => typeof window.__SVG__ === 'string', null, { timeout: 20000 });
    const svg = await page.evaluate(() => window.__SVG__);
    const svgPath = resolve(outDir, `${testCase.id}.mermaid-little.svg`);
    const pngPath = resolve(outDir, `${testCase.id}.mermaid-little.png`);
    await writeFile(svgPath, svg, 'utf8');
    await rasterizeSvg(svg, pngPath);
    return {
      renderer: '@actrium/mermaid-little-web direct CDN import',
      rendererVersion: '11.14.0-1',
      svgPath,
      pngPath,
      semantic: semanticCheck(svg, testCase.expectedTexts),
      geometry: geometryCheck(svg),
      consoleErrors: errors,
    };
  } catch (error) {
    return {
      renderer: '@actrium/mermaid-little-web direct CDN import',
      rendererVersion: '11.14.0-1',
      semantic: {
        pass: false,
        error: error instanceof Error ? error.message : String(error),
        consoleErrors: errors,
      },
      geometry: { pass: false, reason: 'direct-render-failed' },
    };
  } finally {
    await page.close();
  }
}

async function renderOfficialMermaid(browser, testCase) {
  const page = await browser.newPage({ viewport: { width: 900, height: 600 }, deviceScaleFactor: 1 });
  await page.setContent(`<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <style>
      body { margin: 0; padding: 24px; background: white; font-family: Arial, sans-serif; }
      #target { display: inline-block; }
    </style>
  </head>
  <body>
    <div id="target"></div>
    <script type="module">
      import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@${OFFICIAL_MERMAID_VERSION}/dist/mermaid.esm.min.mjs';
      mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: 'default' });
      const source = ${JSON.stringify(testCase.diagram)};
      const { svg } = await mermaid.render('official-${testCase.id}', source);
      document.getElementById('target').innerHTML = svg;
      window.__SVG__ = svg;
    </script>
  </body>
</html>`);

  await page.waitForFunction(() => typeof window.__SVG__ === 'string', null, { timeout: 60000 });
  const svg = await page.evaluate(() => window.__SVG__);
  const svgPath = resolve(outDir, `${testCase.id}.reference.svg`);
  const pngPath = resolve(outDir, `${testCase.id}.reference.png`);
  await writeFile(svgPath, svg, 'utf8');
  await rasterizeSvg(svg, pngPath);
  await page.close();

  return {
    renderer: 'Mermaid official npm package',
    rendererVersion: OFFICIAL_MERMAID_VERSION,
    svgPath,
    pngPath,
    semantic: semanticCheck(svg, testCase.expectedTexts),
    geometry: geometryCheck(svg),
  };
}

async function renderSupramark(browser, testCase) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 });
  const errors = [];
  page.on('console', message => {
    if (message.type() === 'error') errors.push(message.text());
  });
  page.on('pageerror', error => errors.push(error.message));

  await page.goto('https://actrium.github.io/supramark/playground/mermaid/', {
    waitUntil: 'networkidle',
    timeout: 90000,
  });
  await page.locator('textarea').fill(testCase.markdown);
  await page.waitForFunction(() => {
    const loading = document.querySelector('.feature-preview-loading');
    const hidden = document.querySelector('.feature-preview-render-content.is-hidden');
    const svg = document.querySelector('.feature-preview-render-content svg');
    return svg && !loading && !hidden;
  }, null, { timeout: 90000 });

  const svg = await page.locator('.feature-preview-render-content svg').first().evaluate(el => el.outerHTML);
  const svgPath = resolve(outDir, `${testCase.id}.supramark.svg`);
  const pngPath = resolve(outDir, `${testCase.id}.supramark.png`);
  await writeFile(svgPath, svg, 'utf8');
  await rasterizeSvg(svg, pngPath);
  await page.close();

  return {
    renderer: 'Supramark deployed preview',
    url: 'https://actrium.github.io/supramark/playground/mermaid/',
    svgPath,
    pngPath,
    semantic: semanticCheck(svg, testCase.expectedTexts),
    geometry: geometryCheck(svg),
    consoleErrors: errors,
  };
}

async function rasterizeSvg(svg, pngPath) {
  await sharp(Buffer.from(normalizeSvgSize(svg)))
    .flatten({ background: '#ffffff' })
    .png()
    .toFile(pngPath);
}

function normalizeSvgSize(svg) {
  const viewBox = svg.match(/\bviewBox=["']([^"']+)["']/i)?.[1];
  if (!viewBox) return svg;
  const [, , width, height] = viewBox.trim().split(/[\s,]+/).map(Number);
  if (!Number.isFinite(width) || !Number.isFinite(height)) return svg;

  let normalized = svg.replace(/\swidth=["'][^"']*["']/i, '');
  normalized = normalized.replace(/\sheight=["'][^"']*["']/i, '');
  return normalized.replace(/<svg\b/i, `<svg width="${Math.ceil(width)}" height="${Math.ceil(height)}"`);
}

function semanticCheck(svg, expectedTexts) {
  const missingTexts = expectedTexts.filter(text => !svg.includes(text));
  const hasSvg = /<svg[\s>]/i.test(svg);
  const hasViewBox = /\bviewBox=/i.test(svg);
  const hasError = /Engine not configured|unsupported_engine|render_error|Syntax error in text/i.test(svg);
  return {
    pass: hasSvg && hasViewBox && missingTexts.length === 0 && !hasError,
    hasSvg,
    hasViewBox,
    missingTexts,
    hasError,
  };
}

function geometryCheck(svg) {
  const viewBox = parseViewBox(svg);
  if (!viewBox) {
    return { pass: false, reason: 'missing-viewBox' };
  }

  const rects = [...svg.matchAll(/<g\b[^>]*class="[^"]*\bnode\b[^"]*"[^>]*transform="translate\(([^,\s)]+)[,\s]+([^)]+)\)"[\s\S]*?<rect\b([^>]*)>/gi)]
    .map(match => {
      const tx = Number(match[1]);
      const ty = Number(match[2]);
      const attrs = match[3];
      const x = Number(attrs.match(/\bx="([^"]+)"/i)?.[1] ?? 0);
      const y = Number(attrs.match(/\by="([^"]+)"/i)?.[1] ?? 0);
      const width = Number(attrs.match(/\bwidth="([^"]+)"/i)?.[1] ?? NaN);
      const height = Number(attrs.match(/\bheight="([^"]+)"/i)?.[1] ?? NaN);
      if (![tx, ty, x, y, width, height].every(Number.isFinite)) return null;
      return {
        left: tx + x,
        top: ty + y,
        right: tx + x + width,
        bottom: ty + y + height,
      };
    })
    .filter(Boolean);

  if (rects.length === 0) {
    return { pass: true, reason: 'no-node-rects-detected', viewBox, checkedRects: 0 };
  }

  const view = {
    left: viewBox.x,
    top: viewBox.y,
    right: viewBox.x + viewBox.width,
    bottom: viewBox.y + viewBox.height,
  };
  const overflowingRects = rects.filter(
    rect =>
      rect.left < view.left - 0.5 ||
      rect.top < view.top - 0.5 ||
      rect.right > view.right + 0.5 ||
      rect.bottom > view.bottom + 0.5
  );

  return {
    pass: overflowingRects.length === 0,
    viewBox,
    checkedRects: rects.length,
    overflowingRects,
  };
}

function parseViewBox(svg) {
  const raw = svg.match(/\bviewBox=["']([^"']+)["']/i)?.[1];
  if (!raw) return null;
  const [x, y, width, height] = raw.trim().split(/[\s,]+/).map(Number);
  if (![x, y, width, height].every(Number.isFinite)) return null;
  return { x, y, width, height };
}

async function comparePng(referencePath, actualPath, caseId) {
  const referenceMeta = await sharp(referencePath).metadata();
  const actualMeta = await sharp(actualPath).metadata();
  const width = Math.max(referenceMeta.width ?? 0, actualMeta.width ?? 0);
  const height = Math.max(referenceMeta.height ?? 0, actualMeta.height ?? 0);
  const background = { r: 255, g: 255, b: 255, alpha: 1 };

  const referenceBuffer = await sharp(referencePath)
    .resize({ width, height, fit: 'contain', background })
    .ensureAlpha()
    .png()
    .toBuffer();
  const actualBuffer = await sharp(actualPath)
    .resize({ width, height, fit: 'contain', background })
    .ensureAlpha()
    .png()
    .toBuffer();

  const referencePng = PNG.sync.read(referenceBuffer);
  const actualPng = PNG.sync.read(actualBuffer);
  const diff = new PNG({ width, height });
  const matchPixels = pixelmatch.default ?? pixelmatch;
  const diffPixels = matchPixels(
    referencePng.data,
    actualPng.data,
    diff.data,
    width,
    height,
    { threshold: 0.1 }
  );
  const diffPath = resolve(outDir, `${caseId}.diff.png`);
  await writeFile(diffPath, PNG.sync.write(diff));

  return {
    width,
    height,
    diffPixels,
    totalPixels: width * height,
    diffRatio: diffPixels / (width * height),
    threshold: 0.005,
    diffPath,
  };
}
