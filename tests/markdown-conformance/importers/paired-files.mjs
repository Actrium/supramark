import { collectSemanticTypes } from '../lib/semantic/html-semantics.mjs';
import {
  buildSourceFileMetadata,
  contentLineCount,
  normalizeLineEndings,
  verifyVersionProbe,
} from './source-utils.mjs';

export default function importPairedFiles(sourceDocuments, sourceConfig) {
  verifyVersionProbe(sourceDocuments, sourceConfig);
  const pairs = new Map();

  for (const sourceDocument of sourceDocuments) {
    if (!sourceDocument.pairId) continue;
    const pair = pairs.get(sourceDocument.pairId) ?? {};
    if (pair[sourceDocument.role]) {
      throw new Error(`Duplicate ${sourceDocument.role} fixture for pair ${sourceDocument.pairId}`);
    }
    pair[sourceDocument.role] = sourceDocument;
    pairs.set(sourceDocument.pairId, pair);
  }

  const cases = [];
  const caseCountByPath = new Map();
  for (const [pairId, pair] of pairs) {
    const markdownDocument = pair.markdown;
    const htmlDocument = pair.html;
    if (!markdownDocument || !htmlDocument) {
      throw new Error(`Fixture pair ${pairId} must contain markdown and html roles`);
    }
    if (!markdownDocument.coverage) {
      throw new Error(`Fixture pair ${pairId} does not define a coverage mapping`);
    }

    const markdown = normalizeLineEndings(markdownDocument.text);
    const html = normalizeLineEndings(htmlDocument.text);
    const caseId = markdownDocument.caseId ?? pairId;
    cases.push({
      schemaVersion: 1,
      id: `${sourceConfig.name}-${sourceConfig.version}-${caseId}`,
      source: {
        name: sourceConfig.name,
        repository: sourceConfig.repository,
        version: sourceConfig.version,
        path: markdownDocument.path,
        revision: sourceConfig.revision,
        upstreamId: pairId,
        section: markdownDocument.section ?? 'Paired fixtures',
        startLine: 1,
        endLine: contentLineCount(markdown),
      },
      profile: sourceConfig.profile,
      input: { markdown },
      expected: {
        kind: sourceConfig.caseKind ?? 'implementation',
        html,
        semanticTypes: collectSemanticTypes(html),
        comparison: markdownDocument.comparison ?? 'semantic-html',
      },
      coverage: markdownDocument.coverage,
    });
    caseCountByPath.set(markdownDocument.path, 1);
  }

  if (cases.length === 0) throw new Error(`No paired fixtures found for ${sourceConfig.name}`);
  return {
    cases,
    ...buildSourceFileMetadata(sourceDocuments, caseCountByPath),
  };
}
