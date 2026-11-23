import { execFileSync } from 'node:child_process';
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import ts from 'typescript';

const repo = path.resolve(import.meta.dir, '..');
const releaseCandidate = process.env.SUPRAMARK_NPM_RC;
if (releaseCandidate !== undefined && !/^[1-9][0-9]*$/.test(releaseCandidate)) {
  throw new Error('SUPRAMARK_NPM_RC must be a positive release candidate number');
}

const selectionPackage = 'packages/renderers/rn-selection';
const selectionMacOSDirectory = 'native/macos';
const packagePaths = [
  'crates/supramark-markdown/packages/web',
  'crates/graphviz-anywhere/packages/web',
  'crates/graphviz-anywhere/packages/react-native',
  'crates/d2-little/packages/web',
  'crates/mermaid-little/packages/web',
  'crates/plantuml-little/packages/web',
  'packages/core',
  'packages/engines',
  selectionPackage,
  'crates/supramark-markdown/packages/react-native',
  'packages/renderers/rn',
];

type Dependencies = Record<string, string>;
type Manifest = {
  name: string;
  version: string;
  private?: boolean;
  dependencies?: Dependencies;
  optionalDependencies?: Dependencies;
  peerDependencies?: Dependencies;
  peerDependenciesMeta?: Record<string, { optional?: boolean }>;
  devDependencies?: Dependencies;
  scripts?: Record<string, string>;
  files?: string[];
  main?: string;
  module?: string;
  types?: string;
  source?: string;
  exports?: Record<string, unknown>;
  repository?: { type: string; url: string; directory: string };
  publishConfig?: { access: string; registry: string };
  'react-native'?: string;
};

function readManifest(directory: string): Manifest {
  return JSON.parse(readFileSync(path.join(directory, 'package.json'), 'utf8')) as Manifest;
}

function publishName(name: string): string {
  if (name.startsWith('@supramark/')) return name.replace('@supramark/', '@goaskaway/supramark-');
  if (name.startsWith('@actrium/')) return name.replace('@actrium/', '@goaskaway/');
  throw new Error(`Unexpected release scope for ${name}`);
}

function publishVersion(version: string): string {
  if (!releaseCandidate) return version;
  const [base, ...prerelease] = version.split('+')[0].split('-');
  if (prerelease.length === 0) return `${base}-rc.${releaseCandidate}`;
  return `${base}-0.rev.${prerelease.join('-')}.rc.${releaseCandidate}`;
}

const packages = packagePaths.map(directory => {
  const manifest = readManifest(path.join(repo, directory));
  if (manifest.private) throw new Error(`Cannot package private workspace ${manifest.name}`);
  return {
    directory,
    manifest,
    name: publishName(manifest.name),
    version: publishVersion(manifest.version),
  };
});
const byWorkspaceName = new Map(packages.map(pkg => [pkg.manifest.name, pkg]));

function publishDependencies(dependencies: Dependencies): Dependencies {
  return Object.fromEntries(
    Object.entries(dependencies).map(([name, range]) => {
      const workspace = byWorkspaceName.get(name);
      if (workspace) return [workspace.name, workspace.version];
      if (/^(workspace:|file:|link:)/.test(range)) {
        throw new Error(`Local dependency ${name}@${range} is missing from the release set`);
      }
      return [name, range];
    })
  );
}

function createPublishManifest(pkg: (typeof packages)[number]): Manifest {
  const manifest = {
    ...pkg.manifest,
    name: pkg.name,
    version: pkg.version,
    repository: {
      type: 'git',
      url: 'https://github.com/GoAskAway/supramark.git',
      directory: pkg.directory,
    },
    publishConfig: { access: 'public', registry: 'https://registry.npmjs.org/' },
  };
  delete manifest.scripts;
  delete manifest.devDependencies;
  for (const field of ['dependencies', 'optionalDependencies', 'peerDependencies'] as const) {
    if (manifest[field]) manifest[field] = publishDependencies(manifest[field]);
  }
  if (manifest.peerDependenciesMeta) {
    manifest.peerDependenciesMeta = Object.fromEntries(
      Object.entries(manifest.peerDependenciesMeta).map(([name, meta]) => [
        byWorkspaceName.get(name)?.name ?? name,
        meta,
      ])
    );
  }
  if (pkg.directory === 'packages/engines') {
    // Metro resolves imports in the lazy engine loaders while building the bundle.
    manifest.dependencies = {
      ...manifest.dependencies,
      ...Object.fromEntries(
        Object.entries(manifest.peerDependencies ?? {}).filter(
          ([name]) => manifest.peerDependenciesMeta?.[name]?.optional
        )
      ),
    };
    delete manifest.peerDependencies;
    delete manifest.peerDependenciesMeta;
  }
  if (pkg.directory.endsWith('/packages/react-native')) {
    // The published RN packages expose the same source files that Codegen reads.
    manifest.main = 'src/index.ts';
    manifest.types = 'src/index.ts';
    manifest['react-native'] = 'src/index.ts';
    delete manifest.module;
  }
  return manifest;
}

function rewriteModuleSpecifiers(filename: string, source: string, isSource: boolean): string {
  const parsed = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
  const edits: Array<{ start: number; end: number; value: string }> = [];
  function visit(node: ts.Node): void {
    if (ts.isStringLiteralLike(node)) {
      const original = node.text;
      const [scope, name] = original.split('/');
      const packageName = `${scope}/${name}`;
      const workspace = byWorkspaceName.get(packageName);
      let next = workspace ? workspace.name + original.slice(packageName.length) : original;
      if (isSource && /^\.\.?\//.test(next) && next.endsWith('.js')) {
        const localPath = path.resolve(path.dirname(filename), next);
        const stem = localPath.slice(0, -3);
        // Metro resolves platform-specific TypeScript from extensionless imports.
        if (!existsSync(localPath) && ['.ts', '.tsx'].some(ext => existsSync(stem + ext))) {
          next = next.slice(0, -3);
        }
      }
      if (next !== original) {
        edits.push({
          start: node.getStart(parsed),
          end: node.getEnd(),
          value: JSON.stringify(next),
        });
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(parsed);
  for (const edit of edits.reverse()) {
    source = source.slice(0, edit.start) + edit.value + source.slice(edit.end);
  }
  return source;
}

function rewriteDirectory(directory: string, isSource: boolean): void {
  if (!existsSync(directory)) return;
  for (const file of readdirSync(directory, { recursive: true, withFileTypes: true })) {
    if (!file.isFile() || !/\.[cm]?[jt]sx?$/.test(file.name)) continue;
    const filename = path.join(file.parentPath, file.name);
    const source = readFileSync(filename, 'utf8');
    const rewritten = rewriteModuleSpecifiers(filename, source, isSource);
    if (source !== rewritten) writeFileSync(filename, rewritten);
  }
}

type Tarball = { name: string; version: string; filename: string };

function pack(output: string): void {
  mkdirSync(output, { recursive: true });
  const staging = mkdtempSync(path.join(tmpdir(), 'supramark-npm-'));
  const tarballs: Tarball[] = [];
  try {
    for (const [index, pkg] of packages.entries()) {
      const source = path.join(repo, pkg.directory);
      const stage = path.join(staging, String(index));
      mkdirSync(stage);
      for (const entry of [
        'src',
        'dist',
        'ios',
        'macos',
        'android',
        'windows',
        'react-native.config.js',
      ]) {
        const input = path.join(source, entry);
        if (existsSync(input)) cpSync(input, path.join(stage, entry), { recursive: true });
      }
      for (const entry of readdirSync(source)) {
        if (/\.podspec$|^README|^LICEN[CS]E/i.test(entry)) {
          cpSync(path.join(source, entry), path.join(stage, entry), { recursive: true });
        }
      }
      if (!existsSync(path.join(stage, 'LICENSE')))
        cpSync(path.join(repo, 'LICENSE'), path.join(stage, 'LICENSE'));
      cpSync(path.join(repo, 'LICENSES'), path.join(stage, 'LICENSES'), { recursive: true });
      const manifest = createPublishManifest(pkg);
      manifest.files ??= ['src', 'dist', '!**/__tests__', '!**/*.test.*', '!**/*.spec.*'];
      manifest.files = [...manifest.files, 'LICENSES'];
      if (pkg.directory === selectionPackage) {
        cpSync(
          path.join(source, selectionMacOSDirectory),
          path.join(stage, selectionMacOSDirectory),
          {
            recursive: true,
          }
        );
        manifest.files.push(selectionMacOSDirectory);
      }
      if (pkg.directory.endsWith('/packages/web') && manifest.exports) {
        // Keep Web-only Wasm imports out of the native bundle.
        manifest.exports = {
          ...manifest.exports,
          '.': {
            'react-native': './dist/native.js',
            ...(manifest.exports['.'] as Record<string, unknown>),
          },
        };
        writeFileSync(path.join(stage, 'dist/native.js'), 'export {};\n');
      }
      if (pkg.directory === 'packages/core') {
        // Both public RN entry points share one registry, and Metro selects the native loader.
        cpSync(
          path.join(stage, 'src/plugin-loader-rn.ts'),
          path.join(stage, 'src/plugin-loader.native.ts')
        );
      }
      if (pkg.directory === 'crates/supramark-markdown/packages/web') {
        // wasm-pack's Node target emits CommonJS inside an ESM package.
        writeFileSync(path.join(stage, 'dist/node-wasm/package.json'), '{"type":"commonjs"}\n');
      }
      rewriteDirectory(path.join(stage, 'src'), true);
      rewriteDirectory(path.join(stage, 'dist'), false);
      writeFileSync(path.join(stage, 'package.json'), JSON.stringify(manifest, null, 2) + '\n');

      const result = execFileSync(
        'npm',
        ['pack', '--ignore-scripts', '--json', '--pack-destination', output],
        {
          cwd: stage,
          encoding: 'utf8',
        }
      );
      const [packed] = JSON.parse(result) as Array<Tarball & { files: Array<{ path: string }> }>;
      const files = new Set(packed.files.map(file => file.path));
      for (const entry of Object.values({
        main: manifest.main,
        types: manifest.types,
        'react-native': manifest['react-native'],
      })) {
        if (entry && !files.has(entry.replace(/^\.\//, ''))) {
          throw new Error(`${manifest.name}: missing entry point ${entry} in tarball`);
        }
      }
      if (pkg.directory.endsWith('/packages/react-native')) {
        execFileSync(
          'node',
          ['scripts/verify-macos-native-packages.js', '--package-root', source],
          {
            cwd: repo,
            stdio: 'inherit',
          }
        );
        for (const entry of readdirSync(stage).filter(file => file.endsWith('.podspec'))) {
          if (!files.has(entry)) throw new Error(`${manifest.name}: missing ${entry} in tarball`);
        }
        if (![...files].some(file => /^macos\/Frameworks\/lib\/.*\.dylib$/.test(file))) {
          throw new Error(`${manifest.name}: missing macOS dylib in tarball`);
        }
        if (![...files].some(file => /^macos\/Frameworks\/include\/.*\.h$/.test(file))) {
          throw new Error(`${manifest.name}: missing macOS header in tarball`);
        }
      }
      if (
        pkg.directory.endsWith('/packages/web') &&
        ![...files].some(file => file.endsWith('.wasm'))
      ) {
        throw new Error(`${manifest.name}: missing Wasm binary in tarball`);
      }
      tarballs.push({ name: packed.name, version: packed.version, filename: packed.filename });
      console.log(`Packed ${packed.name}@${packed.version}`);
    }
    writeFileSync(path.join(output, 'packages.json'), JSON.stringify(tarballs, null, 2) + '\n');
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}

function verify(output: string): void {
  const tarballs = JSON.parse(
    readFileSync(path.join(output, 'packages.json'), 'utf8')
  ) as Tarball[];
  const fixture = realpathSync(mkdtempSync(path.join(tmpdir(), 'supramark-npm-consumer-')));
  try {
    const dependencies = {
      ...Object.fromEntries(
        tarballs.map(tarball => [tarball.name, `file:${path.join(output, tarball.filename)}`])
      ),
      '@react-native/metro-config': '0.81.5',
      react: '19.1.0',
      'react-native': '0.81.5',
      'react-native-svg': '15.15.3',
    };
    writeFileSync(
      path.join(fixture, 'package.json'),
      JSON.stringify({ name: 'supramark-release-consumer', private: true, dependencies })
    );
    writeFileSync(
      path.join(fixture, 'index.js'),
      [
        "import '@goaskaway/supramark-markdown-native-rn';",
        "import { parse } from '@goaskaway/supramark-core/rn';",
        "import { Supramark } from '@goaskaway/supramark-rn';",
        'export { parse, Supramark };',
      ].join('\n')
    );
    execFileSync('npm', ['install', '--ignore-scripts', '--no-audit', '--no-fund'], {
      cwd: fixture,
      stdio: 'inherit',
    });
    const selectionPod = path.join(
      fixture,
      'node_modules/@goaskaway/supramark-rn-selection',
      selectionMacOSDirectory
    );
    const selectionPodspec = 'SupramarkSelectionMetrics.podspec';
    for (const file of [
      selectionPodspec,
      'SupramarkSelectionMetrics.mm',
      'SupramarkTextGeometry.h',
      'SupramarkTextGeometry.mm',
    ]) {
      if (!existsSync(path.join(selectionPod, file))) {
        throw new Error(`The installed macOS selection pod is missing ${file}`);
      }
    }
    execFileSync('ruby', ['-c', path.join(selectionPod, selectionPodspec)], { stdio: 'inherit' });
    execFileSync(
      'node',
      [
        '--input-type=module',
        '--eval',
        `import { parse } from '@goaskaway/supramark-markdown-web/node';
const ast = parse('# Supramark release smoke test');
if (ast.children[0]?.type !== 'heading') throw new Error('Unexpected Node Markdown output');
`,
      ],
      { cwd: fixture, stdio: 'inherit' }
    );
    writeFileSync(
      path.join(fixture, 'bundle.cjs'),
      `const { getDefaultConfig } = require('@react-native/metro-config');
const { runBuild } = require('metro');
const config = getDefaultConfig(__dirname);
config.projectRoot = __dirname;
config.watchFolders = [__dirname];
config.resolver.useWatchman = false;
config.maxWorkers = 2;
runBuild(config, { entry: 'index.js', platform: 'ios', dev: false, minify: false, out: 'index.bundle' })
  .catch(error => { console.error(error); process.exitCode = 1; });
`
    );
    execFileSync('node', ['bundle.cjs'], { cwd: fixture, stdio: 'inherit' });
    const bundle = readFileSync(path.join(fixture, 'index.bundle.js'), 'utf8');
    if (!bundle.includes('RN runtime requires native markdown parser adapter.')) {
      throw new Error('The Metro smoke bundle did not select the native Markdown loader');
    }
    console.log('Installed tarballs and verified the React Native Markdown bundle');
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
}

const [command, output] = process.argv.slice(2);
if (command === 'plan' && !output) {
  for (const pkg of packages) {
    const manifest = createPublishManifest(pkg);
    console.log(`${manifest.name}@${manifest.version}`);
  }
} else if (command === 'pack' && output) {
  pack(path.resolve(output));
} else if (command === 'verify' && output) {
  verify(path.resolve(output));
} else {
  throw new Error('Usage: bun scripts/npm-release.ts plan | pack <directory> | verify <directory>');
}
