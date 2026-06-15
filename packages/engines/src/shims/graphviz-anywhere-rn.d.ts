/**
 * Type shim for the React Native graphviz binding.
 *
 * The real package (`@kookyleo/graphviz-anywhere-rn`) only ships its
 * declarations as a build artifact (`lib/typescript/index.d.ts`) and its
 * runtime entry imports `react-native`, so it cannot be resolved by `tsc`
 * in this node-targeted workspace — especially before the artifact is
 * built. The module is loaded lazily via `await import(...)` and only on
 * a React Native host; here we just describe the surface the engines
 * package consumes. Wired in through `tsconfig.base.json` `paths`.
 */

export type GraphvizEngine =
  | 'dot'
  | 'neato'
  | 'fdp'
  | 'sfdp'
  | 'circo'
  | 'twopi'
  | 'osage'
  | 'patchwork';

export type GraphvizFormat =
  | 'svg'
  | 'png'
  | 'pdf'
  | 'ps'
  | 'json'
  | 'dot'
  | 'xdot'
  | 'plain';

export function renderDot(
  dot: string,
  engine?: GraphvizEngine,
  format?: GraphvizFormat
): Promise<string>;

export function getVersion(): Promise<string>;
