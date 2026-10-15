#!/usr/bin/env node

/**
 * Supramark Feature Scaffolding Tool (v2)
 *
 * Simplified version: supports only two extension types
 * - Container (:::) - block-level container extension
 * - Input (%%%) - input block extension
 *
 * Usage:
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

/** The extension families this scaffolder can generate. */
type ExtensionType = 'container' | 'input';

function isExtensionType(value: string): value is ExtensionType {
  // `in` walks the prototype chain, so `--type toString` would pass this guard
  // and later read a config that does not exist. Only own keys are types.
  return Object.hasOwn(EXTENSION_TYPES, value);
}

const EXTENSION_TYPES: Record<ExtensionType, ExtensionTypeConfig> = {
  container: {
    label: 'Container (:::)',
    description: 'Block-level container extension, e.g. :::map, :::note, :::html',
    syntax: ':::',
    astType: 'container',
    syntaxFamily: 'container',
  },
  input: {
    label: 'Input (%%%)',
    description: 'Input block extension, e.g. %%%form, %%%survey (in development)',
    syntax: '%%%',
    astType: 'input',
    syntaxFamily: 'input',
  },
};

interface CreateConfig {
  id: string;
  name: string;
  containerName: string;
  extensionType: ExtensionType;
  version: string;
  author: string;
  description: string;
  repositoryDirectory: string;
}

/**
 * Generates the merged feature.ts (implements the ContainerFeature interface)
 *
 * Merges the former feature.ts + extension.ts + syntax.ts
 */
function generateContainerFeatureTemplate(config: CreateConfig): string {
  const { id, name, version, description, containerName, extensionType } = config;
  const camelName = toCamelCase(name);
  const pascalName = capitalize(camelName);
  const extConfig = EXTENSION_TYPES[extensionType]!;

  return `/**
 * ${name} Feature definition
 *
 * Implements the ContainerFeature interface, merging metadata, container
 * definition, and parser registration.
 *
 * @example
 * \`\`\`markdown
 * :::${containerName} title
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
// Container name definition (single source of truth)
// ============================================================================

/**
 * Container names supported by ${name}
 *
 * Globally unique; must not conflict with other Features.
 */
export const ${camelName.toUpperCase()}_CONTAINER_NAMES = ['${containerName}'] as const;

export type ${pascalName}ContainerName = (typeof ${camelName.toUpperCase()}_CONTAINER_NAMES)[number];

// ============================================================================
// Parsing logic
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
          // TODO: add more parsing logic
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
 * Registers the ${name} parser
 *
 * Registers a parse hook for every containerName.
 */
function register${pascalName}Parser(): void {
  for (const name of ${camelName.toUpperCase()}_CONTAINER_NAMES) {
    registerContainerHook(create${pascalName}ContainerHook(name));
  }
}

// ============================================================================
// Feature definition (implements the ContainerFeature interface)
// ============================================================================

/**
 * ${name} Feature
 *
 * ${description || `${extConfig.syntax}${containerName} container extension`}
 */
export const ${camelName}Feature: ContainerFeature = {
  // Metadata
  id: '${id}',
  name: '${name}',
  version: '${version}',
  description: '${description || `${extConfig.syntax}${containerName} container extension`}',

  // Container definition
  containerNames: [...${camelName.toUpperCase()}_CONTAINER_NAMES],

  // Parser registration
  registerParser: register${pascalName}Parser,

  // Renderer export names
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

- [Container Extension Guide](../../docs/architecture/PLUGIN_SYSTEM.zh.md)
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
 * ${name} Web renderer
 *
 * Implements the ContainerWebRenderer interface
 *
 * @packageDocumentation
 */

import React from 'react';
import type { ContainerWebRenderArgs } from '@supramark/core';

/**
 * Web renderer for :::${containerName}
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
 * ${name} React Native renderer
 *
 * Implements the ContainerRNRenderer interface
 *
 * @packageDocumentation
 */

import React from 'react';
import { View, Text } from 'react-native';
import type { ContainerRNRenderArgs } from '@supramark/core';

/**
 * RN renderer for :::${containerName}
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

// Feature definition (main export)
export {
  ${camelName}Feature,
  ${camelName.toUpperCase()}_CONTAINER_NAMES,
  type ${pascalName}ContainerName,
} from './feature.js';

// Examples
export { ${camelName}Examples } from './examples.js';

// Renderers (for registry use)
export { render${pascalName}ContainerWeb } from './runtime.web.js';
export { render${pascalName}ContainerRN } from './runtime.rn.js';
`;
}

function generateJestConfig(jestPresetPath: string): string {
  return `/** @type {import('jest').Config} */
module.exports = {
  // Uses the shared Supramark Jest preset
  // Keeps test configuration consistent with @supramark/core
  ...require('${jestPresetPath}'),

  // Feature-package-specific config can be overridden here
  // For example:
  // testEnvironment: 'jsdom', // if a DOM environment is needed
  // collectCoverage: true,     // enable coverage collection
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
  extensionType: ExtensionType | null;
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
      if (!isExtensionType(nextArg)) {
        throw new Error(
          `Unknown extension type "${nextArg}". Expected one of: ${Object.keys(EXTENSION_TYPES).join(', ')}.`
        );
      }
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
${colors.bright}Supramark Feature Scaffolding Tool v2${colors.reset}

${colors.blue}Usage:${colors.reset}
  bun run feature:create [options]

${colors.blue}Options:${colors.reset}
  -n, --name <name>          Feature name (e.g. "Weather")
  -c, --container <name>     Container/input block name (e.g. "weather", used as :::weather)
  -t, --type <type>          Extension type: container | input (default: container)
  -v, --version <version>    Version number (default: "0.1.0")
  -a, --author <author>      Author (default: "Supramark Team")
  -d, --description <desc>   Short description
  --dry-run                  Only print the list of files to be generated, without writing to disk
  -o, --output-dir <dir>     Output directory (overrides the default location)
  -h, --help                 Show this help message

${colors.blue}Extension types:${colors.reset}
  ${colors.green}container${colors.reset}  block-level container (:::)  e.g. :::map, :::note, :::weather
  ${colors.yellow}input${colors.reset}      input block (%%%)            e.g. %%%form, %%%survey (in development)

${colors.blue}Examples:${colors.reset}
  ${colors.gray}# Interactive creation${colors.reset}
  bun run feature:create

  ${colors.gray}# Create a Container extension via arguments${colors.reset}
  bun run feature:create -- -n "Weather" -c "weather" -d "Weather card"

  ${colors.gray}# Create an Input extension${colors.reset}
  bun run feature:create -- -n "Survey" -c "survey" -t input -d "Survey questionnaire"
`);
}

interface FileToWrite {
  path: string;
  content: string;
  desc: string;
}

async function main(): Promise<void> {
  log('\n🚀 Supramark Feature Scaffolding Tool v2\n', 'bright');

  try {
    const cliOptions = parseArgs();

    let name: string = cliOptions.name!;
    let containerName: string = cliOptions.containerName!;
    let extensionType: ExtensionType = cliOptions.extensionType!;
    const version: string = cliOptions.version;
    const author: string = cliOptions.author;
    let description: string = cliOptions.description;
    const dryRun: boolean = cliOptions.dryRun;
    const outputDirOption: string | null = cliOptions.outputDir;

    const isInteractive = !name || !containerName;

    if (isInteractive) {
      if (!extensionType) {
        const options: Array<SelectOption & { value: ExtensionType }> = [
          {
            value: 'container',
            label: 'Container (:::)',
            description: 'Block-level container extension, e.g. :::map, :::note, :::weather',
          },
          {
            value: 'input',
            label: 'Input (%%%)',
            description: 'Input block extension, e.g. %%%form, %%%survey (in development)',
          },
        ];
        const selected = await selectMenu('Select extension type:', options);
        extensionType = selected && isExtensionType(selected) ? selected : options[0].value;
      }

      const extConfig = EXTENSION_TYPES[extensionType]!;
      log(`\nSelected: ${colors.green}${extConfig.label}${colors.reset}\n`, 'reset');

      if (!containerName) {
        containerName = await question(
          `${extensionType === 'container' ? 'Container' : 'Input block'} name (used as ${extConfig.syntax}xxx, e.g. "weather"): `
        );
        if (!containerName) {
          throw new Error('Name cannot be empty');
        }
      }

      if (!validateContainerName(containerName)) {
        throw new Error(
          `Name "${containerName}" is invalid. It must start with a lowercase letter and contain only lowercase letters, digits, underscores, and hyphens.`
        );
      }

      if (!name) {
        const defaultName = capitalize(containerName);
        const inputName = await question(`Feature name [${defaultName}]: `);
        name = inputName || defaultName;
      }

      if (!description) {
        description = await question('Short description (optional): ');
      }

      const id = `@supramark/feature-${toKebabCase(name)}`;
      log('\n📋 Confirm details:', 'bright');
      log(`  Extension type:   ${colors.green}${extConfig.label}${colors.reset}`, 'reset');
      log(
        `  Syntax:           ${colors.blue}${extConfig.syntax}${containerName}${colors.reset}`,
        'reset'
      );
      log(`  Feature:    ${colors.blue}${name}${colors.reset}`, 'reset');
      log(`  Package ID: ${colors.gray}${id}${colors.reset}`, 'reset');
      if (description) {
        log(`  Description:      ${colors.gray}${description}${colors.reset}`, 'reset');
      }
      log('');

      const confirm = await question('Create it? (Y/n): ');
      if (confirm.toLowerCase() === 'n') {
        log('\nCancelled.\n', 'yellow');
        return;
      }
    } else {
      extensionType = extensionType || 'container';
      if (!EXTENSION_TYPES[extensionType]) {
        throw new Error(`Invalid extension type: ${extensionType}. Options: container, input`);
      }
      if (!validateContainerName(containerName)) {
        throw new Error(
          `Name "${containerName}" is invalid. It must start with a lowercase letter and contain only lowercase letters, digits, underscores, and hyphens.`
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
        `Feature directory already exists: ${path.relative(process.cwd(), basePath)}\nPlease choose a different name or delete the existing directory`
      );
    }

    log(`\n📁 Creating directory structure${dryRun ? ' (dry-run)' : ''}...\n`, 'gray');
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

    log(`\n📝 Generating files${dryRun ? ' (dry-run)' : ''}...\n`, 'gray');
    files.forEach(file => {
      if (!dryRun) {
        fs.writeFileSync(file.path, file.content, 'utf-8');
        log(`  ✓ ${file.desc}`, 'green');
      } else {
        log(`  • ${file.desc}`, 'gray');
      }
    });

    if (dryRun) {
      log('\n(dry-run) No files were written.\n', 'yellow');
      return;
    }

    log('\n📝 Tip:', 'yellow');
    log('  To integrate the new Feature into the project, run:', 'reset');
    log(`  ${colors.green}bun run features:sync${colors.reset}`, 'reset');

    const extConfig = EXTENSION_TYPES[extensionType]!;
    log('\n✨ Feature created successfully!\n', 'bright');
    log('📦 Generated package:', 'yellow');
    log(`  ${colors.blue}${id}${colors.reset}`, 'reset');
    log(`  Location: ${colors.gray}${relativeDir}${colors.reset}`, 'reset');
    log(`  Syntax: ${colors.green}${extConfig.syntax}${containerName}${colors.reset}\n`, 'reset');

    log('📝 Next steps:', 'yellow');
    log(`  1. Edit ${colors.blue}src/feature.ts${colors.reset} to complete the parsing logic`, 'reset');
    log(`  2. Edit ${colors.blue}src/runtime.web.tsx${colors.reset} to implement Web rendering`, 'reset');
    log(`  3. Edit ${colors.blue}src/runtime.rn.tsx${colors.reset} to implement RN rendering`, 'reset');
    log(`  4. Run ${colors.green}bun run build${colors.reset} to compile`, 'reset');
    log(`  5. Run ${colors.green}bun run feature:lint${colors.reset} to check\n`, 'reset');
  } catch (error) {
    log(`\n❌ Error: ${error instanceof Error ? error.message : String(error)}\n`, 'red');
  } finally {
    closeRL();
  }
}

main();
