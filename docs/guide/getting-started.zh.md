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

## 代码块操作按钮

带语言信息字符串的代码块会在 header 上显示操作按钮（两端都有复制，全屏仅 RN）。
按钮是纯图标按钮：图标由库内联绘制（Web 内联 SVG，RN 走 `react-native-svg`），
不引入任何图片资源，颜色跟随主题（Web 使用 `currentColor`）。

### 复制

Web 渲染器默认使用 `navigator.clipboard.writeText`，宿主可以通过 `onCopyCode` 接管：

```tsx
<Supramark
  markdown={markdown}
  onCopyCode={async code => {
    await copyWithHostApi(code);
  }}
/>
```

React Native 渲染器不依赖任何剪贴板库，只有提供 `onCopyCode` 时才显示复制按钮。

### 全屏（仅 React Native）

全屏按钮**只在 RN 上存在**：窄栏里的代码块需要一条逃逸路径，Web 端没有这个场景，
渲染器也不保留任何对应入口。

全屏没有默认行为——只有宿主知道「全屏」在自己的 UI 里是什么（弹层、路由、面板），
因此只有提供 `onFullscreenCode` 时才显示全屏按钮：

```tsx
<Supramark
  markdown={markdown}
  onFullscreenCode={({ code, fenced, lang }) => {
    openFullscreenPanel({ code, fenced, lang });
  }}
/>
```

回调收到一个对象：

| 字段            | 内容                                                                                                                                                                     |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `code`          | 纯代码内容（不含围栏）                                                                                                                                                   |
| `fenced`        | 完整 Markdown 片段（含围栏）。有源码时按 AST `position` 从原文切片，完全保留作者写法（四反引号围栏、`~~~`、信息串空格）；源码不可用或与 AST 不匹配时，按节点重建规范围栏 |
| `lang` / `meta` | fence 信息串的语言与其余部分                                                                                                                                             |
| `node`          | AST 节点本身（含 `position`）。**只读**：它是渲染树里的活节点，快照可能被多个实例共享，修改会外溢到别的渲染                                                              |

`fenced` 是自包含的：可以直接交给另一个 `<Supramark markdown={fenced} />` 渲染，
或原样转发给别处。

以下情形会走重建而非切片。内容始终正确，只是不保留作者的写法：

- **`:::` 容器内**（如 `:::note`）的代码块。容器 body 是单独解析后挂回语法树的，
  其 `position` 相对 body 而非整篇文档，因此无法安全切片；
- **未闭合的围栏**（流式输入）。此时切片不含结尾的换行，重建会补上闭合标记，
  宿主拿到的是完整围栏；
- 少数保真度角落：**引用块内的围栏**（行首的 `> ` 前缀不在切片的识别范围内，一律重建）、
  列表内缩进超过 3 空格的围栏、含「只有空格」的缩进行，以及 CRLF 换行的文档
  （解析器会把内容归一为 LF）。

重建对「代码内容本身含 ```」的块不保真——三反引号无法表达，重新解析会得到不同的文档。
需要通过全屏面板回喂`<Supramark markdown={fenced} />` 时，请让这类代码块待在顶层。

### 开关与文案

| prop                       | 作用                                                           |
| -------------------------- | -------------------------------------------------------------- |
| `codeBlockChrome={false}`  | 关闭整个代码块外观，回到裸 `<pre><code>`（Web）/ 裸 View（RN） |
| `copyButton={false}`       | 只关闭复制按钮                                                 |
| `fullscreenButton={false}` | 只关闭全屏按钮（仅 RN）                                        |
| `codeActionLabels`         | 覆盖按钮文案（读屏名称，以及 Web 的悬停提示）                  |

```tsx
<Supramark
  markdown={markdown}
  onFullscreenCode={openFullscreenPanel}
  codeActionLabels={{ copy: '复制代码', fullscreen: '全屏查看代码' }}
/>
```

`codeActionLabels.fullscreen` 在 Web 上不起作用（没有那个按钮）；其余两端通用。

按钮只有图标、没有可见文字，所以 `codeActionLabels` 是读屏名称与悬停提示的唯一来源；
库只提供英文默认值，不做语言猜测。按钮**没有成功态**：复制后图标与文案都不变。

当前 AST 不区分无语言的 fenced code block 与 indented code block，因此只有 `node.lang`
非空时才显示 header 和按钮。无语言代码块仍使用相同的代码卡片样式；如需让它们也带上
按钮，需要先在 parser AST 中增加 fenced 标记。

Web 默认样式的颜色可以由宿主 CSS 覆盖，适合暗色容器（图标使用 `currentColor`，
会跟随 `--sm-code-action-color`；鼠标悬停时的底色由 `--sm-code-action-hover-bg`
控制，Web 上按钮静止时不带背景）：

```css
.dark-markdown {
  --sm-code-bg: #2d2d2d;
  --sm-code-lang-color: rgba(255, 255, 255, 0.6);
  --sm-code-action-color: rgba(255, 255, 255, 0.6);
  --sm-code-action-hover-bg: rgba(255, 255, 255, 0.1);
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
