#!/usr/bin/env bun
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

function cachedWasmBindgenDirs(): string[] {
  const home = process.env.HOME;
  if (!home) return [];

  const roots = [
    path.join(home, 'Library/Caches/.wasm-pack'),
    path.join(home, '.cache/.wasm-pack'),
  ];
  const dirs: string[] = [];

  for (const root of roots) {
    if (!fs.existsSync(root)) continue;
    for (const entry of fs.readdirSync(root)) {
      if (!entry.startsWith('wasm-bindgen-cargo-install-')) continue;
      const dir = path.join(root, entry);
      const bin = path.join(dir, process.platform === 'win32' ? 'wasm-bindgen.exe' : 'wasm-bindgen');
      if (fs.existsSync(bin)) {
        dirs.push(dir);
      }
    }
  }

  return dirs.sort().reverse();
}

const env = {
  ...process.env,
  PATH: [...cachedWasmBindgenDirs(), process.env.PATH ?? ''].filter(Boolean).join(path.delimiter),
};

const proc = spawn('wasm-pack', process.argv.slice(2), { stdio: 'inherit', env });
proc.on('exit', code => process.exit(code ?? 1));
proc.on('error', error => {
  console.error(error);
  process.exit(1);
});
