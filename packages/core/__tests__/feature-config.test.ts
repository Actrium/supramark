/**
 * Feature configuration system tests
 */

import {
  FeatureRegistry,
  createConfigFromRegistry,
  getEnabledFeatureIds,
  getEnabledFeatures,
  getDiagramFeatureFamily,
  getDiagramFeatureIdsForEngine,
  isFeatureEnabled,
  isDiagramFeatureEnabled,
  isFeatureGroupEnabled,
  getFeatureOptions,
  createCodeHighlightCompileManifest,
  type SupramarkConfig,
  type SupramarkNode,
  type SupramarkFeature,
} from '../src/feature';
import type { SupramarkDiagramNode } from '../src/ast';

function createTestFeature(id: string, type: string): SupramarkFeature<SupramarkNode> {
  return {
    metadata: {
      id,
      name: id,
      version: '1.0.0',
      author: 'Test',
      description: 'Test feature',
      license: 'Apache-2.0',
    },
    syntax: {
      ast: {
        type,
      },
    },
    renderers: {},
    examples: [],
    testing: {},
    documentation: {
      readme: 'Test feature',
    },
  };
}

describe('Feature configuration system', () => {
  beforeEach(() => {
    // Clear the registry
    FeatureRegistry.clear();
  });

  describe('createConfigFromRegistry', () => {
    it('generates an empty config from an empty Registry', () => {
      const config = createConfigFromRegistry();

      expect(config.features).toEqual([]);
      expect(config.options).toEqual({
        cache: true,
        strict: false,
      });
    });

    it('generates a config with all Features enabled from the Registry', () => {
      // Register two Features
      FeatureRegistry.register(createTestFeature('@test/feature-a', 'test-a'));

      FeatureRegistry.register(createTestFeature('@test/feature-b', 'test-b'));

      const config = createConfigFromRegistry(true);

      expect(config.features).toHaveLength(2);
      expect(config.features?.[0]).toEqual({
        id: '@test/feature-a',
        enabled: true,
      });
      expect(config.features?.[1]).toEqual({
        id: '@test/feature-b',
        enabled: true,
      });
    });

    it('supports disabling all Features by default', () => {
      FeatureRegistry.register(createTestFeature('@test/feature-a', 'test-a'));

      const config = createConfigFromRegistry(false);

      expect(config.features).toHaveLength(1);
      expect(config.features?.[0].enabled).toBe(false);
    });
  });

  describe('FeatureRegistry.register', () => {
    it('re-registering an existing id is idempotent and does not throw (HMR re-import)', () => {
      const feature = createTestFeature('@test/feature-a', 'test-a');
      FeatureRegistry.register(feature);

      // Re-registering the same object reference is a silent no-op.
      const spy = jest.spyOn(console, 'warn').mockImplementation(() => {});
      expect(() => FeatureRegistry.register(feature)).not.toThrow();
      expect(spy).not.toHaveBeenCalled();
      spy.mockRestore();

      // A fresh object with the same id (what Vite HMR produces on hot
      // update) replaces the previous entry instead of throwing, and warns
      // so an accidental collision (e.g. duplicated package in node_modules)
      // stays observable in production.
      const refreshed = createTestFeature('@test/feature-a', 'test-a-updated');
      const spy2 = jest.spyOn(console, 'warn').mockImplementation(() => {});
      expect(() => FeatureRegistry.register(refreshed)).not.toThrow();
      expect(spy2).toHaveBeenCalledTimes(1);
      expect(spy2.mock.calls[0][0]).toContain('@test/feature-a');
      spy2.mockRestore();

      expect(FeatureRegistry.get('@test/feature-a')).toBe(refreshed);
      expect(FeatureRegistry.list()).toHaveLength(1);

      // Subsequent HMR-style re-registrations of the same id stay quiet: the
      // duplicate-id warning fires once per session, not once per save.
      const refreshedAgain = createTestFeature('@test/feature-a', 'test-a-updated-2');
      const spy3 = jest.spyOn(console, 'warn').mockImplementation(() => {});
      expect(() => FeatureRegistry.register(refreshedAgain)).not.toThrow();
      expect(spy3).not.toHaveBeenCalled();
      spy3.mockRestore();

      expect(FeatureRegistry.get('@test/feature-a')).toBe(refreshedAgain);
      expect(FeatureRegistry.list()).toHaveLength(1);
    });
  });

  describe('getEnabledFeatureIds', () => {
    it('returns an empty array when there is no config', () => {
      const config: SupramarkConfig = {};
      const ids = getEnabledFeatureIds(config);

      expect(ids).toEqual([]);
    });

    it('returns all enabled Feature IDs', () => {
      const config: SupramarkConfig = {
        features: [
          { id: '@test/feature-a', enabled: true },
          { id: '@test/feature-b', enabled: false },
          { id: '@test/feature-c', enabled: true },
        ],
      };

      const ids = getEnabledFeatureIds(config);

      expect(ids).toEqual(['@test/feature-a', '@test/feature-c']);
    });
  });

  describe('getEnabledFeatures', () => {
    it('returns an empty array when there is no config', () => {
      const config: SupramarkConfig = {};
      const features = getEnabledFeatures(config);

      expect(features).toEqual([]);
    });

    it('returns all enabled Feature definitions', () => {
      // Register Features
      const featureA = createTestFeature('@test/feature-a', 'test-a');
      const featureB = createTestFeature('@test/feature-b', 'test-b');

      FeatureRegistry.register(featureA);
      FeatureRegistry.register(featureB);

      // Config: only A is enabled
      const config: SupramarkConfig = {
        features: [
          { id: '@test/feature-a', enabled: true },
          { id: '@test/feature-b', enabled: false },
        ],
      };

      const enabled = getEnabledFeatures(config);

      expect(enabled).toHaveLength(1);
      expect(enabled[0]).toBe(featureA);
    });

    it('filters out unregistered Features', () => {
      const config: SupramarkConfig = {
        features: [
          { id: '@test/non-existent', enabled: true },
          { id: '@test/another-missing', enabled: true },
        ],
      };

      const enabled = getEnabledFeatures(config);

      expect(enabled).toEqual([]);
    });
  });

  describe('isFeatureEnabled', () => {
    it('returns false when the Feature is not configured', () => {
      const config: SupramarkConfig = {};

      expect(isFeatureEnabled(config, '@test/feature-a')).toBe(false);
    });

    it('returns the correct enabled state', () => {
      const config: SupramarkConfig = {
        features: [
          { id: '@test/feature-a', enabled: true },
          { id: '@test/feature-b', enabled: false },
        ],
      };

      expect(isFeatureEnabled(config, '@test/feature-a')).toBe(true);
      expect(isFeatureEnabled(config, '@test/feature-b')).toBe(false);
      expect(isFeatureEnabled(config, '@test/feature-c')).toBe(false);
    });
  });

  describe('getFeatureOptions', () => {
    it('returns an empty object when the Feature is not configured', () => {
      const config: SupramarkConfig = {};

      expect(getFeatureOptions(config, '@test/feature-a')).toEqual({});
    });

    it('returns an empty object when the Feature has no options', () => {
      const config: SupramarkConfig = {
        features: [{ id: '@test/feature-a', enabled: true }],
      };

      expect(getFeatureOptions(config, '@test/feature-a')).toEqual({});
    });

    it("returns the Feature's configuration options", () => {
      const options = { theme: 'dark', showLineNumbers: true };
      const config: SupramarkConfig = {
        features: [{ id: '@test/feature-a', enabled: true, options }],
      };

      expect(getFeatureOptions(config, '@test/feature-a')).toEqual(options);
    });
  });

  describe('diagram family helpers', () => {
    it('maps built-in diagram engines to their conventional family', () => {
      expect(getDiagramFeatureFamily('mermaid')).toBe('mermaid');
      expect(getDiagramFeatureFamily('plantuml')).toBeNull();
      expect(getDiagramFeatureFamily('vega')).toBe('vega-family');
      expect(getDiagramFeatureFamily('vega-lite')).toBe('vega-family');
      expect(getDiagramFeatureFamily('chart')).toBe('vega-family');
      expect(getDiagramFeatureFamily('chartjs')).toBe('vega-family');
      expect(getDiagramFeatureFamily('echarts')).toBe('echarts');
      expect(getDiagramFeatureFamily('dot')).toBe('graphviz-family');
      expect(getDiagramFeatureFamily('graphviz')).toBe('graphviz-family');
      expect(getDiagramFeatureFamily('custom-engine')).toBeNull();
    });

    it('returns the feature ids for the corresponding family', () => {
      expect(getDiagramFeatureIdsForEngine('mermaid')).toEqual(['@supramark/feature-mermaid']);
      expect(getDiagramFeatureIdsForEngine('plantuml')).toEqual([]);
      expect(getDiagramFeatureIdsForEngine('chart')).toEqual([
        '@supramark/feature-diagram-vega-lite',
      ]);
      expect(getDiagramFeatureIdsForEngine('graphviz')).toEqual(['@supramark/feature-diagram-dot']);
      expect(getDiagramFeatureIdsForEngine('unknown')).toEqual([]);
    });

    it('defaults to enabled when the feature group is absent from the config', () => {
      const config: SupramarkConfig = {
        features: [{ id: '@supramark/feature-math', enabled: true }],
      };

      expect(isFeatureGroupEnabled(config, ['@supramark/feature-mermaid'])).toBe(true);
      expect(isDiagramFeatureEnabled(config, 'mermaid')).toBe(true);
    });

    it('returns false when the feature group is explicitly disabled', () => {
      const config: SupramarkConfig = {
        features: [{ id: '@supramark/feature-mermaid', enabled: false }],
      };

      expect(isFeatureGroupEnabled(config, ['@supramark/feature-mermaid'])).toBe(false);
      expect(isDiagramFeatureEnabled(config, 'mermaid')).toBe(false);
    });

    it('lets the graphviz family share a single switch', () => {
      const config: SupramarkConfig = {
        features: [{ id: '@supramark/feature-diagram-dot', enabled: false }],
      };

      expect(isDiagramFeatureEnabled(config, 'dot')).toBe(false);
      expect(isDiagramFeatureEnabled(config, 'graphviz')).toBe(false);
    });
  });

  describe('integration test', () => {
    it('supports the full configuration flow', () => {
      // 1. Register Features
      FeatureRegistry.register({
        ...createTestFeature('@test/feature-mermaid', 'diagram'),
        syntax: {
          ast: {
            type: 'diagram',
            selector: (node: SupramarkNode) =>
              node.type === 'diagram' && (node as SupramarkDiagramNode).engine === 'mermaid',
          },
        },
      });

      FeatureRegistry.register({
        ...createTestFeature('@test/feature-vega-lite', 'diagram'),
        syntax: {
          ast: {
            type: 'diagram',
            selector: (node: SupramarkNode) =>
              node.type === 'diagram' && (node as SupramarkDiagramNode).engine === 'vega-lite',
          },
        },
      });

      // 2. Generate the default config
      const defaultConfig = createConfigFromRegistry(true);
      expect(defaultConfig.features).toHaveLength(2);

      // 3. User-customized config
      const userConfig: SupramarkConfig = {
        features: [
          { id: '@test/feature-mermaid', enabled: true },
          {
            id: '@test/feature-vega-lite',
            enabled: true,
            options: { theme: 'dark' },
          },
        ],
      };

      // 4. Query the enabled state
      expect(isFeatureEnabled(userConfig, '@test/feature-mermaid')).toBe(true);
      expect(isFeatureEnabled(userConfig, '@test/feature-vega-lite')).toBe(true);

      // 5. Get the configuration options
      const vegaOptions = getFeatureOptions(userConfig, '@test/feature-vega-lite');
      expect(vegaOptions).toEqual({ theme: 'dark' });

      // 6. Get the enabled Features
      const enabledFeatures = getEnabledFeatures(userConfig);
      expect(enabledFeatures).toHaveLength(2);
      expect(enabledFeatures[0].metadata.id).toBe('@test/feature-mermaid');
      expect(enabledFeatures[1].metadata.id).toBe('@test/feature-vega-lite');
    });
  });

  describe('createCodeHighlightCompileManifest', () => {
    it('aggregates the highlight compile assets of enabled Features', () => {
      const base = createTestFeature('@test/feature-code-highlight', 'code-highlight');
      base.compile = { codeHighlight: { runtime: true } };

      const docs = createTestFeature('@test/feature-code-highlight-preset-docs', 'docs');
      docs.compile = {
        codeHighlight: {
          languages: ['TypeScript', 'JSON'],
          languageAliases: { ts: 'TypeScript', json: 'JSON' },
          themes: ['GitHub'],
          defaultThemes: { light: 'GitHub' },
        },
      };

      const manifest = createCodeHighlightCompileManifest([docs, base]);

      expect(manifest).toEqual({
        runtime: true,
        languages: ['JSON', 'TypeScript'],
        languageAliases: { json: 'JSON', ts: 'TypeScript' },
        themes: ['GitHub'],
        defaultThemes: { light: 'GitHub', dark: undefined },
        fullLanguages: false,
        fullThemes: false,
        featureIds: ['@test/feature-code-highlight', '@test/feature-code-highlight-preset-docs'],
      });
    });

    it('merges the full preset into full-set markers', () => {
      const full = createTestFeature('@test/feature-code-highlight-preset-full', 'full');
      full.compile = {
        codeHighlight: {
          languages: ['*'],
          themes: ['*'],
        },
      };

      const manifest = createCodeHighlightCompileManifest([full]);

      expect(manifest.languages).toEqual(['*']);
      expect(manifest.themes).toEqual(['*']);
      expect(manifest.fullLanguages).toBe(true);
      expect(manifest.fullThemes).toBe(true);
    });
  });
});
