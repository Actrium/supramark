import fs from 'node:fs';
import path from 'node:path';
import { discoverFeaturePackages, type FeaturePackageInfo } from './lib-feature-layout';

const repoRoot = process.cwd();

const targets = {
  web: ['examples/react-web-csr/src/all-features.ts'],
  rn: ['examples/react-native/src/all-features.ts'],
};

function generateBundle(platform: 'web' | 'rn') {
  const allFeatures = discoverFeaturePackages();
  const imports: string[] = [];
  const list: string[] = [];

  // The export name comes from the package's own entry point, never from the
  // package name: `@supramark/feature-card-vison` exports `visonFeature`, so a
  // `<camelShortName>Feature` guess writes an import that does not exist.
  // Narrowing once here keeps the consumers below free of non-null assertions.
  const resolved = allFeatures.filter(
    (f): f is FeaturePackageInfo & { exportName: string } => f.exportName !== undefined
  );
  if (resolved.length !== allFeatures.length) {
    console.error('[feature:bundle] Cannot resolve the feature export for:');
    for (const feature of allFeatures) {
      if (feature.exportName !== undefined) continue;
      console.error(
        `  ${feature.shortName} (${feature.name}) — its src/index.ts must export exactly one value named *Feature.`
      );
    }
    process.exit(1);
  }

  resolved.forEach(f => {
    imports.push(`import { ${f.exportName} } from '${f.name}';`);
    list.push(f.exportName);
  });

  const content = `/* AUTO-GENERATED. DO NOT EDIT. */
/**
 * Supramark All-in-One Bundle (${platform})
 *
 * Includes all officially registered Features in the project.
 */

${imports.join('\n')}

export const allFeatures = [
  ${list.join(',\n  ')}
];

export const defaultFullConfig = {
  features: allFeatures
};
`;

  const platformTargets = targets[platform];
  platformTargets.forEach(relPath => {
    const outPath = path.join(repoRoot, relPath);
    // Ensure the target directory exists
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, content, 'utf8');
    console.log(`[feature:bundle] Generated ${relPath} with ${allFeatures.length} features.`);
  });
}

generateBundle('web');
generateBundle('rn');
