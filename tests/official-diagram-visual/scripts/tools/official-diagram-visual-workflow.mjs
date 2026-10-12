import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, extname, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const codexNodeModules = 'C:/Users/fhink/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules';
const { chromium } = requireFirst([
  'playwright',
  'playwright-core',
  `${codexNodeModules}/.pnpm/node_modules/playwright-core`,
]);
const sharp = requireFirst([
  'sharp',
  `${codexNodeModules}/.pnpm/sharp@0.34.5/node_modules/sharp`,
]);
const pixelmatch = requireFirst([
  'pixelmatch',
  `${codexNodeModules}/pixelmatch`,
]);
const { PNG } = requireFirst([
  'pngjs',
  `${codexNodeModules}/pngjs`,
]);

const __dirname = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = resolve(__dirname, '..', '..');
const docsRoot = resolve(workspaceRoot, 'cases');
const outDir = resolve(workspaceRoot, 'artifacts', 'official-diagram-visual-workflow');
const repoPathPrefix = (process.env.REPO_PATH_PREFIX || 'tests/official-diagram-visual')
  .replace(/^\/+|\/+$/g, '');

const DEFAULT_DOCS = [
  resolve(docsRoot, 'official-diagram-rendering-cases.md'),
  resolve(docsRoot, 'official-diagram-rendering-cases-v2.md'),
];
const DEFAULT_CASE_IDS = [
  'mermaid-flowchart-decision',
  'graphviz-fsm',
  'echarts-line-simple',
];

const sourceDocs = (process.env.SOURCE_DOCS || '')
  .split(';')
  .map(s => s.trim())
  .filter(Boolean)
  .map(p => resolve(workspaceRoot, p));
const caseIdsEnv = process.env.CASE_IDS || DEFAULT_CASE_IDS.join(',');
const selectedIds = caseIdsEnv.toLowerCase() === 'all'
  ? []
  : caseIdsEnv
      .split(',')
      .map(s => s.trim())
      .filter(Boolean);
const caseLimit = Number(process.env.CASE_LIMIT || 0);
const targetBaseUrl = process.env.SUPRAMARK_URL || 'https://actrium.github.io/supramark/playground/';
const chromePath = process.env.CHROME_PATH || 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const visualPassThreshold = parseRatioEnv('VISUAL_PASS_THRESHOLD', 0.16);
const visualFailThreshold = parseRatioEnv('VISUAL_FAIL_THRESHOLD', 0.30);
const perceptualSimilarThreshold = parseRatioEnv('PERCEPTUAL_SIMILAR_THRESHOLD', 0.08);
const severeSizeDeltaThreshold = parseRatioEnv('SEVERE_SIZE_DELTA_THRESHOLD', 0.80);
const geometryAspectRatioDeltaThreshold = parseRatioEnv('GEOMETRY_ASPECT_RATIO_DELTA_THRESHOLD', 0.25);
const playwrightHeadless = parseBoolEnv('PLAYWRIGHT_HEADLESS', process.env.CI === 'true');
const viewport = parseViewportEnv(
  'PLAYWRIGHT_VIEWPORT',
  process.env.CI === 'true' ? { width: 1280, height: 900 } : { width: 2048, height: 1096 }
);
const deviceScaleFactor = parseNumberEnv('PLAYWRIGHT_DEVICE_SCALE_FACTOR', 1);
const previewWidth = parseNumberEnv('SUPRAMARK_PREVIEW_WIDTH', 540);

const featureByLanguage = new Map([
  ['mermaid', 'mermaid'],
  ['d2', 'd2'],
  ['plantuml', 'plantuml'],
  ['dot', 'diagram-dot'],
  ['graphviz', 'diagram-dot'],
  ['echarts', 'diagram-echarts'],
  ['vega-lite', 'diagram-vega-lite'],
]);

if (process.env.OFFICIAL_DIAGRAM_VISUAL_SELF_TEST === '1') {
  runSelfTests();
  process.exit(0);
}

await mkdir(outDir, { recursive: true });
await mkdir(resolve(outDir, 'expected'), { recursive: true });
await mkdir(resolve(outDir, 'actual'), { recursive: true });
await mkdir(resolve(outDir, 'diff'), { recursive: true });
await mkdir(resolve(outDir, 'normalized'), { recursive: true });
await mkdir(resolve(outDir, 'issues'), { recursive: true });

const runStartedAt = new Date().toISOString();
const cases = await loadCases(sourceDocs.length ? sourceDocs : DEFAULT_DOCS);
const runnableCases = cases
  .filter(testCase => selectedIds.length === 0 || selectedIds.includes(testCase.id))
  .slice(0, caseLimit > 0 ? caseLimit : undefined);

if (runnableCases.length === 0) {
  throw new Error(`No cases selected. Known ids include: ${cases.slice(0, 20).map(c => c.id).join(', ')}`);
}

const launchOptions = existsSync(chromePath)
  ? { executablePath: chromePath, headless: playwrightHeadless, args: ['--start-maximized'] }
  : { headless: playwrightHeadless, args: ['--start-maximized'] };
const browser = await chromium.launch(launchOptions);
const runEnvironment = {
  playwright: {
    headless: playwrightHeadless,
    viewport,
    deviceScaleFactor,
    executablePath: existsSync(chromePath) ? chromePath : 'playwright-default',
    browserVersion: browser.version(),
  },
  targetBaseUrl,
  previewWidth,
};

const reports = [];
const page = await browser.newPage({ viewport, deviceScaleFactor });
try {
  const initialFeature = runnableCases
    .map(testCase => featureByLanguage.get(testCase.language))
    .find(Boolean);
  const initialUrl = buildInitialUrl(initialFeature);
  await page.goto(initialUrl, { waitUntil: 'networkidle', timeout: 90000 });
  for (const testCase of runnableCases) {
    console.log(`Running ${testCase.id}`);
    const report = await runCase(page, browser, testCase);
    reports.push(report);
    await writeFile(
      resolve(outDir, `${testCase.id}.report.json`),
      JSON.stringify(report, null, 2),
      'utf8'
    );
  }
} finally {
  await page.close().catch(() => {});
  await browser.close();
}

const summary = {
  runStartedAt,
  runFinishedAt: new Date().toISOString(),
  targetBaseUrl,
  runEnvironment,
  visualThresholds: {
    pass: visualPassThreshold,
    fail: visualFailThreshold,
    perceptualSimilar: perceptualSimilarThreshold,
    severeSizeDelta: severeSizeDeltaThreshold,
    geometryAspectRatioDelta: geometryAspectRatioDeltaThreshold,
  },
  total: reports.length,
  passed: reports.filter(r => r.status === 'pass').length,
  review: reports.filter(r => r.status === 'review').length,
  failed: reports.filter(r => r.status === 'fail').length,
  reports: reports.map(r => ({
    id: r.id,
    language: r.language,
    feature: r.feature,
    selectedFeature: r.selectedFeature ?? null,
    selectedExample: r.selectedExample ?? null,
    status: r.status,
    diffRatio: r.visual?.diffRatio ?? null,
    rawDiffRatio: r.visual?.raw?.diffRatio ?? null,
    visualBand: r.visual?.band ?? null,
    pixelBand: r.visual?.pixelBand ?? null,
    severeSizeMismatch: r.visual?.severeSizeMismatch ?? null,
    perceptualDistanceRatio: r.visual?.perceptual?.distanceRatio ?? null,
    geometryAspectRatioDelta: r.geometry?.aspectRatioDelta ?? null,
    geometryAspectRatioMismatch: r.geometry?.aspectRatioMismatch ?? null,
    captureMethod: r.actual?.captureMethod ?? null,
    expectedSize: r.visual?.expectedSize ?? null,
    actualSize: r.visual?.actualSize ?? null,
    sizeDelta: r.visual?.sizeDelta ?? null,
    semanticPass: r.semantic.pass,
    geometryPass: r.geometry.pass,
    errors: r.errors,
  })),
};

const issueResults = await handleIssues(reports);
summary.issues = issueResults;

await writeFile(resolve(outDir, 'summary.json'), JSON.stringify(summary, null, 2), 'utf8');
await writeFile(resolve(outDir, 'report.html'), renderHtmlReport(reports, summary), 'utf8');
await writeFile(
  resolve(outDir, 'CURRENT_RUN_ARTIFACTS.json'),
  JSON.stringify(renderCurrentRunArtifacts(reports, summary), null, 2),
  'utf8'
);
console.log(JSON.stringify(summary, null, 2));
if (process.env.FAIL_ON_FAILURES === '1' && summary.failed > 0) {
  process.exitCode = 1;
}

function requireFirst(moduleIds) {
  const errors = [];
  for (const moduleId of moduleIds) {
    try {
      return require(moduleId);
    } catch (error) {
      errors.push(`${moduleId}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  throw new Error(`Unable to load dependency. Tried:\n${errors.join('\n')}`);
}

function parseRatioEnv(name, fallback) {
  const raw = process.env[name];
  if (!raw) return fallback;
  const normalized = raw.trim().endsWith('%')
    ? Number(raw.trim().slice(0, -1)) / 100
    : Number(raw);
  return Number.isFinite(normalized) && normalized >= 0 ? normalized : fallback;
}

function parseBoolEnv(name, fallback) {
  const raw = process.env[name];
  if (raw == null || raw === '') return fallback;
  return /^(1|true|yes|on)$/i.test(raw.trim());
}

function parseNumberEnv(name, fallback) {
  const raw = process.env[name];
  if (!raw) return fallback;
  const value = Number(raw);
  return Number.isFinite(value) && value > 0 ? value : fallback;
}

function delay(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function parseViewportEnv(name, fallback) {
  const raw = process.env[name];
  if (!raw) return fallback;
  const match = raw.trim().match(/^(\d+)\s*[x,]\s*(\d+)$/i);
  if (!match) return fallback;
  const width = Number(match[1]);
  const height = Number(match[2]);
  if (!Number.isFinite(width) || !Number.isFinite(height) || width < 1 || height < 1) {
    return fallback;
  }
  return { width, height };
}

async function loadCases(files) {
  const allCases = [];
  for (const file of files) {
    if (!existsSync(file)) continue;
    const markdown = await readFile(file, 'utf8');
    const caseMatches = [...markdown.matchAll(/^####\s+([^:\n]+):\s*(.+)$/gm)];
    for (let i = 0; i < caseMatches.length; i += 1) {
      const match = caseMatches[i];
      const id = match[1].trim();
      const title = match[2].trim();
      const start = match.index ?? 0;
      const end = i + 1 < caseMatches.length ? caseMatches[i + 1].index ?? markdown.length : markdown.length;
      const section = markdown.slice(start, end);
      const beforeCase = markdown.slice(0, start);
      const typeMatch = [...beforeCase.matchAll(/^###\s+(.+)$/gm)].at(-1);
      const caseType = typeMatch?.[1]?.trim() ?? '';
      const codeMatch = section.match(/````markdown\s*```([a-zA-Z0-9_-]+)\s*\n([\s\S]*?)\n```\s*````/);
      const imageMatch = section.match(/!\[[^\]]+\]\((assets\/[^)]+)\)/);
      const checksMatch = section.match(/\u5efa\u8bae\u68c0\u67e5\u6587\u672c\uff1a(.+)/); // "recommended check text:" header in cases/*.md (out of scope, stays Chinese)
      const officialSourceMatch = section.match(/\u5b98\u65b9\u6765\u6e90\uff1a([^\n]+)/); // "official source:" header in cases/*.md (out of scope, stays Chinese)
      const officialRenderMatch = section.match(/\u5b98\u65b9\u6e32\u67d3 URL\uff1a([^\n]+)/); // "official render URL:" header in cases/*.md (out of scope, stays Chinese)
      if (!codeMatch || !imageMatch) continue;

      const language = codeMatch[1].trim();
      const code = codeMatch[2].trimEnd();
      const imagePath = resolve(dirname(file), imageMatch[1]);
      const imageSvg = extname(imagePath).toLowerCase() === '.svg'
        ? await readFile(imagePath, 'utf8').catch(() => '')
        : '';
      const expectedTexts = checksMatch
        ? [...checksMatch[1].matchAll(/`([^`]+)`/g)].map(m => m[1])
        : [];
      allCases.push({
        id,
        title,
        language,
        code,
        markdown: `\`\`\`${language}\n${code}\n\`\`\``,
        imagePath,
        imageSvg,
        imageRef: imageMatch[1],
        officialSource: officialSourceMatch?.[1]?.trim() ?? '',
        officialRenderUrl: officialRenderMatch?.[1]?.trim() ?? '',
        expectedTexts,
        caseType,
        docPath: file,
      });
    }
  }
  return allCases;
}

async function runCase(page, browser, testCase) {
  const feature = featureByLanguage.get(testCase.language);
  if (!feature) {
    return {
      id: testCase.id,
      title: testCase.title,
      language: testCase.language,
      ...caseSourceFields(testCase),
      status: 'fail',
      errors: [`No preview feature mapping for language: ${testCase.language}`],
      semantic: { pass: false },
      geometry: { pass: false },
    };
  }

  const consoleErrors = [];
  const onConsole = message => {
    if (message.type() === 'error') consoleErrors.push(message.text());
  };
  const onPageError = error => consoleErrors.push(error.message);
  page.on('console', onConsole);
  page.on('pageerror', onPageError);

  try {
    const url = buildPlaygroundUrl(feature);
    await page.goto(url, { waitUntil: 'networkidle', timeout: 90000 });
    const preview = page;
    const selectedFeature = await selectFeature(preview, feature);
    await delay(200);
    const selectedExample = await selectExampleType(preview, testCase.caseType);
    const selectedPreviewWidth = await setPreviewWidth(preview, previewWidth);
    const beforeRenderHtml = await preview
      .locator('.feature-preview-render-content')
      .first()
      .evaluate(element => element.innerHTML)
      .catch(() => '');
    const editor = preview.locator('textarea').first();
    await editor.waitFor({ timeout: 30000 });
    await editor.fill(testCase.markdown);
    await waitForRenderRefresh(preview, beforeRenderHtml);
    await waitForRender(preview);

    const probe = await preview.evaluate(() => {
      const content = document.querySelector('.feature-preview-render-content');
      const diagram = content?.querySelector('[data-supramark-diagram]');
      const svg = diagram?.querySelector('svg') ?? content?.querySelector('svg');
      const canvas = diagram?.querySelector('canvas') ?? content?.querySelector('canvas');
      const canvasDataUrl = canvas
        ? (() => {
            try {
              return canvas.toDataURL('image/png');
            } catch {
              return '';
            }
          })()
        : '';
      const errors = [...document.querySelectorAll('.feature-preview-render-content, body')]
        .map(el => el.textContent || '')
        .join('\n')
        // cjk-allow: \u6e32\u67d3\u5931\u8d25 means "render failed", a Chinese
        // error string that some renderer/feature may still emit into the
        // DOM at runtime (out of scope here); kept as Unicode escapes.
        .match(/Engine not configured|unsupported_engine|render_error|Syntax error|Error:|\u6e32\u67d3\u5931\u8d25|Failed to resolve module specifier|Failed to fetch dynamically imported module/gi);
      const rect = (diagram ?? svg ?? canvas ?? content)?.getBoundingClientRect();
      return {
        html: diagram?.outerHTML ?? svg?.outerHTML ?? canvas?.outerHTML ?? '',
        svg: svg?.outerHTML ?? '',
        canvasDataUrl,
        text: content?.textContent ?? '',
        rect: rect
          ? { x: rect.x, y: rect.y, width: rect.width, height: rect.height, top: rect.top, left: rect.left }
          : null,
        hasSvg: Boolean(svg),
        hasCanvas: Boolean(canvas),
        errors: errors ? [...new Set(errors)] : [],
      };
    });

    const actualSvgPath = resolve(outDir, 'actual', `${testCase.id}.svg`);
    const actualPngPath = resolve(outDir, 'actual', `${testCase.id}.png`);
    const actualPreviewPath = resolve(outDir, 'actual', `${testCase.id}.preview.png`);
    const actualErrorScreenshotPath = resolve(outDir, 'actual', `${testCase.id}.error.png`);
    const expectedPngPath = resolve(outDir, 'expected', `${testCase.id}.png`);

    if (probe.svg) {
      await writeFile(actualSvgPath, probe.svg, 'utf8');
    }

    const errors = [...new Set([...consoleErrors, ...probe.errors])];
    const semantic = semanticCheck(probe, testCase.expectedTexts);
    const hasRenderableOutput = Boolean(probe.hasSvg || probe.hasCanvas);

    if (errors.length > 0 || !hasRenderableOutput) {
      const errorLocator = preview.locator('.feature-preview-render-content').first();
      if (await errorLocator.count()) {
        await errorLocator.screenshot({ path: actualErrorScreenshotPath });
      }
      const geometry = geometryCheck(probe.svg, probe.rect, testCase.imageSvg);
      return {
        id: testCase.id,
        title: testCase.title,
        language: testCase.language,
        ...caseSourceFields(testCase),
        feature,
        selectedFeature,
        selectedExample,
        selectedPreviewWidth,
        url,
        runEnvironment,
        status: 'fail',
        docPath: relative(workspaceRoot, testCase.docPath),
        expected: {
          sourceSvg: relative(workspaceRoot, testCase.imagePath),
          pngPath: null,
          officialSource: testCase.officialSource,
          officialRenderUrl: testCase.officialRenderUrl,
        },
        actual: {
          svgPath: probe.svg ? relative(workspaceRoot, actualSvgPath) : null,
          pngPath: null,
          screenshotPath: existsSync(actualErrorScreenshotPath)
            ? relative(workspaceRoot, actualErrorScreenshotPath)
            : null,
        },
        semantic,
        geometry,
        visual: null,
        errors: errors.length > 0 ? errors : ['No renderable diagram was produced'],
      };
    }

    const actualCapture = await captureRenderedOutput(preview, browser, probe, actualPngPath);
    await captureVisiblePreview(preview, actualPreviewPath);

    await rasterizeExpectedFile(browser, testCase.imagePath, expectedPngPath);

    const visual = existsSync(actualPngPath)
      ? await comparePng(expectedPngPath, actualPngPath, testCase.id)
      : null;
    const geometry = geometryCheck(probe.svg, probe.rect, testCase.imageSvg);
    const status = classify({ semantic, geometry, visual, errors });

    return {
      id: testCase.id,
      title: testCase.title,
      language: testCase.language,
      ...caseSourceFields(testCase),
      feature,
      selectedFeature,
      selectedExample,
      selectedPreviewWidth,
      url,
      runEnvironment,
      status,
      docPath: relative(workspaceRoot, testCase.docPath),
      expected: {
        sourceSvg: relative(workspaceRoot, testCase.imagePath),
        pngPath: relative(workspaceRoot, expectedPngPath),
        officialSource: testCase.officialSource,
        officialRenderUrl: testCase.officialRenderUrl,
      },
      actual: {
        svgPath: probe.svg ? relative(workspaceRoot, actualSvgPath) : null,
        pngPath: existsSync(actualPngPath) ? relative(workspaceRoot, actualPngPath) : null,
        screenshotPath: existsSync(actualPngPath) ? relative(workspaceRoot, actualPngPath) : null,
        previewPath: existsSync(actualPreviewPath) ? relative(workspaceRoot, actualPreviewPath) : null,
        captureMethod: actualCapture.method,
      },
      semantic,
      geometry,
      visual,
      errors,
    };
  } catch (error) {
    return {
      id: testCase.id,
      title: testCase.title,
      language: testCase.language,
      ...caseSourceFields(testCase),
      feature,
      status: 'fail',
      semantic: { pass: false },
      geometry: { pass: false },
      errors: [
        error instanceof Error ? error.message : String(error),
        ...consoleErrors,
      ],
    };
  } finally {
    page.off('console', onConsole);
    page.off('pageerror', onPageError);
  }
}

function buildInitialUrl(feature) {
  return feature ? buildPlaygroundUrl(feature) : targetBaseUrl;
}

function buildPlaygroundUrl(feature) {
  const routeByFeature = {
    'diagram-dot': 'dot',
    'diagram-echarts': 'echarts',
    'diagram-vega-lite': 'vega-lite',
  };
  const route = routeByFeature[feature] ?? feature;
  return new URL(`${route}/`, ensureTrailingSlash(targetBaseUrl)).toString();
}

function ensureTrailingSlash(value) {
  return value.endsWith('/') ? value : `${value}/`;
}

function caseSourceFields(testCase) {
  return {
    code: testCase.code ?? '',
    markdown: testCase.markdown ?? '',
  };
}

async function selectFeature(page, feature) {
  const featureSelect = page.locator('select').first();
  await featureSelect.waitFor({ timeout: 30000 });

  const options = await featureSelect.locator('option').evaluateAll(nodes =>
    nodes.map(option => ({
      value: option.value,
      label: option.textContent?.trim() ?? '',
    }))
  );
  if (!options.some(option => option.value === feature)) {
    throw new Error(
      `Preview feature dropdown does not contain "${feature}". Available options: ` +
        options.map(option => `${option.label}=${option.value}`).join(', ')
    );
  }

  await featureSelect.selectOption(feature);
  await page.waitForFunction(
    expected => document.querySelector('select')?.value === expected,
    feature,
    { timeout: 30000 }
  );
  return featureSelect.locator('option:checked').textContent();
}

async function setPreviewWidth(page, width) {
  const range = page.locator('input[type="range"]').first();
  if (!(await range.count())) return null;
  const target = String(Math.round(width));
  await range.evaluate((input, value) => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
    setter?.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
  }, target);
  await page.waitForFunction(
    expected => document.querySelector('input[type="range"]')?.value === String(expected),
    target,
    { timeout: 30000 }
  ).catch(() => {});
  await page.waitForTimeout(200);
  return Number(await range.inputValue());
}

async function selectExampleType(page, caseType) {
  if (!caseType) return null;

  await page.waitForFunction(
    () => document.querySelectorAll('select').length >= 2,
    null,
    { timeout: 30000 }
  ).catch(() => {});

  const selects = page.locator('select');
  const count = await selects.count();
  if (count < 2) return null;

  const exampleSelect = selects.nth(1);
  const options = await exampleSelect.locator('option').evaluateAll(nodes =>
    nodes.map(option => ({
      value: option.value,
      label: option.textContent?.trim() ?? '',
    }))
  );
  const target = options.find(option => normalizeLabel(option.label) === normalizeLabel(caseType));
  if (!target) {
    throw new Error(
      `Example dropdown does not contain "${caseType}". Available examples: ` +
        options.map(option => option.label).join(', ')
    );
  }

  await exampleSelect.selectOption(target.value);
  await page.waitForFunction(
    expected => document.querySelectorAll('select')[1]?.value === expected,
    target.value,
    { timeout: 30000 }
  );
  return exampleSelect.locator('option:checked').textContent();
}

function normalizeLabel(value) {
  // cjk-allow: this label is diffed against live dropdown option text that
  // comes straight from the case docs (out of scope for this cleanup), so
  // it still contains Chinese. \u793a\u4f8b is the suffix meaning "example";
  // \uFF08 \uFF09 are full-width parentheses. Written as Unicode escapes,
  // not literal characters, per repo policy.
  return String(value || '')
    .replace(/\s+/g, '')
    .replace(/\u793a\u4f8b/g, '')
    .replace(/echarts/gi, '')
    .replace(/vega-?lite/gi, '')
    .replace(/[()\uFF08\uFF09]/g, '')
    .toLowerCase();
}

async function waitForRender(page) {
  await page.waitForFunction(() => {
    const content = document.querySelector('.feature-preview-render-content');
    const loading = document.querySelector('.feature-preview-loading');
    const hidden = content?.classList.contains('is-hidden');
    const diagram = content?.querySelector('[data-supramark-diagram]');
    const svg = content?.querySelector('svg');
    const canvas = content?.querySelector('canvas');
    // cjk-allow: \u6e32\u67d3\u5931\u8d25 means "render failed" (see probe() above)
    const hasError = /Engine not configured|unsupported_engine|render_error|Syntax error|Error:|\u6e32\u67d3\u5931\u8d25|Failed to resolve module specifier|Failed to fetch dynamically imported module/i.test(
      content?.textContent || ''
    );
    return content && !loading && !hidden && (diagram || svg || canvas || hasError);
  }, null, { timeout: 90000 });

  await page.waitForTimeout(800);
}

async function waitForRenderRefresh(page, previousHtml) {
  await page.waitForFunction(prev => {
    const content = document.querySelector('.feature-preview-render-content');
    const loading = document.querySelector('.feature-preview-loading');
    if (!content) return false;
    return Boolean(loading) || content.innerHTML !== prev;
  }, previousHtml, { timeout: 30000 }).catch(() => {});
}

async function rasterizeExpectedFile(browser, imagePath, pngPath) {
  if (extname(imagePath).toLowerCase() === '.svg') {
    const svg = await readFile(imagePath, 'utf8');
    await screenshotSvg(browser, svg, pngPath);
    return;
  }

  await sharp(imagePath)
    .flatten({ background: '#ffffff' })
    .png()
    .toFile(pngPath);
}

async function screenshotSvg(browser, svg, pngPath) {
  const page = await browser.newPage({ viewport, deviceScaleFactor });
  try {
    await page.setContent(
      `<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <style>
    html, body { margin: 0; padding: 0; background: #fff; }
    svg { display: block; background: #fff; }
  </style>
</head>
<body>${normalizeSvgForBrowser(svg)}</body>
</html>`,
      { waitUntil: 'load' }
    );
    const svgLocator = page.locator('svg').first();
    await svgLocator.waitFor({ timeout: 30000 });
    await page.evaluate(async () => {
      if (document.fonts?.ready) await document.fonts.ready;
    });
    await svgLocator.screenshot({ path: pngPath });
  } finally {
    await page.close();
  }
}

async function captureVisiblePreview(page, pngPath) {
  await screenshotFirstAvailable(page, [
    '.feature-preview-render',
    '.feature-preview-render-shell',
    '.feature-preview-render-content',
  ], pngPath);
}

async function captureRenderedOutput(page, browser, probe, pngPath) {
  const didScreenshot = await screenshotFirstAvailable(page, [
    '.feature-preview-render-content [data-supramark-diagram]',
    '.feature-preview-render-content [data-supramark-diagram] svg',
    '.feature-preview-render-content svg',
    '.feature-preview-render-content [data-supramark-diagram] canvas',
    '.feature-preview-render-content canvas',
  ], pngPath);
  if (didScreenshot) return { method: 'dom-screenshot' };

  if (isPngDataUrl(probe?.canvasDataUrl)) {
    await writePngDataUrl(probe.canvasDataUrl, pngPath);
    return { method: 'canvas-data-url' };
  }

  if (probe?.svg) {
    await screenshotSvg(browser, probe.svg, pngPath);
    return { method: 'svg-outerhtml' };
  }

  return { method: 'none' };
}

function actualCaptureMethod(probe) {
  if (probe?.hasSvg || probe?.hasCanvas || probe?.svg) return 'dom-screenshot';
  if (isPngDataUrl(probe?.canvasDataUrl)) return 'canvas-data-url';
  return 'none';
}

function isPngDataUrl(value) {
  return /^data:image\/png;base64,[A-Za-z0-9+/=]+$/i.test(String(value || ''));
}

async function writePngDataUrl(dataUrl, pngPath) {
  const base64 = String(dataUrl || '').match(/^data:image\/png;base64,(.+)$/i)?.[1];
  if (!base64) throw new Error('Canvas output is not a PNG data URL');
  await writeFile(pngPath, Buffer.from(base64, 'base64'));
}
async function screenshotFirstAvailable(page, selectors, pngPath) {
  for (const selector of selectors) {
    const locator = page.locator(selector).first();
    if (await locator.count()) {
      await locator.evaluate(async element => {
        element.scrollIntoView({ block: 'center', inline: 'center' });
        if (document.fonts?.ready) await document.fonts.ready;
      });
      await locator.screenshot({ path: pngPath });
      return true;
    }
  }
  return false;
}

function normalizeSvgForBrowser(svg) {
  const viewBox = svg.match(/\bviewBox=["']([^"']+)["']/i)?.[1];
  if (!viewBox) return svg;
  const [, , width, height] = viewBox.trim().split(/[\s,]+/).map(Number);
  if (!Number.isFinite(width) || !Number.isFinite(height)) return svg;

  let normalized = svg;
  if (!/\swidth=["'][^"']*["']/i.test(normalized)) {
    normalized = normalized.replace(/<svg\b/i, `<svg width="${Math.ceil(width)}"`);
  }
  if (!/\sheight=["'][^"']*["']/i.test(normalized)) {
    normalized = normalized.replace(/<svg\b/i, `<svg height="${Math.ceil(height)}"`);
  }
  return normalized;
}

function semanticCheck(probe, expectedTexts) {
  const normalizedText = normalizeText(`${probe.text}\n${probe.svg}\n${probe.html}`);
  const missingTexts = expectedTexts.filter(text => !normalizedText.includes(normalizeText(text)));
  const hasVisibleOutput = Boolean(probe.hasSvg || probe.hasCanvas);
  const hasError = probe.errors.length > 0;
  return {
    pass: hasVisibleOutput && missingTexts.length === 0 && !hasError,
    hasVisibleOutput,
    missingTexts,
    hasError,
  };
}

function normalizeText(value) {
  return String(value || '').replace(/\s+/g, ' ').trim();
}

function geometryCheck(svg, rect, expectedSvg = '') {
  if (!rect || rect.width <= 1 || rect.height <= 1) {
    return { pass: false, reason: 'empty-render-rect', rect };
  }
  if (!svg) {
    return { pass: true, reason: 'non-svg-output', rect };
  }
  const viewBox = parseViewBox(svg);
  const expectedViewBox = parseViewBox(expectedSvg);
  const aspectRatio = viewBox ? safeAspectRatio(viewBox) : null;
  const expectedAspectRatio = expectedViewBox ? safeAspectRatio(expectedViewBox) : null;
  const aspectRatioDelta = aspectRatio !== null && expectedAspectRatio !== null
    ? Math.abs(aspectRatio - expectedAspectRatio) / expectedAspectRatio
    : null;
  const aspectRatioMismatch = aspectRatioDelta !== null &&
    aspectRatioDelta >= geometryAspectRatioDeltaThreshold;
  const svgWidth = Number(svg.match(/\bwidth=["']([^"']+)["']/i)?.[1]);
  const svgHeight = Number(svg.match(/\bheight=["']([^"']+)["']/i)?.[1]);
  return {
    pass:
      rect.width > 1 &&
      rect.height > 1 &&
      (!viewBox || (viewBox.width > 1 && viewBox.height > 1)) &&
      (!Number.isFinite(svgWidth) || svgWidth > 1) &&
      (!Number.isFinite(svgHeight) || svgHeight > 1) &&
      !aspectRatioMismatch,
    rect,
    viewBox,
    expectedViewBox,
    aspectRatio,
    expectedAspectRatio,
    aspectRatioDelta,
    aspectRatioMismatch,
    aspectRatioThreshold: geometryAspectRatioDeltaThreshold,
    svgWidth: Number.isFinite(svgWidth) ? svgWidth : null,
    svgHeight: Number.isFinite(svgHeight) ? svgHeight : null,
  };
}

function parseViewBox(svg) {
  const raw = svg.match(/\bviewBox=["']([^"']+)["']/i)?.[1];
  if (!raw) return null;
  const [x, y, width, height] = raw.trim().split(/[\s,]+/).map(Number);
  if (![x, y, width, height].every(Number.isFinite)) return null;
  return { x, y, width, height };
}

function safeAspectRatio(box) {
  if (!box || box.width <= 0 || box.height <= 0) return null;
  return box.width / box.height;
}

async function comparePng(expectedPath, actualPath, caseId) {
  const expectedImage = await loadPngForCompare(expectedPath);
  const actualImage = await loadPngForCompare(actualPath);
  const raw = await compareImageBuffers({
    expectedBuffer: expectedImage.buffer,
    actualBuffer: actualImage.buffer,
    width: Math.max(expectedImage.width, actualImage.width),
    height: Math.max(expectedImage.height, actualImage.height),
    diffPath: resolve(outDir, 'diff', `${caseId}.raw.png`),
  });

  const expectedBounds = findContentBounds(expectedImage.png) ?? fullImageBounds(expectedImage);
  const actualBounds = findContentBounds(actualImage.png) ?? fullImageBounds(actualImage);
  const targetWidth = Math.max(expectedBounds.width, actualBounds.width);
  const targetHeight = Math.max(expectedBounds.height, actualBounds.height);
  const normalizedExpectedPath = resolve(outDir, 'normalized', `${caseId}.expected.png`);
  const normalizedActualPath = resolve(outDir, 'normalized', `${caseId}.actual.png`);

  await normalizeContentImage(expectedPath, expectedBounds, targetWidth, targetHeight, normalizedExpectedPath);
  await normalizeContentImage(actualPath, actualBounds, targetWidth, targetHeight, normalizedActualPath);

  const normalizedExpected = await loadPngForCompare(normalizedExpectedPath);
  const normalizedActual = await loadPngForCompare(normalizedActualPath);
  const normalizedDiffPath = resolve(outDir, 'diff', `${caseId}.png`);
  const normalized = await compareImageBuffers({
    expectedBuffer: normalizedExpected.buffer,
    actualBuffer: normalizedActual.buffer,
    width: targetWidth,
    height: targetHeight,
    diffPath: normalizedDiffPath,
  });
  const perceptual = await comparePerceptualHash(normalizedExpectedPath, normalizedActualPath);
  const sizeDelta = {
    original: sizeDeltaRatio(
      { width: expectedImage.width, height: expectedImage.height },
      { width: actualImage.width, height: actualImage.height }
    ),
    content: sizeDeltaRatio(expectedBounds, actualBounds),
  };
  const pixelBand = visualBand(normalized.diffRatio);
  const severeSizeMismatch = hasSevereSizePerceptualMismatch({ sizeDelta, perceptual, pixelBand });
  const band = visualBandWithEscalation({ pixelBand, perceptual, severeSizeMismatch });

  return {
    width: targetWidth,
    height: targetHeight,
    diffPixels: normalized.diffPixels,
    totalPixels: normalized.totalPixels,
    diffRatio: normalized.diffRatio,
    band,
    pixelBand,
    diffBounds: normalized.diffBounds,
    diffPath: relative(workspaceRoot, normalizedDiffPath),
    normalized: {
      expectedPath: relative(workspaceRoot, normalizedExpectedPath),
      actualPath: relative(workspaceRoot, normalizedActualPath),
      width: targetWidth,
      height: targetHeight,
      diffRatio: normalized.diffRatio,
      diffPixels: normalized.diffPixels,
      diffPath: relative(workspaceRoot, normalizedDiffPath),
    },
    perceptual,
    raw: {
      width: raw.width,
      height: raw.height,
      diffPixels: raw.diffPixels,
      totalPixels: raw.totalPixels,
      diffRatio: raw.diffRatio,
      diffBounds: raw.diffBounds,
      diffPath: relative(workspaceRoot, raw.diffPath),
    },
    expectedSize: {
      width: expectedImage.width,
      height: expectedImage.height,
      contentBounds: expectedBounds,
    },
    actualSize: {
      width: actualImage.width,
      height: actualImage.height,
      contentBounds: actualBounds,
    },
    sizeDelta,
    severeSizeMismatch,
    passThreshold: visualPassThreshold,
    failThreshold: visualFailThreshold,
  };
}

async function comparePerceptualHash(expectedPath, actualPath) {
  const expectedHash = await averageHash(expectedPath);
  const actualHash = await averageHash(actualPath);
  let distance = 0;
  for (let i = 0; i < expectedHash.length; i += 1) {
    if (expectedHash[i] !== actualHash[i]) distance += 1;
  }
  return {
    algorithm: 'average-hash-32',
    distance,
    total: expectedHash.length,
    distanceRatio: distance / expectedHash.length,
    similarThreshold: perceptualSimilarThreshold,
  };
}

async function averageHash(imagePath) {
  const { data } = await sharp(imagePath)
    .flatten({ background: '#ffffff' })
    .resize(32, 32, { fit: 'fill' })
    .grayscale()
    .raw()
    .toBuffer({ resolveWithObject: true });
  let sum = 0;
  for (const value of data) sum += value;
  const average = sum / data.length;
  return [...data].map(value => (value < average ? 1 : 0));
}

async function loadPngForCompare(imagePath) {
  const buffer = await sharp(imagePath)
    .flatten({ background: '#ffffff' })
    .ensureAlpha()
    .png()
    .toBuffer();
  const png = PNG.sync.read(buffer);
  return {
    buffer,
    png,
    width: png.width,
    height: png.height,
  };
}

async function compareImageBuffers({ expectedBuffer, actualBuffer, width, height, diffPath }) {
  const background = { r: 255, g: 255, b: 255, alpha: 1 };
  const expectedResized = await sharp(expectedBuffer)
    .resize({ width, height, fit: 'contain', background })
    .ensureAlpha()
    .png()
    .toBuffer();
  const actualResized = await sharp(actualBuffer)
    .resize({ width, height, fit: 'contain', background })
    .ensureAlpha()
    .png()
    .toBuffer();

  const expectedPng = PNG.sync.read(expectedResized);
  const actualPng = PNG.sync.read(actualResized);
  const diff = new PNG({ width, height });
  const matchPixels = pixelmatch.default ?? pixelmatch;
  const diffPixels = matchPixels(
    expectedPng.data,
    actualPng.data,
    diff.data,
    width,
    height,
    { threshold: 0.1 }
  );
  const diffBounds = findDiffBounds(diff);
  await writeFile(diffPath, PNG.sync.write(diff));
  const diffRatio = diffPixels / (width * height);
  return {
    width,
    height,
    diffPixels,
    totalPixels: width * height,
    diffRatio,
    diffBounds,
    diffPath,
  };
}

async function normalizeContentImage(imagePath, bounds, targetWidth, targetHeight, outPath) {
  await sharp(imagePath)
    .flatten({ background: '#ffffff' })
    .extract({
      left: bounds.x,
      top: bounds.y,
      width: bounds.width,
      height: bounds.height,
    })
    .resize({
      width: targetWidth,
      height: targetHeight,
      fit: 'fill',
    })
    .ensureAlpha()
    .png()
    .toFile(outPath);
}

function findContentBounds(png) {
  let left = png.width;
  let top = png.height;
  let right = -1;
  let bottom = -1;

  for (let y = 0; y < png.height; y += 1) {
    for (let x = 0; x < png.width; x += 1) {
      const idx = (png.width * y + x) * 4;
      const r = png.data[idx];
      const g = png.data[idx + 1];
      const b = png.data[idx + 2];
      const a = png.data[idx + 3];
      if (a > 0 && !(r >= 250 && g >= 250 && b >= 250)) {
        left = Math.min(left, x);
        top = Math.min(top, y);
        right = Math.max(right, x);
        bottom = Math.max(bottom, y);
      }
    }
  }

  if (right < left || bottom < top) return null;
  return {
    x: left,
    y: top,
    width: right - left + 1,
    height: bottom - top + 1,
    right,
    bottom,
  };
}

function fullImageBounds(image) {
  return {
    x: 0,
    y: 0,
    width: image.width,
    height: image.height,
    right: image.width - 1,
    bottom: image.height - 1,
  };
}

function sizeDeltaRatio(expected, actual) {
  const width = expected.width > 0 ? (actual.width - expected.width) / expected.width : 0;
  const height = expected.height > 0 ? (actual.height - expected.height) / expected.height : 0;
  const area = expected.width > 0 && expected.height > 0
    ? ((actual.width * actual.height) - (expected.width * expected.height)) / (expected.width * expected.height)
    : 0;
  return { width, height, area };
}

function visualBand(diffRatio) {
  if (diffRatio <= visualPassThreshold) return 'pass';
  if (diffRatio >= visualFailThreshold) return 'fail';
  return 'review';
}

function visualBandWithEscalation({ pixelBand, perceptual, severeSizeMismatch }) {
  if (severeSizeMismatch && pixelBand === 'pass') return 'review';
  if (severeSizeMismatch) return 'fail';
  if (pixelBand === 'fail' && perceptual.distanceRatio <= perceptualSimilarThreshold) {
    return 'review';
  }
  return pixelBand;
}

function hasSevereSizePerceptualMismatch({ sizeDelta, perceptual }) {
  const contentAreaDelta = Math.abs(sizeDelta?.content?.area ?? 0);
  const originalAreaDelta = Math.abs(sizeDelta?.original?.area ?? 0);
  const severeAreaDelta = Math.max(contentAreaDelta, originalAreaDelta) >= severeSizeDeltaThreshold;
  return severeAreaDelta && (perceptual?.distanceRatio ?? 0) > perceptualSimilarThreshold;
}

function runSelfTests() {
  const tests = [
    {
      name: 'downgrades severe scale-only size collapse with otherwise passing pixels to review',
      actual: visualBandWithEscalation({
        pixelBand: 'pass',
        perceptual: { distanceRatio: 0.16113 },
        severeSizeMismatch: hasSevereSizePerceptualMismatch({
          sizeDelta: {
            original: { width: -0.7925, height: -0.7982, area: -0.9581 },
            content: { width: -0.7925, height: -0.7975, area: -0.9580 },
          },
          perceptual: { distanceRatio: 0.16113 },
          pixelBand: 'pass',
        }),
      }),
      expected: 'review',
    },
    {
      name: 'downgrades severe non-proportional size collapse with otherwise passing pixels to review',
      actual: visualBandWithEscalation({
        pixelBand: 'pass',
        perceptual: { distanceRatio: 0.37891 },
        severeSizeMismatch: hasSevereSizePerceptualMismatch({
          sizeDelta: {
            original: { width: -0.95, height: -0.20, area: -0.96 },
            content: { width: -0.96, height: -0.18, area: -0.97 },
          },
          perceptual: { distanceRatio: 0.37891 },
          pixelBand: 'pass',
        }),
      }),
      expected: 'review',
    },
    {
      name: 'keeps moderate scale-only size changes as pass when visual diff passes',
      actual: visualBandWithEscalation({
        pixelBand: 'pass',
        perceptual: { distanceRatio: 0.20 },
        severeSizeMismatch: hasSevereSizePerceptualMismatch({
          sizeDelta: {
            original: { width: -0.20, height: -0.20, area: -0.36 },
            content: { width: -0.20, height: -0.20, area: -0.36 },
          },
          perceptual: { distanceRatio: 0.20 },
          pixelBand: 'pass',
        }),
      }),
      expected: 'pass',
    },
    {
      name: 'keeps ordinary size differences as pass when visual diff passes',
      actual: visualBandWithEscalation({
        pixelBand: 'pass',
        perceptual: { distanceRatio: 0.06738 },
        severeSizeMismatch: hasSevereSizePerceptualMismatch({
          sizeDelta: {
            original: { area: -0.1851 },
            content: { area: -0.1923 },
          },
          perceptual: { distanceRatio: 0.06738 },
          pixelBand: 'pass',
        }),
      }),
      expected: 'pass',
    },
    {
      name: 'downgrades high pixel diff with similar perceptual hash to review',
      actual: visualBandWithEscalation({
        pixelBand: 'fail',
        perceptual: { distanceRatio: 0.03 },
        severeSizeMismatch: false,
      }),
      expected: 'review',
    },
    {
      name: 'fails SVG geometry when actual viewBox aspect ratio drifts far from official reference',
      actual: geometryCheck(
        '<svg viewBox="0 0 289 432"></svg>',
        { width: 289, height: 432 },
        '<svg viewBox="0 0 206 662"></svg>'
      ).pass,
      expected: false,
    },
    {
      name: 'carries reproduction code into generated reports',
      actual: caseSourceFields({
        code: 'a -> b',
        markdown: '```d2\na -> b\n```',
      }).code,
      expected: 'a -> b',
    },
    {
      name: 'captures rendered SVG from the visible DOM first',
      actual: actualCaptureMethod({
        svg: '<svg viewBox="0 0 10 10"></svg>',
        canvasDataUrl: 'data:image/png;base64,AA==',
      }),
      expected: 'dom-screenshot',
    },
    {
      name: 'captures actual canvas pixels before DOM screenshots',
      actual: actualCaptureMethod({
        svg: '',
        canvasDataUrl: 'data:image/png;base64,AA==',
      }),
      expected: 'canvas-data-url',
    },
    {
      name: 'places reproduction code and official reference immediately after case info',
      actual: renderIssueBody({
        id: 'sample-case',
        language: 'd2',
        code: 'a -> b',
        selectedFeature: 'Diagram (D2)',
        selectedExample: 'Labeled edges',
        docPath: 'cases/sample.md',
        url: 'https://example.test/playground/d2/',
        runEnvironment: {
          playwright: {
            headless: true,
            viewport: { width: 1280, height: 900 },
            deviceScaleFactor: 1,
            browserVersion: 'test',
            executablePath: 'test-chrome',
          },
        },
        expected: {
          sourceSvg: 'cases/assets/sample.svg',
          officialSource: 'https://example.test/source',
          officialRenderUrl: 'https://example.test/render.svg',
        },
        actual: {},
        semantic: { pass: false, missingTexts: [] },
        geometry: { pass: false },
        visual: null,
        errors: [],
      }, 'owner/repo')
        .match(/^## .+$/gm)
        .slice(0, 5)
        .join('>'),
      expected: '## Defect summary>## Case info>## Reproduction code>## Official reference rendering>## Run environment',
    },
    {
      name: 'marks review issue titles for manual review',
      actual: renderIssueTitle({
        language: 'd2',
        selectedExample: 'Labeled edges',
        status: 'review',
        semantic: { missingTexts: [] },
        geometry: { pass: true },
        visual: { diffRatio: 0.2 },
        errors: [],
      }).startsWith('[Needs review] '),
      expected: true,
    },
    {
      name: 'describes missing text and layout mismatch in issue titles',
      actual: renderIssueTitle({
        language: 'plantuml',
        selectedExample: 'Activity Diagram',
        status: 'fail',
        semantic: { missingTexts: ['Client', 'Server'] },
        geometry: { pass: false, aspectRatioMismatch: true },
        visual: { diffRatio: 0.208 },
        errors: [],
      }),
      expected: 'PlantUML Activity Diagram: missing Client, Server, and layout aspect ratio is off',
    },
  ];

  for (const test of tests) {
    if (test.actual !== test.expected) {
      throw new Error(`${test.name}: expected ${test.expected}, got ${test.actual}`);
    }
  }
  console.log(`visual classification self-test passed (${tests.length} checks)`);
}

function findDiffBounds(diffPng) {
  let left = diffPng.width;
  let top = diffPng.height;
  let right = -1;
  let bottom = -1;

  for (let y = 0; y < diffPng.height; y += 1) {
    for (let x = 0; x < diffPng.width; x += 1) {
      const idx = (diffPng.width * y + x) * 4;
      const r = diffPng.data[idx];
      const g = diffPng.data[idx + 1];
      const b = diffPng.data[idx + 2];
      const a = diffPng.data[idx + 3];
      if (a !== 0 && !(r === 255 && g === 255 && b === 255)) {
        left = Math.min(left, x);
        top = Math.min(top, y);
        right = Math.max(right, x);
        bottom = Math.max(bottom, y);
      }
    }
  }

  if (right < left || bottom < top) {
    return null;
  }
  return {
    x: left,
    y: top,
    width: right - left + 1,
    height: bottom - top + 1,
    right,
    bottom,
  };
}

async function handleIssues(reports) {
  const issueReports = reports.filter(report => report.status === 'fail' || report.status === 'review');
  const results = [];
  const submit = process.env.SUBMIT_GITHUB_ISSUES === '1';
  const submitReviewIssues = process.env.SUBMIT_REVIEW_ISSUES === '1';
  const repo = process.env.ISSUE_REPO || process.env.GITHUB_REPOSITORY || 'Actrium/supramark';
  const token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN || '';
  const labels = (process.env.ISSUE_LABELS || 'bug,automated-visual-regression')
    .split(',')
    .map(label => label.trim())
    .filter(Boolean);

  for (const report of reports) {
    if (report.status === 'fail' || report.status === 'review') continue;
    const staleIssuePath = resolve(outDir, 'issues', `${report.id}.md`);
    if (!existsSync(staleIssuePath)) continue;
    await writeFile(
      staleIssuePath,
      renderNonCurrentIssueNote(report),
      'utf8'
    );
  }

  await writeFile(
    resolve(outDir, 'issues', 'CURRENT_ISSUES.md'),
    renderCurrentIssuesIndex(issueReports),
    'utf8'
  );

  for (const report of issueReports) {
    const title = renderIssueTitle(report);
    const issuePath = resolve(outDir, 'issues', `${report.id}.md`);
    const localBody = renderIssueBody(report, repo, { issuePath });
    await writeFile(issuePath, `# ${title}\n\n${localBody}`, 'utf8');

    if (report.status === 'review' && !submitReviewIssues) {
      results.push({
        id: report.id,
        submitted: false,
        reason: 'status is review and SUBMIT_REVIEW_ISSUES is not 1; issue body generated for manual review only.',
        issueBodyPath: relative(workspaceRoot, issuePath),
      });
      continue;
    }

    if (!submit) {
      results.push({
        id: report.id,
        submitted: false,
        reason: 'SUBMIT_GITHUB_ISSUES is not 1; issue body generated only.',
        issueBodyPath: relative(workspaceRoot, issuePath),
      });
      continue;
    }
    if (!token) {
      results.push({
        id: report.id,
        submitted: false,
        reason: 'GH_TOKEN/GITHUB_TOKEN is not set.',
        issueBodyPath: relative(workspaceRoot, issuePath),
      });
      continue;
    }

    try {
      const existing = await findExistingIssue({ repo, token, title });
      if (existing) {
        results.push({
          id: report.id,
          submitted: false,
          reason: 'matching open issue already exists',
          issueUrl: existing.html_url,
          issueBodyPath: relative(workspaceRoot, issuePath),
        });
        continue;
      }

      const githubBody = renderIssueBody(report, repo, { github: true });
      const created = await createGithubIssue({ repo, token, title, body: githubBody, labels });
      results.push({
        id: report.id,
        submitted: true,
        issueUrl: created.html_url,
        issueBodyPath: relative(workspaceRoot, issuePath),
      });
    } catch (error) {
      results.push({
        id: report.id,
        submitted: false,
        reason: error instanceof Error ? error.message : String(error),
        issueBodyPath: relative(workspaceRoot, issuePath),
      });
    }
  }

  return results;
}

function renderCurrentIssuesIndex(issueReports) {
  const lines = [
    '# Current Run Issue Files',
    '',
    'This file is generated by the automation script and lists the cases from this run whose `status` is `fail` or `review`. `review` is for manual triage only by default; a GitHub issue is only auto-submitted when both `SUBMIT_GITHUB_ISSUES=1` and `SUBMIT_REVIEW_ISSUES=1` are set.',
    '',
  ];

  if (issueReports.length === 0) {
    lines.push('No cases in this run were auto-classified as failing or needing manual review.');
    return `${lines.join('\n')}\n`;
  }

  for (const report of issueReports) {
    const marker = report.status === 'review' ? 'Needs review' : 'Fail';
    lines.push(
      `- [${report.id}](./${report.id}.md): ${marker} / ${report.selectedFeature ?? 'not recorded'} / ${report.selectedExample ?? 'not recorded'}`
    );
  }

  return `${lines.join('\n')}\n`;
}
function renderNonCurrentIssueNote(report) {
  return `# Not a current defect: ${report.id}

This file was generated by a previous run, but the latest run did not auto-classify this case as a defect.

## Latest run result

- Case ID: \`${report.id}\`
- Current status: \`${report.status}\`
- Page feature dropdown: \`${report.selectedFeature ?? 'not recorded'}\`
- Type/example dropdown: \`${report.selectedExample ?? 'not recorded'}\`
- Test doc location: \`${report.docPath ?? 'not recorded'}\`
- Supramark page: ${report.url ?? 'not recorded'}
- Full HTML report: \`artifacts/official-diagram-visual-workflow/report.html\`

Treat \`artifacts/official-diagram-visual-workflow/summary.json\` and \`issues/CURRENT_ISSUES.md\` as authoritative; do not treat this stale issue file as a defect from the current run.
`;
}

function renderCurrentRunArtifacts(reports, summary) {
  const files = new Set([
    'artifacts/official-diagram-visual-workflow/summary.json',
    'artifacts/official-diagram-visual-workflow/report.html',
    'artifacts/official-diagram-visual-workflow/CURRENT_RUN_ARTIFACTS.json',
    'artifacts/official-diagram-visual-workflow/issues/CURRENT_ISSUES.md',
  ]);

  for (const report of reports) {
    addArtifact(files, `artifacts/official-diagram-visual-workflow/${report.id}.report.json`);
    addArtifact(files, report.expected?.pngPath);
    addArtifact(files, report.actual?.svgPath);
    addArtifact(files, report.actual?.pngPath);
    addArtifact(files, report.actual?.screenshotPath);
    addArtifact(files, report.actual?.previewPath);
    addArtifact(files, report.visual?.diffPath);
    addArtifact(files, report.visual?.raw?.diffPath);
    addArtifact(files, report.visual?.normalized?.expectedPath);
    addArtifact(files, report.visual?.normalized?.actualPath);
    if (report.status === 'fail' || report.status === 'review') {
      addArtifact(files, `artifacts/official-diagram-visual-workflow/issues/${report.id}.md`);
    }
  }

  return {
    runStartedAt: summary.runStartedAt,
    runFinishedAt: summary.runFinishedAt,
    total: summary.total,
    passed: summary.passed,
    review: summary.review,
    failed: summary.failed,
    visualThresholds: summary.visualThresholds,
    note: 'Only files listed here are considered current for this run. Other files under artifacts may be leftovers from older runs if they were not overwritten.',
    files: [...files].sort(),
  };
}

function addArtifact(files, artifactPath) {
  if (!artifactPath) return;
  files.add(String(artifactPath).replace(/\\/g, '/'));
}

async function findExistingIssue({ repo, token, title }) {
  const query = `repo:${repo} is:issue state:open in:title ${JSON.stringify(title)}`;
  const response = await fetch(`https://api.github.com/search/issues?q=${encodeURIComponent(query)}`, {
    headers: githubHeaders(token),
  });
  if (!response.ok) {
    throw new Error(`GitHub issue search failed: ${response.status} ${await response.text()}`);
  }
  const payload = await response.json();
  return payload.items?.find(item => item.title === title) ?? null;
}

async function createGithubIssue({ repo, token, title, body, labels }) {
  const response = await fetch(`https://api.github.com/repos/${repo}/issues`, {
    method: 'POST',
    headers: githubHeaders(token),
    body: JSON.stringify({ title, body, labels }),
  });
  if (!response.ok) {
    throw new Error(`GitHub issue create failed: ${response.status} ${await response.text()}`);
  }
  return response.json();
}

function githubHeaders(token) {
  return {
    Accept: 'application/vnd.github+json',
    Authorization: `Bearer ${token}`,
    'Content-Type': 'application/json; charset=utf-8',
    'X-GitHub-Api-Version': '2022-11-28',
    'User-Agent': 'supramark-official-diagram-visual-workflow',
  };
}

function issueDiagramLabel(report) {
  const language = new Map([
    ['d2', 'D2'],
    ['plantuml', 'PlantUML'],
    ['mermaid', 'Mermaid'],
    ['dot', 'Graphviz'],
    ['graphviz', 'Graphviz'],
    ['echarts', 'ECharts'],
    ['vega-lite', 'Vega-Lite'],
  ]).get(report.language) ?? report.language;

  const type = String(report.selectedExample || report.title || '')
    // cjk-allow: strips the Chinese suffix meaning "example" (U+793A U+4F8B)
    // that appears in labels sourced live from the preview UI / case docs,
    // which stay in Chinese (case docs are out of scope for this cleanup).
    // Written as Unicode escapes, not literal characters, per repo policy.
    .replace(/\u793a\u4f8b/g, '')
    .replace(/\s+/g, ' ')
    .trim();

  return type ? `${language} ${type}` : language;
}

function renderIssueTitle(report) {
  const diagram = issueDiagramLabel(report);
  const reviewPrefix = report.status === 'review' ? '[Needs review] ' : '';
  const errors = report.errors ?? [];
  const missingTexts = report.semantic?.missingTexts ?? [];
  const missingTextTitle = formatIssueTitleTextList(missingTexts);
  let title;

  if (errors.length > 0 || report.semantic?.hasError) {
    title = `${diagram}: render error`;
  } else if (!report.visual) {
    title = `${diagram}: no comparable image produced`;
  } else if (missingTexts.length && report.geometry?.aspectRatioMismatch) {
    title = `${diagram}: missing ${missingTextTitle}, and layout aspect ratio is off`;
  } else if (missingTexts.length) {
    title = `${diagram}: missing ${missingTextTitle}`;
  } else if (!report.geometry?.pass) {
    title = report.geometry?.aspectRatioMismatch
      ? `${diagram}: layout aspect ratio is off`
      : `${diagram}: diagram size is off`;
  } else if (typeof report.visual.diffRatio === 'number') {
    title = `${diagram}: visual diff ${(report.visual.diffRatio * 100).toFixed(2)}%`;
  } else {
    title = `${diagram}: render result mismatch`;
  }

  return `${reviewPrefix}${title}`;
}

function formatIssueTitleTextList(texts) {
  const visibleTexts = texts.filter(Boolean).slice(0, 3);
  const suffix = texts.length > visibleTexts.length ? ` and ${texts.length} more items` : '';
  return `${visibleTexts.join(', ')}${suffix}`;
}

function renderIssueBody(report, repo, options = {}) {
  const artifactRunUrl = process.env.GITHUB_ACTIONS === 'true'
    ? `${process.env.GITHUB_SERVER_URL || 'https://github.com'}/${process.env.GITHUB_REPOSITORY || repo}/actions/runs/${process.env.GITHUB_RUN_ID || ''}`
    : '';
  const artifactBaseUrl = (process.env.ARTIFACT_BASE_URL || '').replace(/\/+$/, '');
  const diffRatio = typeof report.visual?.diffRatio === 'number'
    ? `${(report.visual.diffRatio * 100).toFixed(3)}%`
    : 'none, no visual diff produced';
  const rawDiffRatio = typeof report.visual?.raw?.diffRatio === 'number'
    ? `${(report.visual.raw.diffRatio * 100).toFixed(3)}%`
    : 'none';
  const perceptualDistance = typeof report.visual?.perceptual?.distanceRatio === 'number'
    ? `${(report.visual.perceptual.distanceRatio * 100).toFixed(3)}%`
    : 'none';
  const passThreshold = typeof report.visual?.passThreshold === 'number'
    ? `${(report.visual.passThreshold * 100).toFixed(3)}%`
    : `${(visualPassThreshold * 100).toFixed(3)}%`;
  const failThreshold = typeof report.visual?.failThreshold === 'number'
    ? `${(report.visual.failThreshold * 100).toFixed(3)}%`
    : `${(visualFailThreshold * 100).toFixed(3)}%`;
  const visualBandText = report.visual?.band ?? 'none';
  const captureMethodText = report.actual?.captureMethod ?? 'none';
  const severeSizeMismatchText = report.visual?.severeSizeMismatch ? 'yes' : 'no';
  const diffBounds = report.visual?.diffBounds
    ? `x=${report.visual.diffBounds.x}, y=${report.visual.diffBounds.y}, width=${report.visual.diffBounds.width}, height=${report.visual.diffBounds.height}`
    : 'none';
  const aspectRatioDelta = typeof report.geometry?.aspectRatioDelta === 'number'
    ? `${(report.geometry.aspectRatioDelta * 100).toFixed(3)}%`
    : 'none';
  const expectedAspectRatio = typeof report.geometry?.expectedAspectRatio === 'number'
    ? report.geometry.expectedAspectRatio.toFixed(6)
    : 'none';
  const actualAspectRatio = typeof report.geometry?.aspectRatio === 'number'
    ? report.geometry.aspectRatio.toFixed(6)
    : 'none';
  const aspectRatioThreshold = typeof report.geometry?.aspectRatioThreshold === 'number'
    ? `${(report.geometry.aspectRatioThreshold * 100).toFixed(3)}%`
    : 'none';
  const expectedViewBox = report.geometry?.expectedViewBox
    ? formatViewBox(report.geometry.expectedViewBox)
    : 'none';
  const actualViewBox = report.geometry?.viewBox
    ? formatViewBox(report.geometry.viewBox)
    : 'none';
  const missingTexts = report.semantic?.missingTexts?.length
    ? report.semantic.missingTexts.map(text => `\`${text}\``).join(', ')
    : 'none';
  const errors = report.errors?.length ? report.errors.map(error => `- ${error}`).join('\n') : '- none';
  const localExpected = report.expected?.pngPath || '';
  const localOfficialReference = options.github
    ? report.expected?.sourceSvg || ''
    : localExpected || report.expected?.sourceSvg || '';
  const localActual = report.actual?.previewPath || report.actual?.pngPath || report.actual?.screenshotPath || '';
  const localDiff = report.visual?.diffPath || '';
  const localRawDiff = report.visual?.raw?.diffPath || '';
  const localNormalizedExpected = report.visual?.normalized?.expectedPath || '';
  const localNormalizedActual = report.visual?.normalized?.actualPath || '';
  const env = report.runEnvironment?.playwright ?? runEnvironment.playwright;
  const envText = [
    `Playwright mode: ${env.headless ? 'headless' : 'headed'}`,
    `Viewport: ${env.viewport?.width ?? '?'}x${env.viewport?.height ?? '?'}`,
    `deviceScaleFactor: ${env.deviceScaleFactor ?? '?'}`,
    `Browser: ${env.browserVersion ?? 'not recorded'}`,
    `Chrome path: ${env.executablePath ?? 'not recorded'}`,
  ].join('\n- ');
  const expectedImage = artifactImagePath(localExpected, { ...options, artifactBaseUrl });
  const officialImage = officialReferenceImagePath({
    localOfficialReference,
    officialRenderUrl: report.expected?.officialRenderUrl || '',
    repo: process.env.GITHUB_REPOSITORY || repo,
    options: { ...options, artifactBaseUrl },
  });
  const actualImage = artifactImagePath(localActual, { ...options, artifactBaseUrl });
  const diffImage = artifactImagePath(localDiff, { ...options, artifactBaseUrl });
  const rawDiffImage = artifactImagePath(localRawDiff, { ...options, artifactBaseUrl });
  const normalizedExpectedImage = artifactImagePath(localNormalizedExpected, { ...options, artifactBaseUrl });
  const normalizedActualImage = artifactImagePath(localNormalizedActual, { ...options, artifactBaseUrl });
  const localImageNote = options.github && !artifactBaseUrl
    ? '\n> GitHub Issue cannot directly display locally generated PNGs. Open this GitHub Actions artifact, or set `ARTIFACT_BASE_URL` to a publicly reachable artifact directory before filing the issue.\n'
    : '';
  const visualSkipNote = report.visual
    ? ''
    : '- Visual comparison: skipped. The page did not render a valid diagram, so a pixel comparison would be meaningless.\n';
  const issueVerdictText = report.status === 'review'
    ? 'needs manual review'
    : 'fail';

  return `## Defect summary

The automated diagram rendering check found that \`${report.id}\`${report.visual ? ' does not match the official reference rendering' : ' did not render a valid diagram on the page'}, and classified it as **${issueVerdictText}**.

## Case info

- Case ID: \`${report.id}\`
- Diagram language: \`${report.language}\`
- Page feature dropdown: \`${report.selectedFeature ?? 'not recorded'}\`
- Type/example dropdown: \`${report.selectedExample ?? 'not recorded'}\`
- Test doc location: \`${report.docPath ?? 'not recorded'}\`
- Supramark page: ${report.url ?? 'not recorded'}

## Reproduction code

\`\`\`text
${report.code ?? report.markdown ?? ''}
\`\`\`

## Official reference rendering

- Official source: ${report.expected?.officialSource || 'not recorded'}
- Official render URL: ${report.expected?.officialRenderUrl || 'not recorded'}
- Official reference image file: \`${report.expected?.sourceSvg || 'not recorded'}\`

${officialImage ? `![Official reference rendering](${officialImage})` : ''}

## Run environment

- ${envText}

## Differences

${visualSkipNote}- Visual diff ratio: ${diffRatio}
- Raw-size direct diff: ${rawDiffRatio}
- Perceptual hash distance: ${perceptualDistance}
- Visual band: ${visualBandText}
- Actual capture method: ${captureMethodText}
- Visual thresholds: pass at ≤ ${passThreshold}, manual review at > ${passThreshold} and < ${failThreshold}, fail at ≥ ${failThreshold}; a high pixel diff is downgraded to manual review if the perceptual hash distance is ≤ ${formatRatio(report.visual?.perceptual?.similarThreshold ?? perceptualSimilarThreshold)}; it is flagged for manual review if the size/area delta is ≥ ${formatRatio(severeSizeDeltaThreshold)} and the perceptual hash distance is > ${formatRatio(perceptualSimilarThreshold)}; it stays fail if the pixel diff alone already reaches the fail threshold
- Severe size + perceptual mismatch: ${severeSizeMismatchText}
- Diff region bounding box: ${diffBounds}
- Expected original image size: ${formatSize(report.visual?.expectedSize)}
- Actual original image size: ${formatSize(report.visual?.actualSize)}
- Original image size delta: ${formatSizeDelta(report.visual?.sizeDelta?.original)}
- Content region size delta: ${formatSizeDelta(report.visual?.sizeDelta?.content)}
- Expected SVG viewBox: ${expectedViewBox}
- Actual SVG viewBox: ${actualViewBox}
- SVG viewBox aspect ratio delta: expected=${expectedAspectRatio}, actual=${actualAspectRatio}, delta=${aspectRatioDelta}, threshold=${aspectRatioThreshold}
- Missing key text: ${missingTexts}
- Semantic check: ${report.semantic?.pass ? 'pass' : 'fail'}
- Geometry check: ${report.geometry?.pass ? 'pass' : 'fail'}

## Errors

${errors}

## Automation artifact locations

> If this issue was filed from a local script run, the paths below are local paths; if filed from GitHub Actions, see this run's workflow artifact.
${artifactRunUrl ? `\n- GitHub Actions run: ${artifactRunUrl}` : ''}
- Expected PNG: \`${localExpected || 'not generated'}\`
- Actual PNG/screenshot: \`${localActual || 'not generated'}\`
- Normalized Expected PNG: \`${localNormalizedExpected || 'not generated'}\`
- Normalized Actual PNG: \`${localNormalizedActual || 'not generated'}\`
- Normalized Diff PNG: \`${localDiff || 'not generated'}\`
- Raw Diff PNG: \`${localRawDiff || 'not generated'}\`
- Full HTML report: \`artifacts/official-diagram-visual-workflow/report.html\`

${localImageNote}

${expectedImage ? `![Expected](${expectedImage})` : ''}
${actualImage ? `![Actual](${actualImage})` : ''}
${normalizedExpectedImage ? `![Normalized Expected](${normalizedExpectedImage})` : ''}
${normalizedActualImage ? `![Normalized Actual](${normalizedActualImage})` : ''}
${diffImage ? `![Diff](${diffImage})` : ''}
${rawDiffImage ? `![Raw Diff](${rawDiffImage})` : ''}
`;
}

function artifactImagePath(artifactPath, { issuePath, artifactBaseUrl, github } = {}) {
  if (!artifactPath) return '';
  const webPath = artifactPath.replace(/\\/g, '/');
  if (artifactBaseUrl) {
    return `${artifactBaseUrl}/${webPath}`;
  }
  if (github) {
    return '';
  }
  if (issuePath) {
    return relative(dirname(issuePath), resolve(workspaceRoot, artifactPath)).replace(/\\/g, '/');
  }
  return webPath;
}

function officialReferenceImagePath({ localOfficialReference, officialRenderUrl, repo, options }) {
  const localImage = artifactImagePath(localOfficialReference, options);
  if (localImage) return localImage;

  if (options?.github && localOfficialReference) {
    const webPath = localOfficialReference.replace(/\\/g, '/');
    const repoPath = repoPathPrefix ? `${repoPathPrefix}/${webPath}` : webPath;
    const ref = process.env.GITHUB_SHA || process.env.GITHUB_REF_NAME || 'main';
    return `https://raw.githubusercontent.com/${repo}/${ref}/${repoPath}`;
  }

  return isEmbeddableImageUrl(officialRenderUrl) ? officialRenderUrl : '';
}

function isEmbeddableImageUrl(value) {
  if (!/^https?:\/\//i.test(value || '')) return false;
  try {
    const url = new URL(value);
    return /\.(png|jpe?g|gif|webp|svg)$/i.test(url.pathname);
  } catch {
    return false;
  }
}

function classify({ semantic, geometry, visual, errors }) {
  if (!semantic.pass || errors.length > 0 || !visual) return 'fail';
  if (!geometry.pass && visual.band !== 'pass') return 'fail';
  return visual.band;
}

function renderHtmlReport(reports, summary) {
  const rows = reports.map(report => {
    const expected = toWebPath(report.expected?.pngPath);
    const actual = toWebPath(report.actual?.previewPath || report.actual?.pngPath || report.actual?.screenshotPath);
    const diff = toWebPath(report.visual?.diffPath);
    const rawDiff = toWebPath(report.visual?.raw?.diffPath);
    const normalizedExpected = toWebPath(report.visual?.normalized?.expectedPath);
    const normalizedActual = toWebPath(report.visual?.normalized?.actualPath);
    return `<section class="case case-${escapeHtml(report.status)}">
  <h2>${escapeHtml(report.id)} <span>${escapeHtml(report.status)}</span></h2>
  <p><strong>${escapeHtml(report.language)}</strong> / ${escapeHtml(report.title || '')}</p>
  <p>selected: ${escapeHtml(report.selectedFeature ?? '')}${report.selectedExample ? ` / ${escapeHtml(report.selectedExample)}` : ''}</p>
  <p>environment: ${summary.runEnvironment.playwright.headless ? 'headless' : 'headed'} / viewport ${summary.runEnvironment.playwright.viewport.width}x${summary.runEnvironment.playwright.viewport.height} / dSF ${summary.runEnvironment.playwright.deviceScaleFactor}</p>
  <p>normalized diff: ${formatRatio(report.visual?.diffRatio)} (${escapeHtml(report.visual?.band ?? 'n/a')}) | pixel band: ${escapeHtml(report.visual?.pixelBand ?? 'n/a')} | raw diff: ${formatRatio(report.visual?.raw?.diffRatio)} | perceptual: ${formatRatio(report.visual?.perceptual?.distanceRatio)} | severe size mismatch: ${report.visual?.severeSizeMismatch ? 'yes' : 'no'} | semantic: ${report.semantic.pass ? 'pass' : 'fail'} | geometry: ${report.geometry.pass ? 'pass' : 'fail'}</p>
  <p>size: expected ${escapeHtml(formatSize(report.visual?.expectedSize))}, actual ${escapeHtml(formatSize(report.visual?.actualSize))}, original delta ${escapeHtml(formatSizeDelta(report.visual?.sizeDelta?.original))}, content delta ${escapeHtml(formatSizeDelta(report.visual?.sizeDelta?.content))}</p>
  ${report.errors?.length ? `<pre class="errors">${escapeHtml(report.errors.join('\n'))}</pre>` : ''}
  <div class="images">
    <figure><figcaption>Official expected</figcaption>${expected ? `<img src="${expected}" />` : ''}</figure>
    <figure><figcaption>Supramark actual</figcaption>${actual ? `<img src="${actual}" />` : ''}</figure>
    <figure><figcaption>Normalized expected</figcaption>${normalizedExpected ? `<img src="${normalizedExpected}" />` : ''}</figure>
    <figure><figcaption>Normalized actual</figcaption>${normalizedActual ? `<img src="${normalizedActual}" />` : ''}</figure>
    <figure><figcaption>Normalized diff</figcaption>${diff ? `<img src="${diff}" />` : ''}</figure>
    <figure><figcaption>Raw diff</figcaption>${rawDiff ? `<img src="${rawDiff}" />` : ''}</figure>
  </div>
</section>`;
  }).join('\n');

  return `<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <title>Official Diagram Visual Workflow</title>
  <style>
    body { margin: 24px; font-family: Arial, sans-serif; color: #202124; background: #f6f7f9; }
    h1 { margin-bottom: 4px; }
    .summary { margin-bottom: 24px; color: #555; }
    .case { background: #fff; border: 1px solid #ddd; border-left-width: 6px; border-radius: 6px; padding: 16px; margin: 16px 0; }
    .case-pass { border-left-color: #188038; }
    .case-review { border-left-color: #f9ab00; }
    .case-fail { border-left-color: #d93025; }
    h2 { display: flex; justify-content: space-between; gap: 16px; margin: 0 0 8px; font-size: 18px; }
    h2 span { text-transform: uppercase; font-size: 12px; align-self: center; }
    .images { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; align-items: start; }
    figure { margin: 0; border: 1px solid #e0e0e0; background: white; padding: 8px; min-height: 160px; }
    figcaption { font-size: 12px; color: #666; margin-bottom: 8px; }
    img { max-width: 100%; height: auto; display: block; background: white; }
    .errors { white-space: pre-wrap; background: #fff4f4; border: 1px solid #ffd3d3; padding: 8px; }
  </style>
</head>
<body>
  <h1>Official Diagram Visual Workflow</h1>
  <div class="summary">Total ${summary.total}, passed ${summary.passed}, review ${summary.review}, failed ${summary.failed}. Visual thresholds: pass ≤ ${formatRatio(summary.visualThresholds.pass)}, fail ≥ ${formatRatio(summary.visualThresholds.fail)}, perceptual similar ≤ ${formatRatio(summary.visualThresholds.perceptualSimilar)}. Environment: ${summary.runEnvironment.playwright.headless ? 'headless' : 'headed'}, viewport ${summary.runEnvironment.playwright.viewport.width}x${summary.runEnvironment.playwright.viewport.height}, dSF ${summary.runEnvironment.playwright.deviceScaleFactor}</div>
  ${rows}
</body>
</html>`;
}

function toWebPath(p) {
  if (!p) return '';
  return relative(outDir, resolve(workspaceRoot, p)).replace(/\\/g, '/');
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

function formatRatio(value) {
  if (typeof value !== 'number') return 'n/a';
  return `${(value * 100).toFixed(3)}%`;
}

function formatSize(value) {
  if (!value || typeof value.width !== 'number' || typeof value.height !== 'number') {
    return 'n/a';
  }
  const content = value.contentBounds
    ? `, content ${value.contentBounds.width}x${value.contentBounds.height}`
    : '';
  return `${value.width}x${value.height}${content}`;
}

function formatViewBox(value) {
  if (!value || typeof value.width !== 'number' || typeof value.height !== 'number') {
    return 'n/a';
  }
  return `${value.x} ${value.y} ${value.width} ${value.height}`;
}

function formatSizeDelta(value) {
  if (!value) return 'n/a';
  return `w ${formatSignedRatio(value.width)}, h ${formatSignedRatio(value.height)}, area ${formatSignedRatio(value.area)}`;
}

function formatSignedRatio(value) {
  if (typeof value !== 'number') return 'n/a';
  const pct = (value * 100).toFixed(2);
  return value > 0 ? `+${pct}%` : `${pct}%`;
}
