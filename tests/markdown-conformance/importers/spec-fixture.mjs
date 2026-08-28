import { createHash } from 'node:crypto';
import { collectSemanticTypes } from '../lib/semantic/html-semantics.mjs';
import {
  normalizeLineEndings,
  parseSpecExamples,
  toUnifiedCase,
} from './spec-examples.mjs';

const LINE_ENDING_MODES = new Set(['normalize', 'preserve']);

/**
 * Generic adapter for CommonMark-style fenced example fixtures.
 *
 * Source configuration can select `lineEndings: "preserve"` for byte-sensitive
 * regression inputs; the default remains normalized LF for existing fixtures.
 */
export default function importSpecFixture(sourceDocuments, sourceConfig) {
  if (!sourceConfig.sectionCoverage || typeof sourceConfig.sectionCoverage !== 'object') {
    throw new Error(`Source ${sourceConfig.name} must configure sectionCoverage`);
  }

  const cases = [];
  const sourceFiles = [];
  const aggregateHash = createHash('sha256');

  for (const sourceDocument of sourceDocuments) {
    const lineEndings = sourceDocument.lineEndings ?? sourceConfig.lineEndings ?? 'normalize';
    if (!LINE_ENDING_MODES.has(lineEndings)) {
      throw new Error(
        `Unsupported lineEndings mode for ${sourceConfig.name}: ${String(lineEndings)}`
      );
    }

    const fixtureText =
      lineEndings === 'preserve'
        ? sourceDocument.text
        : normalizeLineEndings(sourceDocument.text);
    const caseConfig = {
      ...sourceConfig,
      input: sourceDocument.path,
      caseIdNamespace: sourceDocument.caseIdNamespace,
      expectedKind:
        sourceDocument.expectedKind ?? sourceConfig.expectedKind ?? 'normative',
      collectSemanticTypes,
    };
    const documentCases = parseSpecExamples(fixtureText).map(rawCase =>
      toUnifiedCase(rawCase, caseConfig, sourceConfig.sectionCoverage)
    );
    const sourceSha256 = createHash('sha256').update(fixtureText, 'utf8').digest('hex');

    cases.push(...documentCases);
    sourceFiles.push({
      path: sourceDocument.path,
      sourceSha256,
      caseCount: documentCases.length,
    });
    aggregateHash.update(`${sourceDocument.path}\0${sourceSha256}\n`, 'utf8');
  }

  if (
    sourceConfig.expectedCaseCount !== undefined &&
    cases.length !== sourceConfig.expectedCaseCount
  ) {
    throw new Error(
      `Expected ${sourceConfig.expectedCaseCount} ${sourceConfig.name} cases, imported ${cases.length}`
    );
  }

  return {
    cases,
    sourceFiles,
    sourceSha256:
      sourceFiles.length === 1 ? sourceFiles[0].sourceSha256 : aggregateHash.digest('hex'),
  };
}
