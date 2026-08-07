import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SOURCE_NAME_PATTERN = /^[a-z0-9][a-z0-9-]*$/;
const suiteRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sourceConfigDirectory = path.join(suiteRoot, 'config', 'sources');

const availableSources = (await readdir(sourceConfigDirectory))
  .filter(fileName => fileName.endsWith('.json'))
  .map(fileName => path.basename(fileName, '.json'))
  .sort();

if (availableSources.length === 0) {
  throw new Error(`No conformance sources found in ${sourceConfigDirectory}`);
}

const requested = process.argv.slice(2).join(' ').trim() || 'all';
const requestedSources = requested
  // cjk-allow: accept a fullwidth comma in user-provided source lists.
  .split(/[,\uFF0C]/)
  .map(source => source.trim())
  .filter(Boolean);

if (requestedSources.length === 0) {
  throw new Error('At least one conformance source must be selected.');
}
if (requestedSources.includes('all') && requestedSources.length > 1) {
  throw new Error('Use "all" by itself, or provide a comma-separated source list.');
}

const selectedSources = requestedSources[0] === 'all'
  ? availableSources
  : [...new Set(requestedSources)];

for (const sourceName of selectedSources) {
  if (!SOURCE_NAME_PATTERN.test(sourceName)) {
    throw new Error(`Invalid conformance source name: ${sourceName}`);
  }
  if (!availableSources.includes(sourceName)) {
    throw new Error(
      `Unknown conformance source: ${sourceName}. Available sources: ${availableSources.join(', ')}`
    );
  }

  const config = JSON.parse(
    await readFile(path.join(sourceConfigDirectory, `${sourceName}.json`), 'utf8')
  );
  if (config.name !== sourceName) {
    throw new Error(
      `Conformance source config mismatch: ${sourceName} != ${config.name ?? '<missing>'}`
    );
  }
}

process.stdout.write(JSON.stringify(selectedSources));
