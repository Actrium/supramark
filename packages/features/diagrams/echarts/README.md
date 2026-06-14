# Diagram (ECharts) Feature

ECharts 图表支持 Feature。

- 语法：使用围栏代码块：

````markdown
```echarts
{ ... ECharts option JSON ... }
```
````

- AST：统一解析为 `diagram` 节点，`engine` 为 `echarts`。
- 渲染：
  - **Web / RN**：`@supramark/engines/echarts` 加载 upstream JS `echarts` 库，通过 SVG SSR 输出 SVG 字符串；RN 端交给 `react-native-svg` 显示。
