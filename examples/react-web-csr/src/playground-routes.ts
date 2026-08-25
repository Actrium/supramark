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
  { feature: 'weather', slug: 'weather' },
] as const;

const routeByFeature = new Map<string, string>(
  PLAYGROUND_ROUTES.map(route => [route.feature, route.slug])
);
const featureByRoute = new Map<string, string>(
  PLAYGROUND_ROUTES.map(route => [route.slug, route.feature])
);

export function routeSlugForFeature(feature: string): string {
  const slug = routeByFeature.get(feature);
  if (!slug) {
    throw new Error(`No playground route registered for feature: ${feature}`);
  }
  return slug;
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
