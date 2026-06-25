import type {
  SupramarkContainerNode,
  SupramarkNode,
  SupramarkRootNode,
  FeatureConfigWithOptions,
  SupramarkFeature,
} from '@supramark/core';
import { makeFeatureConfigHelpers, FeatureRegistry } from '@supramark/core';
import { mapExamples } from './examples.js';

interface MapContainerData {
  center: [number, number];
  zoom?: number;
  markers?: Array<{
    lat: number;
    lng: number;
    label?: string;
    id?: string;
    data?: Record<string, unknown>;
  }>;
  meta?: Record<string, unknown>;
}

export type SupramarkMapContainerNode = SupramarkContainerNode & {
  name: 'map';
  data: MapContainerData;
};

const isMapContainer = (node: SupramarkNode): node is SupramarkMapContainerNode => {
  return node.type === 'container' && node.name === 'map';
};

/**
 * Map Feature
 *
 * 为 Supramark 提供地图卡片占位支持。
 *
 * - 将 :::map 容器解析为 `map` 节点；
 * - 携带中心点、缩放级别和标记点数据；
 * - 具体地图实现（Apple Maps, Google Maps, Mapbox 等）由宿主决定。
 */
export const mapFeature: SupramarkFeature<SupramarkMapContainerNode> = {
  metadata: {
    id: '@supramark/feature-map',
    name: 'Map',
    version: '0.1.0',
    author: 'Supramark Team',
    description: '使用 :::map 容器定义地图占位节点，支持经纬度中心点和标记。',
    license: 'Apache-2.0',
    tags: ['map', 'location', 'geo', 'container'],
    syntaxFamily: 'container',
  },

  syntax: {
    ast: {
      type: 'container',
      selector: isMapContainer,
      interface: {
        required: ['type', 'name', 'data', 'children'],
        optional: ['params'],
        fields: {
          type: {
            type: 'string',
            description: '节点类型，固定为 "container"。',
          },
          name: {
            type: 'string',
            description: '容器名称，固定为 "map"。',
          },
          data: {
            type: 'object',
            description: '地图数据，包含中心点 center、缩放 zoom 和标记数组 markers。',
          },
          children: {
            type: 'array',
            description: '子节点列表，对于 map 容器通常为空。',
          },
        },
      },
      constraints: {
        allowedParents: ['root', 'paragraph', 'list_item'],
        allowedChildren: [],
      },
      examples: [
        {
          type: 'container',
          name: 'map',
          data: {
            center: [39.9042, 116.4074],
            zoom: 12,
            markers: [{ lat: 39.9042, lng: 116.4074 }],
          },
          children: [],
        } as SupramarkMapContainerNode,
      ],
    },
  },

  renderers: {
    web: {
      platform: 'web',
      infrastructure: {
        needsClientScript: false,
        needsWorker: false,
        needsCache: false,
      },
      dependencies: [],
    },
    rn: {
      platform: 'rn',
      infrastructure: {
        needsWorker: false,
        needsCache: false,
      },
      dependencies: [],
    },
  },

  examples: mapExamples,

  testing: {
    syntaxTests: {
      cases: [
        {
          name: '解析 :::map 容器',
          input: ':::map\ncenter: [0, 0]\n:::',
          expected: {
            type: 'container',
            name: 'map',
          } as SupramarkMapContainerNode,
          options: { typeOnly: true },
        },
      ],
    },
    renderTests: {
      web: [
        {
          name: 'Web 渲染占位图层',
          input: {
            type: 'container',
            name: 'map',
            mode: 'opaque',
            data: { center: [0, 0] },
            children: [],
          } as SupramarkMapContainerNode,
          expected: (output: unknown) => output !== null,
        },
      ],
    },
    integrationTests: {
      cases: [
        {
          name: 'Map 集成测试',
          input: ':::map\ncenter: [0, 0]\n:::',
          validate: (result: unknown) =>
            ((result as SupramarkRootNode).children?.[0] as SupramarkContainerNode | undefined)
              ?.name === 'map',
          platforms: ['web', 'rn'],
        },
      ],
    },
    coverageRequirements: {
      statements: 80,
      branches: 80,
      functions: 80,
      lines: 80,
    },
  },

  documentation: {
    readme: `
# Map Feature

使用 \`:::map\` 容器定义地图占位节点。

支持通过 YAML 或 JSON 配置经纬度、缩放级别和标记点。宿主应用可以根据这些数据渲染交互式地图。
    `.trim(),
    api: {
      interfaces: [
        {
          name: 'MapFeatureOptions',
          description: 'Map 配置选项',
          fields: [
            {
              name: 'provider',
              type: "'apple' | 'google' | 'mapbox' | 'custom'",
              description: '地图服务商标识',
              required: false,
            },
            {
              name: 'defaultZoom',
              type: 'number',
              description: '默认缩放级别',
              required: false,
              default: '12',
            },
          ],
        },
      ],
      functions: [
        {
          name: 'createMapFeatureConfig',
          description: '创建 Map 特性配置',
          parameters: [
            { name: 'enabled', type: 'boolean', description: '是否启用' },
            { name: 'options', type: 'MapFeatureOptions', description: '选项', optional: true },
          ],
          returns: 'MapFeatureConfig',
        },
      ],
      types: [
        {
          name: 'MapFeatureConfig',
          description: '配置类型定义',
          definition: 'type MapFeatureConfig = FeatureConfigWithOptions<MapFeatureOptions>',
        },
      ],
    },
    bestPractices: [
      '在文档中明确标注经纬度顺序。',
      '利用 provider 字段在多平台环境下切换地图内核。',
    ],
  },
};

export interface MapFeatureOptions {
  provider?: 'apple' | 'google' | 'mapbox' | 'custom';
  defaultZoom?: number;
}

export type MapFeatureConfig = FeatureConfigWithOptions<MapFeatureOptions>;

const mapHelpers = makeFeatureConfigHelpers<MapFeatureOptions>('@supramark/feature-map');
export const createMapFeatureConfig = mapHelpers.create;
export const getMapFeatureOptions = mapHelpers.getOptions;

FeatureRegistry.register(mapFeature);
