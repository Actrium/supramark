import { afterAll, describe, expect, it } from 'bun:test';
import { spawnSync } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const fixtureRoot = mkdtempSync(join(tmpdir(), 'supramark-graphviz-retry-'));

afterAll(() => {
  rmSync(fixtureRoot, { recursive: true, force: true });
});

function writeFixture(relativePath: string, contents: string): void {
  const path = join(fixtureRoot, 'graphviz', relativePath);
  mkdirSync(join(path, '..'), { recursive: true });
  writeFileSync(path, contents);
}

function prepareGraphvizSource(commonScript: string, output: string) {
  return spawnSync(
    'bash',
    ['-c', 'source "$1"; prepare_graphviz_source "$2"', 'bash', commonScript, output],
    { encoding: 'utf8' }
  );
}

describe('prepare_graphviz_source', () => {
  it('retries after an unavailable source leaves the output directory behind', () => {
    const scriptsDir = join(fixtureRoot, 'scripts');
    const output = join(fixtureRoot, 'patched');
    const commonScript = join(scriptsDir, 'common.sh');
    mkdirSync(join(fixtureRoot, 'graphviz'), { recursive: true });
    mkdirSync(scriptsDir, { recursive: true });
    copyFileSync(new URL('../../../scripts/common.sh', import.meta.url), commonScript);

    const first = prepareGraphvizSource(commonScript, output);
    expect(first.status).toBe(1);
    expect(existsSync(output)).toBe(true);
    expect(existsSync(join(output, 'lib'))).toBe(false);

    writeFixture(
      'CMakeLists.txt',
      [
        'add_subdirectory(cmd)',
        'add_subdirectory(tclpkg)',
        'find_library(MATH_LIB m)',
        'set(CMAKE_INTERPROCEDURAL_OPTIMIZATION ON)',
      ].join('\n')
    );
    writeFixture('lib/example/CMakeLists.txt', 'add_library(example SHARED example.c)\n');
    writeFixture('lib/example/example.h', '__declspec(dllexport) void example(void);\n');
    writeFixture('lib/gvc/gvusershape.c', '#include <regex.h>\n');
    writeFixture('lib/gvc/gvconfig.c', '#include <regex.h>\n');
    writeFixture('lib/sparse/general.c', 'int run(const char *c) { return system(c); }\n');

    const retry = prepareGraphvizSource(commonScript, output);
    expect(retry.status).toBe(0);
    expect(existsSync(join(output, '.supramark-static-patched'))).toBe(true);
    expect(existsSync(join(output, 'lib/gvc/regex_compat.h'))).toBe(true);
    expect(readFileSync(join(output, 'lib/example/CMakeLists.txt'), 'utf8')).toContain(
      'add_library(example STATIC'
    );
  });
});
