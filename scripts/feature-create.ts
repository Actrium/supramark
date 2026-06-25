#!/usr/bin/env node

/**
 * Supramark Feature 脚手架工具 (v2)
 *
 * 简化版：仅支持两种扩展类型
 * - Container (:::) - 块级容器扩展
 * - Input (%%%) - 输入块扩展
 *
 * 用法：
 *   bun run feature:create
 */

import fs from 'node:fs';
import path from 'node:path';
import {
  getNewFeatureLocation,
  log,
  question,
  selectMenu,
  type SelectOption,
  colors,
  closeRL,
} from './lib-feature-layout';

const REPO_ROOT = path.resolve(__dirname, '..');

function validateContainerName(name: string): boolean {
  return /^[a-z][a-z0-9_-]*$/.test(name);
}

interface ExtensionTypeConfig {
  label: string;
  description: string;
  syntax: string;
  astType: string;
  syntaxFamily: string;
}

const EXTENSION_TYPES: Record<string, ExtensionTypeConfig> = {
  container: {
    label: 'Container (:::)',
    description: '块级容器扩展，如 :::map, :::note, :::html',
    syntax: ':::',
    astType: 'container',
    syntaxFamily: 'container',
  },
  input: {
    label: 'Input (%%%)',
    description: '输入块扩展，如 %%%form, %%%survey（开发中）',
    syntax: '%%%',
    astType: 'input',
    syntaxFamily: 'input',
  },
};

interface CreateConfig {
  id: string;
  name: string;
  containerName: string;
  extensionType: string;
  version: string;
  author: string;
  description: string;
  repositoryDirectory: string;
}

/**
 * 生成合并后的 feature.ts（实现 ContainerFeature 接口）
 *
 * 合并了原来的 feature.ts + extension.ts + syntax.ts
 */
function generateContainerFeatureTemplate(config: CreateConfig): string {
  const { id, name, version, description, containerName, extensionType } = config;
  const camelName = toCamelCase(name);
  const pascalName = capitalize(camelName);
  const extConfig = EXTENSION_TYPES[extensionType]!;

  return `/**
 * ${name} Feature 定义
 *
 * 实现 ContainerFeature 接口，合并了元数据、容器定义和解析器注册。
 *
 * @example
 * \`\`\`markdown
 * :::${containerName} 标题
 * key: value
 * :::
 * \`\`\`
 *
 * @packageDocumentation
 */

import {
  registerContainerHook,
  type ContainerFeature,
  type ContainerHook,
  type ContainerHookContext,
} from '@supramark/core';

// ============================================================================
// 容器名称定义（唯一事实来源）
// ============================================================================

/**
 * ${name} 支持的容器名称
 *
 * 全局唯一，不能与其他 Feature 冲突。
 */
export const ${camelName.toUpperCase()}_CONTAINER_NAMES = ['${containerName}'] as const;

export type ${pascalName}ContainerName = (typeof ${camelName.toUpperCase()}_CONTAINER_NAMES)[number];

// ============================================================================
// 解析逻辑
// ============================================================================

function parse${pascalName}Params(info: string): { title?: string } {
  const parts = (info || '').trim().split(/\\s+/).filter(Boolean);
  const titleParts = parts.length > 1 ? parts.slice(1) : [];
  return {
    title: titleParts.length > 0 ? titleParts.join(' ') : undefined,
  };
}

function create${pascalName}ContainerHook(name: string): ContainerHook {
  return {
    name,
    opaque: true,
    onOpen(ctx: ContainerHookContext) {
      const { token, stack } = ctx;
      const { title } = parse${pascalName}Params(token.info || '');

      const node = {
        type: '${extConfig.astType}' as const,
        name: '${containerName}',
        params: token.info ? String(token.info) : undefined,
        data: {
          title,
          // TODO: 添加更多解析逻辑
        },
        children: [],
      };

      const parent = stack[stack.length - 1];
      parent.children.push(node as any);
      stack.push(node as any);
    },
    onClose(ctx: ContainerHookContext) {
      const top = ctx.stack[ctx.stack.length - 1] as any;
      if (top && top.type === '${extConfig.astType}' && top.name === '${containerName}') {
        ctx.stack.pop();
      }
    },
  };
}

/**
 * 注册 ${name} 解析器
 *
 * 为所有 containerNames 注册解析 hook。
 */
function register${pascalName}Parser(): void {
  for (const name of ${camelName.toUpperCase()}_CONTAINER_NAMES) {
    registerContainerHook(create${pascalName}ContainerHook(name));
  }
}

// ============================================================================
// Feature 定义（实现 ContainerFeature 接口）
// ============================================================================

/**
 * ${name} Feature
 *
 * ${description || `${extConfig.syntax}${containerName} 容器扩展`}
 */
export const ${camelName}Feature: ContainerFeature = {
  // 元数据
  id: '${id}',
  name: '${name}',
  version: '${version}',
  description: '${description || `${extConfig.syntax}${containerName} 容器扩展`}',

  // 容器定义
  containerNames: [...${camelName.toUpperCase()}_CONTAINER_NAMES],

  // 解析器注册
  registerParser: register${pascalName}Parser,

  // 渲染器导出名
  webRendererExport: 'render${pascalName}ContainerWeb',
  rnRendererExport: 'render${pascalName}ContainerRN',
};
`;
}

function generateTestTemplate(config: CreateConfig): string {
  const { name, containerName, extensionType } = config;
  const camelName = toCamelCase(name);
  const extConfig = EXTENSION_TYPES[extensionType]!;

  return `import { ${camelName}Feature } from '../src/feature';
import { validateFeature } from '@supramark/core';

describe('${name} Feature', () => {
  describe('Metadata', () => {
    it('should have valid metadata', () => {
      const result = validateFeature(${camelName}Feature);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('should have correct id', () => {
      expect(${camelName}Feature.metadata.id).toMatch(/^@[\\w-]+\\/feature-[\\w-]+$/);
    });

    it('should have semantic version', () => {
      expect(${camelName}Feature.metadata.version).toMatch(/^\\d+\\.\\d+\\.\\d+$/);
    });

    it('should have syntaxFamily "${extensionType}"', () => {
      expect(${camelName}Feature.metadata.syntaxFamily).toBe('${extensionType}');
    });
  });

  describe('Syntax', () => {
    it('should define AST node type as "${extConfig.astType}"', () => {
      expect(${camelName}Feature.syntax.ast.type).toBe('${extConfig.astType}');
    });

    it('should have selector for name "${containerName}"', () => {
      const selector = ${camelName}Feature.syntax.ast.selector;
      expect(selector).toBeDefined();
      
      // Test selector matches correct node
      const validNode = { type: '${extConfig.astType}', name: '${containerName}', children: [] };
      expect(selector!(validNode as any)).toBe(true);
      
      // Test selector rejects wrong name
      const wrongNode = { type: '${extConfig.astType}', name: 'other', children: [] };
      expect(selector!(wrongNode as any)).toBe(false);
    });
  });
});
`;
}

function generateREADME(config: CreateConfig): string {
  const { name, description, containerName, extensionType } = config;
  const extConfig = EXTENSION_TYPES[extensionType]!;

  return `# ${name}

${description || `${extConfig.syntax}${containerName} ${extensionType} extension for Supramark.`}

## Syntax

\`\`\`markdown
${extConfig.syntax}${containerName}
key: value
another_key: another_value
${extConfig.syntax}
\`\`\`

## AST Node

| Field | Type | Description |
|-------|------|-------------|
| \`type\` | \`'${extConfig.astType}'\` | Node type identifier |
| \`name\` | \`'${containerName}'\` | Extension name |
| \`params\` | \`string?\` | Raw params after \`${extConfig.syntax}${containerName}\` |
| \`data\` | \`object?\` | Parsed structured data |
| \`children\` | \`Node[]\` | Child nodes |

## Platform Support

- [x] Web (React)
- [x] React Native

## Development Status

- [x] Feature definition
- [x] Basic tests
- [ ] Parser implementation
- [ ] Web renderer
- [ ] RN renderer
- [ ] Documentation

## Related

- [Container Extension Guide](../../docs/architecture/PLUGIN_SYSTEM.md)
`;
}

function generatePackageJson(config: CreateConfig): string {
  const { id, name, version, description, repositoryDirectory } = config;
  const kebabName = toKebabCase(name);

  return `{
  "name": "${id}",
  "version": "${version}",
  "description": "${description || name + ' Feature'}",
  "type": "module",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "exports": {
    ".": {
      "types": "./dist/index.d.ts",
      "import": "./dist/index.js",
      "default": "./dist/index.js"
    }
  },
  "files": [
    "dist",
    "src",
    "README.md"
  ],
  "scripts": {
    "build": "tsc -p tsconfig.json",
    "test": "jest",
    "test:watch": "jest --watch",
    "test:coverage": "jest --coverage"
  },
  "keywords": [
    "supramark",
    "feature",
    "${kebabName}",
    "markdown"
  ],
  "author": "${config.author}",
  "license": "Apache-2.0",
  "peerDependencies": {
    "@supramark/core": "workspace:*"
  },
  "devDependencies": {
    "@types/jest": "^29.5.0",
    "jest": "^29.5.0",
    "ts-jest": "^29.1.0",
    "typescript": "^5.5.0"
  },
  "repository": {
    "type": "git",
    "url": "https://github.com/Actrium/supramark.git",
    "directory": "${repositoryDirectory}"
  }
}
`;
}

function generateTsConfig(): string {
  return `{
  "extends": "../../../tsconfig.base.json",
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

function generateExamplesTemplate(config: CreateConfig): string {
  const { name, containerName, extensionType } = config;
  const camelName = toCamelCase(name);
  const extConfig = EXTENSION_TYPES[extensionType]!;

  return `import type { ExampleDefinition } from '@supramark/core';

/**
 * ${name} Feature examples
 */
export const ${camelName}Examples: ExampleDefinition[] = [
  {
    name: 'Basic ${containerName}',
    description: 'A simple ${extConfig.syntax}${containerName} example',
    markdown: \`
${extConfig.syntax}${containerName}
key: value
number: 42
enabled: true
${extConfig.syntax}
\`.trim(),
  },
  {
    name: '${containerName} with params',
    description: '${extConfig.syntax}${containerName} with additional parameters',
    markdown: \`
${extConfig.syntax}${containerName} title="My Title" id=123
content: Hello World
${extConfig.syntax}
\`.trim(),
  },
];
`;
}

function generateContainerRuntimeWebTemplate(config: CreateConfig): string {
  const { name, containerName } = config;
  const pascalName = capitalize(toCamelCase(name));

  return `/**
 * ${name} Web 渲染器
 *
 * 实现 ContainerWebRenderer 接口
 *
 * @packageDocumentation
 */

import React from 'react';
import type { ContainerWebRenderArgs } from '@supramark/core';

/**
 * Web 渲染器 for :::${containerName}
 */
export function render${pascalName}ContainerWeb({
  node,
  key,
  classNames,
  renderChildren,
}: ContainerWebRenderArgs): React.ReactNode {
  const data = node?.data ?? {};
  const title = data.title;

  return (
    <div
      key={key}
      className={\`${containerName}-container \${classNames.paragraph ?? ''}\`.trim()}
    >
      {title ? (
        <p>
          <strong>{title}</strong>
        </p>
      ) : null}
      <div className="${containerName}-content">
        {renderChildren(node.children ?? [])}
      </div>
    </div>
  );
}
`;
}

function generateContainerRuntimeRNTemplate(config: CreateConfig): string {
  const { name, containerName } = config;
  const pascalName = capitalize(toCamelCase(name));

  return `/**
 * ${name} React Native 渲染器
 *
 * 实现 ContainerRNRenderer 接口
 *
 * @packageDocumentation
 */

import React from 'react';
import { View, Text } from 'react-native';
import type { ContainerRNRenderArgs } from '@supramark/core';

/**
 * RN 渲染器 for :::${containerName}
 */
export function render${pascalName}ContainerRN({
  node,
  key,
  styles,
  renderChildren,
}: ContainerRNRenderArgs): React.ReactNode {
  const title = node?.data?.title;

  return (
    <View key={key} style={styles.listItem}>
      {title ? <Text style={[styles.listItemText, { fontWeight: '600' }]}>{title}</Text> : null}
      <Text style={styles.listItemText}>{renderChildren(node.children ?? [])}</Text>
    </View>
  );
}
`;
}

function generateIndexFile(config: CreateConfig): string {
  const { name } = config;
  const camelName = toCamelCase(name);
  const pascalName = capitalize(camelName);

  return `/**
 * ${name} Feature
 *
 * @packageDocumentation
 */

// Feature 定义（主导出）
export {
  ${camelName}Feature,
  ${camelName.toUpperCase()}_CONTAINER_NAMES,
  type ${pascalName}ContainerName,
} from './feature.js';

// 示例
export { ${camelName}Examples } from './examples.js';

// 渲染器（供 registry 使用）
export { render${pascalName}ContainerWeb } from './runtime.web.js';
export { render${pascalName}ContainerRN } from './runtime.rn.js';
`;
}

function generateJestConfig(jestPresetPath: string): string {
  return `/** @type {import('jest').Config} */
module.exports = {
  // 使用 Supramark 共享的 Jest preset
  // 与 @supramark/core 的测试配置保持一致
  ...require('${jestPresetPath}'),

  // Feature 包特定的配置可以在这里覆盖
  // 例如：
  // testEnvironment: 'jsdom', // 如果需要 DOM 环境
  // collectCoverage: true,     // 启用覆盖率收集
};
`;
}

function toCamelCase(str: string): string {
  const normalized = str
    .replace(/([a-z])([A-Z])/g, '$1_$2')
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1_$2');

  return normalized
    .split(/[\s-_]+/)
    .map((word, index) => {
      if (index === 0) {
        return word.toLowerCase();
      }
      return word.charAt(0).toUpperCase() + word.slice(1).toLowerCase();
    })
    .join('');
}

function toKebabCase(str: string): string {
  return str
    .replace(/\s+/g, '-')
    .replace(/([a-z])([A-Z])/g, '$1-$2')
    .toLowerCase();
}

function capitalize(str: string): string {
  if (!str) return '';
  return str.charAt(0).toUpperCase() + str.slice(1);
}

interface CliOptions {
  name: string | null;
  containerName: string | null;
  extensionType: string | null;
  version: string;
  author: string;
  description: string;
  dryRun: boolean;
  outputDir: string | null;
}

function parseArgs(): CliOptions {
  const args = process.argv.slice(2);
  const options: CliOptions = {
    name: null,
    containerName: null,
    extensionType: null,
    version: '0.1.0',
    author: 'Supramark Team',
    description: '',
    dryRun: false,
    outputDir: null,
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    const nextArg = args[i + 1];

    if ((arg === '--name' || arg === '-n') && nextArg) {
      options.name = nextArg;
      i++;
    } else if ((arg === '--container' || arg === '-c') && nextArg) {
      options.containerName = nextArg;
      i++;
    } else if ((arg === '--type' || arg === '-t') && nextArg) {
      options.extensionType = nextArg;
      i++;
    } else if ((arg === '--version' || arg === '-v') && nextArg) {
      options.version = nextArg;
      i++;
    } else if ((arg === '--author' || arg === '-a') && nextArg) {
      options.author = nextArg;
      i++;
    } else if ((arg === '--description' || arg === '-d') && nextArg) {
      options.description = nextArg;
      i++;
    } else if (arg === '--dry-run') {
      options.dryRun = true;
    } else if ((arg === '--output-dir' || arg === '-o') && nextArg) {
      options.outputDir = nextArg;
      i++;
    } else if (arg === '--help' || arg === '-h') {
      showHelp();
      closeRL();
      process.exit(0);
    }
  }

  return options;
}

function showHelp(): void {
  console.log(`
${colors.bright}Supramark Feature 脚手架工具 v2${colors.reset}

${colors.blue}用法：${colors.reset}
  bun run feature:create [选项]

${colors.blue}选项：${colors.reset}
  -n, --name <name>          Feature 名称 (如 "Weather")
  -c, --container <name>     容器/输入块名称 (如 "weather", 用于 :::weather)
  -t, --type <type>          扩展类型: container | input (默认: container)
  -v, --version <version>    版本号 (默认: "0.1.0")
  -a, --author <author>      作者 (默认: "Supramark Team")
  -d, --description <desc>   简短描述
  --dry-run                  仅打印将生成的文件列表，不写入磁盘
  -o, --output-dir <dir>     输出目录（覆盖默认位置）
  -h, --help                 显示此帮助信息

${colors.blue}扩展类型：${colors.reset}
  ${colors.green}container${colors.reset}  块级容器 (:::)  如 :::map, :::note, :::weather
  ${colors.yellow}input${colors.reset}      输入块 (%%%)    如 %%%form, %%%survey (开发中)

${colors.blue}示例：${colors.reset}
  ${colors.gray}# 交互式创建${colors.reset}
  bun run feature:create

  ${colors.gray}# 通过参数创建 Container 扩展${colors.reset}
  bun run feature:create -- -n "Weather" -c "weather" -d "天气卡片"

  ${colors.gray}# 创建 Input 扩展${colors.reset}
  bun run feature:create -- -n "Survey" -c "survey" -t input -d "问卷调查"
`);
}

interface FileToWrite {
  path: string;
  content: string;
  desc: string;
}

async function main(): Promise<void> {
  log('\n🚀 Supramark Feature 脚手架工具 v2\n', 'bright');

  try {
    const cliOptions = parseArgs();

    let name: string = cliOptions.name!;
    let containerName: string = cliOptions.containerName!;
    let extensionType: string = cliOptions.extensionType!;
    const version: string = cliOptions.version;
    const author: string = cliOptions.author;
    let description: string = cliOptions.description;
    const dryRun: boolean = cliOptions.dryRun;
    const outputDirOption: string | null = cliOptions.outputDir;

    const isInteractive = !name || !containerName;

    if (isInteractive) {
      if (!extensionType) {
        const options: SelectOption[] = [
          {
            value: 'container',
            label: 'Container (:::)',
            description: '块级容器扩展，如 :::map, :::note, :::weather',
          },
          {
            value: 'input',
            label: 'Input (%%%)',
            description: '输入块扩展，如 %%%form, %%%survey（开发中）',
          },
        ];
        const selected = await selectMenu('选择扩展类型:', options);
        extensionType = selected || options[0].value;
      }

      const extConfig = EXTENSION_TYPES[extensionType]!;
      log(`\n已选择: ${colors.green}${extConfig.label}${colors.reset}\n`, 'reset');

      if (!containerName) {
        containerName = await question(
          `${extensionType === 'container' ? '容器' : '输入块'}名称 (用于 ${extConfig.syntax}xxx，如 "weather"): `
        );
        if (!containerName) {
          throw new Error('名称不能为空');
        }
      }

      if (!validateContainerName(containerName)) {
        throw new Error(
          `名称 "${containerName}" 无效。必须以小写字母开头，只能包含小写字母、数字、下划线和连字符。`
        );
      }

      if (!name) {
        const defaultName = capitalize(containerName);
        const inputName = await question(`Feature 名称 [${defaultName}]: `);
        name = inputName || defaultName;
      }

      if (!description) {
        description = await question('简短描述 (可选): ');
      }

      const id = `@supramark/feature-${toKebabCase(name)}`;
      log('\n📋 确认信息：', 'bright');
      log(`  扩展类型:   ${colors.green}${extConfig.label}${colors.reset}`, 'reset');
      log(
        `  语法:       ${colors.blue}${extConfig.syntax}${containerName}${colors.reset}`,
        'reset'
      );
      log(`  Feature:    ${colors.blue}${name}${colors.reset}`, 'reset');
      log(`  Package ID: ${colors.gray}${id}${colors.reset}`, 'reset');
      if (description) {
        log(`  描述:       ${colors.gray}${description}${colors.reset}`, 'reset');
      }
      log('');

      const confirm = await question('确认创建? (Y/n): ');
      if (confirm.toLowerCase() === 'n') {
        log('\n已取消。\n', 'yellow');
        return;
      }
    } else {
      extensionType = extensionType || 'container';
      if (!EXTENSION_TYPES[extensionType]) {
        throw new Error(`无效的扩展类型: ${extensionType}。可选: container, input`);
      }
      if (!validateContainerName(containerName)) {
        throw new Error(
          `名称 "${containerName}" 无效。必须以小写字母开头，只能包含小写字母、数字、下划线和连字符。`
        );
      }
    }

    const id = `@supramark/feature-${toKebabCase(name)}`;
    const featureName = toKebabCase(name);
    const defaultLocation = getNewFeatureLocation(featureName, extensionType);
    const basePath = outputDirOption
      ? path.resolve(process.cwd(), outputDirOption)
      : defaultLocation.dir;
    const relativeDir = outputDirOption
      ? path.relative(REPO_ROOT, basePath).replace(/\\/g, '/') || '.'
      : defaultLocation.relativeDir;

    if (!dryRun && fs.existsSync(basePath)) {
      throw new Error(
        `Feature 目录已存在: ${path.relative(process.cwd(), basePath)}\n请选择其他名称或删除现有目录`
      );
    }

    log(`\n📁 创建目录结构${dryRun ? ' (dry-run)' : ''}...\n`, 'gray');
    const dirs = [basePath, path.join(basePath, 'src'), path.join(basePath, '__tests__')];

    if (!dryRun) {
      dirs.forEach(dir => {
        fs.mkdirSync(dir, { recursive: true });
        log(`  ✓ ${path.relative(process.cwd(), dir)}`, 'green');
      });
    } else {
      dirs.forEach(dir => {
        log(`  • ${path.relative(process.cwd(), dir)}`, 'gray');
      });
    }

    const config: CreateConfig = {
      id,
      name,
      containerName,
      extensionType,
      version,
      author,
      description,
      repositoryDirectory: relativeDir,
    };

    const jestPresetPath = path
      .relative(basePath, path.join(REPO_ROOT, 'jest.preset.cjs'))
      .replace(/\\/g, '/');

    const files: FileToWrite[] = [
      {
        path: path.join(basePath, 'package.json'),
        content: generatePackageJson(config),
        desc: 'package.json',
      },
      {
        path: path.join(basePath, 'tsconfig.json'),
        content: generateTsConfig(),
        desc: 'tsconfig.json',
      },
      {
        path: path.join(basePath, 'jest.config.cjs'),
        content: generateJestConfig(
          jestPresetPath.startsWith('.') ? jestPresetPath : `./${jestPresetPath}`
        ),
        desc: 'jest.config.cjs',
      },
      {
        path: path.join(basePath, 'src', 'index.ts'),
        content: generateIndexFile(config),
        desc: 'src/index.ts',
      },
      {
        path: path.join(basePath, 'src', 'feature.ts'),
        content: generateContainerFeatureTemplate(config),
        desc: 'src/feature.ts',
      },
      {
        path: path.join(basePath, 'src', 'examples.ts'),
        content: generateExamplesTemplate(config),
        desc: 'src/examples.ts',
      },
      {
        path: path.join(basePath, 'src', 'runtime.web.tsx'),
        content: generateContainerRuntimeWebTemplate(config),
        desc: 'src/runtime.web.tsx',
      },
      {
        path: path.join(basePath, 'src', 'runtime.rn.tsx'),
        content: generateContainerRuntimeRNTemplate(config),
        desc: 'src/runtime.rn.tsx',
      },
      {
        path: path.join(basePath, '__tests__', 'feature.test.ts'),
        content: generateTestTemplate(config),
        desc: '__tests__/feature.test.ts',
      },
      {
        path: path.join(basePath, 'README.md'),
        content: generateREADME(config),
        desc: 'README.md',
      },
    ];

    log(`\n📝 生成文件${dryRun ? ' (dry-run)' : ''}...\n`, 'gray');
    files.forEach(file => {
      if (!dryRun) {
        fs.writeFileSync(file.path, file.content, 'utf-8');
        log(`  ✓ ${file.desc}`, 'green');
      } else {
        log(`  • ${file.desc}`, 'gray');
      }
    });

    if (dryRun) {
      log('\n(dry-run) 未写入任何文件。\n', 'yellow');
      return;
    }

    log('\n📝 提示：', 'yellow');
    log('  如需将新 Feature 集成到项目中，请运行：', 'reset');
    log(`  ${colors.green}bun run features:sync${colors.reset}`, 'reset');

    const extConfig = EXTENSION_TYPES[extensionType]!;
    log('\n✨ Feature 创建完成！\n', 'bright');
    log('📦 生成的包：', 'yellow');
    log(`  ${colors.blue}${id}${colors.reset}`, 'reset');
    log(`  位置: ${colors.gray}${relativeDir}${colors.reset}`, 'reset');
    log(`  语法: ${colors.green}${extConfig.syntax}${containerName}${colors.reset}\n`, 'reset');

    log('📝 下一步：', 'yellow');
    log(`  1. 编辑 ${colors.blue}src/feature.ts${colors.reset} 完善解析逻辑`, 'reset');
    log(`  2. 编辑 ${colors.blue}src/runtime.web.tsx${colors.reset} 实现 Web 渲染`, 'reset');
    log(`  3. 编辑 ${colors.blue}src/runtime.rn.tsx${colors.reset} 实现 RN 渲染`, 'reset');
    log(`  4. 运行 ${colors.green}bun run build${colors.reset} 编译`, 'reset');
    log(`  5. 运行 ${colors.green}bun run feature:lint${colors.reset} 检查\n`, 'reset');
  } catch (error) {
    log(`\n❌ 错误: ${error instanceof Error ? error.message : String(error)}\n`, 'red');
  } finally {
    closeRL();
  }
}

main();
