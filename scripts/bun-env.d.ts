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
 *
 * One caveat: minimality only keeps the coverage side honest. Any script
 * reaching for an API missing here errors immediately, but a hand-written
 * signature can also mis-declare — if a future Bun changed `spawnSync`'s
 * result shape, this file would silently lie rather than fail. The real
 * invariant is that declared signatures match runtime behaviour, which
 * minimality alone does not guarantee; for these two stable APIs the
 * practical risk is near zero.
 */

interface ImportMeta {
  /** Absolute path of the directory containing the current module (Bun). */
  readonly dir: string;
  /** True when this module is the program's entry point (Bun). */
  readonly main: boolean;
}

declare const Bun: {
  spawnSync(
    command: string[],
    options?: { cwd?: string; stdout?: 'pipe' | 'inherit'; stderr?: 'pipe' | 'inherit' }
  ): {
    /** Exit code, or null when the process was killed by a signal. */
    exitCode: number | null;
    success: boolean;
    stdout: { toString(): string };
    stderr: { toString(): string };
  };
};
