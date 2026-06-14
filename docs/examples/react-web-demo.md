# React Web 演示程序：examples/react-web

`examples/react-web` 是 supramark 提供的 React Web 演示程序，展示如何在 React 应用中使用 `<Supramark />` 组件渲染 Markdown，并通过服务端渲染（SSR）生成完整的 HTML 页面。

## 功能概览

- 采用「目录 + 示例详情」的两页结构：
  - **菜单页**：显示所有示例的列表，点击进入详情页；
  - **详情页**：显示选中示例的 Markdown 源文本和 React 组件渲染效果，可返回菜单页。
- 每个示例包含：
  - 对应的 Markdown 源文本；
  - 使用 `<Supramark />` React 组件渲染后的效果。
- 当前内置示例包括：
  - 基础文本 / 段落；
  - 标题层级（H1-H4）；
  - 列表与任务列表；
  - 代码块（多行代码）；
  - 图表示例：使用 ` ```mermaid` 等代码块生成 `diagram` 节点，并通过 `@supramark/engines` 渲染为 SVG。
- 所有示例数据来自 `examples/demos.mjs`，从各个 Feature 包中聚合示例数据。

## 运行方式

在仓库根目录：

```bash
cd examples/react-web
npm run start      # 默认在 http://localhost:3001 启动
```

或指定端口：

```bash
PORT=8080 npm run start
```

启动后，在浏览器中访问 http://localhost:3001（或指定的端口），即可看到示例列表。

## 技术实现

- 使用 Node.js 的 `http` 模块创建简单的 HTTP 服务器；
- 使用 React 的 `createElement` API 构建组件树（无需 JSX）；
- 使用 `react-dom/server` 的 `renderToString()` 进行服务端渲染（SSR）；
- 在服务端解析 URL 参数来决定展示哪个页面（菜单页或详情页）；
- 使用 `@supramark/web` 的 `parse()` 预先解析 Markdown 为 AST；
- 将 `<Supramark markdown={...} ast={...} />` 组件渲染为 HTML；
- 使用 `@supramark/web` 的 SVG 渲染管线展示图表；具体图表库只由 `@supramark/engines` 管理。

## 与核心库的关系

- 示例程序依赖：
  - `@supramark/core`：提供 AST 与解析能力；
  - `@supramark/web`：提供 `<Supramark />` React 组件与 `parse()` 等工具。

这个示例适合用于：

- 了解如何在 React 应用中集成 Supramark；
- 学习服务端渲染（SSR）Markdown 内容的最佳实践；
- 作为 React 单页应用（SPA）或 Next.js 等框架的参考实现。

## 与 React Native 示例的关系

- `examples/react-native`：使用 `@supramark/rn` + `@supramark/engines` 在 React Native 中渲染 Markdown 和图表；
- `examples/react-web`：使用 `@supramark/web` 在浏览器环境中渲染 Markdown 和图表：
  - 两者共享 supramark AST 与插件体系；
  - 图表渲染细节由 `@supramark/engines` 统一封装，对业务代码透明。
