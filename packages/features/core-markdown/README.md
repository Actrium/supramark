# Core Markdown

基础 Markdown 语法（段落 / 标题 / 列表等）

## 功能特性

- 统一描述「基础 Markdown 语法」所覆盖的 AST 节点集合；
- 覆盖段落、标题、列表、引用、分隔线、普通代码块以及常见行内元素；
- 与 Math / Footnote / Definition List / Admonition / Diagram / Emoji / GFM 等扩展 Feature 并列，没有特例；
- 解析与渲染逻辑仍由 `@supramark/core`、`@supramark/rn`、`@supramark/web` 现有实现负责。

## 语法

典型包含的语法：

```markdown
# 标题

普通段落文本，包含 **粗体**、_斜体_、`inline code` 和 [链接](https://example.com)。

- 列表项 1
- 列表项 2

> 引用段落

---
```

## AST 结构

Core Markdown Feature 通过 selector 匹配的节点包括（但不限于）：

- Block：
  - `root`
  - `paragraph`
  - `heading`
  - `code`
  - `list` / `list_item`
  - `blockquote`
  - `thematic_break`
- Inline：
  - `text`
  - `strong`
  - `emphasis`
  - `inline_code`
  - `link`
  - `image`
  - `break`

扩展节点（`diagram` / `math_*` / `footnote_*` / `definition_*` / `admonition` / `table_*` / `delete` 等）由各自的 Feature 负责。

## 平台支持

- [x] React Native
- [x] Web (React)
- [ ] CLI (终端)

## 开发状态

- [x] AST 定义（在 `@supramark/core` 中完成）
- [x] 解析器实现（`supramark-markdown` AST v2 主线）
- [x] RN 渲染器
- [x] Web 渲染器
- [x] Feature 元数据与接口定义
- [ ] Feature 级测试用例进一步完善

## 示例

在应用中将 Core Markdown 也纳入 Feature 配置：

```ts
import { FeatureRegistry, createConfigFromRegistry } from '@supramark/core';
import { coreMarkdownFeature } from '@supramark/feature-core-markdown';

FeatureRegistry.register(coreMarkdownFeature);

const config = createConfigFromRegistry(true);
```

## 相关资源

- [Feature Interface 文档](../../docs/FEATURE_INTERFACE_IMPROVEMENTS.md)
- [API 文档](../core/docs/api)
