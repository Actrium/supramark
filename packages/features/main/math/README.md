# Math

LaTeX 数学公式支持

## 功能特性

- 为 supramark 提供 **Math / LaTeX 公式** 的 Feature 定义；
- 规范 `math_inline` / `math_block` AST 结构与约束；
- 不直接参与解析 / 渲染，实际逻辑由 `@supramark/core`、`@supramark/web`、`@supramark/rn` 实现；
- 支持通过 `FeatureRegistry` 与配置系统发现 / 管理 Math 能力。

## 语法

当前 Math 语法与常见 Markdown 扩展保持一致：

```markdown
行内公式：这是著名的 $E = mc^2$。

块级公式：

$$
\frac{1}{\sqrt{2\pi\sigma^2}} e^{-\frac{(x - \mu)^2}{2\sigma^2}}
$$
```

## AST 结构

在 supramark AST 中，Math 对应两个节点类型：

```ts
interface SupramarkMathInlineNode {
  type: 'math_inline';
  value: string; // 行内 TeX 文本
}

interface SupramarkMathBlockNode {
  type: 'math_block';
  value: string; // 块级 TeX 文本
}
```

本 Feature 使用：

- `syntax.ast.type = 'math_inline'`
- `syntax.ast.selector(node)` 区分 `math_inline` / `math_block`

## 平台支持

- [x] React Native（通过 `@supramark/rn` + 本地 MathJax 渲染为 SVG）
- [x] Web (React)（通过 `@supramark/web` + KaTeX / MathJax 渲染）
- [ ] CLI (终端)

## 开发状态

- [x] AST 定义（在 `@supramark/core` 中完成）
- [x] 解析器实现（由 `supramark-markdown` AST v2 parser 完成）
- [x] RN 渲染器（本地 MathJax SVG 管线已实现）
- [x] Web 渲染器（首版 KaTeX/MathJax 管线已实现）
- [ ] Feature 级测试用例（当前仅元数据测试）
- [ ] 文档进一步完善

## 示例

在应用中注册并通过配置启用 Math Feature（示意）：

```ts
import { FeatureRegistry, createConfigFromRegistry } from '@supramark/core';
import { mathFeature } from '@supramark/feature-math';

// 应用初始化时注册 Feature
FeatureRegistry.register(mathFeature);

// 生成配置并传入 <Supramark />
const config = createConfigFromRegistry(true);
```

## 相关资源

- [Feature Interface 文档](../../docs/FEATURE_INTERFACE_IMPROVEMENTS.md)
- [API 文档](../core/docs/api)
