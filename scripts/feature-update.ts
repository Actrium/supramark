#!/usr/bin/env node

/**
 * Supramark Feature Incremental Update Tool
 *
 * Usage:
 *   bun run feature:update
 *   bun run feature:update <feature-name>
 *   bun run feature:update -- --fix
 *   bun run feature:update -- --dry-run --fix
 */

import fs from 'node:fs';
import path from 'node:path';
import {
  findFeaturePackageByShortName,
  selectFeature,
  type FeaturePackageInfo,
  log,
  colors,
} from './lib-feature-layout';

interface CheckItem {
  name: string;
  file?: string;
  check?: (pkgPath: string) => boolean;
  severity: 'critical' | 'high' | 'medium' | 'low';
  description: string;
}

const CHECKS: Record<string, CheckItem> = {
  jestConfig: {
    name: 'Jest config',
    file: 'jest.config.cjs',
    severity: 'high',
    description: 'Missing Jest config file, tests cannot run',
  },
  tsConfig: {
    name: 'TypeScript config',
    file: 'tsconfig.json',
    severity: 'high',
    description: 'Missing TypeScript config file',
  },
  srcIndex: {
    name: 'Export entry point',
    file: 'src/index.ts',
    severity: 'high',
    description: 'Missing package export entry file',
  },
  packageJson: {
    name: 'package.json',
    file: 'package.json',
    severity: 'critical',
    description: 'Missing package.json file',
  },
  tsJestDep: {
    name: 'ts-jest dependency',
    check: (pkgPath: string) => {
      const pkgJsonPath = path.join(pkgPath, 'package.json');
      if (!fs.existsSync(pkgJsonPath)) return false;
      const pkgJson = JSON.parse(fs.readFileSync(pkgJsonPath, 'utf-8'));
      const devDeps = pkgJson.devDependencies || {};
      return 'ts-jest' in devDeps;
    },
    severity: 'medium',
    description: 'package.json is missing the ts-jest dependency',
  },
  multiNodeTypeGuidance: {
    name: 'Multi-node-type guidance comment',
    check: (pkgPath: string) => {
      const featurePath = path.join(pkgPath, 'src/feature.ts');
      if (!fs.existsSync(featurePath)) return false;
      const content = fs.readFileSync(featurePath, 'utf-8');
      // Matches the guidance comment that feature-create.ts emits into a new
      // feature.ts. Both phrasings are accepted because the template wording is
      // not identical across the features generated from it over time.
      return content.includes('multi node type handling') || content.includes('Node type notes');
    },
    severity: 'low',
    description: 'Feature definition file is missing multi-node-type handling guidance',
  },
};

function generateJestConfig(): string {
  return `/** @type {import('jest').Config} */
module.exports = {
  ...require('../../jest.preset.cjs'),
};
`;
}

function generateTsConfig(): string {
  return `{
  "extends": "../../tsconfig.base.json",
  "compilerOptions": {
    "outDir": "./dist",
    "rootDir": "./src",
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist", "__tests__"]
}
`;
}

interface FeatureScanResult {
  name: string;
  path: string;
}

/** A check that fired, carrying the CHECKS key it came from. */
type CheckProblem = CheckItem & { key: string };

interface CheckResult {
  issues: CheckProblem[];
  warnings: CheckProblem[];
  suggestions: CheckProblem[];
}

function checkFeaturePackage(featurePkg: FeatureScanResult): CheckResult {
  const issues: CheckProblem[] = [];
  const warnings: CheckProblem[] = [];
  const suggestions: CheckProblem[] = [];

  for (const [key, check] of Object.entries(CHECKS)) {
    let hasProblem = false;

    if (check.file) {
      const filePath = path.join(featurePkg.path, check.file);
      hasProblem = !fs.existsSync(filePath);
    } else if (check.check) {
      hasProblem = !check.check(featurePkg.path);
    }

    if (hasProblem) {
      const item: CheckProblem = {
        ...check,
        key,
      };

      if (check.severity === 'critical' || check.severity === 'high') {
        issues.push(item);
      } else if (check.severity === 'medium') {
        warnings.push(item);
      } else {
        suggestions.push(item);
      }
    }
  }

  return { issues, warnings, suggestions };
}

function generateReport(results: Array<{ feature: FeatureScanResult; result: CheckResult }>): {
  totalIssues: number;
  totalWarnings: number;
  totalSuggestions: number;
} {
  log('\n📊 Feature package check report\n', 'bright');
  log('='.repeat(60), 'gray');

  let totalIssues = 0;
  let totalWarnings = 0;
  let totalSuggestions = 0;

  for (const { feature, result } of results) {
    const { issues, warnings, suggestions } = result;
    const hasProblems = issues.length > 0 || warnings.length > 0 || suggestions.length > 0;

    if (!hasProblems) {
      log(`\n✅ ${feature.name}`, 'green');
      log('   No update needed, all checks passed', 'gray');
      continue;
    }

    log(
      `\n${issues.length > 0 ? '❌' : warnings.length > 0 ? '⚠️' : '💡'} ${feature.name}`,
      issues.length > 0 ? 'red' : warnings.length > 0 ? 'yellow' : 'blue'
    );
    log(`   Path: ${path.relative(process.cwd(), feature.path)}`, 'gray');

    if (issues.length > 0) {
      log('\n   🚨 Critical issues:', 'red');
      issues.forEach(issue => {
        log(`      • ${issue.name}: ${issue.description}`, 'reset');
      });
      totalIssues += issues.length;
    }

    if (warnings.length > 0) {
      log('\n   ⚠️  Warnings:', 'yellow');
      warnings.forEach(warning => {
        log(`      • ${warning.name}: ${warning.description}`, 'reset');
      });
      totalWarnings += warnings.length;
    }

    if (suggestions.length > 0) {
      log('\n   💡 Suggestions:', 'blue');
      suggestions.forEach(suggestion => {
        log(`      • ${suggestion.name}: ${suggestion.description}`, 'reset');
      });
      totalSuggestions += suggestions.length;
    }
  }

  log('\n' + '='.repeat(60), 'gray');
  log('\n📈 Summary:', 'bright');
  log(`   Total packages: ${results.length}`, 'reset');
  log(`   Critical issues: ${totalIssues}`, totalIssues > 0 ? 'red' : 'green');
  log(`   Warnings: ${totalWarnings}`, totalWarnings > 0 ? 'yellow' : 'green');
  log(`   Suggestions: ${totalSuggestions}`, totalSuggestions > 0 ? 'blue' : 'green');

  return { totalIssues, totalWarnings, totalSuggestions };
}

async function autoFix(
  results: Array<{ feature: FeatureScanResult; result: CheckResult }>,
  options: { dryRun?: boolean } = {}
): Promise<number> {
  const { dryRun = false } = options;

  log('\n🔧 Starting auto-fix...\n', 'bright');

  let totalFixed = 0;
  let totalSkipped = 0;

  for (const { feature, result } of results) {
    const { issues, warnings } = result;
    const allProblems = [...issues, ...warnings];

    if (allProblems.length === 0) continue;

    log(`\n📦 ${feature.name}`, 'blue');

    let featureFixed = 0;
    let featureSkipped = 0;

    for (const problem of allProblems) {
      const filePath = CHECKS[problem.key as keyof typeof CHECKS]?.file;

      if (!filePath) {
        log(`   ⏭  Skipped: ${problem.name} (requires manual handling)`, 'gray');
        featureSkipped++;
        continue;
      }

      const fullPath = path.join(feature.path, filePath);
      const relativePath = path.relative(process.cwd(), fullPath);

      if (dryRun) {
        log(`   🔍 [DRY-RUN] Would create: ${relativePath}`, 'yellow');
        log(`      Issue: ${problem.name}`, 'gray');
        continue;
      }

      try {
        let content = '';
        let action = '';

        if (problem.key === 'jestConfig') {
          content = generateJestConfig();
          action = 'Created Jest config';
        } else if (problem.key === 'tsConfig') {
          content = generateTsConfig();
          action = 'Created TypeScript config';
        } else if (problem.key === 'srcIndex') {
          const featurePath = path.join(feature.path, 'src/feature.ts');
          if (fs.existsSync(featurePath)) {
            const featureContent = fs.readFileSync(featurePath, 'utf-8');
            const exportMatch = featureContent.match(/export\s+const\s+(\w+Feature)/);
            const featureName = exportMatch ? exportMatch[1] : 'feature';

            content = `/**
 * ${feature.name
   .split('-')
   .map(w => w.charAt(0).toUpperCase() + w.slice(1))
   .join(' ')} Feature
 *
 * @packageDocumentation
 */

export { ${featureName} } from './feature.js';
`;
            action = 'Created export entry point';
          } else {
            content = `export { feature } from './feature.js';\n`;
            action = 'Created export entry point';
          }
        }

        if (content) {
          const dir = path.dirname(fullPath);
          if (!fs.existsSync(dir)) {
            fs.mkdirSync(dir, { recursive: true });
          }

          fs.writeFileSync(fullPath, content, 'utf-8');
          log(`   ✅ ${action}: ${relativePath}`, 'green');
          featureFixed++;
        }
      } catch (error) {
        log(
          `   ❌ Failed: ${relativePath} (${error instanceof Error ? error.message : String(error)})`,
          'red'
        );
      }
    }

    if (featureFixed > 0) {
      log(`   ─── Fixed ${featureFixed} item(s)`, 'gray');
      totalFixed += featureFixed;
    }
    if (featureSkipped > 0) {
      log(`   ─── Skipped ${featureSkipped} item(s) (requires manual handling)`, 'yellow');
      totalSkipped += featureSkipped;
    }
  }

  log('\n' + '='.repeat(60), 'gray');
  if (totalFixed > 0) {
    log(`\n✅ Fix complete! Fixed ${totalFixed} item(s)`, 'green');
  }
  if (totalSkipped > 0) {
    log(`⚠️  Skipped ${totalSkipped} item(s) (requires manual handling)`, 'yellow');
  }
  if (totalFixed === 0 && totalSkipped === 0) {
    log('\n✅ No fixes needed', 'green');
  }

  return totalFixed;
}

async function main(): Promise<void> {
  log('\n🔍 Supramark Feature Incremental Update Tool\n', 'bright');

  try {
    const args = process.argv.slice(2);
    const dryRunFlag = args.includes('--dry-run');

    if (args.includes('--help') || args.includes('-h')) {
      log(`
${colors.bright}Usage:${colors.reset}
  bun run feature:update              # Interactive selection, auto-checks and fixes
  bun run feature:update <name>       # Check and fix a specific Feature
  bun run feature:update -- --check-only  # Check only, no auto-fix

${colors.blue}Options:${colors.reset}
  --check-only  Check only, no auto-fix (preview mode)
  --dry-run     Preview the fix without actually applying it
  --help, -h    Show this help message

${colors.blue}Examples:${colors.reset}
  ${colors.gray}# Interactive selection with auto-fix${colors.reset}
  bun run feature:update

  ${colors.gray}# Check and fix a specific Feature${colors.reset}
  bun run feature:update gfm

  ${colors.gray}# Check only, no fixing${colors.reset}
  bun run feature:update -- --check-only
`);
      process.exit(0);
    }

    const argFeature = args.find(arg => !arg.startsWith('--'));
    const checkOnly = args.includes('--check-only');

    let selectedFeature: FeaturePackageInfo | null = null;

    if (!argFeature) {
      selectedFeature = await selectFeature('Select the Feature to update:');
      if (!selectedFeature) {
        log('\nCancelled.\n', 'yellow');
        return;
      }
    } else {
      selectedFeature = findFeaturePackageByShortName(argFeature);
      if (!selectedFeature) {
        log(`\n❌ Feature not found: ${argFeature}\n`, 'red');
        return;
      }
    }

    log(`Scanning Feature: ${selectedFeature.shortName}...`, 'gray');

    const results = [
      {
        feature: { name: selectedFeature.shortName, path: selectedFeature.dir },
        result: checkFeaturePackage({ name: selectedFeature.shortName, path: selectedFeature.dir }),
      },
    ];

    const stats = generateReport(results);
    const needFix = stats.totalIssues > 0 || stats.totalWarnings > 0;

    if (needFix && !checkOnly) {
      log('\n🔧 Auto-fixing missing items...\n', 'bright');
      await autoFix(results, { dryRun: dryRunFlag });

      log('\n📦 Rescanning to confirm...\n', 'gray');
      const recheck = [
        {
          feature: { name: selectedFeature.shortName, path: selectedFeature.dir },
          result: checkFeaturePackage({
            name: selectedFeature.shortName,
            path: selectedFeature.dir,
          }),
        },
      ];
      generateReport(recheck);
    } else if (needFix) {
      log('\n💡 Tip: run bun run feature:update to auto-fix missing items\n', 'blue');
    }

    if (stats.totalIssues === 0 && stats.totalWarnings === 0) {
      log('\n✨ All checks passed, the Feature package meets the latest standard!', 'green');
    }
  } catch (error) {
    log(`\n❌ Error: ${error instanceof Error ? error.message : String(error)}\n`, 'red');
    console.error(error);
    process.exit(1);
  }
}

main();
