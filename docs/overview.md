# 概览：supramark 作为集成 / 封装库

supramark 的定位是「自有 Markdown AST v2 解析内核 + 多平台渲染集成库」：

- 在 **解析层**，使用 Rust `supramark-markdown` 输出统一、带 source map 的 AST v2；
- 在 Web / Node / React Native 场景下，保持同一个 `parse(source) -> AST v2` 公共合同；
- 在 **渲染层 (React Native)**，提供一套可扩展的组件映射与插件渲染机制；
- 在 **图表层**，通过 `@supramark/engines` 统一输出 SVG，Web / RN 渲染器只消费结果。

当前分层设计（草案）：

- `@supramark/core`
  - 定义 AST v2 与 AST 后处理插件接口；
  - 通过 `supramark-markdown` 生成 canonical AST v2；
  - 对外只暴露统一 AST 与 `parse` facade，parser rule 不作为 public API 暴露；
  - 提供「语法插件」：GFM、math、diagram、admonition 等。
- `@supramark/rn`
  - 提供 `<Supramark />` 组件，把 supramark AST 渲染为 React Native 组件树；
  - 提供各插件对应的默认渲染器（可覆盖）。
- `@supramark/engines`
  - 图表 / 公式渲染统一出口；
  - 对外暴露 `render({ engine, code }) => Promise<{ format, payload }>`，以 SVG 为主；
  - RN 端通过 native FFI adapter 或 JS SVG-string engine 生成 SVG，再由 `@supramark/rn` 展示。

后续文档会在各插件说明中标明「底层依赖库」与「可替换选项」。
