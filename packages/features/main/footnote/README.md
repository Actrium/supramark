# Footnote

脚注语法支持（引用 + 定义）

## 功能特性

- 为 supramark 提供 **脚注引用 + 定义** 的 Feature 描述；
- 复用 `footnote_reference` / `footnote_definition` AST 节点；
- 不直接实现解析 / 渲染逻辑，由 `@supramark/core` 与 RN/Web 渲染器负责；
- 支持通过 `FeatureRegistry` 与配置系统统一管理脚注能力。

## 语法

当前脚注语法由 `supramark-markdown` AST v2 parser 解析：

```markdown
这里有一个脚注引用[^1]，以及一个内联脚注 ^[内联脚注内容]。

[^1]: 这里是脚注定义内容。
```

## AST 结构

在 supramark AST 中，脚注相关节点为：

```ts
interface SupramarkFootnoteReferenceNode {
  type: 'footnote_reference';
  index: number;
  label?: string;
  subId?: number;
}

interface SupramarkFootnoteDefinitionNode {
  type: 'footnote_definition';
  index: number;
  label?: string;
  children: SupramarkNode[];
}
```

本 Feature 使用：

- `syntax.ast.type = 'footnote_reference'`；
- `syntax.ast.selector(node)` 同时匹配引用与定义两个节点；
- `interface.fields` 描述 index / label / subId / children 等字段含义。

## 平台支持

- [x] React Native（`@supramark/rn` 中已有占位渲染逻辑）
- [x] Web (React)（`@supramark/web` 中已有占位渲染逻辑）
- [ ] CLI (终端)

## 开发状态

- [x] AST 定义（在 `@supramark/core` 中完成）
- [x] 解析器实现（集成到 `supramark-markdown`）
- [x] RN 渲染器（基础占位渲染）
- [x] Web 渲染器（基础占位渲染）
- [x] Feature 元数据与接口定义
- [ ] Feature 级测试用例完善
- [ ] 文档进一步扩展

## 示例

在应用中注册并启用脚注 Feature（示意）：

```ts
import { FeatureRegistry, createConfigFromRegistry } from '@supramark/core';
import { footnoteFeature } from '@supramark/feature-footnote';

FeatureRegistry.register(footnoteFeature);

const config = createConfigFromRegistry(true);
```

## 相关资源

- [Feature Interface 文档](../../docs/FEATURE_INTERFACE_IMPROVEMENTS.md)
- [API 文档](../core/docs/api)
