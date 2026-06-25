#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { type discoverFeaturePackages, findFeaturePackageByShortName } from './lib-feature-layout';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const projectRoot = path.join(__dirname, '..');

const FEATURES = [
  'core-markdown',
  'gfm',
  'math',
  'admonition',
  'definition-list',
  'emoji',
  'footnote',
];

const docsDir = path.join(projectRoot, 'docs/features');
fs.mkdirSync(docsDir, { recursive: true });

console.log('🚀 开始生成 Feature 文档...\n');

function generateIndexPage(): string {
  let doc = `# Features\n\n`;
  doc += `Supramark 提供了丰富的 Feature 扩展，支持 Markdown 语法增强。\n\n`;
  doc += `## 可用 Features\n\n`;

  for (const shortName of FEATURES) {
    doc += `- [@supramark/feature-${shortName}](./${shortName})\n`;
  }

  doc += `\n## 使用方法\n\n`;
  doc += `\`\`\`typescript\n`;
  doc += `import { Supramark } from '@supramark/core';\n`;
  doc += `import { someFeature } from '@supramark/feature-xxx';\n`;
  doc += `\`\`\`\n`;

  return doc;
}

function generateFeatureDoc(
  pkg: ReturnType<typeof discoverFeaturePackages extends (infer T)[] ? T : never>,
  featureName: string,
  featurePath: string
): string {
  const featureContent = fs.readFileSync(featurePath, 'utf-8');

  let doc = `# @supramark/${featureName}\n\n`;

  const descMatch = featureContent.match(/description:\s*['"]([^'"]+)['"]/);
  if (descMatch) {
    doc += `${descMatch[1]}\n\n`;
  }

  doc += `## 安装\n\n`;
  doc += `\`\`\`bash\n`;
  doc += `bun add @supramark/core @supramark/${featureName}\n`;
  doc += `\`\`\`\n\n`;

  const syntaxMatch = featureContent.match(
    /syntax:\s*\{[^}]*ast:\s*\{[^}]*type:\s*['"]([^'"]+)['"]/
  );
  if (syntaxMatch) {
    const astType = syntaxMatch[1];
    doc += `## 语法\n\n`;
    doc += `类型: \`${astType}\`\n\n`;
  }

  doc += `\n---\n*此文档由 scripts/doc-gen-features.ts 自动生成*\n`;
  return doc;
}

const indexContent = generateIndexPage();
fs.writeFileSync(path.join(docsDir, 'index.md'), indexContent);
console.log('✅ 生成 features/index.md');

for (const shortName of FEATURES) {
  const pkg = findFeaturePackageByShortName(shortName);

  if (!pkg) {
    console.error(`❌ 生成失败: 未找到 Feature 包 ${shortName}`);
    continue;
  }

  const featureName = `feature-${shortName}`;
  const featurePath = path.join(pkg.dir, 'src/feature.ts');

  if (!fs.existsSync(featurePath)) {
    console.warn(`  ⚠️  ${shortName}: feature.ts 不存在`);
    continue;
  }

  const docContent = generateFeatureDoc(pkg, featureName, featurePath);
  const outputPath = path.join(docsDir, `${shortName}.md`);
  fs.writeFileSync(outputPath, docContent);
  console.log(`  ✅ 生成 features/${shortName}.md`);
}

console.log('\n✅ Feature 文档生成完成！');
