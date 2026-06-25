import fs from 'node:fs';
import path from 'node:path';
import { discoverFeaturePackages } from './lib-feature-layout';

const repoRoot = process.cwd();

const targets = {
  web: ['examples/react-web-csr/src/all-features.ts'],
  rn: ['examples/react-native/src/all-features.ts'],
};

function generateBundle(platform: 'web' | 'rn') {
  const allFeatures = discoverFeaturePackages();
  const imports: string[] = [];
  const list: string[] = [];

  allFeatures.forEach((f) => {
    imports.push(
      `import { ${f.exportName || f.shortName.replace(/-./g, x => x[1].toUpperCase()) + 'Feature'} } from '${f.name}';`
    );
    list.push(f.exportName || f.shortName.replace(/-./g, x => x[1].toUpperCase()) + 'Feature');
  });

  const content = `/* AUTO-GENERATED. DO NOT EDIT. */
/**
 * Supramark All-in-One Bundle (${platform})
 * 
 * 包含项目中所有已注册的官方 Features。
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
    // 确保目标目录存在
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, content, 'utf8');
    console.log(`[feature:bundle] Generated ${relPath} with ${allFeatures.length} features.`);
  });
}

generateBundle('web');
generateBundle('rn');
