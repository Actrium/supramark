#!/usr/bin/env bun
/**
 * Build all in-tree wasm-bindgen wrapper crates with `wasm-pack`.
 *
 * After step 4 of the super-monorepo merge, supramark's
 * `packages/engines` consumes the four `@actrium/*-web` packages
 * directly from the workspace (no npm registry lookup). For those
 * dynamic imports to resolve, the wrapper crates must have been built
 * locally first — that's what this script does.
 *
 * Usage:
 *   bun run build:wasm                 # build all
 *   bun run build:wasm mermaid         # build a single one
 *
 * CI invokes this once, before tests + lint, after installing rust +
 * wasm-pack via the standard setup actions.
 */

import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

interface WasmTarget {
  /** Short id used as CLI filter (`bun run build:wasm <id>`). */
  id: string;
  /** Path relative to repo root containing the wasm-bindgen crate's package.json. */
  cwd: string;
  /** Human-readable label for log lines. */
  label: string;
}

const TARGETS: WasmTarget[] = [
  {
    id: 'plantuml',
    cwd: 'crates/plantuml-little/packages/web',
    label: '@actrium/plantuml-little-web',
  },
  {
    id: 'd2',
    cwd: 'crates/d2-little/packages/web',
    label: '@actrium/d2-little-web',
  },
  {
    id: 'mermaid',
    cwd: 'crates/mermaid-little/packages/web',
    label: '@actrium/mermaid-little-web',
  },
  {
    id: 'markdown',
    cwd: 'crates/supramark-markdown/packages/web',
    label: '@supramark/markdown-web',
  },
];

const ROOT = process.cwd();

function ensureWasmPackAvailable(): void {
  const result = Bun.spawnSync(['wasm-pack', '--version']);
  if (result.exitCode !== 0) {
    console.error(
      '❌ `wasm-pack` not found on PATH.\n' +
        '   Install with: cargo install wasm-pack\n' +
        '   Or follow https://rustwasm.github.io/wasm-pack/installer/'
    );
    process.exit(1);
  }
}

async function buildOne(target: WasmTarget): Promise<void> {
  const cwd = path.join(ROOT, target.cwd);
  if (!fs.existsSync(path.join(cwd, 'package.json'))) {
    console.error(`❌ ${target.label}: ${target.cwd}/package.json not found`);
    process.exit(1);
  }

  console.log(`\n▶ ${target.label}  (${target.cwd})`);

  await new Promise<void>((resolve, reject) => {
    // Empty RUSTFLAGS / CARGO_BUILD_RUSTFLAGS for the wasm-pack call.
    // Many devs set `~/.cargo/config.toml#build.rustflags` to inject
    // mold ("-C link-arg=-fuse-ld=mold") for fast native linking; that
    // flag is unsupported by `rust-lld` (the wasm32 linker) and breaks
    // builds. The workspace `.cargo/config.toml` also pins
    // `target.wasm32-unknown-unknown.rustflags = []` for cargo's own
    // resolution, but env vars take precedence and re-introduce the
    // global flag — so we strip them here for the wasm pipeline only.
    const env = {
      ...process.env,
      NO_COLOR: process.env.NO_COLOR ?? '',
      RUSTFLAGS: '',
      CARGO_BUILD_RUSTFLAGS: '',
    };
    const proc = spawn('bun', ['run', 'build'], { cwd, stdio: 'inherit', env });
    proc.on('exit', code => {
      if (code === 0) resolve();
      else reject(new Error(`${target.label} build failed (exit ${code})`));
    });
    proc.on('error', reject);
  });

  console.log(`✓ ${target.label}`);
}

const filter = process.argv[2];
const selected = filter ? TARGETS.filter(t => t.id === filter) : TARGETS;

if (selected.length === 0) {
  console.error(
    `❌ No matching target for filter "${filter}".\n` +
      `   Available: ${TARGETS.map(t => t.id).join(', ')}`
  );
  process.exit(1);
}

ensureWasmPackAvailable();

console.log(`📦 Building ${selected.length} wasm-bindgen wrapper(s) — wasm-pack + tsc`);

for (const target of selected) {
  await buildOne(target);
}

console.log(`\n✅ Built ${selected.length} wasm package(s).`);
