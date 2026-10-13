import { describe, expect, it, beforeEach, afterEach } from 'bun:test';
import fs from 'fs';
import os from 'os';
import path from 'path';
import {
  WINDOWS_ARCHES,
  parseArchSelection,
  listAvailableArches,
  stageWindowsFrameworks,
} from '../scripts/windows-staging.js';

const DLL = 'supramark_plantuml_native.dll';
const LIB = 'supramark_plantuml_native.lib';
const HEADER = 'supramark_plantuml.h';

let tmpRoot: string;
let outputDir: string;
let frameworksDest: string;
let fallbackDir: string;

beforeEach(() => {
  tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'plantuml-staging-'));
  outputDir = path.join(tmpRoot, 'output');
  frameworksDest = path.join(tmpRoot, 'windows', 'Frameworks');
  fallbackDir = path.join(tmpRoot, 'packages', 'native', 'include');
});

afterEach(() => {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
});

function writeArchFixture(archOutputDir: string, bytes: string, opts: { header?: boolean } = {}) {
  fs.mkdirSync(path.join(outputDir, archOutputDir, 'bin'), { recursive: true });
  fs.mkdirSync(path.join(outputDir, archOutputDir, 'lib'), { recursive: true });
  fs.writeFileSync(path.join(outputDir, archOutputDir, 'bin', DLL), bytes);
  fs.writeFileSync(path.join(outputDir, archOutputDir, 'lib', LIB), `lib-${bytes}`);
  if (opts.header) {
    fs.mkdirSync(path.join(outputDir, archOutputDir, 'include'), { recursive: true });
    fs.writeFileSync(path.join(outputDir, archOutputDir, 'include', HEADER), 'header-from-tree');
  }
}

function stage(requestedArches: string[] | null = null) {
  return stageWindowsFrameworks({
    outputDir,
    frameworksDest,
    dllName: DLL,
    libName: LIB,
    headerName: HEADER,
    headerFallbackDir: fallbackDir,
    requestedArches,
  });
}

describe('parseArchSelection', () => {
  it('accepts a single arch and comma-separated lists', () => {
    expect(parseArchSelection('x64')).toEqual(['x64']);
    expect(parseArchSelection('x64,arm64')).toEqual(['x64', 'arm64']);
    expect(parseArchSelection('arm64, x64')).toEqual(['arm64', 'x64']);
  });

  it('rejects unknown or empty values', () => {
    expect(() => parseArchSelection('x86')).toThrow();
    expect(() => parseArchSelection('')).toThrow();
  });
});

describe('listAvailableArches', () => {
  it('detects architectures by their runtime DLL', () => {
    writeArchFixture('windows-x86_64', 'x64-dll');
    expect(listAvailableArches(outputDir, DLL)).toEqual(['x64']);

    writeArchFixture('windows-arm64', 'arm64-dll');
    expect(listAvailableArches(outputDir, DLL)).toEqual(['x64', 'arm64']);
  });

  it('ignores output trees without the runtime DLL', () => {
    fs.mkdirSync(path.join(outputDir, 'windows-arm64', 'bin'), { recursive: true });
    expect(listAvailableArches(outputDir, DLL)).toEqual([]);
  });
});

describe('stageWindowsFrameworks', () => {
  it('stages an x86_64-only tree under x64/', () => {
    writeArchFixture('windows-x86_64', 'x64-dll', { header: true });
    const result = stage();

    expect(result.staged).toEqual(['x64']);
    expect(result.missing).toEqual([]);
    expect(result.headerStaged).toBe(true);
    expect(fs.readFileSync(path.join(frameworksDest, 'x64', 'bin', DLL), 'utf8')).toBe('x64-dll');
    expect(fs.readFileSync(path.join(frameworksDest, 'x64', 'lib', LIB), 'utf8')).toBe('lib-x64-dll');
    expect(fs.existsSync(path.join(frameworksDest, 'arm64'))).toBe(false);
    expect(fs.readFileSync(path.join(frameworksDest, 'include', HEADER), 'utf8')).toBe(
      'header-from-tree'
    );
  });

  it('stages an ARM64-only tree instead of skipping Windows (review regression)', () => {
    writeArchFixture('windows-arm64', 'arm64-dll', { header: true });
    const result = stage();

    expect(result.staged).toEqual(['arm64']);
    expect(result.headerStaged).toBe(true);
    expect(fs.readFileSync(path.join(frameworksDest, 'arm64', 'bin', DLL), 'utf8')).toBe('arm64-dll');
    expect(fs.existsSync(path.join(frameworksDest, 'x64'))).toBe(false);
  });

  it('stages both architectures independently when both are present (review regression)', () => {
    writeArchFixture('windows-x86_64', 'x64-dll', { header: true });
    writeArchFixture('windows-arm64', 'arm64-dll');
    const result = stage();

    expect(result.staged).toEqual(['x64', 'arm64']);
    expect(fs.readFileSync(path.join(frameworksDest, 'x64', 'bin', DLL), 'utf8')).toBe('x64-dll');
    expect(fs.readFileSync(path.join(frameworksDest, 'arm64', 'bin', DLL), 'utf8')).toBe('arm64-dll');
  });

  it('restricts staging to explicitly requested architectures', () => {
    writeArchFixture('windows-x86_64', 'x64-dll');
    writeArchFixture('windows-arm64', 'arm64-dll', { header: true });
    const result = stage(['arm64']);

    expect(result.staged).toEqual(['arm64']);
    expect(fs.existsSync(path.join(frameworksDest, 'x64'))).toBe(false);
    expect(fs.readFileSync(path.join(frameworksDest, 'arm64', 'bin', DLL), 'utf8')).toBe('arm64-dll');
  });

  it('reports explicitly requested architectures without artifacts and leaves the staged tree alone', () => {
    writeArchFixture('windows-x86_64', 'x64-dll');
    fs.mkdirSync(frameworksDest, { recursive: true });
    fs.writeFileSync(path.join(frameworksDest, 'sentinel'), 'keep');

    const result = stage(['arm64']);

    expect(result.staged).toEqual([]);
    expect(result.missing).toEqual(['arm64']);
    expect(fs.readFileSync(path.join(frameworksDest, 'sentinel'), 'utf8')).toBe('keep');
  });

  it('does nothing when no architecture was built', () => {
    const result = stage();
    expect(result.staged).toEqual([]);
    expect(result.missing).toEqual([]);
    expect(result.headerStaged).toBe(false);
  });

  it('falls back to the packaged header directory when the built tree has none', () => {
    writeArchFixture('windows-x86_64', 'x64-dll');
    fs.mkdirSync(fallbackDir, { recursive: true });
    fs.writeFileSync(path.join(fallbackDir, HEADER), 'header-from-fallback');

    const result = stage();

    expect(result.headerStaged).toBe(true);
    expect(fs.readFileSync(path.join(frameworksDest, 'include', HEADER), 'utf8')).toBe(
      'header-from-fallback'
    );
  });

  it('rebuilds the staged tree from scratch so stale architectures disappear', () => {
    writeArchFixture('windows-x86_64', 'x64-dll', { header: true });
    writeArchFixture('windows-arm64', 'arm64-dll');
    stage();

    fs.rmSync(path.join(outputDir, 'windows-arm64'), { recursive: true, force: true });
    const result = stage();

    expect(result.staged).toEqual(['x64']);
    expect(fs.existsSync(path.join(frameworksDest, 'arm64'))).toBe(false);
  });

  it('covers every WINDOWS_ARCHES entry', () => {
    expect(WINDOWS_ARCHES.map((a: { name: string }) => a.name)).toEqual(['x64', 'arm64']);
  });
});
