import vm from 'node:vm';
import { Buffer } from 'node:buffer';
import { collectSemanticTypes } from '../lib/semantic/html-semantics.mjs';
import { COMMONMARK_SECTION_COVERAGE } from './commonmark.mjs';
import {
  buildSourceFileMetadata,
  normalizeLineEndings,
  verifyVersionProbe,
} from './source-utils.mjs';

const CAPTURE = Symbol('micromark-capture');
const TEST_SECTION_BY_PATH = {
  'test/io/content/definition.js': 'Link reference definitions',
  'test/io/document/block-quote.js': 'Block quotes',
  'test/io/document/list-item.js': 'List items',
  'test/io/flow/code-fenced.js': 'Fenced code blocks',
  'test/io/flow/code-indented.js': 'Indented code blocks',
  'test/io/flow/heading-atx.js': 'ATX headings',
  'test/io/flow/heading-setext.js': 'Setext headings',
  'test/io/flow/html.js': 'HTML blocks',
  'test/io/flow/thematic-break.js': 'Thematic breaks',
  'test/io/misc/dangerous-html.js': 'Raw HTML',
  'test/io/misc/dangerous-protocols.js': 'Links',
  'test/io/misc/bom.js': 'Textual content',
  'test/io/misc/default-line-ending.js': 'Soft line breaks',
  'test/io/misc/line-ending.js': 'Soft line breaks',
  'test/io/misc/tab.js': 'Tabs',
  'test/io/misc/typed-array.js': 'Textual content',
  'test/io/misc/url.js': 'Links',
  'test/io/misc/zero.js': 'Textual content',
  'test/io/text/autolink.js': 'Autolinks',
  'test/io/text/character-escape.js': 'Backslash escapes',
  'test/io/text/character-reference.js': 'Entity and numeric character references',
  'test/io/text/code.js': 'Code spans',
  'test/io/text/emphasis.js': 'Emphasis and strong emphasis',
  'test/io/text/hard-break.js': 'Hard line breaks',
  'test/io/text/html.js': 'Raw HTML',
  'test/io/text/image.js': 'Images',
  'test/io/text/link-reference.js': 'Links',
  'test/io/text/link-resource.js': 'Links',
  'test/io/text/soft-break.js': 'Soft line breaks',
  'test/io/text/text.js': 'Textual content',
};

export default async function importMicromark(sourceDocuments, sourceConfig) {
  verifyVersionProbe(sourceDocuments, sourceConfig);
  const cases = [];
  const caseCountByPath = new Map();

  for (const sourceDocument of sourceDocuments) {
    if (sourceDocument.role === 'manifest') continue;
    const section = TEST_SECTION_BY_PATH[sourceDocument.path];
    if (!section) throw new Error(`No coverage mapping for micromark fixture: ${sourceDocument.path}`);
    const coverage = COMMONMARK_SECTION_COVERAGE[section];
    if (!coverage) throw new Error(`No CommonMark coverage mapping for micromark section: ${section}`);
    const captured = await captureAssertions(sourceDocument);
    const pathId = sourceDocument.path
      .replace(/^test\/io\//, '')
      .replace(/\.js$/, '')
      .replaceAll('/', '-');

    for (let index = 0; index < captured.length; index += 1) {
      const assertion = captured[index];
      const sequence = String(index + 1).padStart(4, '0');
      cases.push({
        schemaVersion: 1,
        id: `${sourceConfig.name}-${sourceConfig.version}-${pathId}-${sequence}`,
        source: {
          name: sourceConfig.name,
          repository: sourceConfig.repository,
          version: sourceConfig.version,
          path: sourceDocument.path,
          revision: sourceConfig.revision,
          upstreamId: `${assertion.startLine}:${sequence}:${assertion.title}`,
          section,
          startLine: assertion.startLine,
          endLine: assertion.endLine,
        },
        profile: sourceConfig.profile,
        input: {
          markdown: assertion.markdown,
          ...(assertion.options ? { upstreamOptions: assertion.options } : {}),
          ...(assertion.encoding ? { upstreamEncoding: assertion.encoding } : {}),
        },
        expected: {
          kind: 'implementation',
          html: assertion.html,
          semanticTypes: collectSemanticTypes(assertion.html),
          comparison: 'semantic-html',
        },
        coverage,
      });
    }
    caseCountByPath.set(sourceDocument.path, captured.length);
  }

  if (cases.length === 0) throw new Error('No micromark assertions were captured');
  return {
    cases,
    ...buildSourceFileMetadata(sourceDocuments, caseCountByPath),
  };
}

async function captureAssertions(sourceDocument) {
  const source = stripSupportedImports(
    normalizeLineEndings(sourceDocument.text),
    sourceDocument.path,
    sourceDocument.ignoredImports
  );
  const skippedTestTitlePatterns = (sourceDocument.skipTestTitlePatterns ?? []).map(
    pattern => new RegExp(pattern)
  );
  const titleStack = [];
  const pending = [];
  const assertions = [];

  async function runTest(title, options, callback) {
    if (typeof options === 'function') {
      callback = options;
      options = undefined;
    }
    const normalizedTitle = String(title);
    if (
      options?.skip ||
      typeof callback !== 'function' ||
      skippedTestTitlePatterns.some(pattern => pattern.test(normalizedTitle))
    ) {
      return;
    }
    titleStack.push(normalizedTitle);
    try {
      await callback({ test: runTest });
    } finally {
      titleStack.pop();
    }
  }

  function test(...arguments_) {
    const promise = runTest(...arguments_);
    pending.push(promise);
    return promise;
  }
  test.skip = () => Promise.resolve();
  test.only = test;

  function micromark(value, options) {
    const encoding = typeof options === 'string' ? options : undefined;
    const markdown = decodeInput(value, sourceDocument.path, encoding);
    return {
      [CAPTURE]: {
        markdown,
        options: typeof options === 'object' ? normalizeOptions(options) : undefined,
        encoding,
        startLine: callSiteLine(sourceDocument.path),
        title: titleStack.join(' > '),
      },
    };
  }

  const assert = {
    equal(actual, expected) {
      const capture = actual?.[CAPTURE];
      if (!capture) return;
      if (typeof expected !== 'string') {
        throw new Error(`${sourceDocument.path}:${capture.startLine}: expected HTML is not a string`);
      }
      assertions.push({
        ...capture,
        html: expected,
        endLine: Math.max(capture.startLine, callSiteLine(sourceDocument.path)),
      });
    },
  };

  const context = vm.createContext({
    ArrayBuffer,
    Buffer,
    TextDecoder,
    TextEncoder,
    Uint8Array,
    URL,
    assert,
    micromark,
    test,
  });
  new vm.Script(source, { filename: sourceDocument.path }).runInContext(context, {
    timeout: 5_000,
  });
  await Promise.all(pending);
  if (assertions.length === 0) {
    throw new Error(`No Markdown-to-HTML assertions found in ${sourceDocument.path}`);
  }
  return assertions;
}

function stripSupportedImports(source, filePath, ignoredImports = []) {
  const ignoredImportSet = new Set(ignoredImports);
  return source
    .split('\n')
    .map(line => {
      if (!line.startsWith('import ')) return line;
      if (
        line === "import assert from 'node:assert/strict'" ||
        line === "import test from 'node:test'" ||
        line === "import {micromark} from 'micromark'" ||
        ignoredImportSet.has(line)
      ) {
        return '';
      }
      throw new Error(`Unsupported import in ${filePath}: ${line}`);
    })
    .join('\n');
}

function decodeInput(value, filePath, encoding) {
  if (typeof value === 'string') {
    if (encoding) throw new Error(`Unexpected encoding for string input in ${filePath}`);
    return value;
  }
  if (ArrayBuffer.isView(value)) {
    return new TextDecoder(encoding).decode(new Uint8Array(value.buffer, value.byteOffset, value.byteLength));
  }
  throw new Error(`Unsupported micromark input in ${filePath}: ${typeof value}`);
}

function normalizeOptions(options) {
  if (!options || Object.keys(options).length === 0) return undefined;
  return JSON.parse(JSON.stringify(options));
}

function callSiteLine(filePath) {
  const stack = new Error().stack?.split('\n') ?? [];
  for (const line of stack) {
    if (!line.includes(filePath)) continue;
    const match = line.match(/:(\d+):\d+\)?$/);
    if (match) return Number(match[1]);
  }
  throw new Error(`Unable to determine source line for ${filePath}`);
}
