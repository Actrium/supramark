# Feature 脚手架工具使用指南

`create-feature` 是一个交互式命令行工具，帮助快速创建符合 Supramark 规范的 Feature 包。

## 快速开始

### 交互式创建（推荐）

在项目根目录运行：

```bash
npm run create-feature
```

工具会通过交互式问答引导你创建一个完整的 Feature 包。

### 非交互式创建（快速）

通过命令行参数直接创建：

```bash
npm run create-feature -- \
  --name "Vega-Lite" \
  --node-type "diagram" \
  --selector "['vega-lite', 'vega'].includes(node.engine)" \
  --description "Vega-Lite 数据可视化支持"
```

### 查看帮助

```bash
npm run create-feature -- --help
```

## 命令行参数

| 参数            | 简写 | 说明           | 必需 | 默认值           |
| --------------- | ---- | -------------- | ---- | ---------------- |
| `--name`        | `-n` | Feature 名称   | ✅   | -                |
| `--node-type`   | `-t` | AST 节点类型   | ✅   | -                |
| `--id`          | `-i` | Feature ID     | ❌   | 自动生成         |
| `--version`     | `-v` | 版本号         | ❌   | `0.1.0`          |
| `--author`      | `-a` | 作者           | ❌   | `Supramark Team` |
| `--description` | `-d` | 简短描述       | ❌   | -                |
| `--selector`    | `-s` | 节点选择器逻辑 | ❌   | -                |
| `--help`        | `-h` | 显示帮助信息   | -    | -                |

### 参数示例

**基础用法**（只提供必需参数）：

```bash
# 示例 1：单节点类型
npm run create-feature -- -n "Admonition" -t "admonition"

# 示例 2：多节点类型（math_inline/math_block）需要使用 selector
npm run create-feature -- -n "Math Formula" -t "math_inline" -s "node.type === 'math_inline' || node.type === 'math_block'"
```

**完整用法**：

```bash
npm run create-feature -- \
  --name "Vega-Lite" \
  --id "@supramark/feature-vega-lite" \
  --version "0.1.0" \
  --author "Supramark Team" \
  --description "Vega-Lite 数据可视化支持" \
  --node-type "diagram" \
  --selector "['vega-lite', 'vega'].includes(node.engine)"
```

**混合模式**（部分参数 + 交互式）：

```bash
npm run create-feature -- -n "Admonition" -d "提示、警告等特殊块"
# 会提示输入缺失的 node-type 等信息
```

## 使用流程

### 1. 启动工具

```bash
$ npm run create-feature

🚀 Supramark Feature 脚手架工具

请提供 Feature 的基本信息：
```

### 2. 回答问题

工具会依次询问以下信息：

#### 基本信息

**Feature 名称**

```
Feature 名称 (如 "Vega-Lite"):
```

- 输入 Feature 的显示名称，可以包含空格和特殊字符
- 示例：`Vega-Lite`, `Math Formula`, `Admonition`

**Feature ID**

```
Feature ID [@supramark/feature-vega-lite]:
```

- 默认根据名称自动生成：`@supramark/feature-{kebab-case-name}`
- 可以直接回车使用默认值
- 必须符合格式：`@scope/feature-name`

**版本号**

```
版本号 [0.1.0]:
```

- 默认为 `0.1.0`
- 必须符合语义化版本格式：`x.y.z`

**作者**

```
作者 [Supramark Team]:
```

- 默认为 `Supramark Team`
- 可以输入个人或团队名称

**简短描述**

```
简短描述:
```

- 一句话描述此 Feature 的功能
- 示例：`Vega-Lite 数据可视化支持`

#### AST 节点配置

**AST 节点类型**

```
AST 节点类型 (如 "diagram"):
```

- 输入此 Feature 关联的 AST 节点类型
- 可以是已有类型（如 `diagram`, `code`）或新类型
- **注意**：如果你的 Feature 需要处理多个节点类型（如 `math_inline` 和 `math_block`），
  请输入其中一个节点类型，然后在节点选择器中处理多节点逻辑
- 示例：
  - 单节点类型：`diagram`, `admonition`, `code`
  - 多节点类型（需配合 selector）：`math_inline`（然后用 selector 匹配 `math_inline` 和 `math_block`）

**节点选择器**

```
是否需要节点选择器？(y/N):
```

- 如果多个 Feature 共享同一 AST 节点类型，输入 `y`
- **使用场景**：
  - 场景 1：多个 Feature 共享同一节点类型（如 Vega-Lite、Mermaid、PlantUML 都用 `diagram`）
  - 场景 2：一个 Feature 需要处理多个节点类型（如 Math 需要处理 `math_inline` 和 `math_block`）

如果选择 `y`，会继续询问：

```
选择器逻辑 (如 "node.engine === 'vega-lite'"):
```

- 输入 JavaScript 表达式来匹配节点
- 示例（共享节点类型）：`node.engine === 'vega-lite'`
- 示例（共享节点类型）：`['vega-lite', 'vega'].includes(node.engine)`
- 示例（多节点类型）：`node.type === 'math_inline' || node.type === 'math_block'`

### 3. 生成文件

工具会自动创建以下文件结构（示例，main 家族）：

```
packages/features/main/feature-{name}/
├── src/
│   ├── index.ts            # 导出入口
│   └── feature.ts          # Feature 定义文件
├── __tests__/
│   └── feature.test.ts     # 测试文件模板
├── package.json            # npm 包配置
├── tsconfig.json           # TypeScript 配置
├── jest.config.cjs         # 测试配置（复用仓库级 preset）
└── README.md               # 文档模板
```

输出示例：

```
📁 创建目录结构...

  ✓ packages/features/fence/feature-vega-lite
  ✓ packages/features/fence/feature-vega-lite/src
  ✓ packages/features/fence/feature-vega-lite/__tests__

📝 生成文件...

  ✓ package.json
  ✓ tsconfig.json
  ✓ src/index.ts
  ✓ src/feature.ts
  ✓ __tests__/feature.test.ts
  ✓ README.md

✨ Feature 脚手架创建完成！

📦 生成的包：
  @supramark/feature-vega-lite
  位置: packages/features/fence/feature-vega-lite

📝 下一步：
  1. cd packages/features/fence/feature-vega-lite
  2. 完善 src/feature.ts 中的 Feature 定义
  3. 编写测试用例 __tests__/feature.test.ts
  4. 完善 README.md 文档
  5. npm run build 编译 TypeScript
  6. npm test 运行测试

💡 提示：
  • 使用 FeatureRegistry.register(vegaLiteFeature) 注册 Feature
  • 参考文档: docs/CREATE_FEATURE_GUIDE.md
  • 完整示例: docs/features.vega-lite.example.ts
```

## 生成的文件说明

### 0. `package.json`

npm 包配置文件，包含：

- 包名、版本、描述
- 构建和测试脚本
- 依赖声明（peer dependencies: @supramark/core）
- 导出配置（ESM）
- 仓库信息

```json
{
  "name": "@supramark/feature-{name}",
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "build": "tsc -p tsconfig.json",
    "test": "jest",
    ...
  },
  "peerDependencies": {
    "@supramark/core": "workspace:*"
  }
}
```

### 0.5. `tsconfig.json`

TypeScript 配置文件：

- 继承根目录的 `tsconfig.base.json`
- 配置输出目录 `dist/`
- 启用声明文件生成

```json
{
  "extends": "../../../../tsconfig.base.json",
  "compilerOptions": {
    "outDir": "./dist",
    "rootDir": "./src",
    "declaration": true
  }
}
```

### 0.7. `jest.config.cjs`

每个 Feature 包都会生成一个极简的 Jest 配置文件，复用仓库级 preset：

```js
/** @type {import('jest').Config} */
module.exports = {
  // 使用 Supramark 共享的 Jest preset，
  // 与 @supramark/core 的测试配置保持一致。
  ...require('../../jest.preset.cjs'),
};
```

### 0.8. `src/index.ts`

包的导出入口文件：

```typescript
export { vegaLiteFeature } from './feature.js';
```

### 1. `src/feature.ts`

Feature 定义文件，包含：

````typescript
import type { SupramarkFeature, SupramarkNode } from '@supramark/core';

/**
 * {Name} Feature
 *
 * {Description}
 *
 * @example
 * ```markdown
 * TODO: 添加 Markdown 示例
 * ```
 */
export const {camelCaseName}Feature: SupramarkFeature<SupramarkNode> = {
  metadata: {
    id: '{id}',
    name: '{name}',
    version: '{version}',
    author: '{author}',
    description: '{description}',
    license: 'Apache-2.0',
    tags: [], // TODO: 添加标签
  },

  syntax: {
    ast: {
      type: '{nodeType}',
      // selector: (node) => ..., // 如果有选择器

      // 可选：描述节点接口（注意：实际生成的模板会根据常见节点类型自动补全字段）
      interface: {
        required: ['type'],
        optional: [],
        fields: {
          type: {
            type: 'string',
            description: 'Node type identifier',
          },
        },
      },

      // 可选：节点约束
      constraints: {
        allowedParents: ['root'], // TODO: 指定允许的父节点
        allowedChildren: [], // TODO: 指定允许的子节点
      },

      // 可选：示例节点
      examples: [
        // TODO: 添加示例节点
      ],
    },

    // 可选：如果需要自定义解析器
    // parser: { ... },

    // 可选：验证规则
    // validator: { ... },
  },

  // 可选：渲染器定义
  renderers: {
    // rn: { ... },
    // web: { ... },
  },
};

// 注册 Feature（可选）
// FeatureRegistry.register({camelCaseName}Feature);
````

**需要完善 / 校验的部分**：

- 添加 Markdown 示例（`@example` 注释）；
- 添加标签（`tags`）；
- 根据实际 AST 校验并补充接口定义（`interface.required`, `interface.fields`）——  
  对于常见节点类型（如 diagram / math / footnote / admonition 等），CLI 已自动填充推荐字段，通常只需微调；
- 添加示例节点（`examples`）；
- 根据需要添加解析器、验证器、渲染器

### 2. `__tests__/feature.test.ts`

测试文件模板，包含基础测试：

```typescript
import { {camelCaseName}Feature } from '../feature';
import { validateFeature } from '@supramark/core';

describe('{Name} Feature', () => {
  describe('Metadata', () => {
    it('should have valid metadata', () => {
      const result = validateFeature({camelCaseName}Feature);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('should have correct id', () => {
      expect({camelCaseName}Feature.metadata.id).toMatch(/^@[\w-]+\/feature-[\w-]+$/);
    });

    it('should have semantic version', () => {
      expect({camelCaseName}Feature.metadata.version).toMatch(/^\d+\.\d+\.\d+$/);
    });
  });

  describe('Syntax', () => {
    it('should define AST node type', () => {
      expect({camelCaseName}Feature.syntax.ast.type).toBeDefined();
      expect(typeof {camelCaseName}Feature.syntax.ast.type).toBe('string');
    });

    // TODO: 添加更多语法测试
  });

  // TODO: 添加渲染测试
  // TODO: 添加集成测试
});
```

**需要添加的测试**：

- 节点选择器测试（如果有）；
- 解析器测试；
- 渲染器测试；
- 集成测试；

### 3. `README.md`

文档模板，包含：

```markdown
# {Name}

{Description}

## 功能特性

TODO: 描述主要功能

## 语法

TODO: 添加 Markdown 语法示例

## AST 结构

TODO: 描述 AST 节点结构

## 平台支持

- [ ] React Native
- [ ] Web (React)
- [ ] CLI (终端)

## 开发状态

- [ ] AST 定义
- [ ] 解析器实现
- [ ] RN 渲染器
- [ ] Web 渲染器
- [ ] 测试用例
- [ ] 文档完善

## 示例

TODO: 添加使用示例

## 相关资源

- [Feature Interface 文档](../../docs/FEATURE_INTERFACE_IMPROVEMENTS.md)
- [API 文档](../core/docs/api)
```

**需要完善的内容**：

- 功能特性说明
- Markdown 语法示例
- AST 节点结构说明
- 使用示例

## 完整示例

### 创建 Vega-Lite Feature

```bash
$ npm run create-feature

🚀 Supramark Feature 脚手架工具

请提供 Feature 的基本信息：

Feature 名称 (如 "Vega-Lite"): Vega-Lite
Feature ID [@supramark/feature-vega-lite]:
版本号 [0.1.0]:
作者 [Supramark Team]:
简短描述: Vega-Lite 数据可视化支持

AST 节点配置：

AST 节点类型 (如 "diagram"): diagram
是否需要节点选择器？(y/N): y
选择器逻辑 (如 "node.engine === 'vega-lite'"): ['vega-lite', 'vega'].includes(node.engine)

📁 创建文件结构...
  ✓ packages/features/fence/feature-vega-lite
  ✓ packages/features/fence/feature-vega-lite/src
  ✓ packages/features/fence/feature-vega-lite/__tests__

📝 生成文件...
  ✓ packages/features/fence/feature-vega-lite/src/feature.ts
  ✓ packages/features/fence/feature-vega-lite/__tests__/feature.test.ts
  ✓ packages/features/fence/feature-vega-lite/README.md

✨ Feature 脚手架创建完成！

下一步：
  1. cd packages/features/fence/feature-vega-lite
  2. 编辑 src/feature.ts 实现功能逻辑
  3. 编写测试用例 __tests__/feature.test.ts
  4. 完善 README.md 文档
```

### 生成的 feature.ts（部分）

```typescript
export const vegaLiteFeature: SupramarkFeature<SupramarkNode> = {
  metadata: {
    id: '@supramark/feature-vega-lite',
    name: 'Vega-Lite',
    version: '0.1.0',
    author: 'Supramark Team',
    description: 'Vega-Lite 数据可视化支持',
    license: 'Apache-2.0',
    tags: [],
  },

  syntax: {
    ast: {
      type: 'diagram',
      selector: node => node.type === 'diagram' && ['vega-lite', 'vega'].includes(node.engine),

      interface: {
        required: ['type'],
        optional: [],
        fields: {
          type: {
            type: 'string',
            description: 'Node type identifier',
          },
        },
      },

      constraints: {
        allowedParents: ['root'],
        allowedChildren: [],
      },

      examples: [],
    },
  },
};
```

## 后续步骤

创建 Feature 后的典型开发流程：

### 1. 完善 Feature 定义

编辑 `src/feature.ts`：

```typescript
export const vegaLiteFeature: SupramarkFeature<DiagramNode> = {
  metadata: {
    id: '@supramark/feature-vega-lite',
    name: 'Vega-Lite',
    version: '0.1.0',
    author: 'Supramark Team',
    description: 'Vega-Lite 数据可视化支持',
    license: 'Apache-2.0',
    tags: ['diagram', 'chart', 'visualization', 'data-viz'],
  },

  syntax: {
    ast: {
      type: 'diagram',
      selector: node => node.type === 'diagram' && ['vega-lite', 'vega'].includes(node.engine),

      interface: {
        required: ['type', 'engine', 'code'],
        optional: ['meta', 'title', 'width', 'height'],
        fields: {
          type: {
            type: 'string',
            description: 'Node type (diagram)',
          },
          engine: {
            type: 'string',
            description: 'Rendering engine (vega-lite or vega)',
          },
          code: {
            type: 'string',
            description: 'Vega-Lite JSON specification',
          },
        },
      },

      examples: [
        {
          type: 'diagram',
          engine: 'vega-lite',
          code: JSON.stringify({
            data: {
              values: [
                { x: 1, y: 2 },
                { x: 2, y: 3 },
              ],
            },
            mark: 'point',
            encoding: { x: { field: 'x' }, y: { field: 'y' } },
          }),
        },
      ],
    },
  },

  renderers: {
    rn: {
      infrastructure: {
        needsWorker: true,
        needsCache: true,
      },
    },
    web: {
      infrastructure: {
        needsClientScript: true,
      },
    },
  },
};
```

### 2. 注册 Feature

在宿主应用中按需使用：

```typescript
// App.tsx
import { Supramark } from '@supramark/web';
import { vegaLiteFeature } from '@supramark/feature-vega-lite';

export function App() {
  return (
    <Supramark
      config={{
        features: [vegaLiteFeature]
      }}
    />
  );
}
```

### 3. 实现渲染逻辑

在 `@supramark/engines` 中添加 Vega-Lite 渲染支持：

```typescript
// packages/engines/src/vega-lite/index.ts
export async function renderVegaLite(spec: string): Promise<string> {
  // 使用 Vega-Lite 库输出 SVG
  const view = new vega.View(vega.parse(vegaLite.compile(JSON.parse(spec)).spec));
  return await view.toSVG();
}
```

### 4. 编写测试

完善 `__tests__/feature.test.ts`：

```typescript
describe('Vega-Lite Feature', () => {
  describe('Node Selector', () => {
    it('should match vega-lite diagram nodes', () => {
      const node = {
        type: 'diagram',
        engine: 'vega-lite',
        code: '...',
      };
      expect(vegaLiteFeature.syntax.ast.selector!(node)).toBe(true);
    });

    it('should not match other diagram nodes', () => {
      const node = {
        type: 'diagram',
        engine: 'mermaid',
        code: '...',
      };
      expect(vegaLiteFeature.syntax.ast.selector!(node)).toBe(false);
    });
  });
});
```

### 5. 更新示例应用

在示例 App 中展示：

```tsx
// examples/react-native/src/demos.ts
export const demos = [
  {
    id: 'vega-lite',
    title: 'Vega-Lite 图表',
    markdown: `
# Vega-Lite 示例

\`\`\`vega-lite
{
  "data": {"values": [{"x": 1, "y": 2}, {"x": 2, "y": 3}]},
  "mark": "point",
  "encoding": {"x": {"field": "x"}, "y": {"field": "y"}}
}
\`\`\`
    `,
  },
];
```

## 最佳实践

1. **命名规范**：
   - Feature ID：`@scope/feature-{name}`
   - 导出名称：`{camelCase}Feature`
   - 包目录：`packages/features/<family>/feature-{kebab-case}`（family 通常为 main / container / fence）

2. **文档优先**：
   - 先写 README 说明语法和示例
   - 再实现 Feature 定义
   - 最后补充测试

3. **渐进式实现**：
   - 直接用 SupramarkFeature 定义 Feature；
   - 初期可以只填写 metadata + syntax.ast + 简单 renderers；
   - 后续再逐步补全 examples / testing / documentation。

4. **复用已有能力**：
   - 优先使用已有的 AST 节点类型
   - 通过 selector 区分不同 Feature
   - 避免重复定义相似的节点

5. **测试驱动**：
   - 编写测试验证 Feature 定义
   - 测试节点选择器逻辑
   - 集成测试确保端到端工作

## 常见问题

### Q: 何时需要节点选择器？

A: 当多个 Feature 共享同一 AST 节点类型时。例如：

- Vega-Lite、Mermaid、PlantUML 都用 `diagram`
- 通过 `node.engine` 字段区分

### Q: 现在还有 MinimalFeature 吗？

A: 早期版本中存在 `MinimalFeature` 作为快速原型接口，目前已经统一收敛为 `SupramarkFeature`。  
新的 Feature 应直接实现 `SupramarkFeature`，必要时可以先写出最小结构，再结合 `validateFeature` 查看需要补齐的部分。

### Q: Feature 定义后如何使用？

A:

1. 注册到 FeatureRegistry
2. 在运行时包初始化时加载
3. 通过配置控制启用/禁用

### Q: 如何共享 Feature？

A:

- Feature 定义可以独立发布为 npm 包
- 其他项目通过安装包 + 注册即可使用
- 渲染实现可能需要针对不同平台单独提供

## 相关文档

- [Feature Interface 接口定义](../packages/core/src/feature.ts)
- [Feature Interface 改进说明](./FEATURE_INTERFACE_IMPROVEMENTS.md)
- [Feature 生命周期与配置管理](./FEATURE_LIFECYCLE_AND_CONFIG.md)
- [Plugin System 设计](./PLUGIN_SYSTEM.md)

## 反馈与改进

如果在使用过程中遇到问题或有改进建议，请：

- 提交 Issue
- 或直接修改 `scripts/create-feature.js` 并提交 PR

---

**工具版本**：v0.1.0
**最后更新**：2025-12-05
