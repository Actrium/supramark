#!/usr/bin/env node

/**
 * Supramark Feature Web Preview
 *
 * Launch Vite dev server and open the preview page for a specific feature.
 *
 * Usage:
 *   bun run feature:preview:web
 *   bun run feature:preview:web <feature-name>
 */

import path from 'node:path';
import { spawn } from 'node:child_process';
import {
  findFeaturePackageByShortName,
  selectFeature,
  closeRL,
  type FeaturePackageInfo,
  log,
  colors,
} from './lib-feature-layout';

const CSR_DIR = path.resolve(__dirname, '..', 'examples/react-web-csr');

async function main(): Promise<void> {
  const args = process.argv.slice(2);

  if (args.includes('--help') || args.includes('-h')) {
    log(`
${colors.bright}Usage:${colors.reset}
  bun run feature:preview:web              # Interactive selection
  bun run feature:preview:web <name>       # Preview a specific feature

${colors.blue}Examples:${colors.reset}
  ${colors.gray}# Interactive${colors.reset}
  bun run feature:preview:web

  ${colors.gray}# Specific feature${colors.reset}
  bun run feature:preview:web math
  bun run feature:preview:web gfm
  bun run feature:preview:web mermaid
  bun run feature:preview:web diagram-echarts
`);
    process.exit(0);
  }

  const argFeature = args.find(arg => !arg.startsWith('--'));
  let selected: FeaturePackageInfo | null = null;

  if (!argFeature) {
    selected = await selectFeature('Select feature to preview:');
    if (!selected) {
      log('\nCancelled.\n', 'yellow');
      process.exit(1);
    }
  } else {
    selected = findFeaturePackageByShortName(argFeature);
    if (!selected) {
      log(`\nFeature not found: ${argFeature}\n`, 'red');
      process.exit(1);
    }
  }

  closeRL();

  log(`\nStarting preview for: ${selected.shortName}\n`, 'green');

  const child = spawn(
    process.execPath,
    ['x', 'vite', '--host', '--open', `/?feature=${selected.shortName}`],
    { cwd: CSR_DIR, stdio: 'inherit' }
  );

  child.on('exit', code => process.exit(code ?? 0));
}

main().catch(err => {
  log(`\nError: ${err instanceof Error ? err.message : String(err)}\n`, 'red');
  console.error(err);
  process.exit(1);
});
