#!/usr/bin/env tsx
/**
 * features:lint - checks all Features + global uniqueness
 *
 * Usage: bun run features:lint
 *
 * @packageDocumentation
 */

import * as fs from 'fs';
import * as path from 'path';
import { discoverFeaturePackages, type FeaturePackageInfo } from './lib-feature-layout.ts';

// ============================================================================
// Colored output
// ============================================================================

const colors = {
  reset: '\x1b[0m',
  bright: '\x1b[1m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  gray: '\x1b[90m',
};

function log(msg: string, color: keyof typeof colors = 'reset'): void {
  console.log(`${colors[color]}${msg}${colors.reset}`);
}

// ============================================================================
// ContainerNames extraction and uniqueness check
// ============================================================================

/**
 * Extract containerNames from feature.ts source code
 */
function extractContainerNames(sourceCode: string): string[] {
  // Prefer XXX_CONTAINER_NAMES = ['a', 'b'] as const (the single source of truth)
  const constPattern = /\w+_CONTAINER_NAMES\s*=\s*\[([^\]]+)\]\s*as\s*const/;
  const constMatch = sourceCode.match(constPattern);
  if (constMatch) {
    const content = constMatch[1];
    const names = content
      .split(',')
      .map(s => s.trim().replace(/['"]/g, ''))
      .filter(s => s.length > 0);
    if (names.length > 0) return names;
  }

  // Fallback: match a direct containerNames: ['a', 'b'] (no spread)
  const directPattern = /containerNames:\s*\[([^\]]+)\]/;
  const directMatch = sourceCode.match(directPattern);
  if (directMatch) {
    const content = directMatch[1];
    // Skip spread syntax (...XXX)
    if (content.includes('...')) return [];
    const names = content
      .split(',')
      .map(s => s.trim().replace(/['"]/g, ''))
      .filter(s => s.length > 0);
    return names;
  }

  return [];
}

/**
 * Check global uniqueness of containerNames across all features
 */
function checkContainerNamesUniqueness(features: FeaturePackageInfo[]): {
  passed: boolean;
  conflicts: Map<string, string[]>;
  featureContainers: Map<string, string[]>;
} {
  const containerNameMap = new Map<string, string[]>();
  const featureContainers = new Map<string, string[]>();

  for (const feature of features) {
    const featureFile = path.join(feature.dir, 'src/feature.ts');
    if (!fs.existsSync(featureFile)) continue;

    const sourceCode = fs.readFileSync(featureFile, 'utf-8');
    const containerNames = extractContainerNames(sourceCode);

    if (containerNames.length > 0) {
      featureContainers.set(feature.shortName, containerNames);
    }

    for (const name of containerNames) {
      const existing = containerNameMap.get(name) || [];
      existing.push(feature.shortName);
      containerNameMap.set(name, existing);
    }
  }

  const conflicts = new Map<string, string[]>();
  for (const [name, featureList] of containerNameMap) {
    if (featureList.length > 1) {
      conflicts.set(name, featureList);
    }
  }

  return {
    passed: conflicts.size === 0,
    conflicts,
    featureContainers,
  };
}

// ============================================================================
// Single-feature lint (simplified: checks only the key items)
// ============================================================================

interface LintResult {
  name: string;
  kind: FeatureKind;
  passed: boolean;
  errors: string[];
  warnings: string[];
}

function lintFeature(feature: FeaturePackageInfo): LintResult {
  const kind = detectFeatureKind(feature);
  const result: LintResult = {
    name: feature.shortName,
    kind,
    passed: true,
    errors: [],
    warnings: [],
  };

  const featureFile = path.join(feature.dir, 'src/feature.ts');

  // ========== Common checks (all kinds) ==========

  // The bundle generator (scripts/features-gen-bundle.ts) fails loudly when it
  // cannot resolve the feature object from the entry point; surface the same
  // problem at lint time instead of at `features:sync` time.
  if (!feature.exportName) {
    result.errors.push('src/index.ts must export exactly one value named *Feature');
    result.passed = false;
  }

  // Check that feature.ts exists
  if (!fs.existsSync(featureFile)) {
    result.errors.push('Missing src/feature.ts');
    result.passed = false;
    return result;
  }

  const sourceCode = fs.readFileSync(featureFile, 'utf-8');

  // Check that examples.ts exists
  const examplesFile = path.join(feature.dir, 'src/examples.ts');
  if (!fs.existsSync(examplesFile)) {
    result.warnings.push('Missing src/examples.ts');
  }

  // Check that a README exists. A Chinese README is named README.zh.md so that
  // the English-only source check can tell documents apart by filename, so both
  // names satisfy this.
  const hasReadme = ['README.md', 'README.zh.md'].some(name =>
    fs.existsSync(path.join(feature.dir, name))
  );
  if (!hasReadme) {
    result.warnings.push('Missing README.md or README.zh.md');
  }

  // ========== Container-kind-specific checks ==========
  if (kind === 'container') {
    // Must have registerParser
    if (!sourceCode.includes('registerParser:') && !sourceCode.includes('registerParser()')) {
      result.errors.push('ContainerFeature is missing registerParser');
      result.passed = false;
    }

    // Check required fields
    const requiredFields = ['id:', 'name:', 'version:', 'containerNames:'];
    for (const field of requiredFields) {
      if (!sourceCode.includes(field)) {
        result.errors.push(`Missing required field: ${field.replace(':', '')}`);
        result.passed = false;
      }
    }

    // Container kind must have a renderer
    const webRenderer = path.join(feature.dir, 'src/runtime.web.tsx');
    const rnRenderer = path.join(feature.dir, 'src/runtime.rn.tsx');

    if (!fs.existsSync(webRenderer)) {
      result.errors.push('Container kind must have runtime.web.tsx');
      result.passed = false;
    }
    if (!fs.existsSync(rnRenderer)) {
      result.errors.push('Container kind must have runtime.rn.tsx');
      result.passed = false;
    }
  }

  // ========== Input-kind-specific checks ==========
  if (kind === 'input') {
    // Check required fields
    const requiredFields = ['id:', 'name:', 'version:', 'inputNames:'];
    for (const field of requiredFields) {
      if (!sourceCode.includes(field)) {
        result.errors.push(`Missing required field: ${field.replace(':', '')}`);
        result.passed = false;
      }
    }

    // Input kind must have a renderer
    const webRenderer = path.join(feature.dir, 'src/runtime.web.tsx');
    const rnRenderer = path.join(feature.dir, 'src/runtime.rn.tsx');

    if (!fs.existsSync(webRenderer)) {
      result.errors.push('Input kind must have runtime.web.tsx');
      result.passed = false;
    }
    if (!fs.existsSync(rnRenderer)) {
      result.errors.push('Input kind must have runtime.rn.tsx');
      result.passed = false;
    }
  }

  // ========== Basic-kind checks ==========
  if (kind === 'basic') {
    // Old structure: check for metadata
    if (!sourceCode.includes('metadata:')) {
      result.warnings.push('Consider migrating to the new Feature interface');
    }
  }

  return result;
}

// ============================================================================
// Main
// ============================================================================

/**
 * Feature kind
 */
type FeatureKind = 'container' | 'input' | 'basic';

/**
 * Detect the kind of a feature
 */
function detectFeatureKind(feature: FeaturePackageInfo): FeatureKind {
  const featureFile = path.join(feature.dir, 'src/feature.ts');
  if (!fs.existsSync(featureFile)) return 'basic';

  const sourceCode = fs.readFileSync(featureFile, 'utf-8');

  if (sourceCode.includes('containerNames:') || sourceCode.includes('CONTAINER_NAMES')) {
    return 'container';
  }
  if (sourceCode.includes('inputNames:') || sourceCode.includes('INPUT_NAMES')) {
    return 'input';
  }

  return 'basic';
}

async function main(): Promise<void> {
  log('\n🔍 Supramark Features Lint\n', 'bright');

  const allFeatures = discoverFeaturePackages();

  if (allFeatures.length === 0) {
    log('No Feature packages found\n', 'yellow');
    process.exit(1);
  }

  log(`Found ${allFeatures.length} Feature package(s)\n`, 'gray');

  // 1. Lint each feature
  const results: LintResult[] = [];
  let allPassed = true;

  for (const feature of allFeatures) {
    const result = lintFeature(feature);
    results.push(result);
    if (!result.passed) allPassed = false;
  }

  // Group output by kind
  const kindLabels: Record<FeatureKind, string> = {
    container: 'Container',
    input: 'Input',
    basic: 'Basic',
  };

  for (const kind of ['container', 'input', 'basic'] as FeatureKind[]) {
    const kindResults = results.filter(r => r.kind === kind);
    if (kindResults.length === 0) continue;

    log(`\n[${kindLabels[kind]}] (${kindResults.length})`, 'blue');
    log('─'.repeat(60), 'gray');

    for (const result of kindResults) {
      const status = result.passed ? '✓' : '✗';
      const color = result.passed ? 'green' : 'red';
      log(`  ${status} ${result.name}`, color);

      for (const error of result.errors) {
        log(`      ❌ ${error}`, 'red');
      }
      for (const warning of result.warnings) {
        log(`      ⚠️  ${warning}`, 'yellow');
      }
    }
  }

  // 2. Check containerNames global uniqueness (Container kind only)
  const containerFeatures = allFeatures.filter(f => detectFeatureKind(f) === 'container');

  if (containerFeatures.length > 0) {
    log('\nChecking containerNames global uniqueness...\n', 'blue');

    const { passed: uniquenessPassed, conflicts, featureContainers } =
      checkContainerNamesUniqueness(containerFeatures);

    // Show each feature's registered containerNames
    for (const [featureName, containers] of featureContainers) {
      log(`  ${featureName}: ${containers.join(', ')}`, 'gray');
    }

    log('');

    if (uniquenessPassed) {
      log('  ✓ All containerNames are globally unique\n', 'green');
    } else {
      allPassed = false;
      log('  ❌ containerNames conflicts detected:\n', 'red');
      for (const [name, features] of conflicts) {
        log(`     "${name}" is used by multiple Features: ${features.join(', ')}`, 'red');
      }
      log('');
    }
  }

  // 3. Summary
  log('═'.repeat(60), 'blue');
  const passedCount = results.filter(r => r.passed).length;
  log(`Result: ${passedCount}/${results.length} Features passed`, allPassed ? 'green' : 'yellow');

  if (allPassed) {
    log('✅ All checks passed!', 'green');
  } else {
    log('❌ Some checks failed', 'red');
  }
  log('═'.repeat(60) + '\n', 'blue');

  process.exit(allPassed ? 0 : 1);
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
