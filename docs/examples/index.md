# 示例项目

Supramark 的 examples 分成两类：一类是可以直接在文档站浏览的 Feature 示例库，另一类是需要在本地运行的完整宿主项目。

## 站内示例

### [实时 Feature Preview](/preview/?feature=mermaid)

首页挂载的是同一套可交互预览页面：左侧编辑 Markdown，右侧查看实际渲染效果，页面内可以继续切换 Feature 和示例。

本地调试时可以用命令直接打开：

```bash
bun run feature:preview:web
bun run feature:preview:web mermaid
bun run feature:preview:web d2
bun run feature:preview:web plantuml
bun run feature:preview:web diagram-dot
bun run feature:preview:web diagram-echarts
bun run feature:preview:web diagram-vega-lite
```

### [Feature 示例库](./gallery)

从各个 Feature 包的 `src/examples.ts` 自动聚合，展示当前内置语法、容器和图表能力的 Markdown 输入。

## 可运行项目

### [React Web CSR 示例](./react-web-csr)

Vite + React 的浏览器端实时 Markdown 编辑器示例。

### [React Native 示例](./react-native)

Expo / React Native 环境下的 Markdown 与图表渲染示例。

### [构建配置示例](./config-examples)

在 Vite / Webpack 等构建工具中集成 Supramark 的配置参考。

## 运行示例

所有示例项目都可以直接克隆并运行：

```bash
git clone https://github.com/Actrium/supramark.git
cd supramark
bun install
cd examples/react-web-csr
bun run dev
```

## 相关资源

- [快速开始](/guide/getting-started)
- [API 参考](/api/)
- [Features](/features/)
