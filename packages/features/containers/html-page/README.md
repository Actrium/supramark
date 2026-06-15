# HTML Page Feature

使用 `:::html ... :::` 容器声明一个独立的 HTML 页面卡片节点，由宿主负责在隔离页面、弹窗或外部浏览器中打开和渲染页面。

## 功能特性

- 在解析阶段将 `:::html` 容器转换为 `html_page` AST 节点，并保留完整 HTML 字符串；
- 允许通过 `title`/`url`/`meta` 字段定义卡片展示信息与宿主所需的附加数据；
- RN / Web 默认提供占位渲染，具体打开行为交由宿主应用托管（例如独立页面、弹窗或外部浏览器等）。

## 语法

```markdown
:::html
<!doctype html>
<html>
  <head>
    <title>示例页面</title>
  </head>
  <body>
    <h1>Hello HTML Page</h1>
    <p>这里可以放任何需要隔离的内容。</p>
  </body>
</html>
:::
```

## AST 结构

```ts
interface SupramarkHtmlPageNode {
  type: 'html_page';
  html: string;
  title?: string;
  url?: string;
  meta?: Record<string, unknown>;
}
```

## 平台支持

- [x] React Native（占位卡片 + onOpenHtmlPage 回调）
- [x] Web（占位卡片 + 宿主回调）

## 开发状态

- [x] AST 定义
- [x] 解析器实现
- [x] RN 渲染器占位
- [x] Web 渲染器占位
- [ ] 端到端示例完善

## 示例

更多 Markdown 示例见 `src/examples.ts`，并可在 `examples/react-native`、`examples/react-web-csr` 中体验。

## 相关文档

- `docs/FEATURE_INTERFACE_ENHANCEMENT.md`
- `docs/FEATURE_QUALITY_ASSURANCE.md`
