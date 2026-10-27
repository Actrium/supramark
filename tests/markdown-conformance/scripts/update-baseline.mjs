import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SUITE_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const REPOSITORY_ROOT = path.resolve(SUITE_ROOT, '..', '..');
const sourceName = process.argv[2];
if (!sourceName || !/^[a-z0-9][a-z0-9-]*$/.test(sourceName)) {
  throw new Error('Usage: node tests/markdown-conformance/scripts/update-baseline.mjs <source-name>');
}
const artifactDirectory = path.join(SUITE_ROOT, 'artifacts', sourceName);
const baselinePath = path.join(SUITE_ROOT, 'baselines', `${sourceName}.json`);
const fixtureDirectory = path.join(REPOSITORY_ROOT, 'tests', 'cases', '_fixtures', sourceName);

const [summary, semanticFailures, visualFailures, version] = await Promise.all([
  readJson(path.join(artifactDirectory, 'summary.json')),
  readJson(path.join(artifactDirectory, 'failures.json')),
  readJson(path.join(artifactDirectory, 'visual-failures.json')),
  readJson(path.join(fixtureDirectory, 'version.json')),
]);

if (summary.source !== sourceName || version.source !== sourceName) {
  throw new Error(`Source mismatch: argument ${sourceName}, report ${summary.source}, version ${version.source}`);
}
if (summary.total !== version.caseCount) {
  throw new Error(`Refusing to update baseline from a partial run: report has ${summary.total} case(s), source has ${version.caseCount}.`);
}
if (!summary.visual?.enabled || summary.errors !== 0 || summary.visual.errors !== 0) {
  throw new Error('Refusing to update baseline from an unhealthy run: visual comparison must have run, and semantic/visual execution errors must both be 0.');
}
if (summary.sourceCommit !== version.commit) {
  throw new Error(`Refusing to update baseline: source commit mismatch (${summary.sourceCommit} != ${version.commit})`);
}

const baseline = {
  schemaVersion: 2,
  source: sourceName,
  sourceVersion: version.version,
  sourceCommit: version.commit,
  caseCount: version.caseCount,
  parserProfile: summary.profile,
  comparisonTarget: summary.comparisonTarget,
  semanticFailureIds: uniqueSorted(semanticFailures.map(failure => failure.id)),
  visualFailureIds: uniqueSorted(visualFailures.map(failure => failure.id)),
};

// Preserve the hand-maintained `notes` (the `_summary` plus per-case entries,
// in the cmark-gfm.json style): the failure lists above are regenerated from
// the run, but the notes explain each entry — resolved cases and the real
// reason a still-failing case fails — and are only ever updated by hand.
// Regenerating the baseline must not wipe them.
let previousNotes;
try {
  previousNotes = JSON.parse(await readFile(baselinePath, 'utf8')).notes;
} catch {
  // No existing baseline (or unreadable) — start without notes.
}
if (previousNotes) {
  baseline.notes = previousNotes;
}

await mkdir(path.dirname(baselinePath), { recursive: true });
await writeFile(baselinePath, `${JSON.stringify(baseline, null, 2)}\n`, 'utf8');
console.log(
  `Updated ${summary.sourceDisplayName ?? sourceName} baseline: semantic ${baseline.semanticFailureIds.length}, visual ${baseline.visualFailureIds.length} -> ${baselinePath}`
);

async function readJson(filePath) {
  return JSON.parse(await readFile(filePath, 'utf8'));
}

function uniqueSorted(values) {
  return [...new Set(values)].sort();
}
