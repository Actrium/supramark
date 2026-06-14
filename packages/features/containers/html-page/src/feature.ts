import type {
  SupramarkContainerNode,
  SupramarkNode,
  FeatureConfigWithOptions,
  SupramarkConfig,
  SupramarkFeature,
} from '@supramark/core';
import { makeFeatureConfigHelpers, FeatureRegistry } from '@supramark/core';
import { htmlPageExamples } from './examples.js';

interface HtmlPageContainerData {
  html: string;
  title?: string;
  url?: string;
  meta?: Record<string, unknown>;
}

export type SupramarkHtmlPageContainerNode = SupramarkContainerNode & {
  name: 'html';
  data: HtmlPageContainerData;
};

const isHtmlPageContainer = (node: SupramarkNode): node is SupramarkHtmlPageContainerNode => {
  return node.type === 'container' && (node as SupramarkContainerNode).name === 'html';
};

/**
 * Html Page Feature
 *
 * 为 Supramark 提供独立 HTML 页面节点支持。
 *
 * - 将 :::html 容器解析为 `html_page` AST 节点；
 * - 在主 Markdown 流中以“卡片”方式呈现；
 * - 真正的交互行为（如点击卡片打开独立页面）由宿主实现。
 */
export const htmlPageFeature: SupramarkFeature<SupramarkHtmlPageContainerNode> = {
  metadata: {
    id: '@supramark/feature-html-page',
    name: 'HTML Page',
    version: '0.1.0',
    author: 'Supramark Team',
    description: '使用 :::html 容器定义独立 HTML 页面节点，支持卡片式预览。',
    license: 'Apache-2.0',
    tags: ['html', 'page', 'card', 'container'],
    syntaxFamily: 'container',
  },

  syntax: {
    ast: {
      type: 'container',
      selector: isHtmlPageContainer,
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
            description: '容器名称，固定为 "html"。',
          },
          data: {
            type: 'object',
            description: '容器数据，包含完整 HTML 文本及可选元信息。',
          },
          children: {
            type: 'array',
            description: '子节点列表，对于 html 容器通常为空。',
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
          name: 'html',
          data: {
            html: '<!doctype html><html><body>Hello</body></html>',
            title: 'Example Page',
          },
          children: [],
        } as SupramarkHtmlPageContainerNode,
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

  examples: htmlPageExamples,

  testing: {
    syntaxTests: {
      cases: [
        {
          name: '解析 :::html 容器',
          input: ':::html\n<html><body>Test</body></html>\n:::',
          expected: {
            type: 'container',
            name: 'html',
          } as SupramarkHtmlPageContainerNode,
          options: { typeOnly: true },
        },
      ],
    },
    renderTests: {
      web: [
        {
          name: 'Web 渲染占位卡片',
          input: {
            type: 'container',
            name: 'html',
            data: { html: '...' },
            children: [],
          } as SupramarkHtmlPageContainerNode,
          expected: (output: unknown) => output !== null,
        },
      ],
    },
    integrationTests: {
      cases: [
        {
          name: 'HTML Page 集成测试',
          input: ':::html\ncontent\n:::',
          validate: (result: any) => result.children?.[0]?.name === 'html',
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
# HTML Page Feature

使用 \`:::html\` 容器定义独立 HTML 页面节点。

在宿主应用中，该节点通常被渲染为一个卡片预览。当用户交互时，可以通过宿主提供的回调打开一个独立页面、Modal 或外部浏览器来加载该 HTML。
    `.trim(),
    api: {
      interfaces: [
        {
          name: 'HtmlPageFeatureOptions',
          description: 'HTML Page 配置选项',
          fields: [
            {
              name: 'webOpenMode',
              type: "'window' | 'callback-only'",
              description: 'Web 端打开模式',
              required: false,
              default: "'window'",
            },
          ],
        },
      ],
      functions: [
        {
          name: 'createHtmlPageFeatureConfig',
          description: '创建 HTML Page 特性配置',
          parameters: [
            { name: 'enabled', type: 'boolean', description: '是否启用' },
            { name: 'options', type: 'HtmlPageFeatureOptions', description: '选项', optional: true },
          ],
          returns: 'HtmlPageFeatureConfig',
        },
      ],
      types: [
        {
          name: 'HtmlPageFeatureConfig',
          description: '配置类型定义',
          definition: 'type HtmlPageFeatureConfig = FeatureConfigWithOptions<HtmlPageFeatureOptions>',
        },
      ],
    },
    bestPractices: [
      '对于复杂的第三方 HTML 交互，建议使用此特性进行隔离。',
      '配合宿主的 onOpenHtmlPage 回调实现深度集成。',
    ],
  },
};

export interface HtmlPageFeatureOptions {
  webOpenMode?: 'window' | 'callback-only';
  rnOpenMode?: 'callback-only';
}

export type HtmlPageFeatureConfig = FeatureConfigWithOptions<HtmlPageFeatureOptions>;

const htmlPageHelpers = makeFeatureConfigHelpers<HtmlPageFeatureOptions>(
  '@supramark/feature-html-page'
);
export const createHtmlPageFeatureConfig = htmlPageHelpers.create;
export const getHtmlPageFeatureOptions = htmlPageHelpers.getOptions;

FeatureRegistry.register(htmlPageFeature);
