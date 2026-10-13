const EXAMPLE_FENCE = '`'.repeat(32);

export function normalizeLineEndings(value) {
  return value.replace(/\r\n?/g, '\n');
}

export function readFrontMatterValue(source, key) {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = source.match(
    new RegExp(`^${escapedKey}:\\s*['\"]?([^'\"\\n]+)['\"]?\\s*$`, 'm')
  );
  if (!match) throw new Error(`Unable to read ${key} from fixture front matter`);
  return match[1].trim();
}

export function parseSpecExamples(source) {
  // Split only on LF so callers that intentionally preserve source line endings
  // retain any preceding CR bytes (including the cmark CR+CR+LF regression).
  const lines = source.match(/[^\n]*(?:\n|$)/g)?.filter(line => line.length > 0) ?? [];
  const result = [];
  let state = 'document';
  let section = '';
  let startLine = 0;
  let markdown = [];
  let html = [];

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    const lineNumber = index + 1;
    const stripped = line.trim();

    if (state === 'document' && stripped.startsWith(`${EXAMPLE_FENCE} example`)) {
      state = 'markdown';
      startLine = lineNumber;
      markdown = [];
      html = [];
      continue;
    }
    if (state === 'markdown' && stripped === '.') {
      state = 'html';
      continue;
    }
    if (state === 'html' && stripped === EXAMPLE_FENCE) {
      if (!section) throw new Error(`Example at line ${startLine} has no enclosing section`);
      result.push({
        example: result.length + 1,
        section,
        startLine,
        endLine: lineNumber,
        markdown: markdown.join('').replaceAll('\u2192', '\t'),
        html: html.join('').replaceAll('\u2192', '\t'),
      });
      state = 'document';
      continue;
    }

    if (state === 'markdown') markdown.push(line);
    else if (state === 'html') html.push(line);
    else {
      const heading = line.match(/^#+\s+(.+?)\s*\n?$/);
      if (heading) section = heading[1];
    }
  }

  if (state !== 'document') throw new Error(`Unclosed specification example at line ${startLine}`);
  if (result.length === 0) throw new Error('No specification examples found');
  return result;
}

export function toUnifiedCase(rawCase, sourceConfig, sectionCoverage) {
  const caseCoverage = sectionCoverage[rawCase.section];
  if (!caseCoverage) {
    throw new Error(
      `No Supramark coverage mapping for ${sourceConfig.name} section: ${rawCase.section}`
    );
  }
  const idNamespace = sourceConfig.caseIdNamespace ? `-${sourceConfig.caseIdNamespace}` : '';
  return {
    schemaVersion: 1,
    id: `${sourceConfig.name}-${sourceConfig.version}${idNamespace}-${String(rawCase.example).padStart(4, '0')}`,
    source: {
      name: sourceConfig.name,
      repository: sourceConfig.repository,
      version: sourceConfig.version,
      path: sourceConfig.input,
      revision: sourceConfig.revision,
      upstreamId: rawCase.example,
      section: rawCase.section,
      startLine: rawCase.startLine,
      endLine: rawCase.endLine,
    },
    profile: sourceConfig.profile,
    input: { markdown: rawCase.markdown },
    expected: {
      kind: sourceConfig.expectedKind ?? 'normative',
      html: rawCase.html,
      semanticTypes: sourceConfig.collectSemanticTypes(rawCase.html),
      comparison: 'semantic-html',
    },
    coverage: caseCoverage,
  };
}

export function coverage(featureIds, syntax, candidateNodeTypes) {
  return {
    featureIds,
    syntax,
    candidateNodeTypes,
    renderers: ['web', 'react-native'],
  };
}
