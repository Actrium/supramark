/**
 * Feature package layout and discovery utilities.
 *
 * Goals:
 * - Centralize the convention for "which directory a feature lives in" in one place;
 * - Make a future migration from packages/feature-xxx to family-split directories
 *   a change to this implementation layer only;
 * - Give all scripts a shared feature scanning / lookup capability.
 */

import fs from 'node:fs';
import path from 'node:path';
import * as readline from 'node:readline';

const REPO_ROOT = path.resolve(__dirname, '..');
const PACKAGES_DIR = path.join(REPO_ROOT, 'packages');

export interface FeaturePackageInfo {
  name: string;
  shortName: string;
  dir: string;
  relativeDir: string;
  /**
   * The feature object the package exports from its entry point, e.g.
   * `weatherFeature`. Undefined when that entry exports no such value, or more
   * than one — callers that need it (scripts/features-gen-bundle.ts) must fail
   * loudly rather than guess.
   */
  exportName?: string;
  metadata?: {
    id?: string;
    name?: string;
    version?: string;
    description?: string;
    license?: string;
    tags?: string[];
  };
}

interface PackageJson {
  name?: string;
  version?: string;
  description?: string;
  license?: string;
  [key: string]: unknown;
}

export type Color = 'reset' | 'bright' | 'green' | 'yellow' | 'blue' | 'red' | 'gray' | 'cyan';

const colors: Record<Color, string> = {
  reset: '\x1b[0m',
  bright: '\x1b[1m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[36m',
  red: '\x1b[31m',
  gray: '\x1b[90m',
  cyan: '\x1b[36m',
};

export function log(message: string, color: Color = 'reset'): void {
  console.log(`${colors[color]}${message}${colors.reset}`);
}

let rl: readline.Interface | null = null;

function getRL(): readline.Interface {
  if (!rl) {
    rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
    });
  }
  return rl;
}

export function question(prompt: string): Promise<string> {
  return new Promise(resolve => {
    getRL().question(`${colors.blue}${prompt}${colors.reset}`, (answer: string) => {
      resolve(answer.trim());
    });
  });
}

export function closeRL(): void {
  if (rl) {
    rl.close();
    rl = null;
  }
}

export interface SelectOption {
  value: string;
  label: string;
  description?: string;
}

export async function selectMenu(
  prompt: string,
  optionList: SelectOption[],
  _config?: { descPrefix?: string }
): Promise<string | null> {
  log(`\n${prompt}\n`, 'bright');
  optionList.forEach((opt, idx) => {
    const label = opt.description ? `${opt.label} (${opt.description})` : opt.label;
    log(`  ${idx + 1}. ${label}`, 'reset');
  });
  log('');

  const answer = await question(`Select [1-${optionList.length}]: `);
  const idx = parseInt(answer, 10) - 1;
  if (idx >= 0 && idx < optionList.length) {
    return optionList[idx].value;
  }
  return null;
}

export type FeatureKind = 'container' | 'input' | 'basic';

export function getFeatureKind(feature: FeaturePackageInfo): FeatureKind {
  const syntaxFamily = getFeatureSyntaxFamily(feature);
  if (syntaxFamily === 'container') return 'container';
  if (syntaxFamily === 'input') return 'input';
  return 'basic';
}

export async function selectFeature(
  prompt: string = 'Select feature:',
  filter?: FeatureKind | FeatureKind[]
): Promise<FeaturePackageInfo | null> {
  let features = discoverFeaturePackages();

  // Optional filter
  if (filter) {
    const kinds = Array.isArray(filter) ? filter : [filter];
    features = features.filter(f => kinds.includes(getFeatureKind(f)));
  }

  if (features.length === 0) {
    log('No matching feature packages found\n', 'yellow');
    return null;
  }

  // Group by kind
  const kindLabels: Record<FeatureKind, string> = {
    container: 'Container',
    input: 'Input',
    basic: 'Basic',
  };

  const grouped = new Map<FeatureKind, FeaturePackageInfo[]>();
  for (const f of features) {
    const kind = getFeatureKind(f);
    if (!grouped.has(kind)) grouped.set(kind, []);
    grouped.get(kind)!.push(f);
  }

  let index = 1;
  const indexToFeature = new Map<number, FeaturePackageInfo>();

  log(`\n${prompt}\n`, 'bright');

  for (const kind of ['container', 'input', 'basic'] as FeatureKind[]) {
    const group = grouped.get(kind);
    if (!group || group.length === 0) continue;

    log(`[${kindLabels[kind]}]`, 'blue');
    for (const f of group) {
      const desc = f.metadata?.description ? ` (${f.metadata.description})` : '';
      log(`  ${index}. ${f.shortName}${colors.gray}${desc}${colors.reset}`, 'reset');
      indexToFeature.set(index, f);
      index++;
    }
    log('');
  }

  const answer = await question(`Select [1-${index - 1}]: `);
  const selectedIndex = parseInt(answer, 10);

  if (selectedIndex >= 1 && selectedIndex < index) {
    return indexToFeature.get(selectedIndex) || null;
  }

  return null;
}

function safeReadJson(filePath: string): PackageJson | null {
  try {
    const content = fs.readFileSync(filePath, 'utf-8');
    return JSON.parse(content);
  } catch {
    return null;
  }
}

/**
 * The name of the value a feature package exports from its entry point, e.g.
 * `weatherFeature` from `@supramark/feature-weather`.
 *
 * Read statically rather than by importing the entry: entry points re-export
 * both `./runtime.web.js` and `./runtime.rn.js`, and importing the RN runtime
 * outside React Native hangs (admonition, weather) or throws (video).
 *
 * Returns undefined unless exactly one exported value is named `*Feature`, so a
 * non-conforming or ambiguous package surfaces as a failure, never as a guess.
 */
function readFeatureExportName(featureDir: string): string | undefined {
  const entry = path.join(featureDir, 'src', 'index.ts');
  if (!fs.existsSync(entry)) return undefined;

  try {
    const source = fs.readFileSync(entry, 'utf-8');
    const names = new Set<string>();

    // `export { a, b as c } from './feature.js'`, possibly spread over lines.
    // `export type { ... }` does not match — type-only exports are not values.
    for (const block of source.matchAll(/export\s*\{([^}]*)\}/g)) {
      for (const exported of block[1]!.split(',')) {
        const text = exported.trim();
        if (!text || text.startsWith('type ')) continue;
        const name = text
          .split(/\s+as\s+/)
          .pop()!
          .trim();
        // Skip anything that is not a bare identifier: a comment inside the
        // brace block would otherwise ride along with the name after it.
        if (!/^[A-Za-z_$][\w$]*$/.test(name)) continue;
        names.add(name);
      }
    }

    // `export const xFeature = ...`
    for (const declaration of source.matchAll(
      /export\s+(?:const|let|var|function|class)\s+([A-Za-z_$][\w$]*)/g
    )) {
      names.add(declaration[1]!);
    }

    const featureNames = [...names].filter(name => name.endsWith('Feature'));
    return featureNames.length === 1 ? featureNames[0] : undefined;
  } catch {
    // A caller that needs this name fails loudly on undefined; an unreadable
    // entry point must not take down every consumer of discoverFeaturePackages.
    return undefined;
  }
}

export function discoverFeaturePackages(): FeaturePackageInfo[] {
  const results: FeaturePackageInfo[] = [];

  if (!fs.existsSync(PACKAGES_DIR)) {
    return results;
  }

  function walk(dir: string): void {
    const entries = fs.readdirSync(dir, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);

      if (entry.isDirectory()) {
        if (entry.name === 'node_modules' || entry.name === 'dist') continue;

        const pkgJsonPath = path.join(fullPath, 'package.json');
        const pkg = fs.existsSync(pkgJsonPath) ? safeReadJson(pkgJsonPath) : null;

        if (pkg && typeof pkg.name === 'string' && pkg.name.startsWith('@supramark/feature-')) {
          const shortName = pkg.name.split('/')[1]!.replace(/^feature-/, '');
          const relativeDir = path.relative(REPO_ROOT, fullPath) || '.';

          const metadata: FeaturePackageInfo['metadata'] = {};
          const featureFile = path.join(fullPath, 'src/feature.ts');
          if (fs.existsSync(featureFile)) {
            try {
              const content = fs.readFileSync(featureFile, 'utf-8');
              const descMatch = content.match(/description:\s*['"]([^'"]+)['"]/);
              const nameMatch = content.match(/name:\s*['"]([^'"]+)['"]/);
              const syntaxMatch = content.match(/syntaxFamily:\s*['"]([^'"]+)['"]/);

              if (descMatch) {
                metadata.description = descMatch[1];
              }
              if (nameMatch) {
                metadata.name = nameMatch[1];
              }
              if (syntaxMatch) {
                metadata.tags = [syntaxMatch[1]];
              }
            } catch {
              // ignore
            }
          }

          results.push({
            name: pkg.name,
            shortName,
            dir: fullPath,
            relativeDir: relativeDir.replace(/\\/g, '/'),
            exportName: readFeatureExportName(fullPath),
            metadata,
          });
        } else {
          walk(fullPath);
        }
      }
    }
  }

  walk(PACKAGES_DIR);
  return results;
}

export function getFeatureSyntaxFamily(feature: FeaturePackageInfo): string | null {
  const featureFile = path.join(feature.dir, 'src/feature.ts');
  if (!fs.existsSync(featureFile)) {
    return null;
  }

  try {
    const content = fs.readFileSync(featureFile, 'utf-8');

    // New structure: check the containerNames field (ContainerFeature interface)
    if (content.includes('containerNames:') || content.includes('CONTAINER_NAMES')) {
      return 'container';
    }

    // New structure: check the inputNames field (InputFeature interface, future)
    if (content.includes('inputNames:') || content.includes('INPUT_NAMES')) {
      return 'input';
    }

    // Legacy structure: check the syntaxFamily field
    const match = content.match(/syntaxFamily:\s*['"]([^'"]+)['"]/);
    return match ? match[1] : null;
  } catch {
    return null;
  }
}

export function discoverInteractiveFeatures(): FeaturePackageInfo[] {
  const all = discoverFeaturePackages();
  return all.filter(feature => {
    const syntaxFamily = getFeatureSyntaxFamily(feature);
    return syntaxFamily === 'container' || syntaxFamily === 'input';
  });
}

export function findFeaturePackageByShortName(shortName: string): FeaturePackageInfo | null {
  const all = discoverFeaturePackages();
  return all.find(item => item.shortName === shortName) || null;
}

export function getNewFeatureLocation(
  kebabName: string,
  syntaxFamily: 'main' | 'container' | 'containers' | 'fence' | 'diagrams' = 'main'
): {
  dir: string;
  relativeDir: string;
} {
  // New directory convention: packages/features/<family>/<kebab-name>/
  //   main → main
  //   container / containers → containers
  //   fence / diagrams → diagrams
  const familyFolder =
    syntaxFamily === 'main'
      ? 'main'
      : syntaxFamily === 'fence' || syntaxFamily === 'diagrams'
        ? 'diagrams'
        : 'containers';
  const dir = path.join(PACKAGES_DIR, 'features', familyFolder, kebabName);
  const relativeDir = path.relative(REPO_ROOT, dir) || '.';

  return {
    dir,
    relativeDir: relativeDir.replace(/\\/g, '/'),
  };
}

export { colors };
