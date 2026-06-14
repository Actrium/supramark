import type { ExampleDefinition } from '@supramark/core';

/**
 * Map Feature 示例
 *
 * 使用 :::map 容器定义一张地图卡片：
 * - center: 地图中心点；
 * - zoom: 缩放级别；
 * - marker: 单个输入标记点，AST v2 输出为 data.markers[]。
 */
export const mapExamples: ExampleDefinition[] = [
  {
    name: '基本地图卡片',
    description: '使用 :::map 定义一个带中心点与标记点的地图卡片。',
    markdown: `
# 地图示例（Map）

下面的容器会被识别为一个 map 节点，并在主文档中渲染为「地图卡片」：

:::map
center: [34.05, -118.24]
zoom: 12
marker:
  lat: 34.05
  lng: -118.24
:::
    `.trim(),
  },
  {
    name: '仅指定中心点的地图',
    description: '只提供 center，不指定 marker，用于展示某个区域概览。',
    markdown: `
:::map
center: [31.2304, 121.4737]
zoom: 10
:::
    `.trim(),
  },
];
