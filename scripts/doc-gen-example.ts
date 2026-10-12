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
    title: 'React Web CSR Example',
    path: 'examples/react-web-csr',
    description: 'Browser-based live Markdown editor example built with Vite + React.',
  },
  {
    name: 'react-native',
    title: 'React Native Example',
    path: 'examples/react-native',
    description: 'Markdown and diagram rendering example for the Expo / React Native environment.',
  },
  {
    name: 'config-examples',
    title: 'Build Configuration Examples',
    path: 'examples/config-examples',
    description:
      'Configuration reference for integrating Supramark into build tools such as Vite / Webpack.',
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

console.log('🚀 Generating example docs...\n');

function generateExampleIndex(): string {
  let doc = `# Example Projects\n\n`;
  doc += `Supramark's examples fall into two categories: a Feature example gallery you can browse directly on the docs site, and full host projects that need to run locally.\n\n`;
  doc += `## In-Site Examples\n\n`;
  doc += `### [Live Playground](/playground/)\n\n`;
  doc += `The homepage hosts this same interactive playground: edit Markdown on the left, see the actual rendered output on the right, and switch between Features and examples. Stable deep links such as [Mermaid](/playground/mermaid/) and [D2](/playground/d2/) open a Feature directly.\n\n`;
  doc += `To open it locally for debugging, run:\n\n`;
  doc += codeFence('bash', FEATURE_PREVIEW_COMMANDS.join('\n'));
  doc += `\n\n`;

  doc += `### [Feature Example Gallery](./gallery)\n\n`;
  doc += `Automatically aggregated from each Feature package's \`src/examples.ts\`, showing Markdown input that exercises the currently built-in syntax, container, and diagram capabilities.\n\n`;
  doc += `## Runnable Projects\n\n`;

  for (const example of EXAMPLES) {
    doc += `### [${example.title}](./${example.name})\n\n`;
    doc += `${example.description}\n\n`;
  }

  doc += `## Running the Examples\n\n`;
  doc += `All example projects can be cloned and run directly:\n\n`;
  doc += `\`\`\`bash\n`;
  doc += `git clone https://github.com/Actrium/supramark.git\n`;
  doc += `cd supramark\n`;
  doc += `bun install\n`;
  doc += `cd examples/react-web-csr\n`;
  doc += `bun run dev\n`;
  doc += `\`\`\`\n\n`;

  doc += `## Related Resources\n\n`;
  doc += `- [Getting Started](/guide/getting-started.zh)\n`;
  doc += `- [API Reference](/api/)\n`;
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
    doc += `A complete ${example.title}, demonstrating how Supramark is used in practice.\n\n`;
  }

  doc += `## Getting Started\n\n`;
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
    doc += `## Live Playground\n\n`;
    doc += `This is the unified playground hosted on this site's homepage. Run the command below to interactively pick a Feature; passing a Feature name opens its stable route directly, and you can still switch to other diagrams or examples via the dropdown menu in the browser.\n\n`;
    doc += codeFence('bash', FEATURE_PREVIEW_COMMANDS.join('\n'));
    doc += `\n\n`;
  }

  const deps = data.packageJson.dependencies as Record<string, string> | undefined;
  if (deps) {
    const supramarkDeps = Object.keys(deps).filter(dep => dep.startsWith('@supramark/'));
    if (supramarkDeps.length > 0) {
      doc += `## Supramark Dependencies\n\n`;
      for (const dep of supramarkDeps) {
        const version = deps[dep];
        doc += `- \`${dep}\` - ${version}\n`;
      }
      doc += `\n`;
    }
  }

  if (data.sourceFiles.length > 0) {
    doc += `## Source Code\n\n`;

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

  doc += `## Project Structure\n\n`;
  doc += `\`\`\`\n`;
  doc += `${example.path}/\n`;
  doc += `├── src/\n`;
  doc += `├── public/\n`;
  doc += `├── package.json\n`;
  doc += `└── README.md\n`;
  doc += `\`\`\`\n\n`;

  doc += `## Related Resources\n\n`;
  doc += `- [Getting Started](/guide/getting-started.zh)\n`;
  doc += `- [API Reference](/api/)\n`;
  doc += `- [Other Examples](/examples/)\n\n`;
  doc += `---\n*This document is auto-generated by scripts/doc-gen-example.ts*\n`;

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
  let doc = `# Feature Example Gallery\n\n`;
  doc += `This page is automatically aggregated from each Feature package's \`src/examples.ts\`, currently covering **${groups.length} Features** and **${totalExamples} examples**.\n\n`;
  doc += `These examples show the raw Markdown input; for the full live playground, open [Mermaid](/playground/mermaid/) or run \`bun run feature:preview:web\`.\n\n`;

  doc += `## Table of Contents\n\n`;
  for (const group of groups) {
    doc += `- [${group.title}](#${slugify(group.title)}) (${group.examples.length})\n`;
  }
  doc += `\n`;

  for (const group of groups) {
    doc += `## ${group.title}\n\n`;
    doc += `Package: \`${group.packageName}\`  \n`;
    doc += `Path: \`${group.path}\`\n\n`;

    for (const example of group.examples) {
      doc += `### ${example.name}\n\n`;
      if (example.description) {
        doc += `${example.description}\n\n`;
      }
      doc += codeFence('markdown', example.markdown);
      doc += `\n\n`;
    }
  }

  doc += `---\n*This document is auto-generated by scripts/doc-gen-example.ts*\n`;
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
console.log('✅ Generated examples/index.md');

for (const example of EXAMPLES) {
  console.log(`📱 Processing example: ${example.title}`);

  try {
    const exampleData = extractExampleData(example);
    const docContent = generateExampleDoc(exampleData, example);
    const outputPath = path.join(docsDir, `${example.name}.md`);
    fs.writeFileSync(outputPath, docContent);
    console.log(`  ✅ Generated examples/${example.name}.md`);
  } catch (err) {
    console.error(`  ❌ Failed: ${err instanceof Error ? err.message : String(err)}`);
  }
}

const featureGallery = await collectFeatureExamples();
fs.writeFileSync(path.join(docsDir, 'gallery.md'), generateFeatureGallery(featureGallery));
console.log('✅ Generated examples/gallery.md');

console.log('\n✅ Example docs generation complete!');
