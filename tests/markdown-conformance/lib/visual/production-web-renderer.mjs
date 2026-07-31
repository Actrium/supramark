import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { createServer } from 'vite';

const SUITE_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const HARNESS_ROOT = path.join(SUITE_ROOT, 'browser', 'commonmark-renderer');
const CONFIG_PATH = path.join(HARNESS_ROOT, 'vite.config.mjs');

export async function renderWithProductionWebRenderer({ cases, astById }) {
  const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH;
  const server = await createServer({
    configFile: CONFIG_PATH,
    root: HARNESS_ROOT,
    server: { host: '127.0.0.1', port: 0, strictPort: false },
  });

  let browser;
  try {
    await server.listen();
    const url = server.resolvedUrls?.local?.[0];
    if (!url) throw new Error('Vite did not expose a local renderer URL');

    browser = await chromium.launch({
      headless: true,
      ...(executablePath ? { executablePath } : {}),
      args: ['--font-render-hinting=none'],
    });
    const browserVersion = browser.version();
    const context = await browser.newContext({
      colorScheme: 'light',
      locale: 'zh-CN',
      reducedMotion: 'reduce',
      timezoneId: 'UTC',
    });
    await context.route('**/*', route => {
      const requestUrl = new URL(route.request().url());
      if (requestUrl.hostname === '127.0.0.1') route.continue();
      else route.abort();
    });
    const page = await context.newPage();
    const pageErrors = [];
    page.on('pageerror', error => pageErrors.push(error.stack ?? error.message));
    await page.goto(url, { waitUntil: 'networkidle' });
    await page.waitForFunction(() => typeof window.renderSupramarkCase === 'function');

    const htmlById = new Map();
    const errorsById = new Map();
    for (const testCase of cases) {
      const ast = astById.get(testCase.id);
      if (ast === undefined) {
        errorsById.set(testCase.id, ['Rust parser did not produce an AST']);
        continue;
      }

      const pageErrorOffset = pageErrors.length;
      try {
        // GFM "Disallowed Raw HTML" (tagfilter) is a per-extension feature in
        // cmark-gfm: only the spec.txt "Disallowed Raw HTML (extension)" and
        // extensions.txt "HTML tag filter" sections are rendered with the
        // filter on. Mirror that here so CommonMark-core raw HTML (passthrough)
        // is unaffected.
        const tagfilterSections = new Set([
          'Disallowed Raw HTML (extension)',
          'HTML tag filter',
        ]);
        const gfmTagfilter = tagfilterSections.has(testCase.source?.section ?? '');
        // GFM footnote section format (trailing <section> + backrefs) is a
        // per-extension feature; enable it only for the cmark-gfm Footnotes
        // family of sections. CommonMark has no footnotes extension.
        const sectionName = testCase.source?.section ?? '';
        const gfmFootnoteStyle =
          testCase.id.startsWith('cmark-gfm-') && sectionName.toLowerCase().includes('footnote');
        // cmark-gfm 0.29's HTML renderer flattens nested strong (a strong
        // whose parent is strong emits no <strong> wrapper), while CommonMark
        // 0.31 keeps the nesting. The two references diverge on the same
        // input, so the flattening is enabled only for the cmark-gfm source.
        const flattenNestedStrong = testCase.id.startsWith('cmark-gfm-');
        const response = await page.evaluate(
          request => window.renderSupramarkCase(request),
          { id: testCase.id, markdown: testCase.input.markdown, ast, gfmTagfilter, gfmFootnoteStyle, flattenNestedStrong }
        );
        const errors = [...response.errors, ...pageErrors.slice(pageErrorOffset)];
        if (errors.length > 0) errorsById.set(testCase.id, errors);
        else htmlById.set(testCase.id, response.html);
      } catch (error) {
        errorsById.set(testCase.id, [error.stack ?? error.message]);
      }
    }

    await context.close();
    return {
      htmlById,
      errorsById,
      environment: {
        implementation: 'packages/renderers/web/src/Supramark.tsx',
        parser: 'supramark-markdown (default profile)',
        harness: 'vite',
        browser: { name: 'chromium', version: browserVersion },
      },
    };
  } finally {
    if (browser) await browser.close();
    await server.close();
  }
}
