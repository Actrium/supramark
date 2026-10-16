export const DEFAULT_PLAYGROUND_FEATURE = 'core-markdown';

export const PLAYGROUND_ROUTES = [
  { feature: 'admonition', slug: 'admonition' },
  { feature: 'core-markdown', slug: 'core-markdown' },
  { feature: 'd2', slug: 'd2' },
  { feature: 'definition-list', slug: 'definition-list' },
  { feature: 'diagram-dot', slug: 'dot' },
  { feature: 'diagram-echarts', slug: 'echarts' },
  { feature: 'diagram-vega-lite', slug: 'vega-lite' },
  { feature: 'emoji', slug: 'emoji' },
  { feature: 'footnote', slug: 'footnote' },
  { feature: 'gfm', slug: 'gfm' },
  { feature: 'html-page', slug: 'html-page' },
  { feature: 'map', slug: 'map' },
  { feature: 'math', slug: 'math' },
  { feature: 'mermaid', slug: 'mermaid' },
  { feature: 'plantuml', slug: 'plantuml' },
  { feature: 'video', slug: 'video' },
  { feature: 'weather', slug: 'weather' },
] as const;

/**
 * Feature packages that deliberately have no playground page.
 *
 * `bun run feature:preview:web` discovers features by scanning every
 * `@supramark/feature-*` package (`scripts/lib-feature-layout.ts`), which is a
 * wider set than the Markdown syntaxes that make sense as a playground page.
 * Every scanned package must be either routed above or listed here — with a
 * reason, since the preview CLI shows it to whoever asks for that feature.
 * `scripts/check-playground-routes.ts` fails when a package is in neither list.
 */
export const PLAYGROUND_EXCLUDED_FEATURES = [
  {
    feature: 'card-vison',
    reason:
      'Parses the :::vison container, but its renderer delegates to @actrium/vison-web, which the playground does not depend on.',
  },
  {
    feature: 'code-highlight',
    reason:
      'Code-fence rendering framework rather than a syntax of its own — its effect shows up in the Core Markdown examples.',
  },
  {
    feature: 'code-highlight-preset-dev',
    reason: 'Build-time language/theme preset — its selector matches no AST node.',
  },
  {
    feature: 'code-highlight-preset-docs',
    reason: 'Build-time language/theme preset — its selector matches no AST node.',
  },
  {
    feature: 'code-highlight-preset-full',
    reason: 'Build-time language/theme preset — its selector matches no AST node.',
  },
] as const;

const routeByFeature = new Map<string, string>(
  PLAYGROUND_ROUTES.map(route => [route.feature, route.slug])
);
const featureByRoute = new Map<string, string>(
  PLAYGROUND_ROUTES.map(route => [route.slug, route.feature])
);
const exclusionReasonByFeature = new Map<string, string>(
  PLAYGROUND_EXCLUDED_FEATURES.map(entry => [entry.feature, entry.reason])
);

export type PlaygroundRouteStatus =
  | { kind: 'routed'; slug: string }
  | { kind: 'excluded'; reason: string }
  | { kind: 'unregistered' };

/**
 * Whether a feature package has a playground page, and why not when it does
 * not. Callers that can report a reason to a human use this; callers that
 * cannot keep using `routeSlugForFeature`.
 */
export function playgroundRouteStatus(feature: string): PlaygroundRouteStatus {
  const slug = routeByFeature.get(feature);
  if (slug) return { kind: 'routed', slug };

  const reason = exclusionReasonByFeature.get(feature);
  if (reason) return { kind: 'excluded', reason };

  return { kind: 'unregistered' };
}

export function routeSlugForFeature(feature: string): string {
  const status = playgroundRouteStatus(feature);
  switch (status.kind) {
    case 'routed':
      return status.slug;
    case 'excluded':
      throw new Error(`Feature has no playground page: ${feature} — ${status.reason}`);
    default:
      throw new Error(`No playground route registered for feature: ${feature}`);
  }
}

export function featureForRouteSlug(slug: string): string | undefined {
  return featureByRoute.get(slug);
}

export function featureFromPlaygroundPath(pathname: string, basePath: string): string | undefined {
  const normalizedBase = normalizeBasePath(basePath);
  if (!pathname.startsWith(normalizedBase)) return undefined;

  const relativePath = pathname.slice(normalizedBase.length).replace(/^\/+|\/+$/g, '');
  if (!relativePath) return undefined;

  const [slug] = relativePath.split('/');
  return featureForRouteSlug(slug);
}

export function playgroundPathForFeature(feature: string, basePath: string): string {
  return `${normalizeBasePath(basePath)}${routeSlugForFeature(feature)}/`;
}

function normalizeBasePath(basePath: string): string {
  const withLeadingSlash = basePath.startsWith('/') ? basePath : `/${basePath}`;
  return withLeadingSlash.endsWith('/') ? withLeadingSlash : `${withLeadingSlash}/`;
}
