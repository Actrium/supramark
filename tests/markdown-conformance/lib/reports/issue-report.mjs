import { htmlToSemanticTree } from '../semantic/html-semantics.mjs';

const REPRESENTATIVE_LIMIT = 5;
const DETAIL_LIMIT = 4_000;

export function buildConformanceIssueMetadata(summary) {
  const source = summary.source;
  const sourceDisplayName = summary.sourceDisplayName ?? source;
  return {
    schemaVersion: 1,
    source,
    title: '[' + sourceDisplayName + '] Conformance failures: not-passing cases found',
    labels: ['bug'],
    // Load-bearing: .github/workflows/markdown-conformance.yml matches
    // existing issues by this marker substring. Do not change it without
    // updating the workflow in the same change.
    marker: '<!-- supramark-' + source + '-conformance -->',
  };
}

export function renderConformanceIssue({
  summary,
  semanticFailures,
  visualFailures,
  caseById,
  astById,
  actualHtmlById,
  sourceVersion,
}) {
  const metadata = buildConformanceIssueMetadata(summary);
  const sourceName = summary.source;
  const sourceDisplayName = summary.sourceDisplayName ?? sourceName;
  const visualEnabled = summary.visual?.enabled === true;
  const comparisonScope = visualEnabled ? 'semantic and visual' : 'semantic';
  const runCommand = `node tests/markdown-conformance/scripts/${visualEnabled ? 'run-visual.mjs' : 'run.mjs'} ${sourceName}`;
  const visualProblemDescription = visualEnabled
    ? `${summary.visual.notPassed} not-passing case(s) at the visual layer`
    : 'the visual layer was not run';
  const visualResult = visualEnabled
    ? `**${summary.visual.passed}/${summary.visual.total} passed**, ${summary.visual.notPassed} not passed`
    : '**not run**';
  const workflowUrl = summary.runtime?.workflowUrl;
  const representatives = selectRepresentatives(
    summary.failureGroups ?? [],
    semanticFailures,
    visualFailures
  );
  const representativeId =
    representatives[0]?.id ?? semanticFailures[0]?.id ?? visualFailures[0]?.id ?? sourceName;
  const lines = [
    metadata.marker,
    `# CommonMark ${comparisonScope} conformance test failures`,
    '',
    '## **Problem description**',
    '',
    `Supramark ran ${summary.total} CommonMark ${sourceVersion} spec case(s) using the \`${summary.profile ?? 'supramark-default'}\` parser configuration.`,
    `${summary.notPassed} case(s) not passed at the semantic layer; ${visualProblemDescription}; ${summary.overallNotPassedCases} case(s) have at least one kind of difference in total.`,
    '',
    '### Run summary',
    '',
    '| Item | Result |',
    '| --- | --- |',
    `| Supramark commit | \`${escapeTableCell(summary.runtime?.supramarkCommit ?? 'unknown')}\` |`,
    `| Local branch/ref | \`${escapeTableCell(summary.runtime?.gitRef ?? 'unknown')}\` |`,
    `| Workspace state | ${summary.runtime?.workspaceDirty ? '**has uncommitted changes**' : 'clean'} |`,
    `| CommonMark source | ${sourceVersion} / \`${summary.sourceCommit}\` |`,
    `| Comparison target | \`${summary.comparisonTarget}\` |`,
    `| Parser | ${escapeTableCell(summary.runtime?.parserName ?? 'supramark-markdown')} ${escapeTableCell(summary.runtime?.parserVersion ?? 'unknown')} |`,
    `| Renderer | \`${escapeTableCell(summary.visual.renderer?.implementation ?? 'not loaded')}\` |`,
    `| Browser | Chromium ${escapeTableCell(summary.visual.browser?.version ?? 'not run')} |`,
    `| Platform | ${escapeTableCell(summary.runtime?.platform ?? 'unknown')} / ${escapeTableCell(summary.runtime?.arch ?? 'unknown')} / Node ${escapeTableCell(summary.runtime?.nodeVersion ?? 'unknown')} |`,
    `| Semantic result | **${summary.passed}/${summary.total} passed**, ${summary.notPassed} not passed |`,
    `| Visual result | ${visualResult} |`,
    `| Generated at | ${summary.generatedAt} |`,
    ...(workflowUrl ? [`| Actions run | [open run](${workflowUrl}) |`] : []),
    '',
    ...renderBaselineDelta(summary.baseline),
    '### Failing feature clusters',
    '',
    `| Feature cluster | Unique failing cases | Semantic not passed | ${visualEnabled ? 'Visual not passed' : 'Visual (not run)'} | Suspected layer |`,
    '| --- | ---: | ---: | ---: | --- |',
  ];

  for (const group of summary.failureGroups ?? []) {
    lines.push(
      `| ${escapeTableCell(`${group.sectionLabel} (${group.section})`)} | ${group.uniqueCases} | ${group.semanticNotPassed} | ${visualEnabled ? group.visualNotPassed : '-'} | ${escapeTableCell(group.suspectedLayer)} |`
    );
  }
  if ((summary.failureGroups ?? []).length === 0) {
    lines.push(`| - | 0 | 0 | ${visualEnabled ? 0 : '-'} | - |`);
  }

  lines.push(
    '',
    '## **Reproduction steps**',
    '',
    '### Full reproduction',
    '',
    '1. Install case dependencies from the repository root: `pnpm --dir tests/markdown-conformance install --frozen-lockfile`.',
    visualEnabled
      ? '2. Install the pinned Chromium build: `node tests/markdown-conformance/node_modules/playwright/cli.js install chromium`.'
      : '2. This run only performs semantic comparison, so Chromium does not need to be installed.',
    '3. Build the parser: `cargo build -p supramark-markdown --bin supramark-markdown`.',
    '4. Import the pinned data source: `node tests/markdown-conformance/scripts/import.mjs commonmark`.',
    '5. Validate the unified cases: `node tests/markdown-conformance/scripts/validate.mjs commonmark`.',
    `6. Run the ${comparisonScope} comparison: \`${runCommand}\`.`,
    '',
    '### Reproducing a single case (does not overwrite the full report)',
    '',
    '**Bash (Linux/macOS):**',
    '',
    fencedCode([
      `export CASE_IDS="${representativeId}"`,
      'export FAIL_ON_FAILURES="0"',
      'export ARTIFACT_DIR="tests/markdown-conformance/artifacts/manual/$CASE_IDS"',
      runCommand,
      'unset CASE_IDS FAIL_ON_FAILURES ARTIFACT_DIR',
    ].join('\n'), 'bash'),
    '',
    '**PowerShell (Windows):**',
    '',
    fencedCode([
      `$env:CASE_IDS = "${representativeId}"`,
      '$env:FAIL_ON_FAILURES = "0"',
      '$env:ARTIFACT_DIR = "tests/markdown-conformance/artifacts/manual/$env:CASE_IDS"',
      runCommand,
      'Remove-Item Env:CASE_IDS',
      'Remove-Item Env:FAIL_ON_FAILURES',
      'Remove-Item Env:ARTIFACT_DIR',
    ].join('\n'), 'powershell'),
    '',
    '## **Expected result**',
    '',
    `- **Spec semantics**: the final HTML/DOM semantics of all ${summary.total} cases match the expected HTML from the CommonMark spec.`,
    visualEnabled
      ? '- **Derived visuals**: under the same Chromium, CSS, viewport, and font environment, screenshots of the expected HTML and the Supramark actual HTML match.'
      : '- **Derived visuals**: visual testing was not run this time, so whether the screenshots match cannot be determined.',
    '',
    '> CommonMark does not prescribe CSS or a product theme; the visual layer is an aid for spotting differences in the final DOM, content, and structure.',
    '',
    '## **Actual result**',
    '',
    visualEnabled
      ? `Production web renderer DOM semantics passed ${summary.passed}/${summary.total} case(s); visual passed ${summary.visual.passed}/${summary.visual.total} case(s).`
      : `Semantic passed ${summary.passed}/${summary.total} case(s); visual testing was not run.`,
    '',
    ...renderCompleteResultsInstructions({ summary, workflowUrl }),
    '### Representative failing cases',
    '',
    'These cases are included only to help triage quickly from within the issue; one is picked per failing feature cluster. See the previous section for how to download the `commonmark-conformance-report` artifact for the full results and evidence.',
    '',
  );

  for (const representative of representatives) {
    lines.push(...renderRepresentative({
      representative,
      testCase: caseById.get(representative.id),
      ast: astById.get(representative.id),
      actualHtml: actualHtmlById.get(representative.id),
      visualEnabled,
    }));
  }

  if (representatives.length === 0) lines.push('No failing cases to display.', '');
  lines.push(
    '### Download package contents',
    '',
    '- `summary.md`: the full summary.',
    visualEnabled ? '- `report.html`: a filterable visual report.' : '- `report.html`: the HTML summary report for this semantic-only run.',
    '- `failures.json` / `visual-failures.json`: machine-readable detail.',
    '- `evidence/<case ID>/`: actual AST, actual HTML, and expected/actual semantic trees.',
    ...(visualEnabled ? ['- `visual/<case ID>/`: expected, actual, and pixel-diff images.'] : []),
    ...(workflowUrl ? [`- [open the Actions run and download the full artifact](${workflowUrl}).`] : []),
    '',
    `Generated at: ${summary.generatedAt}`,
    ''
  );
  return `${lines.join('\n')}\n`
    .replaceAll('CommonMark', sourceDisplayName)
    .replaceAll('commonmark', sourceName);
}

function renderCompleteResultsInstructions({ summary, workflowUrl }) {
  const visualEnabled = summary.visual?.enabled === true;
  const comparisonScope = visualEnabled ? 'semantic and visual' : 'semantic';
  const packageDescription = visualEnabled
    ? `The issue body only shows ${REPRESENTATIVE_LIMIT} representative failing case(s). The full test package contains the summary for all ${summary.total} case(s), every semantic failure's detail, every visual failure's detail, screenshots, and per-case evidence.`
    : `The issue body only shows ${REPRESENTATIVE_LIMIT} representative failing case(s). The full test package contains the summary for all ${summary.total} case(s), every semantic failure's detail, and per-case evidence; visual testing was not run, so it does not include visual screenshots.`;
  const openRunStep = workflowUrl
    ? `1. Open [this GitHub Actions run](${workflowUrl}). Even if the workflow ultimately shows as failed, the report artifact is still uploaded to the run.`
    : `1. Open the repository's **Actions** page and go to this **CommonMark ${comparisonScope} conformance verification** run.`;
  return [
    '## **Download and review the full test results**',
    '',
    packageDescription,
    '',
    openRunStep,
    '2. On the run\'s **Summary** page, scroll to the bottom to find the **Artifacts** section.',
    '3. Click to download the artifact named **`commonmark-conformance-report`**; your browser will download a ZIP file.',
    visualEnabled
      ? '4. **Extract the ZIP in full into the same directory.** Do not copy or move only `report.html`, or the `visual/` screenshots and `evidence/` it references may fail to display.'
      : '4. **Extract the ZIP in full into the same directory.** Do not copy or move only `report.html`, or the `evidence/` it references may fail to display.',
    visualEnabled
      ? '5. Double-click **`report.html`** at the root of the extracted directory to view the summary in your browser, filter through every failing visual case, and compare the expected, actual, and diff images.'
      : '5. Double-click **`report.html`** at the root of the extracted directory to view the summary for this semantic-only run in your browser.',
    visualEnabled
      ? '6. Open **`summary.md`** to see the full list of semantic and visual failures; for a case-by-case check, go into **`evidence/<case ID>/`** to see the Markdown, expected/actual HTML, AST, and normalized semantic tree.'
      : '6. Open **`summary.md`** to see the full list of semantic failures; for a case-by-case check, go into **`evidence/<case ID>/`** to see the Markdown, expected/actual HTML, AST, and normalized semantic tree.',
    '',
    '### Opening it locally',
    '',
    '**Windows PowerShell**',
    '',
    fencedCode([
      'Expand-Archive -LiteralPath .\\commonmark-conformance-report.zip -DestinationPath .\\commonmark-conformance-report',
      'Start-Process .\\commonmark-conformance-report\\report.html',
    ].join('\n'), 'powershell'),
    '',
    '**macOS / Linux**',
    '',
    fencedCode([
      'unzip commonmark-conformance-report.zip -d commonmark-conformance-report',
      '# macOS',
      'open commonmark-conformance-report/report.html',
      '# Linux',
      'xdg-open commonmark-conformance-report/report.html',
    ].join('\n'), 'bash'),
    '',
    '> Artifacts are currently retained for 30 days. Once that window has passed, open a more recent run to download from, or re-run the workflow.',
    '',
  ];
}
function renderBaselineDelta(baseline) {
  if (!baseline?.configured) {
    return [
      '### Comparison against the approved baseline',
      '',
      '> No approved baseline is configured, so this run cannot distinguish new, resolved, and persistent failures.',
      '',
    ];
  }
  const lines = [
    '### Comparison against the approved baseline',
    '',
    `Baseline: \`${baseline.path}\`, scope: ${baseline.scope === 'selected' ? 'cases selected for this run' : 'all cases'}.`,
    '',
    '| Comparison layer | New failures | Resolved | Persistent failures |',
    '| --- | ---: | ---: | ---: |',
    `| Semantic | ${baseline.semantic.added.length} | ${baseline.semantic.resolved.length} | ${baseline.semantic.persistent.length} |`,
    `| Visual | ${baseline.visual.added.length} | ${baseline.visual.resolved.length} | ${baseline.visual.persistent.length} |`,
    `| Unique failing cases | ${baseline.overall.added.length} | ${baseline.overall.resolved.length} | ${baseline.overall.persistent.length} |`,
    '',
  ];
  const changed = [
    ['New failures', baseline.overall.added],
    ['Resolved', baseline.overall.resolved],
  ].filter(([, ids]) => ids.length > 0);
  if (changed.length > 0) {
    lines.push('<details>', '<summary>View case IDs for the baseline changes</summary>', '');
    for (const [label, ids] of changed) {
      lines.push(`- ${label}: ${ids.map(id => `\`${id}\``).join(', ')}`);
    }
    lines.push('', '</details>', '');
  }
  return lines;
}

function renderRepresentative({ representative, testCase, ast, actualHtml, visualEnabled }) {
  const { semantic, visual } = representative;
  const expectedSemantic = testCase ? htmlToSemanticTree(testCase.expected.html) : null;
  const actualSemantic = actualHtml === undefined ? null : htmlToSemanticTree(actualHtml);
  const upstreamUrl = testCase ? sourceUrl(testCase.source) : null;
  const evidence = semantic?.evidence ?? visual?.evidence;
  const lines = [
    '<details>',
    `<summary><code>${representative.id}</code> &middot; ${escapeInline(representative.sectionLabel)} &middot; ${escapeInline(failureHeadline(semantic, visual))}</summary>`,
    '',
    '| Item | Content |',
    '| --- | --- |',
    `| Feature cluster | ${escapeTableCell(`${representative.sectionLabel} (${representative.section})`)} |`,
    `| Suspected layer | ${escapeTableCell(representative.suspectedLayer)} |`,
    `| Spec location | ${upstreamUrl ? `[${escapeTableCell(`${testCase.source.path} L${testCase.source.startLine}–L${testCase.source.endLine}`)}](${upstreamUrl})` : '-'} |`,
    `| Semantic types | expected: ${inlineCodeList(semantic?.expectedSemanticTypes)}; actual: ${inlineCodeList(semantic?.actualSemanticTypes)} |`,
    `| First diff | ${semantic?.difference ? `\`${escapeTableCell(semantic.difference.path)}\` (${escapeTableCell(semantic.difference.reason)})` : '-'} |`,
    `| Visual diff | ${visual ? `${visual.diffPixels ?? '-'} px / ${formatPercent(visual.diffRatio)}` : visualEnabled ? 'no visual failure' : 'visual testing not run'} |`,
    `| Evidence directory | ${evidence ? `\`${escapeTableCell(evidence.directory)}\`` : '-'} |`,
    '',
  ];
  if (testCase) {
    lines.push('#### Markdown input', '', fencedCode(testCase.input.markdown, 'markdown'), '');
    lines.push('#### CommonMark expected HTML', '', fencedCode(testCase.expected.html, 'html'), '');
  }
  lines.push('#### Supramark actual HTML', '', fencedCode(actualHtml ?? 'no actual HTML was produced', 'html'), '');
  lines.push('#### Supramark parser AST', '', fencedCode(prettyLimited(ast), 'json'), '');
  lines.push('#### Normalized semantic tree', '');
  lines.push('Expected:', '', fencedCode(prettyLimited(expectedSemantic), 'json'), '');
  lines.push('Actual:', '', fencedCode(prettyLimited(actualSemantic), 'json'), '');
  if (visual?.images) {
    lines.push(
      `Visual images are in the artifact: \`${visual.images.expected}\`, \`${visual.images.actual}\`, \`${visual.images.diff}\`.`,
      ''
    );
  }
  lines.push('</details>', '');
  return lines;
}

function selectRepresentatives(groups, semanticFailures, visualFailures) {
  const semanticById = new Map(semanticFailures.map(failure => [failure.id, failure]));
  const visualById = new Map(visualFailures.map(failure => [failure.id, failure]));
  const selected = [];
  const selectedIds = new Set();
  for (const group of groups) {
    const candidates = [...new Set([
      ...semanticFailures.filter(failure => failure.section === group.section).map(failure => failure.id),
      ...visualFailures.filter(failure => failure.section === group.section).map(failure => failure.id),
    ])].sort((left, right) => {
      const leftBoth = Number(semanticById.has(left) && visualById.has(left));
      const rightBoth = Number(semanticById.has(right) && visualById.has(right));
      return rightBoth - leftBoth || left.localeCompare(right);
    });
    const id = candidates.find(candidate => !selectedIds.has(candidate));
    if (!id) continue;
    selectedIds.add(id);
    selected.push({
      id,
      section: group.section,
      sectionLabel: group.sectionLabel,
      suspectedLayer: group.suspectedLayer,
      semantic: semanticById.get(id),
      visual: visualById.get(id),
    });
    if (selected.length >= REPRESENTATIVE_LIMIT) break;
  }
  return selected;
}

function sourceUrl(source) {
  const repository = String(source.repository).replace(/\.git$/, '');
  return `${repository}/blob/${source.revision}/${source.path}#L${source.startLine}-L${source.endLine}`;
}

function failureHeadline(semantic, visual) {
  if (semantic?.status === 'error') return 'Semantic execution error';
  if (semantic?.typeDifference) return 'Render type mismatch';
  if (semantic?.difference) return 'DOM semantic mismatch';
  if (visual?.status === 'error') return 'Visual execution error';
  return 'Browser screenshot mismatch';
}

function fencedCode(value, language = '') {
  const content = String(value ?? '');
  const longest = Math.max(2, ...([...content.matchAll(/`+/g)].map(match => match[0].length)));
  const fence = '`'.repeat(longest + 1);
  return `${fence}${language}\n${content}\n${fence}`;
}

function prettyLimited(value) {
  const content = value === null || value === undefined
    ? 'not generated'
    : JSON.stringify(value, null, 2);
  if (content.length <= DETAIL_LIMIT) return content;
  return `${content.slice(0, DETAIL_LIMIT)}\n...(content truncated; see the evidence directory for the full file)`;
}

function inlineCodeList(values) {
  return values?.length ? values.map(value => `\`${escapeTableCell(value)}\``).join(', ') : '-';
}

function formatPercent(value) {
  return Number.isFinite(value) ? `${(value * 100).toFixed(4)}%` : '-';
}

function escapeTableCell(value) {
  return String(value ?? '').replaceAll('|', '\\|').replaceAll('\n', '<br>');
}

function escapeInline(value) {
  return String(value ?? '').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
}
