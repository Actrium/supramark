# 核心概念

理解 Supramark 的核心设计理念和概念。

## 什么是 Supramark？

Supramark 是一个跨平台的 Markdown 渲染引擎，专为 React Native 和 Web 应用设计。它提供：

- 🎯 **统一的 AST** - 跨平台一致的抽象语法树
- 🧩 **模块化 Features** - 按需启用的功能扩展
- 📱 **原生渲染** - React Native 直接消费 SVG / native adapter 输出
- 🌐 **Web 支持** - React 组件开箱即用

## AST（抽象语法树）

### 什么是 AST？

AST 是 Markdown 文档的结构化表示，将文本解析为树状数据结构。

```typescript
// Markdown 文本
const markdown = `# Hello World\n\nThis is **bold** text.`;

// 解析为 AST
const ast = parse(markdown);
// {
//   type: 'root',
//   children: [
//     { type: 'heading', depth: 1, children: [...] },
//     { type: 'paragraph', children: [...] }
//   ]
// }
```

### Supramark 节点类型

Supramark 定义了 80+ 种节点类型，涵盖：

- **基础节点**：paragraph, heading, list, code
- **格式节点**：strong, emphasis, link, image
- **扩展节点**：table, math, diagram, admonition
- **自定义节点**：通过 Features 扩展

完整的节点类型定义请查看 [TypeDoc API 文档](/typedoc/)。

## Feature 系统

### Feature 是什么？

Feature 是 Supramark 的功能扩展单元，每个 Feature 提供：

1. **语法定义** - 如何解析特定的 Markdown 语法
2. **AST 节点** - 该 Feature 引入的节点类型
3. **渲染器** - 如何在不同平台渲染节点

### Feature 接口

```typescript
interface SupramarkFeature {
  metadata: {
    id: string;
    name: string;
    description: string;
    version: string;
  };

  syntax?: {
    // 语法定义
  };

  renderers?: {
    web?: WebRenderer;
    rn?: RNRenderer;
  };

  documentation?: {
    // 文档和示例
  };
}
```

### 内置 Features

Supramark 提供以下内置 Features：

- **@supramark/feature-core-markdown** - 标准 Markdown
- **@supramark/feature-gfm** - GitHub Flavored Markdown
- **@supramark/feature-math** - LaTeX 数学公式
- **@supramark/feature-admonition** - 提示框
- **@supramark/feature-emoji** - Emoji 短代码
- **@supramark/feature-footnote** - 脚注
- **@supramark/feature-definition-list** - 定义列表

查看完整列表：[Features 文档](/features/)

## 配置系统

### 基础配置

```typescript
import { Supramark } from '@supramark/web'
import { mathFeature, gfmFeature } from '@supramark/feature-math'

const config = {
  features: [
    mathFeature,
    gfmFeature,
  ]
}

<Supramark markdown={markdown} config={config} />
```

### Feature 配置

某些 Features 支持配置选项：

```typescript
import { createMathFeatureConfig } from '@supramark/feature-math';

const config = {
  features: [
    createMathFeatureConfig(true, {
      engine: 'katex', // 或 'mathjax'
      displayMode: true,
    }),
  ],
};
```

## 渲染流程

### 1. 解析阶段

Markdown 文本 → AST

```typescript
import { parse } from '@supramark/core';

const ast = await parse(markdown, { config });
```

### 2. 渲染阶段

AST → React 组件树

```typescript
// Web
import { Supramark } from '@supramark/web'
<Supramark markdown={markdown} />

// React Native
import { Supramark } from '@supramark/rn'
<Supramark markdown={markdown} />
```

### 3. 展示阶段

React 组件树 → 最终输出

- Web: DOM 元素
- React Native: 原生组件（Text, View, Image 等）

## 跨平台策略

### 一致的 API

Web 和 RN 使用相同的 API：

```typescript
// 两个平台完全相同的代码
<Supramark
  markdown={markdown}
  config={config}
  style={customStyle}
/>
```

### 平台特定渲染

虽然 API 一致，但渲染实现不同：

- **Web**: 渲染为 HTML 元素 (`<p>`, `<h1>`, `<img>`)
- **RN**: 渲染为原生组件 (`<Text>`, `<View>`, `<Image>`)

### 重度依赖处理

某些功能依赖较重的渲染引擎（如图表渲染）：

- **Web**: `@supramark/engines/web` 输出 SVG
- **RN**: `@supramark/engines/rn` 通过 native FFI adapter 或 JS SVG-string engine 输出 SVG

## 扩展性

### 自定义 Feature

你可以创建自己的 Feature：

```typescript
import { defineFeature } from '@supramark/core';

const myFeature = defineFeature({
  metadata: {
    id: 'my-custom-feature',
    name: 'My Custom Feature',
    version: '1.0.0',
  },

  syntax: {
    // 定义解析规则
  },

  renderers: {
    web: MyWebRenderer,
    rn: MyRNRenderer,
  },
});
```

详细指南：[自定义 Feature 开发](/guide/custom-features)

### 自定义样式

覆盖默认样式：

```typescript
<Supramark
  markdown={markdown}
  style={{
    heading1: { fontSize: 32, fontWeight: 'bold' },
    paragraph: { lineHeight: 1.6 },
    link: { color: '#007AFF' }
  }}
/>
```

## 性能优化

### 解析缓存

Supramark 自动缓存解析结果：

```typescript
// 第一次解析
const ast1 = await parse(markdown); // 较慢

// 相同内容第二次解析
const ast2 = await parse(markdown); // 从缓存获取
```

### 增量渲染

只重新渲染变化的部分：

```typescript
// React 自动处理
<Supramark markdown={markdown} />
```

### 图表延迟加载

图表只在需要时渲染：

```typescript
<Supramark
  markdown={markdown}
  config={{
    lazyLoadDiagrams: true  // 默认启用
  }}
/>
```

## 最佳实践

### 1. 按需引入 Features

只引入你需要的 Features：

```typescript
// ✅ 好 - 只引入需要的
import { gfmFeature } from '@supramark/feature-gfm';

// ❌ 差 - 引入所有
import * as features from '@supramark/features';
```

### 2. 复用配置对象

配置对象可以复用：

```typescript
const config = {
  features: [mathFeature, gfmFeature]
}

// 多个组件使用相同配置
<Supramark markdown={md1} config={config} />
<Supramark markdown={md2} config={config} />
```

### 3. 类型安全

使用 TypeScript 获得类型提示：

```typescript
import type { SupramarkConfig } from '@supramark/core';

const config: SupramarkConfig = {
  features: [
    /* ... */
  ],
};
```

## 下一步

- [架构设计](/guide/architecture) - 深入了解内部架构
- [Features 列表](/features/) - 浏览所有可用功能
- [API 参考](/api/) - 查看完整 API 文档
