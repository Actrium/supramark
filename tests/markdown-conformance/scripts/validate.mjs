import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  explicitInputConfigs,
  inputGlobConfigs,
  isConfiguredInputPath,
  matchesInputGlob,
} from '../lib/source-fixtures.mjs';

const SUITE_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const REPOSITORY_ROOT = path.resolve(SUITE_ROOT, '..', '..');
const FIXTURES_ROOT = path.join(REPOSITORY_ROOT, 'tests', 'cases', '_fixtures');
const sourceName = process.argv[2];

if (!sourceName || !/^[a-z0-9][a-z0-9-]*$/.test(sourceName)) {
  console.error('Usage: node tests/markdown-conformance/scripts/validate.mjs <source-name>');
  process.exitCode = 2;
} else {
  await validate(sourceName);
}

async function validate(name) {
  const fixtureDirectory = path.join(FIXTURES_ROOT, name);
  const sourceConfig = JSON.parse(
    await readFile(path.join(SUITE_ROOT, 'config', 'sources', `${name}.json`), 'utf8')
  );
  const casesText = await readFile(path.join(fixtureDirectory, 'cases.json'), 'utf8');
  const version = JSON.parse(await readFile(path.join(fixtureDirectory, 'version.json'), 'utf8'));
  const document = JSON.parse(casesText);

  assert(document.schemaVersion === 1, 'unsupported cases schema version');
  assert(document.source === name, 'cases source name mismatch');
  assert(document.source === version.source, 'cases/version source mismatch');
  assert(document.profile === sourceConfig.profile, 'cases profile mismatch');
  assert(Array.isArray(document.cases), 'cases.json must contain a cases array');
  assert(document.cases.length === version.caseCount, 'case count does not match version.json');
  assert(document.cases.length > 0, 'no cases were imported');
  assert(version.repository === sourceConfig.repository, 'source repository mismatch');
  assert(version.version === sourceConfig.version, 'source version mismatch');
  assert(version.commit === sourceConfig.revision, 'configured source commit mismatch');
  const explicitPaths = explicitInputConfigs(sourceConfig).map(input => input.path);
  const inputGlobs = inputGlobConfigs(sourceConfig);
  const versionFixtures = version.fixtures ?? [
    { path: version.fixture, sourceSha256: version.sourceSha256, caseCount: version.caseCount },
  ];
  const versionPaths = versionFixtures.map(fixture => fixture.path);
  if (inputGlobs.length === 0) {
    assert(JSON.stringify(versionPaths) === JSON.stringify(explicitPaths), 'configured fixture paths mismatch');
  } else {
    for (const explicitPath of explicitPaths) {
      assert(versionPaths.includes(explicitPath), `configured fixture is missing: ${explicitPath}`);
    }
    for (const inputGlob of inputGlobs) {
      assert(
        versionPaths.some(filePath => matchesInputGlob(filePath, inputGlob)),
        `configured fixture glob is empty: ${inputGlob.pattern}`
      );
    }
    assert(
      JSON.stringify(version.fixturePatterns) ===
        JSON.stringify(inputGlobs.map(inputGlob => inputGlob.pattern)),
      'configured fixture patterns mismatch'
    );
    assert(
      JSON.stringify(version.fixtureExcludes) ===
        JSON.stringify(inputGlobs.map(inputGlob => inputGlob.exclude ?? [])),
      'configured fixture exclusions mismatch'
    );
  }
  assert(new Set(versionPaths).size === versionPaths.length, 'version.json contains duplicate fixture paths');
  for (const fixture of versionFixtures) {
    assert(isConfiguredInputPath(fixture.path, sourceConfig), `${fixture.path}: unconfigured fixture path`);
    assert(/^[0-9a-f]{64}$/.test(fixture.sourceSha256), `${fixture.path}: invalid source SHA-256`);
    assert(Number.isInteger(fixture.caseCount) && fixture.caseCount >= 0, `${fixture.path}: invalid case count`);
  }
  assert(
    versionFixtures.reduce((total, fixture) => total + fixture.caseCount, 0) === version.caseCount,
    'fixture case counts do not add up to the source case count'
  );
  assert(version.license === sourceConfig.license, 'source license mismatch');
  assert(/^[0-9a-f]{40}$/.test(version.commit), 'version.json does not contain a full commit');
  assert(/^[0-9a-f]{64}$/.test(version.sourceSha256), 'invalid source SHA-256');

  const ids = new Set();
  const upstreamIds = new Set();
  const sectionCounts = new Map();
  const fixtureCounts = new Map();
  for (const testCase of document.cases) {
    assert(testCase.schemaVersion === 1, `${testCase.id}: unsupported case schema version`);
    assert(/^[a-z0-9][a-z0-9._-]*$/.test(testCase.id), `${testCase.id}: invalid case ID`);
    assert(!ids.has(testCase.id), `${testCase.id}: duplicate case ID`);
    ids.add(testCase.id);
    assert(testCase.source.name === name, `${testCase.id}: source name mismatch`);
    assert(testCase.source.repository === version.repository, `${testCase.id}: repository mismatch`);
    assert(testCase.source.version === version.version, `${testCase.id}: version mismatch`);
    assert(testCase.source.revision === version.commit, `${testCase.id}: source commit mismatch`);
    assert(versionPaths.includes(testCase.source.path), `${testCase.id}: fixture path mismatch`);
    const upstreamKey = `${testCase.source.path}\0${testCase.source.upstreamId}`;
    assert(!upstreamIds.has(upstreamKey), `${testCase.id}: duplicate upstream ID`);
    assert(Number.isInteger(testCase.source.startLine) && testCase.source.startLine >= 1, `${testCase.id}: invalid start line`);
    assert(Number.isInteger(testCase.source.endLine) && testCase.source.endLine >= testCase.source.startLine, `${testCase.id}: invalid end line`);
    upstreamIds.add(upstreamKey);
    fixtureCounts.set(
      testCase.source.path,
      (fixtureCounts.get(testCase.source.path) ?? 0) + 1
    );
    assert(testCase.profile === document.profile, `${testCase.id}: profile mismatch`);
    assert(typeof testCase.input.markdown === 'string', `${testCase.id}: missing Markdown input`);
    if (testCase.input.upstreamOptions !== undefined) {
      assert(
        testCase.input.upstreamOptions &&
          typeof testCase.input.upstreamOptions === 'object' &&
          !Array.isArray(testCase.input.upstreamOptions),
        `${testCase.id}: invalid upstream options`
      );
    }
    if (testCase.input.upstreamEncoding !== undefined) {
      assert(
        typeof testCase.input.upstreamEncoding === 'string' && testCase.input.upstreamEncoding.length > 0,
        `${testCase.id}: invalid upstream encoding`
      );
    }
    assert(
      testCase.expected.kind === 'normative' || testCase.expected.kind === 'implementation',
      `${testCase.id}: invalid expected result kind`
    );
    assert(typeof testCase.expected.html === 'string', `${testCase.id}: missing expected HTML`);
    assert(
      testCase.expected.comparison === 'semantic-html' || testCase.expected.comparison === 'exact-html',
      `${testCase.id}: invalid comparison mode`
    );
    assert(Array.isArray(testCase.expected.semanticTypes), `${testCase.id}: missing semantic types`);
    assert(Array.isArray(testCase.coverage.candidateNodeTypes), `${testCase.id}: missing node type mapping`);
    assert(
      Array.isArray(testCase.coverage.syntax) && testCase.coverage.syntax.length > 0,
      `${testCase.id}: missing syntax mapping`
    );
    assert(
      Array.isArray(testCase.coverage.featureIds) &&
        testCase.coverage.featureIds.length > 0 &&
        testCase.coverage.featureIds.every(featureId => /^@supramark\/feature-/.test(featureId)),
      `${testCase.id}: invalid feature mapping`
    );
    assert(
      testCase.coverage.renderers.includes('web') &&
        testCase.coverage.renderers.includes('react-native'),
      `${testCase.id}: renderer mapping incomplete`
    );
    sectionCounts.set(
      testCase.source.section,
      (sectionCounts.get(testCase.source.section) ?? 0) + 1
    );
  }

  for (const fixture of versionFixtures) {
    assert((fixtureCounts.get(fixture.path) ?? 0) === fixture.caseCount, `${fixture.path}: case count mismatch`);
  }

  const actualSections = Object.fromEntries(
    [...sectionCounts.entries()].sort(([left], [right]) => left.localeCompare(right))
  );
  assert(JSON.stringify(actualSections) === JSON.stringify(version.sections), 'section counts mismatch');
  console.log(
    `Validated ${document.cases.length} ${name} cases at commit ${version.commit}; cases.json SHA-256 ${createHash('sha256').update(casesText).digest('hex')}`
  );
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
