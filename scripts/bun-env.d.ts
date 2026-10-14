/**
 * Minimal typings for the Bun globals these scripts use.
 *
 * The workspace does not depend on `@types/bun`, and adding it just for
 * `import.meta.dir` plus one `Bun.spawnSync` call would pull an entire runtime's
 * types into the check. Declaring exactly what is used keeps the check honest
 * without the dependency; extend this file when a script reaches for more.
 *
 * These declarations are global, so any tsconfig that includes scripts/ inherits
 * them. That is intended — these scripts run under bun and nowhere else — but it
 * is also why this file should stay at the minimum the scripts actually use.
 */

interface ImportMeta {
  /** Absolute path of the directory containing the current module (Bun). */
  readonly dir: string;
}

declare const Bun: {
  spawnSync(
    command: string[],
    options?: { cwd?: string; stdout?: 'pipe' | 'inherit'; stderr?: 'pipe' | 'inherit' }
  ): {
    exitCode: number;
    success: boolean;
    stdout: { toString(): string };
    stderr: { toString(): string };
  };
};
