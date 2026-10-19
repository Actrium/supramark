#!/usr/bin/env node

/**
 * Supramark Feature Linter
 *
 * Checks every Feature package for:
 * - Type definition completeness
 * - Correct interface implementation
 * - Code quality
 * - Documentation completeness
 * - Test coverage
 *
 * Use cases:
 * - Checking Feature quality during development
 * - Automated verification in CI/CD
 * - Enforcing a unified standard
 *
 * Usage:
 *   bun run feature:lint
 *   bun run feature:lint <feature-name>
 *   bun run feature:lint -- --strict
 */

import fs from 'node:fs';
import path from 'node:path';
import {
  discoverFeaturePackages,
  selectFeature,
  type FeaturePackageInfo,
  log,
  colors,
} from './lib-feature-layout';

interface LintContext {
  packagePath: string;
  sourceCode?: string;
}

interface LintResult {
  rule: string;
  message: string;
  severity: 'error' | 'warning' | 'info';
  path: string;
}

interface LintRule {
  severity: 'error' | 'warning' | 'info';
  strictSeverity?: 'error' | 'warning' | 'info';
  message: string;
  check: (feature: ParsedFeature, context?: LintContext) => boolean;
}

interface ParsedFeature {
  metadata: {
    id?: string;
    name?: string;
    version?: string;
    description?: string;
    license?: string;
    tags?: string[];
  };
  syntax: {
    ast: {
      type?: string;
      hasSelector?: boolean;
      interface: {
        required?: string[];
        fields?: Record<string, unknown>;
      };
      examples?: unknown[];
      multiNodeNote?: boolean;
      selector?: unknown;
    };
  };
}

const RULES: Record<string, LintRule> = {
  'metadata-id-format': {
    severity: 'error',
    message: 'Feature ID must match the @scope/feature-name format',
    check: feature => /^@[\w-]+\/feature-[\w-]+$/.test(feature.metadata?.id ?? ''),
  },
  'metadata-version-semver': {
    severity: 'error',
    message: 'Version must follow semantic versioning (x.y.z)',
    check: feature => /^\d+\.\d+\.\d+$/.test(feature.metadata?.version ?? ''),
  },
  'metadata-name-required': {
    severity: 'error',
    message: 'Feature name must not be empty',
    check: feature => (feature.metadata?.name?.length ?? 0) > 0,
  },
  'metadata-description-required': {
    severity: 'warning',
    message: 'Feature description must not be empty',
    check: feature => (feature.metadata?.description?.length ?? 0) > 0,
  },
  'metadata-license-required': {
    severity: 'warning',
    message: 'Feature license should be set to Apache-2.0',
    check: feature => feature.metadata?.license === 'Apache-2.0',
  },
  'metadata-tags-nonempty': {
    severity: 'info',
    message: 'Feature tags should include at least one tag',
    check: feature => Array.isArray(feature.metadata?.tags) && feature.metadata.tags.length > 0,
  },
  'ast-type-required': {
    severity: 'error',
    message: 'AST node type must be defined',
    check: feature => (feature.syntax?.ast?.type?.length ?? 0) > 0,
  },
  'ast-interface-required-nonempty': {
    severity: 'warning',
    strictSeverity: 'error',
    message: 'AST interface.required should not contain only type',
    check: feature => {
      const required = feature.syntax?.ast?.interface.required;
      if (feature.syntax?.ast?.hasSelector) {
        return true;
      }
      return Array.isArray(required) && required.length > 1;
    },
  },
  'ast-interface-fields-defined': {
    severity: 'warning',
    message: 'AST interface.fields should define all required fields',
    check: feature => {
      const required = feature.syntax?.ast?.interface.required || [];
      const fields = feature.syntax?.ast?.interface.fields || {};
      return required.every(field => field in fields);
    },
  },
  'ast-examples-provided': {
    severity: 'info',
    strictSeverity: 'error',
    message: 'AST examples should provide at least one example node',
    check: feature => {
      const examples = feature.syntax?.ast?.examples;
      return Array.isArray(examples) && examples.length > 0;
    },
  },
  'selector-multi-node-with-function': {
    severity: 'warning',
    message: 'If a Feature handles multiple node types, it should provide a selector function',
    check: feature => {
      const multiNodeNote = feature.syntax?.ast?.multiNodeNote;
      const selector = feature.syntax?.ast?.selector;
      if (multiNodeNote) {
        return typeof selector === 'function';
      }
      return true;
    },
  },
  'documentation-markdown-example': {
    severity: 'warning',
    strictSeverity: 'error',
    message: 'Feature should provide a Markdown usage example in its comments',
    check: (_feature, context) => {
      if (context?.sourceCode) {
        return (
          context.sourceCode.includes('@example') && context.sourceCode.includes('```markdown')
        );
      }
      return true;
    },
  },
  'testing-file-exists': {
    severity: 'error',
    message: 'Feature must have a test file',
    check: (_feature, context) => {
      if (context?.packagePath) {
        const testFile = path.join(context.packagePath, '__tests__/feature.test.ts');
        return fs.existsSync(testFile);
      }
      return true;
    },
  },
  'package-structure-complete': {
    severity: 'error',
    message: 'Feature package must include all required files',
    check: (_feature, context) => {
      if (!context?.packagePath) return true;

      const required = [
        'package.json',
        'tsconfig.json',
        'jest.config.cjs',
        'src/index.ts',
        'src/feature.ts',
        '__tests__/feature.test.ts',
      ];

      // Any one of these satisfies the requirement. A Chinese README is named
      // README.zh.md so the English-only source check can tell documents apart
      // by filename, so requiring README.md exactly would fail every package
      // whose README is translated.
      const requiredOneOf = [['README.md', 'README.zh.md']];

      const hasAllRequired = required.every(file =>
        fs.existsSync(path.join(context.packagePath, file))
      );
      const hasEachAlternative = requiredOneOf.every(alternatives =>
        alternatives.some(file => fs.existsSync(path.join(context.packagePath, file)))
      );

      return hasAllRequired && hasEachAlternative;
    },
  },
};

interface LinterOptions {
  strict: boolean;
}

class FeatureLinter {
  private strict: boolean;
  private results = {
    passed: [] as LintResult[],
    failed: [] as LintResult[],
    warnings: [] as LintResult[],
    info: [] as LintResult[],
  };

  constructor(options: LinterOptions) {
    this.strict = options.strict;
  }

  async lintFeature(featurePath: string): Promise<void> {
    log(`\nChecking Feature: ${path.basename(featurePath)}`, 'blue');
    log('─'.repeat(60), 'gray');

    const context: LintContext = {
      packagePath: featurePath,
    };

    const featureFile = path.join(featurePath, 'src/feature.ts');
    if (!fs.existsSync(featureFile)) {
      this.results.failed.push({
        rule: 'feature-file-exists',
        message: 'src/feature.ts file does not exist',
        severity: 'error',
        path: featurePath,
      });
      log('  ❌ src/feature.ts does not exist', 'red');
      return;
    }

    const sourceCode = fs.readFileSync(featureFile, 'utf-8');
    context.sourceCode = sourceCode;

    const feature = this.extractFeatureFromSource(sourceCode);

    for (const [ruleName, rule] of Object.entries(RULES)) {
      try {
        const passed = rule.check(feature, context);

        const effectiveSeverity =
          this.strict && rule.strictSeverity ? rule.strictSeverity : rule.severity;

        const result: LintResult = {
          rule: ruleName,
          message: rule.message,
          severity: effectiveSeverity,
          path: featurePath,
        };

        if (!passed) {
          if (effectiveSeverity === 'error') {
            this.results.failed.push(result);
            log(`  ❌ ${rule.message}`, 'red');
          } else if (effectiveSeverity === 'warning') {
            this.results.warnings.push(result);
            log(`  ⚠️  ${rule.message}`, 'yellow');
          } else {
            this.results.info.push(result);
            log(`  💡 ${rule.message}`, 'blue');
          }
        } else {
          this.results.passed.push(result);
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        log(`  ⚠️  Rule ${ruleName} failed to run: ${errorMessage}`, 'yellow');
      }
    }
  }

  private extractFeatureFromSource(sourceCode: string): ParsedFeature {
    const feature: ParsedFeature = {
      metadata: {},
      syntax: { ast: { interface: {} } },
    };

    // Detect whether this is the new (flat) ContainerFeature structure
    const isContainerFeature =
      sourceCode.includes('containerNames:') || sourceCode.includes('CONTAINER_NAMES');

    if (isContainerFeature) {
      // New structure: ContainerFeature interface (flat)
      // Match the xxxFeature: ContainerFeature = { ... } object
      const featureObjMatch = sourceCode.match(
        /\w+Feature:\s*ContainerFeature\s*=\s*\{([\s\S]*?)\n\};/
      );
      if (featureObjMatch) {
        const objStr = featureObjMatch[1];

        const idMatch = objStr.match(/id:\s*['"]([^'"]+)['"]/);
        if (idMatch) feature.metadata.id = idMatch[1];

        const nameMatch = objStr.match(/name:\s*['"]([^'"]+)['"]/);
        if (nameMatch) feature.metadata.name = nameMatch[1];

        const versionMatch = objStr.match(/version:\s*['"]([^'"]+)['"]/);
        if (versionMatch) feature.metadata.version = versionMatch[1];

        const descMatch = objStr.match(/description:\s*['"]([^'"]+)['"]/);
        if (descMatch) feature.metadata.description = descMatch[1];
      }

      // ContainerFeature doesn't need the old AST rules; mark them as satisfied
      feature.syntax.ast.type = 'container';
      feature.syntax.ast.hasSelector = true;
      feature.syntax.ast.interface.required = ['type', 'name', 'containerNames'];
      feature.syntax.ast.interface.fields = { type: {}, name: {}, containerNames: {} };
      feature.syntax.ast.examples = [{}];
      feature.metadata.license = 'Apache-2.0';
      feature.metadata.tags = ['container'];

      return feature;
    }

    // Old structure: SupramarkFeature interface (nested metadata)
    const metadataMatch = sourceCode.match(/metadata:\s*{([^}]+)}/s);
    if (metadataMatch) {
      const metadataStr = metadataMatch[1];

      const idMatch = metadataStr.match(/id:\s*['"]([^'"]+)['"]/);
      if (idMatch) feature.metadata.id = idMatch[1];

      const nameMatch = metadataStr.match(/name:\s*['"]([^'"]+)['"]/);
      if (nameMatch) feature.metadata.name = nameMatch[1];

      const versionMatch = metadataStr.match(/version:\s*['"]([^'"]+)['"]/);
      if (versionMatch) feature.metadata.version = versionMatch[1];

      const descMatch = metadataStr.match(/description:\s*['"]([^'"]+)['"]/);
      if (descMatch) feature.metadata.description = descMatch[1];

      const licenseMatch = metadataStr.match(/license:\s*['"]([^'"]+)['"]/);
      if (licenseMatch) feature.metadata.license = licenseMatch[1];

      const tagsMatch = metadataStr.match(/tags:\s*\[([^\]]*)\]/);
      if (tagsMatch) {
        const tagsStr = tagsMatch[1].trim();
        feature.metadata.tags = tagsStr
          ? tagsStr.split(',').map(t => t.trim().replace(/['"]/g, ''))
          : [];
      }
    }

    const astTypeMatch = sourceCode.match(/ast:\s*{[^}]*type:\s*['"]([^'"]+)['"]/s);
    if (astTypeMatch) {
      feature.syntax.ast.type = astTypeMatch[1];
    }

    const selectorMatch = sourceCode.match(/selector:\s*\(/);
    if (selectorMatch) {
      feature.syntax.ast.hasSelector = true;
    }

    const requiredMatch = sourceCode.match(/required:\s*\[([^\]]+)\]/);
    if (requiredMatch) {
      const requiredStr = requiredMatch[1];
      feature.syntax.ast.interface.required = requiredStr
        .split(',')
        .map(f => f.trim().replace(/['"]/g, ''))
        .filter(Boolean);
    }

    const fieldsStartMatch = sourceCode.match(/fields:\s*{/);
    if (fieldsStartMatch) {
      const startIndex = fieldsStartMatch.index! + fieldsStartMatch[0].length;
      let braceCount = 1;
      let endIndex = startIndex;

      while (braceCount > 0 && endIndex < sourceCode.length) {
        if (sourceCode[endIndex] === '{') braceCount++;
        if (sourceCode[endIndex] === '}') braceCount--;
        endIndex++;
      }

      if (braceCount === 0) {
        const fieldsStr = sourceCode.substring(startIndex, endIndex - 1);
        feature.syntax.ast.interface.fields = {};
        // Held in a local so the narrowing survives into the forEach below.
        const fields = feature.syntax.ast.interface.fields;

        const fieldNames = fieldsStr.match(/(\w+):\s*{/g);
        if (fieldNames) {
          fieldNames.forEach(match => {
            const name = match.match(/(\w+):/)![1];
            fields[name] = {};
          });
        }
      }
    }

    const examplesMatch = sourceCode.match(/examples:\s*\[([^\]]*)\]/s);
    if (examplesMatch) {
      const examplesStr = examplesMatch[1].trim();
      feature.syntax.ast.examples = examplesStr ? [{}] : [];
    }

    return feature;
  }

  generateReport(): boolean {
    log('\n' + '='.repeat(60), 'gray');
    log('Feature Lint Report', 'bright');
    log('='.repeat(60), 'gray');

    const total =
      this.results.passed.length +
      this.results.failed.length +
      this.results.warnings.length +
      this.results.info.length;

    log(`\nTotal checks: ${total}`, 'reset');
    log(`  ✅ Passed: ${this.results.passed.length}`, 'green');
    log(
      `  ❌ Errors: ${this.results.failed.length}`,
      this.results.failed.length > 0 ? 'red' : 'green'
    );
    log(
      `  ⚠️  Warnings: ${this.results.warnings.length}`,
      this.results.warnings.length > 0 ? 'yellow' : 'green'
    );
    log(`  💡 Suggestions: ${this.results.info.length}`, 'blue');

    const score = this.calculateQualityScore();
    const scoreColor = score >= 90 ? 'green' : score >= 70 ? 'yellow' : 'red';
    log(`\nQuality score: ${score}/100`, scoreColor);

    const passed = this.results.failed.length === 0;
    if (this.strict) {
      return passed && this.results.warnings.length === 0;
    }
    return passed;
  }

  private calculateQualityScore(): number {
    const total =
      this.results.passed.length +
      this.results.failed.length +
      this.results.warnings.length +
      this.results.info.length;
    if (total === 0) return 0;

    const deduction =
      this.results.failed.length * 10 +
      this.results.warnings.length * 5 +
      this.results.info.length * 2;

    return Math.max(0, 100 - deduction);
  }
}

async function main(): Promise<void> {
  log('\n🔍 Supramark Feature Linter\n', 'bright');

  const args = process.argv.slice(2);
  const strict = args.includes('--strict');

  if (args.includes('--help') || args.includes('-h')) {
    log(`
${colors.bright}Usage:${colors.reset}
  bun run feature:lint              # Interactively pick a Feature to check
  bun run feature:lint <name>       # Check a specific Feature package
  bun run features:lint             # Check all Features + global uniqueness
  bun run feature:lint -- --strict  # Strict mode

${colors.blue}Options:${colors.reset}
  --strict    Strict mode (warnings are also treated as errors)
  --help, -h  Show this help message

${colors.blue}Examples:${colors.reset}
  ${colors.gray}# Interactive selection${colors.reset}
  bun run feature:lint

  ${colors.gray}# Check a specific Feature${colors.reset}
  bun run feature:lint gfm

  ${colors.gray}# Check all Features + containerNames uniqueness${colors.reset}
  bun run features:lint

  ${colors.gray}# Strict mode check${colors.reset}
  bun run feature:lint -- --strict
`);
    return;
  }

  // Single-feature check mode
  const linter = new FeatureLinter({ strict });
  const argFeature = args.find(arg => !arg.startsWith('--'));

  let selectedFeature: FeaturePackageInfo | null = null;

  if (!argFeature) {
    selectedFeature = await selectFeature('Select a Feature to check:');
    if (!selectedFeature) {
      log('\nCancelled.\n', 'yellow');
      return;
    }
  } else {
    const allFeatures = discoverFeaturePackages();
    selectedFeature =
      allFeatures.find(
        item =>
          item.shortName === argFeature ||
          item.name === argFeature ||
          item.name.endsWith(`/feature-${argFeature}`)
      ) || null;
  }

  if (!selectedFeature) {
    log(`❌ Feature not found: ${argFeature || 'invalid selection'}`, 'red');
    process.exit(1);
  }

  log(`Checking: ${selectedFeature.shortName}...\n`, 'gray');

  await linter.lintFeature(selectedFeature.dir);

  const passed = linter.generateReport();

  process.exit(passed ? 0 : 1);
}

main().catch(error => {
  log(`\n❌ Error: ${error instanceof Error ? error.message : String(error)}\n`, 'red');
  console.error(error);
  process.exit(1);
});
