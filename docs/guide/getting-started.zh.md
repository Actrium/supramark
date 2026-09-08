# 快速开始

快速上手 Supramark，在 React Native 和 Web 项目中渲染 Markdown。

## 安装

```bash
# 使用 npm
npm install @supramark/core

# Web 端
npm install @supramark/web

# React Native 端
npm install @supramark/rn
```

## Web 端使用

```typescript
import { Supramark } from '@supramark/web'

const markdown = `
# Hello Supramark

这是一个 **强大** 的跨平台 Markdown 渲染引擎。

- 支持标准 Markdown
- 支持 GFM
- 支持数学公式 $E=mc^2$
`

function App() {
  return (
    <Supramark markdown={markdown} />
  )
}
```

## React Native 端使用

```typescript
import { Supramark } from '@supramark/rn'
import { View } from 'react-native'

const markdown = `
# Hello Supramark

原生 SVG 渲染！
`

function App() {
  return (
    <View>
      <Supramark markdown={markdown} />
    </View>
  )
}
```

## 代码块复制

Web 渲染器默认对带语言信息字符串的代码块显示复制按钮，并使用
`navigator.clipboard.writeText`。宿主也可以通过 `onCopyCode` 接管复制行为：

```tsx
<Supramark
  markdown={markdown}
  onCopyCode={async code => {
    await copyWithHostApi(code);
  }}
/>
```

React Native 渲染器不依赖任何剪贴板库，只有提供 `onCopyCode` 时才显示按钮。
两端都可以通过 `copyButton={false}` 关闭复制 UI；Web 此时恢复独立的
`<pre><code>` 结构。

当前 AST 不区分无语言的 fenced code block 与 indented code block，因此只有
`node.lang` 非空时才显示复制按钮。无语言代码块仍使用相同的代码卡片样式，但不显示
header 和按钮；如需让它们也可复制，需要先在 parser AST 中增加 fenced 标记。

Web 默认样式的颜色可以由宿主 CSS 覆盖，适合暗色容器：

```css
.dark-markdown {
  --sm-code-bg: #2d2d2d;
  --sm-code-lang-color: rgba(255, 255, 255, 0.6);
  --sm-code-btn-bg: rgba(255, 255, 255, 0.25);
  --sm-code-btn-color: #fff;
}
```

## 启用 Features

```typescript
import { Supramark } from '@supramark/web'
import { mathFeature } from '@supramark/feature-math'
import { gfmFeature } from '@supramark/feature-gfm'

<Supramark
  markdown={markdown}
  config={{
    features: [
      mathFeature,
      gfmFeature,
    ]
  }}
/>
```

## 下一步

- [核心概念](/guide/concepts.zh) - 了解 Supramark 的设计理念
- [Features 列表](/features/) - 查看所有可用功能
- [API 参考](/api/) - 深入了解 API
