# React Native 演示程序：examples/react-native

`examples/react-native` 是 supramark 自带的 React Native 演示程序，用于在真机 / 模拟器上体验各类 Markdown 语法与图表插件的渲染效果。

## 功能概览

- 采用「目录 + 示例详情」的两页结构：
  - **菜单页**：显示所有示例的列表，点击进入详情页；
  - **详情页**：显示选中示例的 Markdown 源文本和渲染效果，可返回菜单页。
- 每个示例包含：
  - 对应的 Markdown 源文本；
  - 使用 `<Supramark />` 渲染后的实际效果；
- 当前内置示例包括：
  - 基础文本 / 段落；
  - 标题层级（H1-H4）；
  - 列表与任务列表；
  - 代码块（多行代码）；
  - 数学公式（Math / LaTeX）；
  - 脚注、定义列表、Admonition、Emoji 等；
  - 图表示例：使用 ` ```mermaid` / ` ```plantuml` / ` ```vega-lite` / ` ```echarts` 等代码块生成 `diagram` 节点，在 RN 中通过 `@supramark/engines` 输出 SVG，再由 `<Supramark />` 展示最终图像。
- 所有示例数据来自 `examples/demos.js（从各 Feature 包聚合）`，与 Web 示例共享。

## 运行方式

在仓库根目录：

```bash
cd examples/react-native
npm run start      # 如有需要会自动执行根目录 npm install
```

脚本会在缺少根目录依赖时自动执行一次依赖安装，随后启动 Expo DevTools。根据 Expo 提示，在 iOS / Android 模拟器或真机上运行。

## 与核心库的关系

- 示例程序依赖：
  - `@supramark/core`：提供 AST v2 类型与 `parse(source)` 解析 facade；
  - `@supramark/rn`：将 supramark AST 渲染为 React Native 组件；
  - `@supramark/engines`：为示例中的 `diagram` 节点输出 SVG，RN 端通过 native FFI adapter 或 JS SVG-string engine 渲染。

随着 supramark 支持的语法和插件增多，可以持续往示例目录中添加新的条目，用于验证和演示新的能力。
