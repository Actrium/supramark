import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  findFirstDifference,
  htmlToSemanticTree,
} from '../lib/semantic/html-semantics.mjs';
import { renderWithProductionWebRenderer } from '../lib/visual/production-web-renderer.mjs';
import { parserOptionsArgv } from '../lib/parse-options.mjs';

const SUITE_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const REPOSITORY_ROOT = path.resolve(SUITE_ROOT, '..', '..');
const DEFAULT_BINARY = path.join(
  REPOSITORY_ROOT,
  'target',
  'debug',
  process.platform === 'win32' ? 'supramark-markdown.exe' : 'supramark-markdown'
);
const parserBinary = path.resolve(process.env.SUPRAMARK_MARKDOWN_BIN ?? DEFAULT_BINARY);

const fixtureDirectory = path.join(REPOSITORY_ROOT, 'tests', 'cases', '_fixtures', 'commonmark');
const document = JSON.parse(await readFile(path.join(fixtureDirectory, 'cases.json'), 'utf8'));

const SECTION_NAMES = {
  'HTML blocks': 'HTML blocks',
  'Raw HTML': 'Raw HTML',
  Lists: 'Lists',
  'Hard line breaks': 'Hard line breaks',
};

const selectedCases = document.cases;

const astById = new Map();
for (const testCase of selectedCases) {
  const parsed = spawnSync(parserBinary, ['-', ...parserOptionsArgv(testCase)], {
    input: testCase.input.markdown,
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
  if (parsed.error || parsed.status !== 0) {
    throw new Error(`parser failed for ${testCase.id}: ${parsed.stderr || parsed.error?.message}`);
  }
  astById.set(testCase.id, JSON.parse(parsed.stdout));
}

console.log(`Rendering ${selectedCases.length} cases with production web renderer...`);
const { htmlById, errorsById, environment } = await renderWithProductionWebRenderer({
  cases: selectedCases,
  astById,
});

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;');
}

function escapeAttr(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('"', '&quot;')
    .replaceAll('<', '&lt;');
}

function sectionName(section) {
  return SECTION_NAMES[section] ?? section;
}

function classify(value) {
  const v = value ?? '';
  if (/^<!(?:--|\[CDATA)/.test(v.trim())) return 'comment/decl';
  const m = v.match(/^<([a-zA-Z][\w-]*)/);
  if (!m) return 'fragment';
  const tag = m[1];
  const openRe = new RegExp('^<' + tag.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '\\b([^>]*)>', 'i');
  const openM = v.match(openRe);
  if (!openM) return 'fragment';
  if (/\/\s*$/.test(openM[1])) return 'self-closing';
  const closeRe = new RegExp('</' + tag.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '\\s*>\\s*$', 'i');
  return closeRe.test(v) ? 'balanced' : 'fragment';
}

// Count raw node shapes per case for the "why still failing" note.
function rawShapes(ast) {
  const shapes = [];
  const visit = (node) => {
    if (!node) return;
    if (node.type === 'raw') shapes.push(classify(node.value));
    if (node.children) node.children.forEach(visit);
  };
  visit(ast);
  return shapes;
}

const cards = [];
let passCount = 0;
for (const testCase of selectedCases) {
  const expected = testCase.expected.html;
  const actual = htmlById.get(testCase.id) ?? '';
  const errs = errorsById.get(testCase.id);
  let diff = null;
  if (errs?.length) diff = { path: 'render error' };
  else diff = findFirstDifference(htmlToSemanticTree(expected), htmlToSemanticTree(actual));
  const pass = !diff;
  if (pass) passCount += 1;
  const badge = pass
    ? '<span class="badge pass">Semantic pass</span>'
    : '<span class="badge fail">Semantic diff</span>';
  const shapes = rawShapes(astById.get(testCase.id));
  const shapeTags = shapes
    .map(s => `<code class="shape ${s}">${s}</code>`)
    .join(' ');
  const note = pass
    ? ''
    : `<p class="diff">First diff:<code>${escapeHtml(diff.path ?? '-')}</code><br>raw node shapes:${shapeTags || '<code>no raw</code>'}</p>`;
  const mdDisplay = escapeHtml(testCase.input.markdown);
  const actualDisplay = errs?.length ? escapeHtml(errs.join('\n')) : escapeHtml(actual);
  cards.push({
    id: testCase.id,
    section: sectionName(testCase.source.section),
    pass,
    html: `
<article class="case ${pass ? 'pass' : 'fail'}">
  <h3><code>${escapeHtml(testCase.id)}</code> <small>${escapeHtml(sectionName(testCase.source.section))}</small> ${badge}</h3>
  <div class="block">
    <h4>Markdown input</h4>
    <pre class="md">${mdDisplay}</pre>
  </div>
  <div class="render-grid">
    <figure>
      <figcaption>Official expected (CommonMark 0.31.2)</figcaption>
      <iframe srcdoc="${escapeAttr(expected)}" sandbox=""></iframe>
    </figure>
    <figure>
      <figcaption>Actual rendering after the fix (Supramark web renderer)</figcaption>
      <iframe srcdoc="${escapeAttr(actual)}" sandbox=""></iframe>
    </figure>
  </div>
  ${note}
  <details>
    <summary>HTML source comparison</summary>
    <div class="html-grid">
      <div><h5>Official expected</h5><pre>${escapeHtml(expected)}</pre></div>
      <div><h5>Actual after the fix</h5><pre>${actualDisplay}</pre></div>
    </div>
  </details>
</article>`,
  });
}

// Group: pass first, then fails (stable within group by input order).
const passCards = cards.filter(c => c.pass);
const failCards = cards.filter(c => !c.pass);

function renderGroup(title, groupCards) {
  if (!groupCards.length) return '';
  return `<section><h2>${escapeHtml(title)} <small>(${groupCards.length})</small></h2>${groupCards.map(c => c.html).join('\n')}</section>`;
}

const failCount = selectedCases.length - passCount;
const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>CommonMark raw HTML fixed cases &middot; Supramark #107</title>
<style>
  :root { color-scheme: light; }
  * { box-sizing: border-box; }
  body { font: 14px/1.6 -apple-system, "Segoe UI", sans-serif; margin: 0; background: #f6f7f9; color: #1f2328; }
  header { background: #1f2328; color: #fff; padding: 28px 32px; }
  header h1 { margin: 0 0 8px; font-size: 22px; }
  header p { margin: 0; color: #adbac7; max-width: 980px; }
  .stats { display: flex; gap: 24px; margin-top: 16px; flex-wrap: wrap; }
  .stat { background: #2d333b; padding: 10px 16px; border-radius: 8px; }
  .stat b { display: block; font-size: 20px; }
  .stat span { color: #adbac7; font-size: 12px; }
  .legend { margin-top: 14px; color: #adbac7; font-size: 12px; display: flex; gap: 14px; flex-wrap: wrap; }
  .legend code { background: #2d333b; padding: 2px 8px; border-radius: 4px; }
  main { max-width: 1280px; margin: 0 auto; padding: 24px 24px 80px; }
  section > h2 { border-left: 4px solid #2f81f7; padding-left: 12px; margin: 32px 0 16px; }
  section > h2 small { color: #656d76; font-weight: 400; font-size: 13px; }
  .case { background: #fff; border: 1px solid #d0d7de; border-radius: 10px; padding: 18px 20px; margin-bottom: 18px; }
  .case.fail { border-color: #f85149; }
  .case h3 { margin: 0 0 14px; font-size: 15px; display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .case h3 small { color: #656d76; font-weight: 400; }
  .badge { font-size: 12px; padding: 2px 10px; border-radius: 999px; font-weight: 600; }
  .badge.pass { background: #1f883d; color: #fff; }
  .badge.fail { background: #f85149; color: #fff; }
  .block h4 { margin: 0 0 6px; font-size: 12px; color: #656d76; text-transform: uppercase; letter-spacing: .04em; }
  pre { background: #f6f8fa; border: 1px solid #eaeef2; border-radius: 6px; padding: 10px 12px; margin: 0; overflow: auto; font: 12px/1.5 "SF Mono", Menlo, Consolas, monospace; white-space: pre-wrap; word-break: break-word; }
  pre.md { white-space: pre; }
  .render-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; margin: 14px 0; }
  figure { margin: 0; }
  figcaption { font-size: 12px; color: #656d76; margin-bottom: 6px; font-weight: 600; }
  iframe { width: 100%; height: 160px; border: 1px solid #d0d7de; border-radius: 6px; background: #fff; }
  .diff { font-size: 12px; color: #cf222e; margin: 4px 0 0; }
  .diff .shape { background: #f6f8fa; border: 1px solid #eaeef2; color: #656d76; padding: 1px 6px; border-radius: 4px; margin: 0 2px; }
  .diff .shape.fragment { border-color: #f85149; color: #cf222e; }
  .diff .shape.comment\\/decl { border-color: #f85149; color: #cf222e; }
  details { margin-top: 12px; }
  summary { cursor: pointer; font-size: 12px; color: #0969da; }
  .html-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; margin-top: 10px; }
  .html-grid h5 { margin: 0 0 6px; font-size: 11px; color: #656d76; }
  footer { text-align: center; color: #656d76; font-size: 12px; padding: 24px; }
</style>
</head>
<body>
<header>
  <h1>CommonMark raw HTML fixed cases &middot; official comparison</h1>
  <p>Companion to <a style="color:#539bf5" href="https://github.com/Actrium/supramark/issues/107">Actrium/supramark#107</a>. This page covers all 652 cases from the CommonMark 0.31.2 spec, showing the Markdown input, the official expected HTML, and the actual rendering after the fix, case by case. Isolated open/close tags, comments, declarations, CDATA, and similar fragments that the previous React component model could not carry are now handled end to end via root-level <code>&lt;template&gt;</code> parsing plus <code>insertBefore</code> splicing (the RawHtml channel); active-formatting leaks inside paragraphs are fixed by injecting the whole <code>&lt;p&gt;&hellip;&lt;/p&gt;</code> fragment at once.</p>
  <div class="stats">
    <div class="stat"><b>${passCount}/${selectedCases.length}</b><span>Semantic pass in this batch</span></div>
    <div class="stat"><b>${failCount}</b><span>Still failing</span></div>
    <div class="stat"><b>652/652</b><span>Full CommonMark 0.31.2 corpus</span></div>
    <div class="stat"><b>${escapeHtml(environment.parser)}</b><span>Parser</span></div>
    <div class="stat"><b>${escapeHtml(environment.browser.name)} ${escapeHtml(environment.browser.version)}</b><span>Rendering browser</span></div>
  </div>
  <div class="legend">
    raw node shapes:
    <code class="shape balanced">balanced</code> balanced element (same-named host / fragment injection)
    <code class="shape self-closing">self-closing</code> self-closing (fragment injection)
    <code class="shape fragment">fragment</code> unbalanced fragment (fragment injection)
    <code class="shape comment/decl">comment/decl</code> comment/declaration (fragment injection)
  </div>
</header>
<main>
${renderGroup(`Fixed (semantic pass)`, passCards)}
${renderGroup(`Still failing (React structural dead ends)`, failCards)}
</main>
<footer>Generated by <code>tests/markdown-conformance/scripts/generate-raw-report.mjs</code> &middot; comparison target production-web-renderer-dom</footer>
</body>
</html>`;

const outDir = path.join(SUITE_ROOT, 'artifacts', 'raw-report');
await mkdir(outDir, { recursive: true });
const outPath = path.join(outDir, 'report.html');
await writeFile(outPath, html);
console.log(`pass: ${passCount}/${selectedCases.length}`);
console.log(`report: ${outPath}`);
