import { spawnSync } from 'node:child_process';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { astToHtml } from '../lib/semantic/ast-semantics.mjs';
import { renderConformanceHtmlReport } from '../lib/reports/html-report.mjs';
import {
  buildConformanceIssueMetadata,
  renderConformanceIssue,
} from '../lib/reports/issue-report.mjs';
import {
  attachEvidence,
  buildFailureGroups,
  buildRuntimeMetadata,
  compareWithBaseline,
  readOptionalJson,
  writeFailureEvidence,
} from '../lib/reports/conformance-diagnostics.mjs';
import {
  collectSemanticTypesFromTree,
  findFirstDifference,
  htmlToSemanticTree,
} from '../lib/semantic/html-semantics.mjs';
import { effectiveExpected } from '../lib/expected-overrides.mjs';
// Display names for CommonMark spec sections. The section keys below already
// match the spec's own English headings, so this map is effectively the
// identity function today; it is kept as a lookup table (rather than
// collapsed to `return section`) so a source with differently-named sections
// can still supply a friendlier display label without touching call sites.
const SECTION_NAMES = {
  Tabs: 'Tabs',
  'Backslash escapes': 'Backslash escapes',
  'Entity and numeric character references': 'Entity and numeric character references',
  Precedence: 'Precedence',
  'Thematic breaks': 'Thematic breaks',
  'ATX headings': 'ATX headings',
  'Setext headings': 'Setext headings',
  'Indented code blocks': 'Indented code blocks',
  'Fenced code blocks': 'Fenced code blocks',
  'HTML blocks': 'HTML blocks',
  'Link reference definitions': 'Link reference definitions',
  Paragraphs: 'Paragraphs',
  'Blank lines': 'Blank lines',
  'Block quotes': 'Block quotes',
  'List items': 'List items',
  Lists: 'Lists',
  Inlines: 'Inlines',
  'Code spans': 'Code spans',
  'Emphasis and strong emphasis': 'Emphasis and strong emphasis',
  Links: 'Links',
  Images: 'Images',
  Autolinks: 'Autolinks',
  'Raw HTML': 'Raw HTML',
  'Hard line breaks': 'Hard line breaks',
  'Soft line breaks': 'Soft line breaks',
  'Textual content': 'Textual content',
};


const SUITE_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const REPOSITORY_ROOT = path.resolve(SUITE_ROOT, '..', '..');
const sourceName = process.argv[2];
if (!sourceName || !/^[a-z0-9][a-z0-9-]*$/.test(sourceName)) {
  throw new Error('Usage: node tests/markdown-conformance/scripts/run.mjs <source-name>');
}
const BASELINE_PATH = path.join(SUITE_ROOT, 'baselines', `${sourceName}.json`);
const DEFAULT_BINARY = path.join(
  REPOSITORY_ROOT,
  'target',
  'debug',
  process.platform === 'win32' ? 'supramark-markdown.exe' : 'supramark-markdown'
);
const parserBinary = path.resolve(process.env.SUPRAMARK_MARKDOWN_BIN ?? DEFAULT_BINARY);
// Parser profile selection. CommonMark-core spec examples are normative
// CommonMark: bare URLs/emails stay literal, so the GFM autolink extension
// must be OFF for them. Supramark's shipped default keeps that extension ON
// (the product's GFM profile). The commonmark source (0.31.2) is entirely
// CommonMark-core, and cmark-gfm's spec.txt CommonMark-core sections (those
// not suffixed `(extension)`) mirror cmark-gfm's own spec.txt run, which
// executes those sections without the autolink extension. Both are parsed
// with `--no-gfm-autolink`. GFM extension cases — cmark-gfm's `(extension)`
// sections and the entire extensions.txt file — keep the default. Issue #202
// resolved the commonmark-source cases (0602/0608/0611/0612); issue #203
// extends the same treatment to the cmark-gfm source's CommonMark-core
// Autolinks section (spec-0610/0616/0619/0620).
// cmark-gfm runs a split config (core sections off, extension sections on),
// so it gets its own profile label instead of hiding under the default name.
const parserProfile =
  sourceName === 'commonmark'
    ? 'supramark-commonmark'
    : sourceName === 'cmark-gfm'
      ? 'supramark-cmark-gfm'
      : 'supramark-default';
function parserArgsFor(testCase) {
  return isCommonMarkCoreCase(testCase) ? ['--no-gfm-autolink', '-'] : ['-'];
}
// Whether a case is normative CommonMark (no GFM autolink extension). The
// commonmark source is CommonMark-core throughout; cmark-gfm's spec.txt
// mixes CommonMark-core sections with GFM `(extension)` sections, and only
// the core sections qualify — `(extension)` sections and extensions.txt are
// GFM and keep the autolink extension on.
function isCommonMarkCoreCase(testCase) {
  if (sourceName === 'commonmark') return true;
  if (sourceName === 'cmark-gfm') {
    return (
      testCase.source.path === 'test/spec.txt' &&
      !testCase.source.section.endsWith('(extension)')
    );
  }
  return false;
}
const failOnFailures = process.env.FAIL_ON_FAILURES !== '0';
// Gate mode decides what a non-zero exit means (see buildGate below).
// 'regression' (default) fails only on movement away from the recorded
// baseline; 'absolute' fails on any not-passing case, which is what every
// run did before a source with a standing failure set existed.
const gateMode = process.env.CONFORMANCE_GATE === 'absolute' ? 'absolute' : 'regression';
const visualEnabled = process.env.VISUAL_COMPARE === '1';
const filter = process.env.CASE_IDS
  ? new Set(process.env.CASE_IDS.split(',').map(value => value.trim()).filter(Boolean))
  : null;
const fixtureDirectory = path.join(
  REPOSITORY_ROOT,
  'tests',
  'cases',
  '_fixtures',
  sourceName
);
const document = JSON.parse(await readFile(path.join(fixtureDirectory, 'cases.json'), 'utf8'));
const version = JSON.parse(await readFile(path.join(fixtureDirectory, 'version.json'), 'utf8'));
const sourceConfig = JSON.parse(
  await readFile(path.join(SUITE_ROOT, 'config', 'sources', `${sourceName}.json`), 'utf8')
);
const sourceDisplayName = sourceConfig.displayName ?? sourceConfig.name;
if (sourceConfig.name !== sourceName || document.source !== sourceName || version.source !== sourceName) {
  throw new Error(`Source mismatch: argument ${sourceName}, config ${sourceConfig.name}, cases ${document.source}, version ${version.source}`);
}
if (document.profile !== sourceConfig.profile) {
  throw new Error(`Case profile does not match source config: ${document.profile} != ${sourceConfig.profile}`);
}
const baselineDocument = await readOptionalJson(BASELINE_PATH);
const selectedCases = filter
  ? document.cases.filter(testCase => filter.has(testCase.id))
  : document.cases;
const caseById = new Map(selectedCases.map(testCase => [testCase.id, testCase]));
const artifactDirectory = process.env.ARTIFACT_DIR
  ? path.resolve(REPOSITORY_ROOT, process.env.ARTIFACT_DIR)
  : path.join(SUITE_ROOT, 'artifacts', sourceName);
const actualHtmlById = new Map();
const astById = new Map();
let productionRendererErrorsById = new Map();
let semanticTarget = 'ast-projection';
let results = selectedCases.map(runCase);
await mkdir(artifactDirectory, { recursive: true });

let visualExecution = {
  enabled: false,
  result: 'not-run',
  total: 0,
  passed: 0,
  failed: 0,
  errors: 0,
  notPassed: 0,
  bySection: {},
  failures: [],
};
if (visualEnabled) {
  try {
    const { renderWithProductionWebRenderer } = await import(
      '../lib/visual/production-web-renderer.mjs'
    );
    const { compareVisualCases } = await import('../lib/visual/visual-compare.mjs');
    const productionRenderer = await renderWithProductionWebRenderer({
      cases: selectedCases,
      astById,
    });
    actualHtmlById.clear();
    for (const [id, html] of productionRenderer.htmlById) {
      actualHtmlById.set(id, html);
    }
    productionRendererErrorsById = productionRenderer.errorsById;
    semanticTarget = 'production-web-renderer-dom';
    results = selectedCases.map(compareProductionCase);
    visualExecution = {
      enabled: true,
      ...(await compareVisualCases({
        cases: selectedCases,
        actualHtmlById,
        rendererErrorsById: productionRenderer.errorsById,
        artifactDirectory,
        sectionName,
      })),
      renderer: productionRenderer.environment,
    };
  } catch (error) {
    visualExecution = {
      enabled: true,
      result: 'error',
      profile: `${sourceName}-visual-v1`,
      browser: null,
      total: selectedCases.length,
      passed: 0,
      failed: 0,
      errors: selectedCases.length,
      notPassed: selectedCases.length,
      bySection: {},
      failures: [{
        id: `${sourceName}-visual-environment`,
        section: 'Visual test environment',
        status: 'error',
        error: error.stack ?? error.message,
      }],
    };
  }
}
const failedCases = results.filter(result => result.status === 'fail');
const errors = results.filter(result => result.status === 'error');
const skippedCases = results.filter(result => result.status === 'skip' || result.skipped);
const notPassed = [...failedCases, ...errors];
const typeMismatchCount = failedCases.filter(result => result.typeDifference).length;
const sectionSummary = summarize(results, result => result.section);
const { failures: visualFailures, ...visualSummary } = visualExecution;
const overallNotPassedCases = new Set([
  ...notPassed.map(result => result.id),
  ...visualFailures.map(result => result.id),
]);
const generatedAt = new Date().toISOString();
const failureGroups = buildFailureGroups(notPassed, visualFailures, sectionName);
const baseline = compareWithBaseline({
  baseline: baselineDocument,
  baselinePath: BASELINE_PATH,
  sourceCommit: version.commit,
  parserProfile,
  comparisonTarget: semanticTarget,
  allCaseCount: document.cases.length,
  selectedCaseIds: selectedCases.map(testCase => testCase.id),
  semanticFailures: notPassed,
  visualFailures,
});
const runtime = buildRuntimeMetadata({
  repositoryRoot: REPOSITORY_ROOT,
  parserBinary,
  astById,
  workflowUrl: githubWorkflowUrl(),
});
const summary = {
  schemaVersion: 3,
  generatedAt,
  runtime,
  baseline,
  failureGroups,
  locale: 'en-US',
  result: notPassed.length === 0 && visualExecution.notPassed === 0 ? 'pass' : 'fail',
  gate: buildGate({
    mode: gateMode,
    baseline,
    // Union, not a sum: a case can fail both comparisons and must count once.
    notPassedCount: overallNotPassedCases.size,
    semanticErrorCount: errors.length,
    visualErrorCount: visualExecution.errors,
  }),
  source: sourceConfig.name,
  sourceDisplayName: sourceConfig.displayName ?? sourceConfig.name,
  profile: parserProfile,
  comparisonTarget: semanticTarget,
  sourceCommit: version.commit,
  parserBinary,
  total: results.length,
  passed: results.length - notPassed.length - skippedCases.length,
  failed: failedCases.length,
  errors: errors.length,
  skipped: skippedCases.length,
  notPassed: notPassed.length,
  typeMismatches: typeMismatchCount,
  overallNotPassedCases: overallNotPassedCases.size,
  bySection: Object.fromEntries(
    Object.entries(sectionSummary).map(([section, counts]) => [
      section,
      { sectionLabel: sectionName(section), ...counts },
    ])
  ),
  visual: visualSummary,
};
const evidenceById = await writeFailureEvidence({
  artifactDirectory,
  semanticFailures: notPassed,
  visualFailures,
  caseById,
  astById,
  actualHtmlById,
});
const semanticFailureRecords = attachEvidence(notPassed, evidenceById);
const visualFailureRecords = attachEvidence(visualFailures, evidenceById);
const issuePath = path.join(artifactDirectory, 'issue.md');
const issueMetadataPath = path.join(artifactDirectory, 'issue-metadata.json');
await mkdir(artifactDirectory, { recursive: true });
await writeFile(path.join(artifactDirectory, 'summary.json'), `${JSON.stringify(summary, null, 2)}\n`);
await writeFile(
  path.join(artifactDirectory, 'failures.json'),
  `${JSON.stringify(semanticFailureRecords, null, 2)}\n`
);
await writeFile(
  path.join(artifactDirectory, 'visual-failures.json'),
  `${JSON.stringify(visualFailureRecords, null, 2)}\n`
);
await writeFile(
  path.join(artifactDirectory, 'summary.md'),
  renderSummaryMarkdown(summary, semanticFailureRecords, visualFailureRecords)
);
await writeFile(
  path.join(artifactDirectory, 'report.html'),
  renderConformanceHtmlReport({
    summary,
    visualFailures,
    semanticFailures: semanticFailureRecords,
    caseById,
    sourceVersion: version.version,
  })
);
if (summary.result === 'fail') {
  const issueMetadata = buildConformanceIssueMetadata(summary);
  await Promise.all([
    writeFile(
      issuePath,
      renderConformanceIssue({
        summary,
        semanticFailures: semanticFailureRecords,
        visualFailures: visualFailureRecords,
        caseById,
        astById,
        actualHtmlById,
        sourceVersion: version.version,
      })
    ),
    writeFile(issueMetadataPath, JSON.stringify(issueMetadata, null, 2) + '\n'),
  ]);
} else {
  await Promise.all([
    rm(issuePath, { force: true }),
    rm(issueMetadataPath, { force: true }),
  ]);
}

console.log(`${sourceDisplayName} semantic comparison: passed ${summary.passed}/${summary.total}, skipped ${summary.skipped}, not passed ${summary.notPassed}`);
if (summary.visual.enabled) {
  console.log(`${sourceDisplayName} visual comparison: passed ${summary.visual.passed}/${summary.visual.total}, skipped ${summary.visual.skipped}, not passed ${summary.visual.notPassed}`);
} else {
  console.log(`${sourceDisplayName} visual comparison: not run (enable with run-visual.mjs ${sourceName})`);
}
console.log(`Summary: ${path.join(artifactDirectory, 'summary.md')}`);
console.log(`HTML report: ${path.join(artifactDirectory, 'report.html')}`);
if (summary.result === 'fail') console.log(`Issue body: ${issuePath}`);
console.log(`gate[${summary.gate.mode}]: ${summary.gate.failed ? 'FAIL' : 'PASS'} - ${summary.gate.reason}`);
if (summary.gate.failed && failOnFailures) process.exitCode = 1;

// Decide whether this run should fail the workflow, and say why in one place.
//
// A source is allowed to carry a standing set of known-failing cases (cmark-gfm
// starts at 58 semantic / 29 visual), so "any case failed" is the wrong gate:
// it makes main permanently red and tells a PR nothing about whether it made
// things worse. The gate that carries information is movement away from the
// recorded baseline.
//
// Two things still fail regardless of the baseline:
//   - execution errors, which mean the parser or the harness broke rather than
//     a case merely disagreeing. A panic must never be absorbed as "expected".
//   - an unusable baseline. Without one there is nothing to compare against, so
//     staying quiet would report a green run that checked nothing. This is also
//     what makes RUN_VISUAL=false loud instead of silently ungated: the visual
//     run and the baseline are keyed to production-web-renderer-dom, so a
//     semantic-only run lands on baseline-target-mismatch and fails here.
function buildGate({ mode, baseline, notPassedCount, semanticErrorCount, visualErrorCount }) {
  const errorCount = semanticErrorCount + visualErrorCount;
  if (errorCount > 0) {
    return {
      mode,
      failed: true,
      kind: 'execution-errors',
      reason: `${errorCount} execution error(s): parser or harness failure, not a case disagreement`,
      errorCount,
    };
  }
  if (mode === 'absolute') {
    return {
      mode,
      failed: notPassedCount > 0,
      kind: 'absolute',
      reason: `${notPassedCount} not-passing case(s) under absolute mode`,
      notPassedCount,
    };
  }
  if (!baseline.configured) {
    return {
      mode,
      failed: true,
      kind: 'baseline-unusable',
      reason: `cannot gate on regressions: baseline ${baseline.reason} (${baseline.path})`,
      baselineReason: baseline.reason,
    };
  }
  const added = baseline.overall.added;
  const resolved = baseline.overall.resolved;
  return {
    mode,
    failed: added.length > 0,
    kind: 'regression',
    reason: added.length > 0
      ? `${added.length} case(s) regressed against baseline: ${added.slice(0, 5).join(', ')}${added.length > 5 ? ', ...' : ''}`
      : `no regression against baseline (${resolved.length} resolved, ${baseline.overall.persistent.length} still failing)`,
    added,
    resolved,
    scope: baseline.scope,
  };
}

function runCase(testCase) {
  const parsed = spawnSync(parserBinary, parserArgsFor(testCase), {
    input: testCase.input.markdown,
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
  if (parsed.error || parsed.status !== 0) {
    return {
      id: testCase.id,
      section: testCase.source.section,
      status: 'error',
      exitCode: parsed.status,
      signal: parsed.signal,
      error:
        parsed.error?.message ||
        parsed.stderr.trim() ||
        `Parser exited with status ${parsed.status} and signal ${parsed.signal ?? 'none'}`,
    };
  }

  try {
    const ast = JSON.parse(parsed.stdout);
    astById.set(testCase.id, ast);
    const actualHtml = astToHtml(ast);
    actualHtmlById.set(testCase.id, actualHtml);
    return compareHtmlCase(testCase, ast, actualHtml);
  } catch (error) {
    return {
      id: testCase.id,
      section: testCase.source.section,
      status: 'error',
      error: error.stack ?? error.message,
    };
  }
}

function compareProductionCase(testCase) {
  const rendererErrors = productionRendererErrorsById.get(testCase.id);
  if (rendererErrors?.length) {
    return {
      id: testCase.id,
      section: testCase.source.section,
      status: 'error',
      stage: 'production-web-renderer',
      error: rendererErrors.join('\n'),
    };
  }
  const ast = astById.get(testCase.id);
  const actualHtml = actualHtmlById.get(testCase.id);
  if (!ast || actualHtml === undefined) {
    return {
      id: testCase.id,
      section: testCase.source.section,
      status: 'error',
      stage: 'production-web-renderer',
      error: 'The Supramark production web renderer did not produce actual HTML.',
    };
  }
  return compareHtmlCase(testCase, ast, actualHtml);
}

function compareHtmlCase(testCase, ast, actualHtml) {
  // cmark-gfm's test file marks a few crash-safety edge cases with an
  // `<IGNORE>` sentinel as the expected HTML, and test/spec_tests.py
  // auto-passes them. The sentinel is not a real rendering target, but the
  // cmark-gfm 0.29.0.gfm.13 binary does produce real HTML for these inputs;
  // lib/expected-overrides.mjs captures that real output and we compare
  // Supramark against it instead of skipping. See issue #144 (extensions-0020).
  const expected = effectiveExpected(testCase);
  if (expected.isIgnoreWithoutOverride) {
    // `<IGNORE>` with no binary override recorded: there is no real expected
    // HTML to compare against (the fixture's sentinel is not a rendering
    // target and no cmark-binary output was captured). Surface this as a
    // skipped case rather than inflating `passed`, so the summary cannot
    // claim a comparison that never happened.
    return {
      id: testCase.id,
      section: testCase.source.section,
      status: 'skip',
      skipped: 'ignore-sentinel',
    };
  }
  const expectedTree = htmlToSemanticTree(expected.html);
  const actual = htmlToSemanticTree(actualHtml);
  const difference = findFirstDifference(expectedTree, actual);
  const actualSemanticTypes = collectSemanticTypesFromTree(actual);
  const actualNodeTypes = collectAstTypes(ast);
  const typeDifference = compareTypes(expected.semanticTypes, actualSemanticTypes);
  return {
    id: testCase.id,
    section: testCase.source.section,
    status: difference || typeDifference ? 'fail' : 'pass',
    expectedSemanticTypes: expected.semanticTypes,
    actualSemanticTypes,
    actualNodeTypes,
    ...(expected.isIgnoreOverride ? { ignoreOverride: true } : {}),
    ...(typeDifference ? { typeDifference } : {}),
    ...(difference ? { difference } : {}),
  };
}

function renderSummaryMarkdown(summaryDocument, semanticFailures, visualFailures) {
  const lines = [
    `# ${sourceDisplayName} semantic & visual conformance summary`,
    '',
    `- Overall result: **${summaryDocument.result}**`,
    `- Source: ${sourceDisplayName} ${version.version}`,
    `- Pinned commit: \`${summaryDocument.sourceCommit}\``,
    `- Parser profile: \`${summaryDocument.profile}\``,
    `- Semantic comparison target: ${summaryDocument.comparisonTarget}`,
    `- Total cases: ${summaryDocument.total}`,
    `- Cases with at least one difference: ${summaryDocument.overallNotPassedCases}`,
    '',
    '## Semantic comparison results',
    '',
    `- Passed: ${summaryDocument.passed}`,
    `- Skipped (\`<IGNORE>\` without override): ${summaryDocument.skipped}`,
    `- Semantic differences: ${summaryDocument.failed}`,
    `- Execution errors: ${summaryDocument.errors}`,
    `- Render type mismatches: ${summaryDocument.typeMismatches}`,
    '',
    '### Semantic results by section',
    '',
    '| Section | Total | Passed | Skipped | Semantic diff | Execution error |',
    '| --- | ---: | ---: | ---: | ---: | ---: |',
  ];
  for (const [section, counts] of Object.entries(summaryDocument.bySection)) {
    lines.push(
      `| ${escapeTableCell(`${counts.sectionLabel} (${section})`)} | ${counts.total} | ${counts.passed} | ${counts.skipped} | ${counts.failed} | ${counts.errors} |`
    );
  }
  lines.push('', '### Not-passing semantic cases', '');
  if (semanticFailures.length === 0) {
    lines.push('All semantic cases passed.');
  } else {
    lines.push('| Case | Section | Category | First diff location |', '| --- | --- | --- | --- |');
    for (const failure of semanticFailures) {
      lines.push(
        `| \`${failure.id}\` | ${escapeTableCell(sectionName(failure.section))} | ${failureCategory(failure)} | \`${escapeTableCell(failure.difference?.path ?? '-')}\` |`
      );
    }
  }

  lines.push('', '## Browser visual comparison results', '');
  if (!summaryDocument.visual.enabled) {
    lines.push(`Visual comparison was not enabled for this run. Run \`node tests/markdown-conformance/scripts/run-visual.mjs ${sourceName}\` to enable it.`);
  } else {
    lines.push(
      `- Test result: **${summaryDocument.visual.result}**`,
      '- HTML visual report: [open report](./report.html)',
      `- Browser: Chromium ${summaryDocument.visual.browser?.version ?? 'failed to launch'}`,
      `- Actual rendering implementation: ${summaryDocument.visual.renderer?.implementation ?? 'not loaded'}`,
      `- Style profile: \`${summaryDocument.visual.profile}\``,
      `- Pinned width: ${summaryDocument.visual.viewport?.width ?? '-'}px`,
      `- Passed: ${summaryDocument.visual.passed}/${summaryDocument.visual.total}`,
      `- Skipped (\`<IGNORE>\` without override): ${summaryDocument.visual.skipped}`,
      `- Pixel differences: ${summaryDocument.visual.failed}`,
      `- Execution errors: ${summaryDocument.visual.errors}`,
      '',
      '### Visual results by section',
      '',
      '| Section | Total | Passed | Skipped | Pixel diff | Execution error |',
      '| --- | ---: | ---: | ---: | ---: | ---: |'
    );
    for (const [section, counts] of Object.entries(summaryDocument.visual.bySection ?? {})) {
      lines.push(
        `| ${escapeTableCell(`${counts.sectionLabel} (${section})`)} | ${counts.total} | ${counts.passed} | ${counts.skipped} | ${counts.failed} | ${counts.errors} |`
      );
    }
    lines.push('', '### Not-passing visual cases', '');
    if (visualFailures.length === 0) {
      lines.push('All visual cases passed.');
    } else {
      lines.push('| Case | Section | Category | Diff pixels | Diff ratio | Images |', '| --- | --- | --- | ---: | ---: | --- |');
      for (const failure of visualFailures) {
        const images = failure.images
          ? `[expected](${failure.images.expected}) &middot; [actual](${failure.images.actual}) &middot; [diff](${failure.images.diff})`
          : '-';
        lines.push(
          `| \`${failure.id}\` | ${escapeTableCell(sectionName(failure.section))} | ${visualFailureCategory(failure)} | ${failure.diffPixels ?? '-'} | ${formatPercent(failure.diffRatio)} | ${images} |`
        );
      }

      const failuresWithImages = visualFailures.filter(failure => failure.images);
      if (failuresWithImages.length > 0) {
        lines.push(
          '',
          '#### Visual diff images',
          '',
          'Shown below as expected / actual / diff, in that order; click an image to view it at full size.',
          ''
        );
        for (const failure of failuresWithImages) {
          lines.push(
            `**\`${failure.id}\` — ${sectionName(failure.section)}**`,
            '',
            '| Expected | Actual | Diff |',
            '| --- | --- | --- |',
            `| [![expected: ${failure.id}](${failure.images.expected})](${failure.images.expected}) | [![actual: ${failure.id}](${failure.images.actual})](${failure.images.actual}) | [![diff: ${failure.id}](${failure.images.diff})](${failure.images.diff}) |`,
            ''
          );
        }
      }
    }
  }
  lines.push('', 'See `summary.json`, `failures.json`, and `visual-failures.json` for the full machine-readable data.', '');
  return `${lines.join('\n')}\n`;
}

function visualFailureCategory(failure) {
  return failure.status === 'error' ? 'Visual execution error' : 'Browser screenshot mismatch';
}

function formatPercent(value) {
  return Number.isFinite(value) ? `${(value * 100).toFixed(4)}%` : '-';
}

function collectAstTypes(root) {
  const result = [];
  const seen = new Set();
  function walk(node) {
    if (!seen.has(node.type)) {
      seen.add(node.type);
      result.push(node.type);
    }
    for (const child of node.children ?? []) walk(child);
  }
  walk(root);
  return result.filter(type => type !== 'root');
}

function compareTypes(expectedTypes, actualTypes) {
  const expected = new Set(expectedTypes);
  const actual = new Set(actualTypes);
  const missing = expectedTypes.filter(type => !actual.has(type));
  const unexpected = actualTypes.filter(type => !expected.has(type));
  return missing.length > 0 || unexpected.length > 0 ? { missing, unexpected } : null;
}

function summarize(values, getKey) {
  const result = {};
  for (const value of values) {
    const key = getKey(value);
    result[key] ??= { total: 0, passed: 0, failed: 0, errors: 0, skipped: 0 };
    result[key].total += 1;
    if (value.status === 'skip' || value.skipped) result[key].skipped += 1;
    else if (value.status === 'pass') result[key].passed += 1;
    else if (value.status === 'error') result[key].errors += 1;
    else result[key].failed += 1;
  }
  return result;
}

function failureCategory(failure) {
  if (failure.status === 'error') return 'Execution error';
  if (failure.typeDifference) return 'Render type mismatch';
  const reasons = {
    value: 'Text or attribute value mismatch',
    type: 'Value type mismatch',
    'array-type': 'Node collection type mismatch',
    'array-length': 'Child count mismatch',
    'object-keys': 'Node structure or attribute mismatch',
  };
  return reasons[failure.difference?.reason] ?? 'Semantic structure mismatch';
}

function sectionName(section) {
  return SECTION_NAMES[section] ?? section;
}

function escapeTableCell(value) {
  return String(value).replaceAll('|', '\\|').replaceAll('\n', '<br>');
}

function githubWorkflowUrl() {
  const server = process.env.GITHUB_SERVER_URL;
  const repository = process.env.GITHUB_REPOSITORY;
  const runId = process.env.GITHUB_RUN_ID;
  return server && repository && runId ? `${server}/${repository}/actions/runs/${runId}` : null;
}
