import { describe, expect, it } from 'bun:test';
import {
  DEFAULT_PLAYGROUND_FEATURE,
  PLAYGROUND_ROUTES,
  featureFromPlaygroundPath,
  playgroundPathForFeature,
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
