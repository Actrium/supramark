/**
 * Feature 配置系统测试
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

describe('Feature 配置系统', () => {
  beforeEach(() => {
    // 清空注册表
    FeatureRegistry.clear();
  });

  describe('createConfigFromRegistry', () => {
    it('应该从空的 Registry 生成空配置', () => {
      const config = createConfigFromRegistry();

      expect(config.features).toEqual([]);
      expect(config.options).toEqual({
        cache: true,
        strict: false,
      });
    });

    it('应该从 Registry 生成启用所有 Feature 的配置', () => {
      // 注册两个 Features
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

    it('应该支持默认禁用所有 Features', () => {
      FeatureRegistry.register(createTestFeature('@test/feature-a', 'test-a'));

      const config = createConfigFromRegistry(false);

      expect(config.features).toHaveLength(1);
      expect(config.features?.[0].enabled).toBe(false);
    });
  });

  describe('getEnabledFeatureIds', () => {
    it('应该返回空数组当没有配置时', () => {
      const config: SupramarkConfig = {};
      const ids = getEnabledFeatureIds(config);

      expect(ids).toEqual([]);
    });

    it('应该返回所有启用的 Feature IDs', () => {
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
    it('应该返回空数组当没有配置时', () => {
      const config: SupramarkConfig = {};
      const features = getEnabledFeatures(config);

      expect(features).toEqual([]);
    });

    it('应该返回所有启用的 Feature 定义', () => {
      // 注册 Features
      const featureA = createTestFeature('@test/feature-a', 'test-a');
      const featureB = createTestFeature('@test/feature-b', 'test-b');

      FeatureRegistry.register(featureA);
      FeatureRegistry.register(featureB);

      // 配置：只启用 A
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

    it('应该过滤掉未注册的 Features', () => {
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
    it('应该返回 false 当 Feature 未配置时', () => {
      const config: SupramarkConfig = {};

      expect(isFeatureEnabled(config, '@test/feature-a')).toBe(false);
    });

    it('应该返回正确的启用状态', () => {
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
    it('应该返回空对象当 Feature 未配置时', () => {
      const config: SupramarkConfig = {};

      expect(getFeatureOptions(config, '@test/feature-a')).toEqual({});
    });

    it('应该返回空对象当 Feature 没有 options 时', () => {
      const config: SupramarkConfig = {
        features: [{ id: '@test/feature-a', enabled: true }],
      };

      expect(getFeatureOptions(config, '@test/feature-a')).toEqual({});
    });

    it('应该返回 Feature 的配置选项', () => {
      const options = { theme: 'dark', showLineNumbers: true };
      const config: SupramarkConfig = {
        features: [{ id: '@test/feature-a', enabled: true, options }],
      };

      expect(getFeatureOptions(config, '@test/feature-a')).toEqual(options);
    });
  });

  describe('diagram family helpers', () => {
    it('应该将内置 diagram engine 映射到约定的 family', () => {
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

    it('应该返回对应 family 的 feature ids', () => {
      expect(getDiagramFeatureIdsForEngine('mermaid')).toEqual(['@supramark/feature-mermaid']);
      expect(getDiagramFeatureIdsForEngine('plantuml')).toEqual([]);
      expect(getDiagramFeatureIdsForEngine('chart')).toEqual([
        '@supramark/feature-diagram-vega-lite',
      ]);
      expect(getDiagramFeatureIdsForEngine('graphviz')).toEqual(['@supramark/feature-diagram-dot']);
      expect(getDiagramFeatureIdsForEngine('unknown')).toEqual([]);
    });

    it('应该在 feature group 未出现在配置中时默认启用', () => {
      const config: SupramarkConfig = {
        features: [{ id: '@supramark/feature-math', enabled: true }],
      };

      expect(isFeatureGroupEnabled(config, ['@supramark/feature-mermaid'])).toBe(true);
      expect(isDiagramFeatureEnabled(config, 'mermaid')).toBe(true);
    });

    it('应该在 feature group 被显式禁用时返回 false', () => {
      const config: SupramarkConfig = {
        features: [{ id: '@supramark/feature-mermaid', enabled: false }],
      };

      expect(isFeatureGroupEnabled(config, ['@supramark/feature-mermaid'])).toBe(false);
      expect(isDiagramFeatureEnabled(config, 'mermaid')).toBe(false);
    });

    it('应该让 graphviz family 共享同一开关', () => {
      const config: SupramarkConfig = {
        features: [{ id: '@supramark/feature-diagram-dot', enabled: false }],
      };

      expect(isDiagramFeatureEnabled(config, 'dot')).toBe(false);
      expect(isDiagramFeatureEnabled(config, 'graphviz')).toBe(false);
    });
  });

  describe('集成测试', () => {
    it('应该支持完整的配置流程', () => {
      // 1. 注册 Features
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

      // 2. 生成默认配置
      const defaultConfig = createConfigFromRegistry(true);
      expect(defaultConfig.features).toHaveLength(2);

      // 3. 用户自定义配置
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

      // 4. 查询启用状态
      expect(isFeatureEnabled(userConfig, '@test/feature-mermaid')).toBe(true);
      expect(isFeatureEnabled(userConfig, '@test/feature-vega-lite')).toBe(true);

      // 5. 获取配置选项
      const vegaOptions = getFeatureOptions(userConfig, '@test/feature-vega-lite');
      expect(vegaOptions).toEqual({ theme: 'dark' });

      // 6. 获取启用的 Features
      const enabledFeatures = getEnabledFeatures(userConfig);
      expect(enabledFeatures).toHaveLength(2);
      expect(enabledFeatures[0].metadata.id).toBe('@test/feature-mermaid');
      expect(enabledFeatures[1].metadata.id).toBe('@test/feature-vega-lite');
    });
  });

  describe('createCodeHighlightCompileManifest', () => {
    it('应该汇总已启用 Feature 的高亮编译资产', () => {
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

    it('应该把 full preset 合并为全量标记', () => {
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
