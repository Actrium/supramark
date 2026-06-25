import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'node:fs';
import { resolve, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

import { generate, computeHash } from './generator.js';
import { validate, ConfigError } from './validator.js';
import type { SupramarkConfig } from './types.js';

const CLI_VERSION = '0.1.0';
const DEFAULT_CONFIG_PATH = './supramark.config.json';
const DEFAULT_OUT_PATH = './src/generated/supramark.ts';

const PRESETS_DIR = resolve(
  dirname(fileURLToPath(import.meta.url)),
  'presets'
);

// ----------------------------------------------------------------------------
// argv 解析（不引 mri/cac，手写一个极简的）
// ----------------------------------------------------------------------------
interface Args {
  config?: string;
  out?: string;
  preset?: string;
  check: boolean;
  help: boolean;
  version: boolean;
  listPresets: boolean;
}

function parseArgs(argv: string[]): Args {
  const args: Args = {
    check: false,
    help: false,
    version: false,
    listPresets: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    switch (a) {
      case '-c':
      case '--config':
        args.config = argv[++i];
        break;
      case '-o':
      case '--out':
        args.out = argv[++i];
        break;
      case '-p':
      case '--preset':
        args.preset = argv[++i];
        break;
      case '--check':
        args.check = true;
        break;
      case '-h':
      case '--help':
        args.help = true;
        break;
      case '-v':
      case '--version':
        args.version = true;
        break;
      case '--list-presets':
        args.listPresets = true;
        break;
      default:
        if (a.startsWith('-')) {
          throw new Error(`Unknown flag: ${a}`);
        }
    }
  }
  return args;
}

function printHelp(): void {
  console.log(`supramark-gen v${CLI_VERSION}

Usage:
  supramark-gen [options]

Options:
  -c, --config <path>    配置文件路径          [default: ${DEFAULT_CONFIG_PATH}]
  -o, --out <path>       生成文件路径          [default: ${DEFAULT_OUT_PATH}]
  -p, --preset <name>    使用内置 preset       [与 --config 互斥]
      --check            仅校验；漂移即失败（CI 用）
      --list-presets     列出可用 preset
  -h, --help             显示此帮助
  -v, --version          打印 CLI 版本

Examples:
  supramark-gen --preset docs
  supramark-gen -c my.config.json -o src/gen.ts
  supramark-gen --check
`);
}

function listPresets(): void {
  const presets = ['minimal', 'blog', 'docs', 'data-viz', 'ai-chat'];
  console.log('Available presets (bun x supramark-gen --preset <name>):');
  for (const p of presets) {
    console.log(`  - ${p}`);
  }
  console.log('\nPreset file:', PRESETS_DIR);
}

// ----------------------------------------------------------------------------
// 读 config（文件 or preset）
// ----------------------------------------------------------------------------
function loadConfig(
  args: Args
): { config: SupramarkConfig; source: string; path: string } {
  if (args.preset) {
    if (args.config) {
      throw new Error('--preset and --config are mutually exclusive');
    }
    const presetPath = resolve(PRESETS_DIR, `${args.preset}.json`);
    if (!existsSync(presetPath)) {
      throw new Error(`Preset not found: ${args.preset}. Use --list-presets to see options.`);
    }
    const source = readFileSync(presetPath, 'utf-8');
    return {
      config: JSON.parse(source) as SupramarkConfig,
      source,
      path: `preset:${args.preset}`,
    };
  }

  const configPath = resolve(args.config ?? DEFAULT_CONFIG_PATH);
  if (!existsSync(configPath)) {
    throw new Error(
      `Config not found at ${relative(process.cwd(), configPath)}. ` +
        `Use --preset <name> to start with a template.`
    );
  }
  const source = readFileSync(configPath, 'utf-8');
  return {
    config: JSON.parse(source) as SupramarkConfig,
    source,
    path: relative(process.cwd(), configPath),
  };
}

// ----------------------------------------------------------------------------
// 主流程
// ----------------------------------------------------------------------------
// Public CLI entry: the `Promise<number>` signature is a stable contract that
// callers `await` (see bin/supramark-gen.mjs), so the body stays `async` even
// though it currently performs no asynchronous work.
// eslint-disable-next-line @typescript-eslint/require-await
export async function main(argv: string[] = process.argv.slice(2)): Promise<number> {
  let args: Args;
  try {
    args = parseArgs(argv);
  } catch (e) {
    console.error(`Error: ${(e as Error).message}`);
    return 1;
  }

  if (args.help) {
    printHelp();
    return 0;
  }
  if (args.version) {
    console.log(CLI_VERSION);
    return 0;
  }
  if (args.listPresets) {
    listPresets();
    return 0;
  }

  let loaded: ReturnType<typeof loadConfig>;
  try {
    loaded = loadConfig(args);
  } catch (e) {
    console.error(`Error: ${(e as Error).message}`);
    return 1;
  }

  const { config, source, path: configPath } = loaded;

  try {
    validate(config);
  } catch (e) {
    if (e instanceof ConfigError) {
      console.error(`Config error: ${e.message}`);
      return 2;
    }
    throw e;
  }

  const generated = generate({
    config,
    cliVersion: CLI_VERSION,
    configSource: source,
    configPath,
  });

  const outPath = resolve(args.out ?? config.out ?? DEFAULT_OUT_PATH);

  if (args.check) {
    if (!existsSync(outPath)) {
      console.error(`Check failed: ${relative(process.cwd(), outPath)} does not exist. Run supramark-gen.`);
      return 3;
    }
    const existing = readFileSync(outPath, 'utf-8');
    if (existing === generated) {
      console.log(`ok: ${relative(process.cwd(), outPath)} is in sync with ${configPath}`);
      return 0;
    }
    console.error(
      `Check failed: ${relative(process.cwd(), outPath)} drifted from ${configPath}.\n` +
        `Run supramark-gen to sync (expected sha256 ${computeHash(source)}).`
    );
    return 3;
  }

  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, generated);
  console.log(
    `Generated ${relative(process.cwd(), outPath)} from ${configPath} (sha256 ${computeHash(source)})`
  );
  return 0;
}
