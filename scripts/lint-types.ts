/**
 * Type-aware lint pass.
 *
 * `recommended-type-checked` needs the TypeScript type checker, which is far
 * heavier than the syntactic `lint` run and OOMs when pointed at the whole
 * monorepo at once. So we lint each package's `src` directory in its own
 * process (bounded heap), using `.eslintrc.types.cjs`, and aggregate.
 *
 * Usage:
 *   bun scripts/lint-types.ts            # all packages
 *   bun scripts/lint-types.ts <dir>...   # only the given package dirs
 */
import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

const repoRoot = resolve(import.meta.dir, '..');
const config = resolve(repoRoot, '.eslintrc.types.cjs');
const eslintBin = resolve(repoRoot, 'node_modules/.bin/eslint');

function discoverPackageSrcDirs(): string[] {
  const out = execFileSync(
    'find',
    [
      'packages',
      'crates',
      '-name',
      'tsconfig.json',
      '-not',
      '-path',
      '*/node_modules/*',
      '-not',
      '-path',
      '*/dist/*',
      '-not',
      '-path',
      '*/lib/*',
    ],
    { cwd: repoRoot, encoding: 'utf8' }
  );
  const dirs = new Set<string>();
  for (const line of out.split('\n')) {
    if (!line.trim()) continue;
    const dir = line.replace(/\/tsconfig\.json$/, '');
    if (existsSync(resolve(repoRoot, dir, 'src'))) dirs.add(dir);
  }
  return [...dirs].sort();
}

interface EslintMessage {
  ruleId: string | null;
  line: number;
  column: number;
  message: string;
  severity: number;
}
interface EslintResult {
  filePath: string;
  messages: EslintMessage[];
}

function lintDir(dir: string): EslintResult[] {
  try {
    const stdout = execFileSync(
      eslintBin,
      [`${dir}/src`, '--ext', '.ts,.tsx', '--no-eslintrc', '--config', config, '-f', 'json'],
      {
        cwd: repoRoot,
        encoding: 'utf8',
        maxBuffer: 64 * 1024 * 1024,
        env: { ...process.env, NODE_OPTIONS: '--max-old-space-size=6144' },
      }
    );
    return JSON.parse(stdout) as EslintResult[];
  } catch (err: unknown) {
    // eslint exits non-zero when it reports problems; stdout still holds JSON.
    const e = err as { stdout?: string };
    if (e.stdout) {
      try {
        return JSON.parse(e.stdout) as EslintResult[];
      } catch {
        /* fall through */
      }
    }
    console.error(`[lint:types] failed to run on ${dir}`);
    throw err;
  }
}

const targets = process.argv.slice(2);
const dirs = targets.length ? targets : discoverPackageSrcDirs();

let total = 0;
const byRule: Record<string, number> = {};
for (const dir of dirs) {
  const results = lintDir(dir);
  let n = 0;
  for (const file of results) {
    for (const m of file.messages) {
      n++;
      total++;
      byRule[m.ruleId ?? '(parse)'] = (byRule[m.ruleId ?? '(parse)'] ?? 0) + 1;
      const rel = file.filePath.replace(`${repoRoot}/`, '');
      console.log(`${rel}:${m.line}:${m.column}  ${m.ruleId ?? '(parse)'}  ${m.message}`);
    }
  }
  if (n) console.log(`  -> ${dir}: ${n}`);
}

console.log(`\n[lint:types] total: ${total}`);
for (const [rule, count] of Object.entries(byRule).sort((a, b) => b[1] - a[1])) {
  console.log(`  ${String(count).padStart(5)}  ${rule}`);
}
process.exit(total > 0 ? 1 : 0);
