import type { ExampleDefinition } from '@supramark/core';

/**
 * HTML Page Feature 示例
 */
export const htmlPageExamples: ExampleDefinition[] = [
  {
    name: 'HTML Page 卡片',
    description: '使用 :::html 容器定义独立 HTML 页面，在 Markdown 中以卡片形式呈现。',
    markdown: `
# HTML Page 示例

下面的容器会被识别为一个 html_page 节点，并在主文档中渲染为「HTML Page 卡片」：

:::html
<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>HTML Page 示例</title>
    <style>
      body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; padding: 24px; }
      h1 { color: #2f54eb; }
      p { line-height: 1.6; }
    </style>
  </head>
  <body>
    <h1>这是一个独立 HTML 页面</h1>
    <p>它可以包含自己的 CSS 和 JS，在宿主提供的隔离页面或 ShadowDOM 容器中单独运行。</p>
  </body>
</html>
:::
    `.trim(),
  },
];
