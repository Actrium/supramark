# React Native 示例

这是 supramark 的 React Native 演示程序，用于在真机 / 模拟器上体验各类 Markdown 语法与图表插件的渲染效果。

主页包括一个简易的「示例目录」，目前内置若干示例：

- 基础文本 / 段落；
- 标题层级（H1-H4）；
- 列表与任务列表；
- 代码块（多行代码展示）；
- 数学公式（Math / LaTeX）；
- 脚注、定义列表、Admonition、Emoji 等；
- 图表示例：使用 ` ```mermaid` / ` ```plantuml` / ` ```vega-lite` / ` ```echarts` 等代码块生成 `diagram` 节点；ECharts 与 Vega-Lite 走 JS SVG-string engine，Mermaid / PlantUML / D2 走 native FFI adapter，最终都交给 `react-native-svg` 显示。

选择左侧的某一项，可以在右侧看到：

- 对应示例的 Markdown 源文本；
- 使用 `<Supramark />` 渲染后的实际效果。

## 运行方式

在仓库根目录：

```bash
cd examples/react-native
npm run start      # 如有需要会自动执行根目录 npm install
```

然后根据 Expo 提示，在 iOS / Android 模拟器或真机上运行。

## 快速开始

```bash
cd examples/react-native
bun install
bun run start
```

## Supramark 依赖

- `@supramark/core` - workspace:\*
- `@supramark/feature-admonition` - workspace:\*
- `@supramark/feature-core-markdown` - workspace:\*
- `@supramark/feature-definition-list` - workspace:\*
- `@supramark/feature-diagram-dot` - workspace:\*
- `@supramark/feature-diagram-echarts` - workspace:\*
- `@supramark/feature-diagram-vega-lite` - workspace:\*
- `@supramark/feature-emoji` - workspace:\*
- `@supramark/feature-footnote` - workspace:\*
- `@supramark/feature-gfm` - workspace:\*
- `@supramark/feature-math` - workspace:\*
- `@supramark/feature-html-page` - workspace:\*
- `@supramark/feature-map` - workspace:\*
- `@supramark/rn` - workspace:\*

## 项目结构

```
examples/react-native/
├── src/
├── public/
├── package.json
└── README.md
```

## 相关资源

- [快速开始](/guide/getting-started)
- [API 参考](/api/)
- [其他示例](/examples/)

---

_此文档由 scripts/doc-gen-example.ts 自动生成_
