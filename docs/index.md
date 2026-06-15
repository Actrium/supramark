---
layout: home
hero:
  name: Supramark
  text: 面向多端宿主的 Markdown 基础设施
  tagline: 以 Feature 为产品单元，把 Markdown 扩展、图表引擎、语义 AST 和跨平台渲染收束到一套可组合接口。
  image:
    src: /brand/supramark-mark.svg
    alt: Supramark brand mark
  actions:
    - theme: brand
      text: 快速开始
      link: /guide/getting-started
    - theme: alt
      text: 设计系统
      link: /guide/design-system
    - theme: alt
      text: GitHub
      link: https://github.com/kookyleo/supramark
features:
  - title: Feature-first 扩展
    details: 核心 Markdown、GFM、数学公式、脚注、定义列表、Admonition、Emoji 等能力以独立 Feature 组合。
  - title: 统一图表引擎
    details: mermaid / plantuml / d2 / graphviz 等图表统一收口到 engines 层，渲染器只消费标准 SVG 输出。
  - title: 语义 AST
    details: AST v2 将 source map、协作批注、diagram meta 和语义节点作为长期扩展目标。
  - title: 多端渲染
    details: React Native、Web 与小程序宿主共享核心协议，按平台注入 renderer 和 diagram runtime。
  - title: 文档自动化
    details: Feature metadata、examples、testing 和 documentation 字段驱动文档生成，减少手写漂移。
  - title: 可扩展视觉系统
    details: docs 主题基于 Ultraviolet token 体系构建，后续页面、组件和示例都能复用同一套语义变量。
---
