#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { pathToFileURL } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const projectRoot = path.join(__dirname, '..');

interface ExampleInfo {
  name: string;
  title: string;
  path: string;
  description: string;
}

const EXAMPLES: ExampleInfo[] = [
  {
    name: 'react-web-csr',
    title: 'React Web CSR 示例',
    path: 'examples/react-web-csr',
    description: 'Vite + React 的浏览器端实时 Markdown 编辑器示例。',
  },
  {
    name: 'react-native',
    title: 'React Native 示例',
    path: 'examples/react-native',
    description: 'Expo / React Native 环境下的 Markdown 与图表渲染示例。',
  },
  {
    name: 'config-examples',
    title: '构建配置示例',
    path: 'examples/config-examples',
    description: '在 Vite / Webpack 等构建工具中集成 Supramark 的配置参考。',
  },
];

const FEATURE_PREVIEW_COMMANDS = [
  'bun run feature:preview:web',
  'bun run feature:preview:web mermaid',
  'bun run feature:preview:web d2',
  'bun run feature:preview:web plantuml',
  'bun run feature:preview:web diagram-dot',
  'bun run feature:preview:web diagram-echarts',
  'bun run feature:preview:web diagram-vega-lite',
];

const docsDir = path.join(projectRoot, 'docs/examples');
fs.mkdirSync(docsDir, { recursive: true });

console.log('🚀 开始生成示例文档...\n');

function generateExampleIndex(): string {
  let doc = `# 示例项目\n\n`;
  doc += `Supramark 的 examples 分成两类：一类是可以直接在文档站浏览的 Feature 示例库，另一类是需要在本地运行的完整宿主项目。\n\n`;
  doc += `## 站内示例\n\n`;
  doc += `### [实时 Feature Preview](/preview/?feature=mermaid)\n\n`;
  doc += `首页挂载的是同一套可交互预览页面：左侧编辑 Markdown，右侧查看实际渲染效果，页面内可以继续切换 Feature 和示例。\n\n`;
  doc += `本地调试时可以用命令直接打开：\n\n`;
  doc += codeFence('bash', FEATURE_PREVIEW_COMMANDS.join('\n'));
  doc += `\n\n`;

  doc += `### [Feature 示例库](./gallery)\n\n`;
  doc += `从各个 Feature 包的 \`src/examples.ts\` 自动聚合，展示当前内置语法、容器和图表能力的 Markdown 输入。\n\n`;
  doc += `## 可运行项目\n\n`;

  for (const example of EXAMPLES) {
    doc += `### [${example.title}](./${example.name})\n\n`;
    doc += `${example.description}\n\n`;
  }

  doc += `## 运行示例\n\n`;
  doc += `所有示例项目都可以直接克隆并运行：\n\n`;
  doc += `\`\`\`bash\n`;
  doc += `git clone https://github.com/kookyleo/supramark.git\n`;
  doc += `cd supramark\n`;
  doc += `bun install\n`;
  doc += `cd examples/react-web-csr\n`;
  doc += `bun run dev\n`;
  doc += `\`\`\`\n\n`;

  doc += `## 相关资源\n\n`;
  doc += `- [快速开始](/guide/getting-started)\n`;
  doc += `- [API 参考](/api/)\n`;
  doc += `- [Features](/features/)\n`;

  return doc;
}

interface ExampleData {
  packageJson: Record<string, unknown>;
  readme: string;
  sourceFiles: Array<{ name: string; path: string; content: string }>;
}

function extractExampleData(example: ExampleInfo): ExampleData {
  const examplePath = path.join(projectRoot, example.path);

  let packageJson: Record<string, unknown> = {};
  try {
    const pkgPath = path.join(examplePath, 'package.json');
    packageJson = JSON.parse(fs.readFileSync(pkgPath, 'utf-8'));
  } catch {
    // ignore
  }

  let readme = '';
  try {
    const readmePath = path.join(examplePath, 'README.md');
    readme = fs.readFileSync(readmePath, 'utf-8');
  } catch {
    // README may not exist
  }

  const sourceFiles: Array<{ name: string; path: string; content: string }> = [];
  const srcDir = path.join(examplePath, 'src');

  try {
    const entries = fs.readdirSync(srcDir, { withFileTypes: true });
    for (const entry of entries) {
      if (entry.isFile()) {
        const ext = path.extname(entry.name);
        if (['.ts', '.tsx', '.js', '.jsx'].includes(ext)) {
          const filePath = path.join(srcDir, entry.name);
          const content = fs.readFileSync(filePath, 'utf-8');
          sourceFiles.push({ name: entry.name, path: filePath, content });
        }
      }
    }
  } catch {
    // src dir may not exist
  }

  return { packageJson, readme, sourceFiles };
}

function generateExampleDoc(data: ExampleData, example: ExampleInfo): string {
  let doc = `# ${example.title}\n\n`;

  if (data.readme) {
    const readmeLines = data.readme.split('\n');
    const contentStart = readmeLines.findIndex(line => line.trim() && !line.startsWith('#'));
    if (contentStart > 0) {
      doc += readmeLines.slice(contentStart).join('\n') + '\n\n';
    }
  } else {
    doc += `完整的 ${example.title}，展示 Supramark 的实际使用方法。\n\n`;
  }

  doc += `## 快速开始\n\n`;
  doc += `\`\`\`bash\n`;
  doc += `cd ${example.path}\n`;
  doc += `bun install\n`;
  if (data.packageJson.scripts) {
    const scripts = data.packageJson.scripts as Record<string, string>;
    if (scripts.dev || scripts.start) {
      doc += `bun run ${scripts.dev ? 'dev' : 'start'}\n`;
    }
  }
  doc += `\`\`\`\n\n`;

  if (example.name === 'react-web-csr') {
    doc += `## 实时 Feature Preview\n\n`;
    doc += `这是当前站点首页挂载的效果预览页面。直接运行下面的命令可以交互式选择 Feature；传入 Feature 名称时会打开指定类型，浏览器里仍然可以通过下拉菜单切换其它图表或示例。\n\n`;
    doc += codeFence('bash', FEATURE_PREVIEW_COMMANDS.join('\n'));
    doc += `\n\n`;
  }

  const deps = data.packageJson.dependencies as Record<string, string> | undefined;
  if (deps) {
    const supramarkDeps = Object.keys(deps).filter(dep => dep.startsWith('@supramark/'));
    if (supramarkDeps.length > 0) {
      doc += `## Supramark 依赖\n\n`;
      for (const dep of supramarkDeps) {
        const version = deps[dep];
        doc += `- \`${dep}\` - ${version}\n`;
      }
      doc += `\n`;
    }
  }

  if (data.sourceFiles.length > 0) {
    doc += `## 源代码\n\n`;

    const mainFiles = data.sourceFiles
      .filter(f => ['index', 'App', 'main'].some(name => f.name.includes(name)))
      .slice(0, 2);

    for (const file of mainFiles) {
      doc += `### ${file.name}\n\n`;
      const snippet = extractCodeSnippet(file.content);
      const ext = path.extname(file.name).slice(1);
      doc += `\`\`\`${ext}\n`;
      doc += snippet;
      doc += `\n\`\`\`\n\n`;
    }
  }

  doc += `## 项目结构\n\n`;
  doc += `\`\`\`\n`;
  doc += `${example.path}/\n`;
  doc += `├── src/\n`;
  doc += `├── public/\n`;
  doc += `├── package.json\n`;
  doc += `└── README.md\n`;
  doc += `\`\`\`\n\n`;

  doc += `## 相关资源\n\n`;
  doc += `- [快速开始](/guide/getting-started)\n`;
  doc += `- [API 参考](/api/)\n`;
  doc += `- [其他示例](/examples/)\n\n`;
  doc += `---\n*此文档由 scripts/doc-gen-example.ts 自动生成*\n`;

  return doc;
}

interface FeatureGalleryGroup {
  packageName: string;
  title: string;
  path: string;
  examples: Array<{
    name: string;
    description?: string;
    markdown: string;
  }>;
}

async function collectFeatureExamples(): Promise<FeatureGalleryGroup[]> {
  const files = findFiles(path.join(projectRoot, 'packages/features'), 'src/examples.ts')
    .filter(file => !file.includes(`${path.sep}dist${path.sep}`))
    .sort();

  const groups: FeatureGalleryGroup[] = [];

  for (const file of files) {
    const module = await import(pathToFileURL(file).href);
    const examples = Object.values(module).find(value => Array.isArray(value)) as
      | FeatureGalleryGroup['examples']
      | undefined;

    if (!examples || examples.length === 0) continue;

    const packageRoot = path.dirname(path.dirname(file));
    const packageJsonPath = path.join(packageRoot, 'package.json');
    const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8')) as {
      name?: string;
    };
    const packageName = packageJson.name ?? path.basename(packageRoot);

    groups.push({
      packageName,
      title: titleFromPackageName(packageName),
      path: path.relative(projectRoot, packageRoot),
      examples: examples.filter(example => typeof example.markdown === 'string'),
    });
  }

  return groups.sort((a, b) => a.title.localeCompare(b.title));
}

function findFiles(root: string, suffix: string): string[] {
  const results: string[] = [];
  const entries = fs.readdirSync(root, { withFileTypes: true });

  for (const entry of entries) {
    const fullPath = path.join(root, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === 'node_modules' || entry.name === 'dist') continue;
      results.push(...findFiles(fullPath, suffix));
      continue;
    }
    if (fullPath.endsWith(suffix)) {
      results.push(fullPath);
    }
  }

  return results;
}

function titleFromPackageName(packageName: string): string {
  return packageName
    .replace(/^@supramark\/feature-/, '')
    .split('-')
    .map(part => (part.length <= 3 ? part.toUpperCase() : part[0].toUpperCase() + part.slice(1)))
    .join(' ');
}

function generateFeatureGallery(groups: FeatureGalleryGroup[]): string {
  const totalExamples = groups.reduce((sum, group) => sum + group.examples.length, 0);
  let doc = `# Feature 示例库\n\n`;
  doc += `本页从各个 Feature 包的 \`src/examples.ts\` 自动聚合，当前包含 **${groups.length} 个 Feature**、**${totalExamples} 个示例**。\n\n`;
  doc += `这些示例展示的是 Markdown 输入本身；完整实时预览请打开 [首页预览](/preview/?feature=mermaid)，或运行 \`bun run feature:preview:web\`。\n\n`;

  doc += `## 目录\n\n`;
  for (const group of groups) {
    doc += `- [${group.title}](#${slugify(group.title)}) (${group.examples.length})\n`;
  }
  doc += `\n`;

  for (const group of groups) {
    doc += `## ${group.title}\n\n`;
    doc += `包：\`${group.packageName}\`  \n`;
    doc += `路径：\`${group.path}\`\n\n`;

    for (const example of group.examples) {
      doc += `### ${example.name}\n\n`;
      if (example.description) {
        doc += `${example.description}\n\n`;
      }
      doc += codeFence('markdown', example.markdown);
      doc += `\n\n`;
    }
  }

  doc += `---\n*此文档由 scripts/doc-gen-example.ts 自动生成*\n`;
  return doc;
}

function slugify(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9\u4e00-\u9fa5]+/g, '-')
    .replace(/^-|-$/g, '');
}

function codeFence(language: string, content: string): string {
  const fence = content.includes('```') ? '````' : '```';
  return `${fence}${language}\n${content.trimEnd()}\n${fence}`;
}

function extractCodeSnippet(content: string): string {
  const lines = content.split('\n');
  const codeLines: string[] = [];
  let skipImports = true;

  for (const line of lines) {
    const trimmed = line.trim();
    if (skipImports && (trimmed.startsWith('import ') || trimmed.startsWith('//'))) {
      continue;
    }
    if (trimmed && !trimmed.startsWith('import ')) {
      skipImports = false;
    }
    if (!skipImports && !trimmed.startsWith('//')) {
      codeLines.push(line);
    }
  }

  return codeLines.slice(0, 50).join('\n');
}

fs.writeFileSync(path.join(docsDir, 'index.md'), generateExampleIndex());
console.log('✅ 生成 examples/index.md');

for (const example of EXAMPLES) {
  console.log(`📱 处理示例: ${example.title}`);

  try {
    const exampleData = extractExampleData(example);
    const docContent = generateExampleDoc(exampleData, example);
    const outputPath = path.join(docsDir, `${example.name}.md`);
    fs.writeFileSync(outputPath, docContent);
    console.log(`  ✅ 生成 examples/${example.name}.md`);
  } catch (err) {
    console.error(`  ❌ 失败: ${err instanceof Error ? err.message : String(err)}`);
  }
}

const featureGallery = await collectFeatureExamples();
fs.writeFileSync(path.join(docsDir, 'gallery.md'), generateFeatureGallery(featureGallery));
console.log('✅ 生成 examples/gallery.md');

console.log('\n✅ 示例文档生成完成！');
