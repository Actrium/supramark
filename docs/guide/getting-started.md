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

- [核心概念](/guide/concepts) - 了解 Supramark 的设计理念
- [Features 列表](/features/) - 查看所有可用功能
- [API 参考](/api/) - 深入了解 API
