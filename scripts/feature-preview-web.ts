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
  discoverFeaturePackages,
  findFeaturePackageByShortName,
  selectMenu,
  closeRL,
  type FeaturePackageInfo,
  type SelectOption,
  log,
  colors,
} from './lib-feature-layout';
import {
  PLAYGROUND_ROUTES,
  playgroundPathForFeature,
  playgroundRouteStatus,
} from '../examples/react-web-csr/src/playground-routes';

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
    // The shared selectFeature() lists every discovered package and offers no
    // way to mark the ones without a playground page, so the menu is built
    // here instead: the playground route table decides what is previewable,
    // and the label says so before the user commits to a package that would
    // be rejected below.
    const discovered = discoverFeaturePackages();
    const options: SelectOption[] = discovered.map(pkg => {
      const status = playgroundRouteStatus(pkg.shortName);
      const statusNote =
        status.kind === 'excluded'
          ? 'no playground page'
          : status.kind === 'unregistered'
            ? 'not routed in the playground'
            : undefined;
      return {
        value: pkg.shortName,
        label: pkg.shortName,
        description: statusNote ?? pkg.metadata?.description,
      };
    });
    const choice = await selectMenu('Select feature to preview:', options);
    if (!choice) {
      log('\nCancelled.\n', 'yellow');
      process.exit(1);
    }
    // The lookup cannot miss: every option's value is a shortName from `discovered`.
    selected = discovered.find(pkg => pkg.shortName === choice)!;
  } else {
    selected = findFeaturePackageByShortName(argFeature);
    if (!selected) {
      log(`\nFeature not found: ${argFeature}\n`, 'red');
      process.exit(1);
    }
  }

  closeRL();

  // Feature discovery scans every `@supramark/feature-*` package, so the
  // selection can name a package the playground never routed. Report that
  // here rather than letting the path builder throw a stack trace.
  const status = playgroundRouteStatus(selected.shortName);

  if (status.kind === 'excluded') {
    log(`\n"${selected.shortName}" has no playground page.\n`, 'yellow');
    log(`  ${status.reason}\n`, 'gray');
    log(`  Previewable features: ${PLAYGROUND_ROUTES.map(r => r.feature).join(', ')}\n`, 'gray');
    process.exit(1);
  }

  if (status.kind === 'unregistered') {
    log(`\n"${selected.shortName}" is not registered as a playground route.\n`, 'red');
    log(
      '  Add it to PLAYGROUND_ROUTES (or PLAYGROUND_EXCLUDED_FEATURES) in examples/react-web-csr/src/playground-routes.ts.\n',
      'gray'
    );
    process.exit(1);
  }

  log(`\nStarting preview for: ${selected.shortName}\n`, 'green');

  const child = spawn(
    process.execPath,
    ['x', 'vite', '--host', '--open', playgroundPathForFeature(selected.shortName, '/')],
    { cwd: CSR_DIR, stdio: 'inherit' }
  );

  child.on('exit', code => process.exit(code ?? 0));
}

main().catch(err => {
  log(`\nError: ${err instanceof Error ? err.message : String(err)}\n`, 'red');
  console.error(err);
  process.exit(1);
});
