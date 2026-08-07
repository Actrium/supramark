import { createHash } from 'node:crypto';

export function normalizeLineEndings(value) {
  return value.replace(/\r\n?/g, '\n');
}

export function sourceHash(value) {
  return createHash('sha256').update(normalizeLineEndings(value), 'utf8').digest('hex');
}

export function buildSourceFileMetadata(sourceDocuments, caseCountByPath) {
  const aggregateHash = createHash('sha256');
  const sourceFiles = sourceDocuments.map(sourceDocument => {
    const sourceSha256 = sourceHash(sourceDocument.text);
    aggregateHash.update(`${sourceDocument.path}\0${sourceSha256}\n`, 'utf8');
    return {
      path: sourceDocument.path,
      ...(sourceDocument.role ? { role: sourceDocument.role } : {}),
      sourceSha256,
      caseCount: caseCountByPath.get(sourceDocument.path) ?? 0,
    };
  });
  return { sourceFiles, sourceSha256: aggregateHash.digest('hex') };
}

export function verifyVersionProbe(sourceDocuments, sourceConfig) {
  const probe = sourceConfig.versionProbe;
  if (!probe) return;
  const sourceDocument = sourceDocuments.find(document => document.path === probe.path);
  if (!sourceDocument) throw new Error(`Version probe fixture is missing: ${probe.path}`);
  const match = normalizeLineEndings(sourceDocument.text).match(
    new RegExp(probe.pattern, probe.flags ?? 'm')
  );
  const actual = match?.[probe.group ?? 1];
  if (!actual) throw new Error(`Unable to read source version from ${probe.path}`);
  if (actual !== sourceConfig.version) {
    throw new Error(
      `${sourceConfig.name} version mismatch: source is ${actual}, configuration expects ${sourceConfig.version}`
    );
  }
}

export function contentLineCount(value) {
  const normalized = normalizeLineEndings(value);
  if (normalized.length === 0) return 1;
  const count = normalized.split('\n').length;
  return normalized.endsWith('\n') ? Math.max(1, count - 1) : count;
}
