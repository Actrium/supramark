# @supramark/feature-video

视频嵌入容器，通过 `:::video` + JSON 配置体在 Markdown 中嵌入可播放的视频。

## 语法

```markdown
:::video
{
"src": "https://example.com/demo.mp4"
}
:::
```

## 配置字段

| 字段       | 类型    | 说明                                   |
| ---------- | ------- | -------------------------------------- |
| `src`      | string  | 视频 URL（渲染必需）                   |
| `poster`   | string  | 封面图 URL                             |
| `title`    | string  | 标题 / 无障碍说明                      |
| `autoplay` | boolean | 自动播放（浏览器通常要求同时 `muted`） |
| `loop`     | boolean | 循环播放                               |
| `muted`    | boolean | 静音                                   |
| `controls` | boolean | 显示原生控件，默认 `true`              |
| `width`    | number  | 播放器宽度占容器百分比（1-100）        |

未知字段会被忽略；JSON 非法时渲染内联错误卡片，而不是让整篇文档失败。

## 平台行为

- **Web**：渲染原生 `<video controls poster>`。
- **React Native**：RN 没有内置视频组件，默认渲染封面图（或占位块）+ 播放按钮，
  点击后经 `Linking` 打开系统播放器。需要内联播放的宿主可用
  `react-native-video` / `expo-av` 自行实现渲染函数，并通过
  `<Supramark containerRenderers={{ video: myRenderer }} />` 注入。

## 宿主接入

```tsx
import { videoFeature, renderVideoContainerWeb } from '@supramark/feature-video';

videoFeature.registerParser();

<Supramark
  config={{ features: [videoFeature] }}
  containerRenderers={{ video: renderVideoContainerWeb }}
/>;
```
