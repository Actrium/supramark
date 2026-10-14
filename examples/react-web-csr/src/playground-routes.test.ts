import { describe, expect, it } from 'bun:test';
import {
  DEFAULT_PLAYGROUND_FEATURE,
  PLAYGROUND_EXCLUDED_FEATURES,
  PLAYGROUND_ROUTES,
  featureFromPlaygroundPath,
  playgroundPathForFeature,
  playgroundRouteStatus,
  routeSlugForFeature,
} from './playground-routes';

describe('playground routes', () => {
  it('uses unique feature names and route slugs', () => {
    expect(new Set(PLAYGROUND_ROUTES.map(route => route.feature)).size).toBe(
      PLAYGROUND_ROUTES.length
    );
    expect(new Set(PLAYGROUND_ROUTES.map(route => route.slug)).size).toBe(PLAYGROUND_ROUTES.length);
  });

  it('keeps the root playground focused on Markdown', () => {
    expect(DEFAULT_PLAYGROUND_FEATURE).toBe('core-markdown');
  });

  it('maps public diagram slugs to internal feature ids', () => {
    expect(routeSlugForFeature('diagram-dot')).toBe('dot');
    expect(routeSlugForFeature('diagram-echarts')).toBe('echarts');
    expect(routeSlugForFeature('diagram-vega-lite')).toBe('vega-lite');
  });

  it('builds paths under the deployed GitHub Pages base', () => {
    expect(playgroundPathForFeature('mermaid', '/supramark/playground/')).toBe(
      '/supramark/playground/mermaid/'
    );
  });

  it('resolves direct and refreshed feature paths', () => {
    expect(featureFromPlaygroundPath('/supramark/playground/d2/', '/supramark/playground/')).toBe(
      'd2'
    );
    expect(
      featureFromPlaygroundPath(
        '/supramark/playground/vega-lite/index.html',
        '/supramark/playground/'
      )
    ).toBe('diagram-vega-lite');
    expect(
      featureFromPlaygroundPath('/supramark/playground/', '/supramark/playground/')
    ).toBeUndefined();
  });
});

// The full routing-vs-discovery agreement check needs the filesystem and lives
// in scripts/check-playground-routes.ts (run by `bun run quality`). These cases
// cover the invariants of the two tables themselves.
describe('playground route coverage', () => {
  it('lists each excluded feature once', () => {
    const features = PLAYGROUND_EXCLUDED_FEATURES.map(entry => entry.feature);
    expect(new Set(features).size).toBe(features.length);
  });

  it('never routes a feature it also excludes', () => {
    const routed = new Set<string>(PLAYGROUND_ROUTES.map(route => route.feature));
    const overlap = PLAYGROUND_EXCLUDED_FEATURES.filter(entry => routed.has(entry.feature));
    expect(overlap).toEqual([]);
  });

  it('gives every exclusion a reason to show the preview CLI', () => {
    for (const entry of PLAYGROUND_EXCLUDED_FEATURES) {
      expect(entry.reason.trim().length).toBeGreaterThan(0);
    }
  });

  it('reports whether a feature is routed, excluded, or unknown', () => {
    expect(playgroundRouteStatus('mermaid')).toEqual({ kind: 'routed', slug: 'mermaid' });
    expect(playgroundRouteStatus('card-vison')).toEqual({
      kind: 'excluded',
      reason: expect.any(String),
    });
    expect(playgroundRouteStatus('not-a-feature')).toEqual({ kind: 'unregistered' });
  });

  it('explains the exclusion when a route is built for an unroutable feature', () => {
    expect(() => routeSlugForFeature('card-vison')).toThrow(/no playground page/i);
    expect(() => routeSlugForFeature('not-a-feature')).toThrow(/No playground route registered/);
  });
});
