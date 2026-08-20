# WikiLink

知识库 Markdown 的 WikiLink 语法支持（`[[target]]` / `[[target|label]]` / `[[target#section]]`）

## 功能特性

- 为 supramark 提供 **WikiLink** 的 Feature 描述；
- 使用 `wiki_link` AST 节点，解析由 `supramark-markdown` 的 `wikilink` 运行时选项门控（**默认关闭**，保持 CommonMark/GFM 输出字节级不变）；
- 在 `SupramarkConfig.features` 中启用本 Feature 会自动开启解析器选项，无需重复传参；
- 目标到文件/URL 的解析（resolution）是宿主职责：通过 `resolveWikiLink` 回调注入，下游（如 Markon）在工作区内自行解析；
- 支持通过 `FeatureRegistry` 与配置系统统一管理。

## 语法

```markdown
链接到某个页面：[[Project Plan]]。

带显示文本：[[Project Plan|the plan]]。

带标题锚点：[[Project Plan#Roadmap]]，两者兼有：
[[Project Plan#Roadmap|the roadmap]]。

同页锚点：[[#Q2-goals]]。
```

解析规则（对齐 Obsidian/Logseq）：

| 形态 | 结果 |
|---|---|
| `[[` … 首个 `]]` | 闭合为一个 wikilink |
| 目标部分内首个 `\|` 之前 / 之后 | target / label（label 内后续 `\|`、`#` 均字面） |
| 目标部分内首个 `#` | 分割出 section；块引用 `[[note#^abc]]` 的 `^abc` 原样保留 |
| `[[]]`、`[[\|x]]`、`[[#]]`、`[[target\|]]` | 降级为字面文本 |
| 内容含 `[`、换行、或未紧跟 `]` 的单个 `]` | 整体降级，走 CommonMark 括号语义 |
| 未闭合 `[[foo` | 降级为字面文本，**不发诊断**（普通散文可能合法包含 `[[`） |
| 转义 `\[[foo]]`、code span / fence 内 | 保持字面 |
| 内容空白 | 不 trim，原样保留（resolver 端自行处理） |

## AST 结构

```ts
interface SupramarkWikiLinkNode {
  type: 'wiki_link';
  target: string;   // 空字符串表示同页锚点（[[#section]]）
  section?: string; // 目标部分首个 # 之后
  label?: string;   // 首个 | 之后，纯文本无 children
  position?: SourcePosition; // 覆盖整个 [[…]] 区间，UTF-8 / UTF-16 双坐标
}
```

## 使用

```tsx
import { wikilinkFeature, createWikilinkFeatureConfig } from '@supramark/feature-wikilink';

<Supramark
  config={{
    features: [wikilinkFeature],
    featureConfigs: [
      createWikilinkFeatureConfig(true, {
        resolveWikiLink: ({ target, section }) =>
          `/notes/${encodeURIComponent(target)}${section ? `#${section}` : ''}`,
      }),
    ],
  }}
/>;
```

- resolver 缺失或返回 `null`/`undefined` 时，渲染为**样式化但不可导航**的文本（不产出 `href="#"` 假链接）；
- 仅在 `parse()` 层使用时也可显式传 `{ wikilink: true }`。

## 行为变化说明（选项开启时）

- `[label [x]](url)` 中 link label 内的 `[[x]]` 会成为 Link 的 WikiLink 子节点；
- 存在 `[target]: url` 引用定义时，`[[target]]` 仍是 wikilink（wiki 语义优先于 CommonMark 快捷引用）。

选项关闭（默认）时以上全部保持 CommonMark/GFM 原行为。

## 平台支持

- [x] Web (React)（`@supramark/web`）
- [ ] React Native — **当前不可用**：RN 侧 native 模块尚未支持 parser options；请求 wikilink 时抛显式错误（见下方"第三方渲染器升级提示"），native FFI 桥接为后续 issue
- [ ] CLI (终端)

## 开发状态

- [x] AST 定义（在 `@supramark/core` 中完成）
- [x] 解析器实现（`supramark-markdown` 运行时选项 `wikilink`，默认关）
- [x] Web 渲染器
- [x] Feature 元数据与接口定义

## 第三方渲染器升级提示

自定义渲染器若对未知 inline 节点 `default: return null`，升级后需为 `wiki_link` 增加 case，否则节点会被静默丢弃。最简实现：显示文本取 `label ?? (section != null ? \`${target} > ${section}\` : target)`。
