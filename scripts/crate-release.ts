#!/usr/bin/env bun
/**
 * Per-crate release bookkeeping for the Rust workspace.
 *
 * Why this exists: the only automated `cargo publish` in this repository is
 * graphviz-release.yml, which ships graphviz-anywhere and nothing else. Every
 * other crate -- mermaid-little, supramark-diagram, supramark-markdown and the
 * rest -- was published by hand, with no tag, no release and no gate, so a
 * landed fix stayed unreleased until someone remembered to run `cargo publish`.
 * #254 and #255 were fixed on main on 2026-10-16 and were still unpublished
 * twelve days later: the downstream consumer had the fix in git and no version
 * to depend on.
 *
 * The pipeline this script serves:
 *
 *   1. the fix PR carries the version bump. `bun run crate:bump <crate>` makes
 *      the edit; `crate:check` runs in the Lint job and fails the PR when a
 *      crate's source changed without one.
 *   2. on merge, crate-auto-tag.yml asks this script which publishable crates
 *      have a manifest version that is not on crates.io, tags each one
 *      `<crate>-v<version>` and dispatches crate-release.yml.
 *   3. crate-release.yml re-checks the tag against the manifest, runs the
 *      crate's own gate (fmt, clippy, tests) and publishes that one crate.
 *
 * The version line is the trigger, so nothing reaches crates.io that a human
 * did not bump in a reviewed PR. crates.io versions are immutable -- a wrong
 * publish burns the number permanently -- so that property is the point.
 *
 * Commands:
 *   list                        publishable crates, versions, intra-workspace deps
 *   next <crate> [--set V]      print the version the next release would take
 *   bump <crate>... [--set V]   write that version and propagate dependents' pins
 *   check [--base <ref>]        CI gate; see checkCommand() for what it asserts
 *   pending [--json]            crates whose manifest version is not on crates.io
 *   verify-tag <crate>-v<ver>   assert a release tag matches the manifest
 *   tag-for <crate>             the tag a release of the manifest version takes
 *   wait-deps <crate> [--timeout S]  block until the workspace deps it needs are on crates.io
 *
 * `--set` is the escape hatch for anything larger than the smallest increment
 * (a minor bump, or rebasing onto a new upstream version). The automatic path
 * only ever takes the smallest step, which is what makes it safe to automate.
 */

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const ROOT = process.cwd();

/**
 * Crates excluded from this pipeline, with the reason. graphviz-anywhere is
 * published by graphviz-release.yml on a `v*` tag: its build.rs downloads
 * per-platform release assets at install time, so the publish has to wait for
 * the cross-platform build matrix and the asset verification that workflow
 * runs. Tag namespaces do not collide -- that one owns `v*`, this one owns
 * `<crate>-v*`.
 */
const EXCLUDED: Record<string, string> = {
  'graphviz-anywhere': 'published by graphviz-release.yml on a v* tag (needs release assets first)',
};

const DEP_TABLE = /(^|\.)(dependencies|dev-dependencies|build-dependencies)$/;

interface DepEdge {
  /** The TOML key, which is an alias when the entry renames the crate. */
  key: string;
  /** Crate the entry resolves to, after `package = "..."` renames. */
  crate: string;
  /** Version requirement exactly as written, e.g. `=11.14.0-6`. Empty if absent. */
  req: string;
  /** The `path` value as written, relative to the manifest's directory. */
  rel: string;
  table: string;
  /** 0-based index into the manifest's lines, for the surgical rewrite. */
  line: number;
}

interface Crate {
  name: string;
  version: string;
  /** Repo-relative manifest path. */
  manifest: string;
  /** Repo-relative crate directory. */
  dir: string;
  publish: boolean;
  versionLine: number;
  deps: DepEdge[];
}

// ── Manifest reading ────────────────────────────────────────────────────────
// A minimal Cargo.toml scanner, for the same reason license-check.ts has one:
// everything needed here is a flat scalar or a single-line inline table, and a
// TOML dependency would have to be vendored for CI. The writer edits the one
// line it matched and leaves every other byte alone, so comments and ordering
// survive a bump.

/** Strip a trailing `# comment`, ignoring a `#` that sits inside a string. */
function stripComment(line: string): string {
  let quoted = false;
  for (let i = 0; i < line.length; i += 1) {
    const ch = line[i];
    if (ch === '"') quoted = !quoted;
    else if (ch === '#' && !quoted) return line.slice(0, i);
  }
  return line;
}

function parseManifest(manifestRel: string): Crate | null {
  const abs = path.join(ROOT, manifestRel);
  const lines = fs.readFileSync(abs, 'utf-8').split('\n');
  const dir = path.dirname(manifestRel);

  let section = '';
  let name = '';
  let version = '';
  let versionLine = -1;
  let publish = true;
  const pathDeps: DepEdge[] = [];

  lines.forEach((raw, index) => {
    const line = stripComment(raw).trim();
    if (!line) return;

    const sectionMatch = line.match(/^\[(.+?)\]$/);
    if (sectionMatch) {
      section = sectionMatch[1]!.trim();
      return;
    }

    if (section === 'package') {
      const kv = line.match(/^(name|version|publish)\s*=\s*(.+)$/);
      if (!kv) return;
      const value = kv[2]!.trim();
      if (kv[1] === 'name') name = value.replace(/^"|"$/g, '');
      else if (kv[1] === 'version' && versionLine === -1) {
        version = value.replace(/^"|"$/g, '');
        versionLine = index;
      } else if (kv[1] === 'publish') publish = value !== 'false';
      return;
    }

    if (!DEP_TABLE.test(section)) return;
    const kv = line.match(/^([A-Za-z0-9_.-]+)\s*=\s*(.+)$/);
    if (!kv) return;
    const value = kv[2]!.trim();
    // Only an inline table can carry `path`; `serde = "1"` never can.
    if (!value.startsWith('{')) return;
    const depPath = value.match(/\bpath\s*=\s*"([^"]+)"/);
    if (!depPath) return;
    const req = value.match(/\bversion\s*=\s*"([^"]+)"/);
    const renamed = value.match(/\bpackage\s*=\s*"([^"]+)"/);
    pathDeps.push({
      key: kv[1]!,
      crate: renamed ? renamed[1]! : kv[1]!,
      req: req ? req[1]! : '',
      rel: depPath[1]!,
      table: section,
      line: index,
    });
  });

  if (!name || versionLine === -1) return null; // virtual manifest, e.g. the workspace root

  return {
    name,
    version,
    manifest: manifestRel,
    dir,
    publish,
    versionLine,
    // `crate` still holds the name as written here. loadWorkspace() resolves
    // each `rel` against the member list, which is the only place that knows
    // every crate directory, and drops the edges that point outside it.
    deps: pathDeps,
  };
}

interface Workspace {
  crates: Crate[];
  byName: Map<string, Crate>;
  /** crate name -> crates that depend on it by path. */
  dependents: Map<string, Crate[]>;
}

function workspaceMembers(): string[] {
  const text = fs.readFileSync(path.join(ROOT, 'Cargo.toml'), 'utf-8');
  const block = text.match(/members\s*=\s*\[([\s\S]*?)\]/);
  if (!block) throw new Error('Cargo.toml: no [workspace] members list found');
  return [...block[1]!.matchAll(/"([^"]+)"/g)].map(m => m[1]!);
}

function loadWorkspace(): Workspace {
  const dirs = workspaceMembers();
  const crates: Crate[] = [];
  for (const dir of dirs) {
    const manifest = path.posix.join(dir, 'Cargo.toml');
    if (!fs.existsSync(path.join(ROOT, manifest))) {
      throw new Error(`Cargo.toml lists member ${dir}, but ${manifest} does not exist`);
    }
    const crate = parseManifest(manifest);
    if (crate) crates.push(crate);
  }

  const byDir = new Map(crates.map(c => [c.dir, c]));
  const byName = new Map(crates.map(c => [c.name, c]));

  // Resolve each path dependency to a member crate. Two kinds of edge are
  // dropped: one pointing outside the workspace, because nothing here can bump
  // it, and a crate's dev-dependency on itself -- mermaid-little and
  // plantuml-little both carry `{ path = "." }` to turn on a test-only feature,
  // which is not a release relationship.
  for (const crate of crates) {
    crate.deps = crate.deps.flatMap(edge => {
      const depDir = path.posix.normalize(path.posix.join(crate.dir, edge.rel));
      const target = byDir.get(depDir);
      if (!target || target.name === crate.name) return [];
      return [{ ...edge, crate: target.name }];
    });
  }

  const dependents = new Map<string, Crate[]>();
  for (const crate of crates) {
    for (const edge of crate.deps) {
      const list = dependents.get(edge.crate) ?? [];
      if (!list.includes(crate)) list.push(crate);
      dependents.set(edge.crate, list);
    }
  }

  return { crates, byName, dependents };
}

function publishable(ws: Workspace): Crate[] {
  return ws.crates.filter(c => c.publish && !EXCLUDED[c.name]);
}

// ── Versions ────────────────────────────────────────────────────────────────
// The workspace carries two version shapes. Crates that track an upstream
// project keep the upstream version and append a numeric revision --
// mermaid-little 11.14.0-6, plantuml-little 1.2026.2-4, d2-little 0.7.1-1 --
// where the leading part states which upstream release was ported and only the
// revision is ours to move. First-party crates use plain semver (0.1.6).
//
// So the automatic step is "increment the revision when there is one, else
// increment the patch". Bumping the patch of 11.14.0-6 would claim mermaid
// 11.14.1, an upstream release that may not exist.

interface Semver {
  major: number;
  minor: number;
  patch: number;
  pre: string;
}

export function parseVersion(version: string): Semver {
  const m = version.match(/^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/);
  if (!m) throw new Error(`cannot parse version "${version}"`);
  return { major: Number(m[1]), minor: Number(m[2]), patch: Number(m[3]), pre: m[4] ?? '' };
}

export function formatVersion(v: Semver): string {
  return `${v.major}.${v.minor}.${v.patch}${v.pre ? `-${v.pre}` : ''}`;
}

/** The smallest increment this pipeline is allowed to make on its own. */
export function nextVersion(version: string): string {
  const v = parseVersion(version);
  if (v.pre === '') return formatVersion({ ...v, patch: v.patch + 1 });
  if (/^\d+$/.test(v.pre)) return formatVersion({ ...v, pre: String(Number(v.pre) + 1) });
  throw new Error(
    `version "${version}" has a non-numeric pre-release; pass --set <version> to say what the next one is`
  );
}

function comparePre(a: string, b: string): number {
  if (a === b) return 0;
  // Semver: a version with a pre-release sorts below the same version without.
  if (a === '') return 1;
  if (b === '') return -1;
  const as = a.split('.');
  const bs = b.split('.');
  for (let i = 0; i < Math.max(as.length, bs.length); i += 1) {
    const x = as[i];
    const y = bs[i];
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    const xn = /^\d+$/.test(x);
    const yn = /^\d+$/.test(y);
    if (xn && yn) {
      if (Number(x) !== Number(y)) return Number(x) < Number(y) ? -1 : 1;
    } else if (xn !== yn) {
      return xn ? -1 : 1; // numeric identifiers sort below alphanumeric ones
    } else if (x !== y) {
      return x < y ? -1 : 1;
    }
  }
  return 0;
}

export function compareVersions(a: string, b: string): number {
  const x = parseVersion(a);
  const y = parseVersion(b);
  if (x.major !== y.major) return x.major < y.major ? -1 : 1;
  if (x.minor !== y.minor) return x.minor < y.minor ? -1 : 1;
  if (x.patch !== y.patch) return x.patch < y.patch ? -1 : 1;
  return comparePre(x.pre, y.pre);
}

/**
 * Does a requirement still accept this version? Only the two forms the
 * workspace actually uses are decided here: an exact `=X` and a bare/caret
 * requirement. Anything else returns `unknown`, and the caller reports it
 * instead of guessing -- a wrong "yes" would publish a crate pinned to a
 * version that no longer exists.
 */
export function reqAccepts(req: string, version: string): boolean | 'unknown' {
  const trimmed = req.trim();
  if (trimmed === '') return 'unknown';
  if (trimmed.startsWith('=')) return trimmed.slice(1).trim() === version;
  const caret = trimmed.startsWith('^') ? trimmed.slice(1).trim() : trimmed;
  if (!/^\d+(\.\d+){0,2}(-[0-9A-Za-z.-]+)?$/.test(caret)) return 'unknown';
  // Cargo does not let a requirement without a pre-release match a pre-release
  // version. Applying only the caret bound here would answer `true` for
  // `0.1` vs `0.1.2-1`, which cargo itself rejects -- and a wrong "yes"
  // publishes a requirement cargo cannot resolve.
  if (!caret.includes('-') && version.includes('-')) return 'unknown';
  // Pad a partial requirement (`0.1`) to a comparable floor (`0.1.0`).
  const parts = caret.split('-')[0]!.split('.');
  const pre = caret.includes('-') ? caret.slice(caret.indexOf('-') + 1) : '';
  while (parts.length < 3) parts.push('0');
  const floor = `${parts.join('.')}${pre ? `-${pre}` : ''}`;
  if (compareVersions(version, floor) < 0) return false;
  // Caret upper bound: the leftmost non-zero component may not change.
  const f = parseVersion(floor);
  const v = parseVersion(version);
  if (f.major > 0) return v.major === f.major;
  if (f.minor > 0) return v.major === 0 && v.minor === f.minor;
  return v.major === 0 && v.minor === 0 && v.patch === f.patch;
}

function rewriteReq(req: string, version: string): string {
  const trimmed = req.trim();
  if (trimmed.startsWith('=')) return `=${version}`;
  if (trimmed.startsWith('^')) return `^${version}`;
  return version;
}

// ── Manifest writing ────────────────────────────────────────────────────────

function editLine(manifestRel: string, lineIndex: number, edit: (line: string) => string): void {
  const abs = path.join(ROOT, manifestRel);
  const lines = fs.readFileSync(abs, 'utf-8').split('\n');
  const before = lines[lineIndex];
  if (before === undefined) throw new Error(`${manifestRel}: line ${lineIndex + 1} does not exist`);
  const after = edit(before);
  if (after === before) throw new Error(`${manifestRel}:${lineIndex + 1}: edit matched nothing: ${before.trim()}`);
  lines[lineIndex] = after;
  fs.writeFileSync(abs, lines.join('\n'));
}

// ── Cargo.lock ──────────────────────────────────────────────────────────────
// The lock file records every member's version, so a bump leaves it stale.
// Cargo would rewrite it on the next build and the diff would land in whatever
// PR happened to run cargo first, so the bump updates it here instead.

function refreshLock(): boolean {
  try {
    execFileSync('cargo', ['update', '--workspace', '--offline', '--quiet'], {
      cwd: ROOT,
      stdio: ['ignore', 'ignore', 'pipe'],
    });
    return true;
  } catch {
    return false;
  }
}

function lockVersions(): Map<string, string[]> {
  const lockPath = path.join(ROOT, 'Cargo.lock');
  const versions = new Map<string, string[]>();
  if (!fs.existsSync(lockPath)) return versions;
  const text = fs.readFileSync(lockPath, 'utf-8');
  for (const block of text.split(/\n\[\[package\]\]\n/)) {
    const name = block.match(/^name = "([^"]+)"/m);
    const version = block.match(/^version = "([^"]+)"/m);
    if (!name || !version) continue;
    const list = versions.get(name[1]!) ?? [];
    list.push(version[1]!);
    versions.set(name[1]!, list);
  }
  return versions;
}

// ── crates.io ───────────────────────────────────────────────────────────────

/** Sparse-index path for a crate name, per the registry layout. */
function indexPath(name: string): string {
  const n = name.toLowerCase();
  if (n.length === 1) return `1/${n}`;
  if (n.length === 2) return `2/${n}`;
  if (n.length === 3) return `3/${n[0]}/${n}`;
  return `${n.slice(0, 2)}/${n.slice(2, 4)}/${n}`;
}

interface Published {
  /** Every non-yanked version, oldest first. Empty means the name is unused. */
  versions: string[];
  /** Versions yanked on crates.io. A yanked version can never be republished. */
  yanked: string[];
  /** The crate exists on crates.io, yanked versions included. */
  exists: boolean;
}

const INDEX_ATTEMPTS = 3;
const INDEX_RETRY_BASE_MS = 1000;

async function publishedVersions(name: string): Promise<Published> {
  const url = `https://index.crates.io/${indexPath(name)}`;
  let lastError: Error | null = null;
  for (let attempt = 1; attempt <= INDEX_ATTEMPTS; attempt += 1) {
    try {
      const response = await fetch(url, {
        headers: { 'user-agent': 'supramark-crate-release (https://github.com/Actrium/supramark)' },
      });
      if (response.status === 404) return { versions: [], yanked: [], exists: false };
      if (!response.ok) throw new Error(`crates.io index returned ${response.status} for ${name}`);
      const body = await response.text();
      const versions: string[] = [];
      const yanked: string[] = [];
      for (const line of body.split('\n')) {
        if (!line.trim()) continue;
        const entry = JSON.parse(line) as { vers: string; yanked?: boolean };
        if (entry.yanked) yanked.push(entry.vers);
        else versions.push(entry.vers);
      }
      return { versions, yanked, exists: true };
    } catch (error) {
      lastError = error as Error;
      if (attempt < INDEX_ATTEMPTS) {
        await new Promise(resolve => setTimeout(resolve, INDEX_RETRY_BASE_MS * 2 ** (attempt - 1)));
      }
    }
  }
  throw lastError ?? new Error(`crates.io index request failed for ${name}`);
}

// ── Git ─────────────────────────────────────────────────────────────────────

function git(args: string[]): string {
  return execFileSync('git', args, { cwd: ROOT, encoding: 'utf-8' }).trimEnd();
}

function changedFiles(base: string): string[] {
  return git(['diff', '--name-only', base, 'HEAD']).split('\n').filter(Boolean);
}

function versionAt(ref: string, manifestRel: string): string | null {
  let text: string;
  try {
    text = git(['show', `${ref}:${manifestRel}`]);
  } catch {
    return null; // the manifest did not exist at that ref
  }
  let section = '';
  for (const raw of text.split('\n')) {
    const line = stripComment(raw).trim();
    const sectionMatch = line.match(/^\[(.+?)\]$/);
    if (sectionMatch) {
      section = sectionMatch[1]!.trim();
      continue;
    }
    if (section !== 'package') continue;
    const kv = line.match(/^version\s*=\s*"([^"]+)"/);
    if (kv) return kv[1]!;
  }
  return null;
}

/**
 * Which crate owns a path, by longest matching crate directory. The
 * sub-crates under `crates/<name>/packages/` are separate members that are
 * never published, so a change there belongs to them and must not demand a
 * release of the library above it.
 */
function owningCrate(ws: Workspace, file: string): Crate | null {
  let best: Crate | null = null;
  for (const crate of ws.crates) {
    if (file === crate.manifest || file.startsWith(`${crate.dir}/`)) {
      if (!best || crate.dir.length > best.dir.length) best = crate;
    }
  }
  return best;
}

/**
 * Does this file change what a `cargo publish` of its crate would ship?
 * Documentation, tests, benches, examples and CI config do not, so they do not
 * demand a version bump.
 */
function affectsPackage(file: string, crate: Crate): boolean {
  const rel = file === crate.manifest ? 'Cargo.toml' : file.slice(crate.dir.length + 1);
  if (rel.startsWith('tests/') || rel.startsWith('benches/') || rel.startsWith('examples/')) return false;
  if (rel.startsWith('.github/') || rel.startsWith('docs/')) return false;
  if (/\.(md|license)$/i.test(rel)) return false;
  if (rel === 'CHANGELOG.md' || rel === 'UPSTREAM.md') return false;
  return true;
}

// ── Commands ────────────────────────────────────────────────────────────────

function listCommand(ws: Workspace): void {
  for (const crate of publishable(ws)) {
    const deps = crate.deps.map(d => `${d.crate}@${d.req || '(no version)'}`).join(', ');
    console.log(`${crate.name.padEnd(26)} ${crate.version.padEnd(14)} ${crate.manifest}${deps ? `\n${' '.repeat(42)}deps: ${deps}` : ''}`);
  }
  const skipped = Object.entries(EXCLUDED);
  if (skipped.length > 0) {
    console.log('\nnot in this pipeline:');
    for (const [name, reason] of skipped) console.log(`  ${name}: ${reason}`);
  }
}

interface BumpResult {
  crate: string;
  from: string;
  to: string;
  /** Why this crate moved: requested, or carried by a dependency bump. */
  reason: string;
}

interface Edit {
  manifest: string;
  line: number;
  description: string;
  apply: (line: string) => string;
}

interface BumpPlan {
  bumps: BumpResult[];
  edits: Edit[];
}

/**
 * Work out which crates a bump moves, and which manifest lines change.
 *
 * A dependent follows when its requirement can no longer resolve to the new
 * version -- which is always true of an exact pin such as
 * `mermaid-little = "=11.14.0-6"`. Rewriting that pin changes what the
 * dependent ships, so the dependent needs a release of its own, and so on up
 * the graph. A caret requirement that still accepts the new version needs
 * neither edit nor release, and a path-only dependency (the wasm and native
 * sub-crates, which are never published) carries no requirement to maintain.
 *
 * The plan is computed in full before anything is written, so a requirement
 * this script cannot read aborts the whole bump instead of leaving half of it
 * on disk.
 */
function planBump(ws: Workspace, requested: Array<{ crate: Crate; version: string }>): BumpPlan {
  const bumps: BumpResult[] = [];
  const edits: Edit[] = [];
  const versions = new Map<string, string>();
  const queue = requested.map(r => ({ crate: r.crate, version: r.version, reason: 'requested' }));

  while (queue.length > 0) {
    const item = queue.shift()!;
    if (versions.has(item.crate.name)) continue;
    versions.set(item.crate.name, item.version);
    bumps.push({ crate: item.crate.name, from: item.crate.version, to: item.version, reason: item.reason });
    const version = item.version;

    edits.push({
      manifest: item.crate.manifest,
      line: item.crate.versionLine,
      description: `${item.crate.name} version -> ${version}`,
      apply: line => line.replace(/version\s*=\s*"[^"]*"/, `version = "${version}"`),
    });

    for (const dependent of ws.dependents.get(item.crate.name) ?? []) {
      for (const edge of dependent.deps.filter(e => e.crate === item.crate.name)) {
        if (edge.req === '') continue;
        const accepts = reqAccepts(edge.req, version);
        if (accepts === true) continue;
        if (accepts === 'unknown') {
          throw new Error(
            `${dependent.manifest}:${edge.line + 1}: cannot tell whether "${edge.req}" accepts ` +
              `${item.crate.name} ${version}; update it by hand`
          );
        }
        const req = rewriteReq(edge.req, version);
        edits.push({
          manifest: dependent.manifest,
          line: edge.line,
          description: `${dependent.name}: ${edge.crate} requirement -> ${req}`,
          apply: line => {
            if (!/\bversion\s*=\s*"[^"]*"/.test(line)) {
              throw new Error(
                `${dependent.manifest}:${edge.line + 1}: dependency on ${edge.crate} has no version requirement to update`
              );
            }
            return line.replace(/\bversion\s*=\s*"[^"]*"/, `version = "${req}"`);
          },
        });
        // Only a crate this pipeline publishes needs a release of its own. The
        // wasm and native wrappers set `publish = false`: their requirement is
        // kept in step, but they have no version to move.
        if (!dependent.publish || EXCLUDED[dependent.name] || versions.has(dependent.name)) continue;
        queue.push({
          crate: dependent,
          version: nextVersion(dependent.version),
          reason: `follows ${item.crate.name} ${version}`,
        });
      }
    }
  }

  return { bumps, edits };
}

function applyEdits(edits: Edit[]): void {
  for (const edit of edits) {
    editLine(edit.manifest, edit.line, edit.apply);
  }
}

function bumpCommand(ws: Workspace, names: string[], set: string | null): void {
  if (names.length === 0) throw new Error('bump needs at least one crate name');
  if (set && names.length > 1) throw new Error('--set applies to a single crate');

  const requested = names.map(name => {
    const crate = ws.byName.get(name);
    if (!crate) throw new Error(`unknown crate "${name}"; run \`list\` to see the publishable ones`);
    if (!crate.publish || EXCLUDED[name]) {
      throw new Error(`${name} is not published by this pipeline${EXCLUDED[name] ? `: ${EXCLUDED[name]}` : ''}`);
    }
    const version = set ?? nextVersion(crate.version);
    if (compareVersions(version, crate.version) <= 0) {
      throw new Error(`${name}: ${version} is not greater than the current ${crate.version}`);
    }
    return { crate, version };
  });

  const plan = planBump(ws, requested);
  applyEdits(plan.edits);
  for (const r of plan.bumps) {
    console.log(`${r.crate}: ${r.from} -> ${r.to}  (${r.reason})`);
  }
  if (refreshLock()) console.log('Cargo.lock refreshed');
  else console.log('Cargo.lock NOT refreshed (cargo unavailable or offline resolution failed) -- run `cargo update --workspace`');
  console.log('\nCommit this together with the fix. On merge the release pipeline tags and publishes it.');
}

function nextCommand(ws: Workspace, name: string, set: string | null): void {
  const crate = ws.byName.get(name);
  if (!crate) throw new Error(`unknown crate "${name}"`);
  console.log(set ?? nextVersion(crate.version));
}

/**
 * Order release candidates so a dependency is tagged and dispatched before
 * anything that depends on it. Dispatch order is not a completion guarantee
 * (`crate-release.yml` waits on the index before publishing), but it keeps the
 * common case from racing.
 */
function pendingOrder(ws: Workspace, crates: Crate[]): Crate[] {
  const pending = new Set(crates.map(crate => crate.name));
  const ordered: Crate[] = [];
  const visited = new Set<string>();
  const visit = (crate: Crate): void => {
    if (visited.has(crate.name)) return;
    visited.add(crate.name);
    for (const edge of crate.deps) {
      const dependency = ws.byName.get(edge.crate);
      if (dependency && pending.has(dependency.name)) visit(dependency);
    }
    ordered.push(crate);
  };
  for (const crate of crates) visit(crate);
  return ordered;
}

async function pendingCommand(ws: Workspace, asJson: boolean): Promise<void> {
  const rows: Array<{ crate: string; version: string; tag: string }> = [];
  const candidates: Crate[] = [];
  for (const crate of publishable(ws)) {
    const published = await publishedVersions(crate.name);
    if (!published.exists) {
      // A name with nothing on crates.io needs a deliberate first publish: it
      // claims the name, and ownership cannot be undone from a workflow.
      console.error(`note: ${crate.name} has never been published; release it with a manual dispatch`);
      continue;
    }
    if (published.versions.includes(crate.version)) continue;
    if (published.yanked.includes(crate.version)) {
      // crates.io never lets a yanked version be republished, so retrying is
      // impossible: the manifest has to move to a new version.
      console.error(
        `note: ${crate.name} ${crate.version} is yanked on crates.io; bump the manifest to publish again`
      );
      continue;
    }
    candidates.push(crate);
  }
  for (const crate of pendingOrder(ws, candidates)) {
    rows.push({ crate: crate.name, version: crate.version, tag: `${crate.name}-v${crate.version}` });
  }
  if (asJson) console.log(JSON.stringify(rows));
  else for (const row of rows) console.log(`${row.crate}\t${row.version}\t${row.tag}`);
}

export function parseTag(tag: string): { crate: string; version: string } {
  const m = tag.match(/^(.+)-v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$/);
  if (!m) throw new Error(`tag "${tag}" is not <crate>-v<version>`);
  return { crate: m[1]!, version: m[2]! };
}

/**
 * Identity of one release, as `KEY=VALUE` lines the workflow appends straight
 * to `$GITHUB_OUTPUT`. Keeping the parse here means the workflow never has to
 * split a crate name from a version in shell, where `mermaid-little-v11.14.0-7`
 * is easy to cut in the wrong place.
 */
function printReleaseId(crate: Crate): void {
  console.log(`crate=${crate.name}`);
  console.log(`version=${crate.version}`);
  console.log(`tag=${crate.name}-v${crate.version}`);
}

function releaseTarget(ws: Workspace, name: string): Crate {
  const crate = ws.byName.get(name);
  if (!crate) throw new Error(`"${name}" is not a workspace member`);
  if (EXCLUDED[name]) throw new Error(`${name} is ${EXCLUDED[name]}`);
  if (!crate.publish) throw new Error(`${name} sets publish = false`);
  return crate;
}

function verifyTagCommand(ws: Workspace, tag: string): void {
  const { crate: name, version } = parseTag(tag);
  const crate = releaseTarget(ws, name);
  if (crate.version !== version) {
    throw new Error(`tag ${tag} does not match ${crate.manifest}, which says version "${crate.version}"`);
  }
  console.error(`${tag} matches ${crate.manifest}`);
  printReleaseId(crate);
}

function tagForCommand(ws: Workspace, name: string): void {
  const crate = releaseTarget(ws, name);
  console.error(`${crate.name} ${crate.version} per ${crate.manifest}`);
  printReleaseId(crate);
}

const WAIT_POLL_MS = 10_000;

/**
 * Block until every workspace dependency this release needs is resolvable on
 * crates.io.
 *
 * `cargo publish` resolves intra-workspace dependencies against the registry,
 * not the path, so a dependent released from the same push as its dependency
 * can reach `publish` before the dependency exists. `crate-auto-tag.yml` orders
 * dispatches by dependencies, but dispatch order is not a completion order --
 * this is the part that actually waits.
 *
 * The test is whether cargo can resolve the requirement against the index
 * *now*: if an already-published version satisfies it, there is nothing to
 * wait for. An exact pin to the in-tree version is the case that does have to
 * wait, because the published manifest will name that exact version.
 */
async function waitDepsCommand(ws: Workspace, name: string, timeoutSeconds: number): Promise<void> {
  const crate = releaseTarget(ws, name);
  const waits: Array<{ name: string; version: string; req: string }> = [];

  for (const edge of crate.deps) {
    if (edge.req === '') continue;
    const dependency = ws.byName.get(edge.crate);
    if (!dependency) continue;

    const published = await publishedVersions(dependency.name);
    const satisfiable = published.versions.some(version => reqAccepts(edge.req, version) === true);
    if (satisfiable) continue;

    const treeAccepts = reqAccepts(edge.req, dependency.version);
    if (treeAccepts === false) {
      // A stale requirement that neither a published version nor the in-tree
      // version satisfies. Waiting cannot fix it; the bookkeeping gate
      // (`crate:check`) reports the same inconsistency.
      throw new Error(
        `${crate.name} requires ${dependency.name} "${edge.req}", but the tree has ` +
          `${dependency.version} and crates.io has nothing that satisfies it`
      );
    }
    if (!dependency.publish || EXCLUDED[dependency.name]) {
      // A version this pipeline will never publish: waiting would only burn
      // the timeout. It has to be published by hand (graphviz-release.yml or a
      // manual dispatch) before this crate can ship.
      throw new Error(
        `${crate.name} requires ${dependency.name} ${dependency.version} ` +
          `(requirement "${edge.req}"), but that crate is not published by this pipeline`
      );
    }
    waits.push({ name: dependency.name, version: dependency.version, req: edge.req });
  }

  if (waits.length === 0) {
    console.log(`${crate.name}: no workspace dependency needs to be published first`);
    return;
  }

  const deadline = Date.now() + timeoutSeconds * 1000;
  for (const dependency of waits) {
    for (;;) {
      // A transient index failure must not end the whole wait window;
      // publishedVersions already retries, and the deadline below still
      // bounds this loop.
      let published: Published | null = null;
      try {
        published = await publishedVersions(dependency.name);
      } catch {
        published = null;
      }
      if (published?.versions.includes(dependency.version)) {
        console.log(`${dependency.name} ${dependency.version} is on crates.io`);
        break;
      }
      if (Date.now() >= deadline) {
        throw new Error(
          `${crate.name} requires ${dependency.name} ${dependency.version} ` +
            `(requirement "${dependency.req}"), but it is not on crates.io after ${timeoutSeconds}s`
        );
      }
      await new Promise(resolve => setTimeout(resolve, WAIT_POLL_MS));
    }
  }
}

/**
 * The PR gate. Three assertions, in order of how badly each one breaks a
 * release:
 *
 *   1. Intra-workspace requirements accept the versions in the tree. An exact
 *      pin left behind by a bump makes `cargo publish` fail -- or worse,
 *      succeed against a version that is about to be superseded.
 *   2. Every member's version matches Cargo.lock, so the bump did not leave
 *      the lock for someone else's PR to update.
 *   3. A crate whose packaged contents changed has a version that differs from
 *      the base ref and is not already on crates.io. This is what turns "fix
 *      merged" into "fix released": without it the fix lands and nothing
 *      publishes, which is exactly how #254 and #255 stayed unreleased.
 */
async function checkCommand(ws: Workspace, base: string | null, offline: boolean): Promise<number> {
  const problems: string[] = [];
  const notes: string[] = [];

  for (const crate of ws.crates) {
    for (const edge of crate.deps) {
      const target = ws.byName.get(edge.crate)!;
      if (edge.req === '') {
        // A path-only dev-dependency is stripped when the crate is packaged,
        // so it never blocks a publish. Any other table does.
        const devOnly = /(^|\.)dev-dependencies$/.test(edge.table);
        if (!devOnly && crate.publish && !EXCLUDED[crate.name]) {
          problems.push(
            `${crate.manifest}:${edge.line + 1}: path dependency on ${edge.crate} has no version requirement, ` +
              `so ${crate.name} cannot be published`
          );
        }
        continue;
      }
      const accepts = reqAccepts(edge.req, target.version);
      if (accepts === false) {
        problems.push(
          `${crate.manifest}:${edge.line + 1}: requirement "${edge.req}" on ${edge.crate} does not accept ` +
            `the version in the tree (${target.version}) -- run \`bun run crate:bump ${edge.crate}\``
        );
      } else if (accepts === 'unknown') {
        notes.push(`${crate.manifest}:${edge.line + 1}: requirement "${edge.req}" on ${edge.crate} not understood; review by hand`);
      }
    }
  }

  const locked = lockVersions();
  for (const crate of ws.crates) {
    const versions = locked.get(crate.name);
    if (!versions) continue; // not every member is in the lock (wasm-only members may be absent)
    if (!versions.includes(crate.version)) {
      problems.push(
        `Cargo.lock has ${crate.name} ${versions.join(', ')} but ${crate.manifest} says ${crate.version} -- ` +
          `run \`cargo update --workspace\``
      );
    }
  }

  if (base) {
    const files = changedFiles(base);
    const touched = new Map<string, string[]>();
    for (const file of files) {
      const crate = owningCrate(ws, file);
      if (!crate || !crate.publish || EXCLUDED[crate.name]) continue;
      if (!affectsPackage(file, crate)) continue;
      const list = touched.get(crate.name) ?? [];
      list.push(file);
      touched.set(crate.name, list);
    }

    for (const [name, changed] of touched) {
      const crate = ws.byName.get(name)!;
      const baseVersion = versionAt(base, crate.manifest);
      if (baseVersion === null) continue; // new crate in this PR; its first publish is manual
      if (baseVersion === crate.version) {
        problems.push(
          `${name} ships changed files (${changed.slice(0, 3).join(', ')}${changed.length > 3 ? `, +${changed.length - 3} more` : ''}) ` +
            `but its version is still ${crate.version}. Run \`bun run crate:bump ${name}\`, or label the PR ` +
            `\`release:skip\` if this change needs no release.`
        );
        continue;
      }
      if (compareVersions(crate.version, baseVersion) <= 0) {
        problems.push(`${name}: version went backwards, ${baseVersion} -> ${crate.version}`);
        continue;
      }
      if (offline) continue;
      try {
        const published = await publishedVersions(name);
        if (published.versions.includes(crate.version)) {
          problems.push(
            `${name} ${crate.version} is already on crates.io; pick the next one ` +
              `(\`bun run crate:next ${name}\` after rebasing on main)`
          );
        }
      } catch (error) {
        notes.push(`could not reach crates.io for ${name}: ${(error as Error).message}`);
      }
    }

    if (touched.size === 0) notes.push('no publishable crate ships changed files in this diff');
  } else {
    notes.push('no --base given: checked requirement and lock consistency only');
  }

  for (const note of notes) console.log(`note: ${note}`);
  for (const problem of problems) console.error(`error: ${problem}`);
  if (problems.length > 0) {
    console.error(`\n${problems.length} release bookkeeping problem(s). See docs/guide/CRATE_RELEASE.zh.md.`);
    return 1;
  }
  console.log('release bookkeeping ok');
  return 0;
}

// ── Entry point ─────────────────────────────────────────────────────────────

function flag(args: string[], name: string): string | null {
  const index = args.indexOf(name);
  if (index === -1) return null;
  const value = args[index + 1];
  if (value === undefined) throw new Error(`${name} needs a value`);
  args.splice(index, 2);
  return value;
}

function has(args: string[], name: string): boolean {
  const index = args.indexOf(name);
  if (index === -1) return false;
  args.splice(index, 1);
  return true;
}

function positiveInt(value: string | null, fallback: number, name: string): number {
  if (value === null) return fallback;
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer`);
  return parsed;
}

async function main(argv: string[]): Promise<number> {
  const args = [...argv];
  const command = args.shift() ?? 'check';
  const set = flag(args, '--set');
  const base = flag(args, '--base');
  const timeout = flag(args, '--timeout');
  const asJson = has(args, '--json');
  const offline = has(args, '--offline');
  const ws = loadWorkspace();

  switch (command) {
    case 'list':
      listCommand(ws);
      return 0;
    case 'next':
      nextCommand(ws, args[0] ?? '', set);
      return 0;
    case 'bump':
      bumpCommand(ws, args, set);
      return 0;
    case 'pending':
      await pendingCommand(ws, asJson);
      return 0;
    case 'verify-tag':
      verifyTagCommand(ws, args[0] ?? '');
      return 0;
    case 'tag-for':
      tagForCommand(ws, args[0] ?? '');
      return 0;
    case 'wait-deps':
      await waitDepsCommand(ws, args[0] ?? '', positiveInt(timeout, 900, '--timeout'));
      return 0;
    case 'check':
      return await checkCommand(ws, base, offline);
    default:
      console.error(`unknown command "${command}"; see the header of scripts/crate-release.ts`);
      return 2;
  }
}

// Importable for the unit tests, which exercise the version algebra directly.
if (import.meta.main) {
  try {
    process.exit(await main(process.argv.slice(2)));
  } catch (error) {
    console.error(`error: ${(error as Error).message}`);
    process.exit(1);
  }
}
