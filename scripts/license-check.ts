#!/usr/bin/env bun
/**
 * License compliance check for the supramark super-monorepo.
 *
 * Three responsibilities:
 *   1. Every package.json under workspace declares `license` with an
 *      SPDX expression we know about.
 *   2. Every workspace-member Cargo.toml declares `license` with an
 *      SPDX expression we know about (cargo-deny covers transitive
 *      Rust dep auditing; this only validates first-party crate
 *      manifests).
 *   3. The declared license is on the allow-list documented in
 *      `docs/architecture/LICENSE_COMPATIBILITY.md` and `deny.toml`.
 *
 * CI runs this as part of `bun run quality`.
 */

import fs from 'node:fs';
import path from 'node:path';

// ── Allow-list: must match deny.toml and LICENSE_COMPATIBILITY.md §2 ──
// Bare SPDX identifiers; expressions like "Apache-2.0 OR MIT" are
// expanded by ALLOWED_EXPRESSIONS below.
const ALLOWED_LICENSES = new Set<string>([
  'Apache-2.0',
  'MIT',
  'MIT-0',
  'BSD-2-Clause',
  'BSD-3-Clause',
  'ISC',
  'Unicode-DFS-2016',
  'Unicode-3.0',
  'MPL-2.0',
  'EPL-1.0',
  'EPL-2.0',
  'LGPL-3.0-or-later',
  'Zlib',
  'CC0-1.0',
  'CC-BY-4.0',
]);

const ALLOWED_EXPRESSIONS = new Set<string>([
  'Apache-2.0 OR MIT',
  'Apache-2.0 WITH LLVM-exception',
  'MIT OR Apache-2.0',
]);

// Packages that may carry tighter license restrictions than the
// monorepo default. Listed for traceability — actual enforcement happens
// when these are introduced (steps 2-4).
const KNOWN_NON_DEFAULT: Record<string, string> = {
  '@kookyleo/plantuml-little-web':
    'GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0 OR EPL-2.0 OR MIT',
  '@kookyleo/graphviz-anywhere-web': 'EPL-1.0',
  '@kookyleo/graphviz-anywhere-rn': 'EPL-1.0',
  '@kookyleo/d2-little-web': 'MPL-2.0',
};

// ── Skip patterns ──────────────────────────────────────────────────────
// Paths we deliberately do not enforce SPDX on, with the reason. Each
// entry is matched against the path relative to repo root.
const SKIP_PATTERNS: Array<{ matcher: (p: string) => boolean; reason: string }> = [
  {
    // React Native's lib/commonjs/ and lib/module/ stubs are
    // {"type":"commonjs"} / {"type":"module"} sentinels, used purely for
    // module-resolution. They are not packages.
    matcher: p => /\/lib\/(commonjs|module)\/package\.json$/.test(p),
    reason: 'RN module-resolution stub, not a publishable package',
  },
  {
    // Upstream graphviz-anywhere's RN example app omits a license
    // field. Tracked for upstream patch-back via crates/graphviz-anywhere/UPSTREAM.md.
    matcher: p => p === 'crates/graphviz-anywhere/examples/react-native/package.json',
    reason: 'upstream-merged; private demo app, license patch tracked in UPSTREAM.md',
  },
  {
    // Private test-only helper inside plantuml-little. Not published;
    // upstream omits the license field. Tracked in
    // crates/plantuml-little/UPSTREAM.md for patch-back.
    matcher: p => p === 'crates/plantuml-little/tests/support/package.json',
    reason: 'upstream-merged; private test helper, license patch tracked in UPSTREAM.md',
  },
  {
    // Same pattern as plantuml-little: private deterministic reference
    // SVG generator. Not published. Tracked in
    // crates/mermaid-little/UPSTREAM.md.
    matcher: p => p === 'crates/mermaid-little/tests/support/package.json',
    reason: 'upstream-merged; private test helper, license patch tracked in UPSTREAM.md',
  },
];

// ── Workspace discovery ────────────────────────────────────────────────
const ROOT = process.cwd();

function readJson<T = unknown>(p: string): T {
  return JSON.parse(fs.readFileSync(p, 'utf-8')) as T;
}

interface PackageJson {
  name?: string;
  version?: string;
  license?: string;
  private?: boolean;
}

function findManifests(filename: 'package.json' | 'Cargo.toml'): string[] {
  const found: string[] = [];
  const skip = new Set(['node_modules', '.git', 'dist', 'build', 'coverage', 'target']);

  function walk(dir: string, depth: number): void {
    if (depth > 6) return;
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (skip.has(entry.name)) continue;
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full, depth + 1);
      } else if (entry.isFile() && entry.name === filename) {
        found.push(full);
      }
    }
  }

  walk(ROOT, 0);
  return found.sort();
}

// Minimal Cargo.toml parser — we only need `[package]` name + license,
// which are flat string scalars. Avoids pulling in a TOML dependency.
function readCargoPackage(p: string): { name?: string; license?: string; isWorkspaceOnly: boolean } {
  const text = fs.readFileSync(p, 'utf-8');
  const lines = text.split('\n');
  let inPackage = false;
  let inOtherSection = false;
  let name: string | undefined;
  let license: string | undefined;
  let sawPackage = false;

  for (const raw of lines) {
    const line = raw.replace(/#.*$/, '').trim();
    if (!line) continue;
    const sectionMatch = line.match(/^\[(.+?)\]$/);
    if (sectionMatch) {
      const section = sectionMatch[1];
      inPackage = section === 'package';
      inOtherSection = !inPackage;
      if (inPackage) sawPackage = true;
      continue;
    }
    if (inPackage) {
      const kv = line.match(/^(\w[\w-]*)\s*=\s*(.+)$/);
      if (!kv) continue;
      const key = kv[1];
      let value = kv[2].trim();
      // Strip surrounding quotes (single or double).
      if (
        (value.startsWith('"') && value.endsWith('"')) ||
        (value.startsWith("'") && value.endsWith("'"))
      ) {
        value = value.slice(1, -1);
      }
      if (key === 'name') name = value;
      else if (key === 'license') license = value;
    } else if (!inOtherSection) {
      // Pre-section content (rare); ignore.
    }
  }

  return { name, license, isWorkspaceOnly: !sawPackage };
}

function isAllowed(license: string): boolean {
  if (ALLOWED_LICENSES.has(license)) return true;
  if (ALLOWED_EXPRESSIONS.has(license)) return true;
  // Tolerate "(A OR B)" parenthesised SPDX forms.
  const stripped = license.replace(/[()]/g, '').trim();
  if (ALLOWED_EXPRESSIONS.has(stripped)) return true;
  // SPDX `OR` disjunction: downstream may pick any single operand. The
  // expression as a whole is acceptable iff at least one operand is on
  // the allow-list. Accepts mixes like
  //   "GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0 OR EPL-2.0 OR MIT"
  // — even though GPL-3.0-or-later alone would be rejected, the
  // consumer can take the Apache or MIT branch.
  if (/\bOR\b/.test(stripped)) {
    const operands = stripped.split(/\s+OR\s+/).map(s => s.trim());
    if (operands.some(op => ALLOWED_LICENSES.has(op))) return true;
  }
  return false;
}

// ── Main ───────────────────────────────────────────────────────────────
const issues: string[] = [];
const seen: Array<{ path: string; name: string; license: string; kind: 'npm' | 'cargo' }> = [];

for (const pkgPath of findManifests('package.json')) {
  const rel = path.relative(ROOT, pkgPath);
  if (SKIP_PATTERNS.some(s => s.matcher(rel))) continue;
  let pkg: PackageJson;
  try {
    pkg = readJson<PackageJson>(pkgPath);
  } catch (e) {
    issues.push(`${rel}: failed to parse package.json (${(e as Error).message})`);
    continue;
  }

  const name = pkg.name ?? '<unnamed>';

  if (!pkg.license) {
    issues.push(`${rel} (${name}): missing "license" field`);
    continue;
  }

  if (!isAllowed(pkg.license)) {
    issues.push(
      `${rel} (${name}): license "${pkg.license}" is not on the allow-list. ` +
        `Update docs/architecture/LICENSE_COMPATIBILITY.md and deny.toml first.`
    );
    continue;
  }

  seen.push({ path: rel, name, license: pkg.license, kind: 'npm' });
}

for (const cargoPath of findManifests('Cargo.toml')) {
  const rel = path.relative(ROOT, cargoPath);
  let pkg: { name?: string; license?: string; isWorkspaceOnly: boolean };
  try {
    pkg = readCargoPackage(cargoPath);
  } catch (e) {
    issues.push(`${rel}: failed to parse Cargo.toml (${(e as Error).message})`);
    continue;
  }

  // Skip virtual workspaces (e.g. supramark root, the inner shadowed
  // workspace inside crates/graphviz-anywhere/) — they have no
  // [package] section to audit.
  if (pkg.isWorkspaceOnly) continue;

  const name = pkg.name ?? '<unnamed>';

  if (!pkg.license) {
    issues.push(`${rel} (${name}): missing "license" field in [package]`);
    continue;
  }

  if (!isAllowed(pkg.license)) {
    issues.push(
      `${rel} (${name}): license "${pkg.license}" is not on the allow-list. ` +
        `Update docs/architecture/LICENSE_COMPATIBILITY.md and deny.toml first.`
    );
    continue;
  }

  seen.push({ path: rel, name, license: pkg.license, kind: 'cargo' });
}

// Surface known non-default licenses (informational, not a violation).
const nonDefault = seen.filter(p => p.license !== 'Apache-2.0');

const npmCount = seen.filter(p => p.kind === 'npm').length;
const cargoCount = seen.filter(p => p.kind === 'cargo').length;
console.log(`\n📋 License audit · ${npmCount} package.json + ${cargoCount} Cargo.toml manifests\n`);

if (nonDefault.length > 0) {
  console.log('  Non-default-license packages:');
  for (const p of nonDefault) {
    const expected = KNOWN_NON_DEFAULT[p.name];
    const tag = expected
      ? expected === p.license
        ? '✓ matches expected'
        : `⚠️  expected ${expected}`
      : '(first-party override)';
    console.log(`    ${p.license.padEnd(20)} ${p.name}  ${tag}`);
  }
  console.log();
}

if (issues.length > 0) {
  console.error(`❌ ${issues.length} license issue(s):\n`);
  for (const issue of issues) console.error(`   • ${issue}`);
  console.error();
  process.exit(1);
}

console.log(`✅ All ${seen.length} manifests declare an allow-listed license.\n`);
