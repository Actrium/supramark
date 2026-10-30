#!/usr/bin/env bun
/**
 * Self-test for scripts/crate-release.ts.
 *
 * The version algebra is the part worth testing: the workspace carries two
 * version shapes (plain semver, and an upstream version plus a numeric
 * revision), and the rule that decides which component moves is also the rule
 * that decides what gets published. A wrong answer here would publish
 * `mermaid-little 11.14.1`, claiming an upstream mermaid release that does not
 * exist, and crates.io versions cannot be taken back.
 *
 * The second half runs the real script against a throwaway workspace that
 * reproduces the shape that matters: an exact pin (`= version`) that has to be
 * rewritten and carried into a dependent release, a caret requirement that
 * must be left alone, and an unpublished wrapper with a path-only dependency
 * that must not be touched at all.
 *
 * `bun run quality` runs this.
 */

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { compareVersions, nextVersion, parseTag, planWaitDeps, reqAccepts } from './crate-release.ts';

let failures = 0;

function test(name: string, body: () => void): void {
  try {
    body();
    console.log(`ok   ${name}`);
  } catch (error) {
    failures += 1;
    console.error(`FAIL ${name}\n     ${(error as Error).message.split('\n').join('\n     ')}`);
  }
}

// ── Version algebra ─────────────────────────────────────────────────────────

test('nextVersion moves the patch of a plain version', () => {
  assert.equal(nextVersion('0.1.6'), '0.1.7');
  assert.equal(nextVersion('1.0.0'), '1.0.1');
});

test('nextVersion moves the revision of an upstream-tracking version', () => {
  assert.equal(nextVersion('11.14.0-6'), '11.14.0-7');
  assert.equal(nextVersion('11.14.0-9'), '11.14.0-10');
  assert.equal(nextVersion('1.2026.2-4'), '1.2026.2-5');
  assert.equal(nextVersion('0.7.1-1'), '0.7.1-2');
});

test('nextVersion refuses a pre-release it cannot increment', () => {
  assert.throws(() => nextVersion('1.0.0-rc.1'), /--set/);
});

test('compareVersions orders revisions numerically, not as text', () => {
  assert.equal(compareVersions('11.14.0-9', '11.14.0-10'), -1);
  assert.equal(compareVersions('0.1.7', '0.1.7'), 0);
  assert.equal(compareVersions('0.2.0', '0.1.9'), 1);
  // A pre-release sorts below the release of the same version.
  assert.equal(compareVersions('11.14.0-6', '11.14.0'), -1);
});

test('reqAccepts decides an exact pin by equality', () => {
  assert.equal(reqAccepts('=11.14.0-6', '11.14.0-6'), true);
  assert.equal(reqAccepts('=11.14.0-6', '11.14.0-7'), false);
});

test('reqAccepts applies the caret bound of a bare requirement', () => {
  assert.equal(reqAccepts('0.1', '0.1.2'), true);
  assert.equal(reqAccepts('0.1', '0.2.0'), false);
  assert.equal(reqAccepts('0.1.1', '0.1.0'), false);
  assert.equal(reqAccepts('1.2026.2-4', '1.2026.2-5'), true);
  assert.equal(reqAccepts('0.2.2', '0.2.7'), true);
});

test('reqAccepts reports a requirement it does not model', () => {
  assert.equal(reqAccepts('>=1, <2', '1.5.0'), 'unknown');
  assert.equal(reqAccepts('', '1.0.0'), 'unknown');
});

test('reqAccepts refuses a pre-release that a plain caret cannot match', () => {
  // Cargo does not let `0.1` resolve `0.1.2-1`. Answering `true` here would
  // publish a requirement cargo itself rejects.
  assert.equal(reqAccepts('0.1', '0.1.2-1'), 'unknown');
  assert.equal(reqAccepts('0.1.2', '0.1.2-1'), 'unknown');
  // A requirement that names a pre-release still compares normally.
  assert.equal(reqAccepts('1.2026.2-4', '1.2026.2-5'), true);
  assert.equal(reqAccepts('=11.14.0-6', '11.14.0-7'), false);
});

test('parseTag splits a release tag, and rejects a bare version tag', () => {
  assert.deepEqual(parseTag('mermaid-little-v11.14.0-7'), { crate: 'mermaid-little', version: '11.14.0-7' });
  assert.deepEqual(parseTag('supramark-diagram-v0.1.7'), { crate: 'supramark-diagram', version: '0.1.7' });
  // `v0.2.7` belongs to graphviz-release.yml; this pipeline must not claim it.
  assert.throws(() => parseTag('v0.2.7'), /<crate>-v<version>/);
});

test('planWaitDeps waits only when the index cannot resolve the requirement', () => {
  const lookupWith = (published: Record<string, string[]>) => (name: string) => ({
    versions: published[name] ?? [],
  });
  const dep = (over: Partial<{ version: string; req: string; publish: boolean; excluded: boolean }>) => ({
    name: 'dep',
    version: '1.0.0',
    req: '=1.0.0',
    publish: true,
    excluded: false,
    ...over,
  });

  // Exact pin to the in-tree version: it has to appear before this can publish.
  assert.deepEqual(planWaitDeps({ crate: 'top', deps: [dep({})] }, lookupWith({})), {
    waits: [{ name: 'dep', version: '1.0.0', req: '=1.0.0' }],
  });

  // A caret an older published release already satisfies: nothing to wait for.
  assert.deepEqual(
    planWaitDeps(
      { crate: 'top', deps: [dep({ version: '1.2.0', req: '1.0' })] },
      lookupWith({ dep: ['1.0.5'] })
    ),
    { waits: [] }
  );

  // A stale pin: fail fast instead of burning the timeout.
  const stale = planWaitDeps({ crate: 'top', deps: [dep({ version: '2.0.0', req: '=1.0.0' })] }, lookupWith({}));
  assert.ok('error' in stale && /nothing that satisfies/.test(stale.error));

  // A dependency this pipeline never publishes: fail fast.
  const excluded = planWaitDeps({ crate: 'top', deps: [dep({ excluded: true })] }, lookupWith({}));
  assert.ok('error' in excluded && /not published by this pipeline/.test(excluded.error));
});

// ── Bump propagation, against a throwaway workspace ─────────────────────────

const SCRIPT = path.join(path.dirname(fileURLToPath(import.meta.url)), 'crate-release.ts');

function writeFixture(root: string): void {
  const write = (rel: string, body: string): void => {
    fs.mkdirSync(path.join(root, path.dirname(rel)), { recursive: true });
    fs.writeFileSync(path.join(root, rel), body);
  };

  write(
    'Cargo.toml',
    ['[workspace]', 'resolver = "2"', 'members = [', '  "crates/leaf",', '  "crates/leaf/packages/web",', '  "crates/mid",', '  "crates/top",', ']', ''].join('\n')
  );

  write(
    'crates/leaf/Cargo.toml',
    ['[package]', 'name = "leaf"', 'version = "11.14.0-6"', 'edition = "2021"', '', '[dev-dependencies]', '# a test-only feature switch, not a release relationship', 'leaf = { path = ".", features = ["x"] }', ''].join('\n')
  );
  write('crates/leaf/src/lib.rs', '\n');

  write(
    'crates/leaf/packages/web/Cargo.toml',
    ['[package]', 'name = "leaf-web"', 'version = "11.14.0-1"', 'edition = "2021"', 'publish = false', '', '[dependencies]', 'leaf = { path = "../.." }', ''].join('\n')
  );
  write('crates/leaf/packages/web/src/lib.rs', '\n');

  write(
    'crates/mid/Cargo.toml',
    ['[package]', 'name = "mid"', 'version = "0.1.6"', 'edition = "2021"', '', '[dependencies]', '# exact pin: the published mid must name one exact leaf', 'leaf = { path = "../leaf", version = "=11.14.0-6", features = ["y"] }', ''].join('\n')
  );
  write('crates/mid/src/lib.rs', '\n');

  write(
    'crates/top/Cargo.toml',
    ['[package]', 'name = "top"', 'version = "2.0.0"', 'edition = "2021"', '', '[dependencies]', 'mid = { path = "../mid", version = "0.1" }', ''].join('\n')
  );
  write('crates/top/src/lib.rs', '\n');
}

/**
 * Run the real script in `root`. stderr is captured rather than inherited, so
 * the failures these tests assert on do not look like test output; execFileSync
 * folds it into the thrown error's message, which is what assert.throws reads.
 */
function run(root: string, args: string[]): string {
  return execFileSync(process.execPath, [SCRIPT, ...args], {
    cwd: root,
    encoding: 'utf-8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
}

test('bump rewrites an exact pin, carries the dependent, and leaves the rest alone', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'crate-release-'));
  try {
    writeFixture(root);
    const output = run(root, ['bump', 'leaf']);
    assert.match(output, /leaf: 11\.14\.0-6 -> 11\.14\.0-7 {2}\(requested\)/);
    assert.match(output, /mid: 0\.1\.6 -> 0\.1\.7 {2}\(follows leaf 11\.14\.0-7\)/);

    const read = (rel: string): string => fs.readFileSync(path.join(root, rel), 'utf-8');
    assert.match(read('crates/leaf/Cargo.toml'), /^version = "11\.14\.0-7"$/m);
    assert.match(read('crates/mid/Cargo.toml'), /version = "=11\.14\.0-7"/);
    assert.match(read('crates/mid/Cargo.toml'), /^version = "0\.1\.7"$/m);
    // The caret requirement still accepts mid 0.1.7, so top is not a release.
    assert.match(read('crates/top/Cargo.toml'), /^version = "2\.0\.0"$/m);
    assert.match(read('crates/top/Cargo.toml'), /version = "0\.1"/);
    // The unpublished wrapper has no requirement to keep in step.
    assert.match(read('crates/leaf/packages/web/Cargo.toml'), /^version = "11\.14\.0-1"$/m);
    assert.match(read('crates/leaf/packages/web/Cargo.toml'), /leaf = \{ path = "\.\./, 'the path-only dependency is untouched');
    // Comments and feature lists survive, because only the matched line moves.
    assert.match(read('crates/mid/Cargo.toml'), /features = \["y"\]/);
    assert.match(read('crates/mid/Cargo.toml'), /# exact pin/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('bump refuses a version that is not an increment', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'crate-release-'));
  try {
    writeFixture(root);
    assert.throws(() => run(root, ['bump', 'leaf', '--set', '11.14.0-6']), /not greater than/);
    assert.throws(() => run(root, ['bump', 'leaf-web']), /not published by this pipeline|publish = false/);
    assert.throws(() => run(root, ['bump', 'nope']), /unknown crate/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('check reports a pin left behind by a hand-edited version', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'crate-release-'));
  try {
    writeFixture(root);
    const manifest = path.join(root, 'crates/leaf/Cargo.toml');
    fs.writeFileSync(
      manifest,
      fs.readFileSync(manifest, 'utf-8').replace('version = "11.14.0-6"', 'version = "11.14.0-7"')
    );
    assert.throws(
      () => run(root, ['check', '--offline']),
      /requirement "=11\.14\.0-6" on leaf does not accept the version in the tree/
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('verify-tag and tag-for name one release for the workflow', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'crate-release-'));
  try {
    writeFixture(root);
    // stdout is `KEY=VALUE` for $GITHUB_OUTPUT; the human-readable line goes to stderr.
    assert.equal(
      run(root, ['verify-tag', 'leaf-v11.14.0-6']),
      'crate=leaf\nversion=11.14.0-6\ntag=leaf-v11.14.0-6\n'
    );
    assert.equal(run(root, ['tag-for', 'mid']), 'crate=mid\nversion=0.1.6\ntag=mid-v0.1.6\n');
    assert.throws(() => run(root, ['verify-tag', 'leaf-v11.14.0-7']), /does not match/);
    assert.throws(() => run(root, ['verify-tag', 'leaf-web-v11.14.0-1']), /publish = false/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

if (failures > 0) {
  console.error(`\n${failures} self-test failure(s)`);
  process.exit(1);
}
console.log('\ncrate-release self-test passed');
