# 架构设计

深入了解 Supramark 的内部架构和设计决策。

## 架构概览

Supramark 采用分层架构，从底层到上层分为：

```
┌─────────────────────────────────────┐
│     应用层（User Applications）      │
│  React Native App  │  React Web App │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│      渲染层（Render Layer）          │
│  @supramark/rn  │  @supramark/web   │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│      功能层（Feature Layer）         │
│  Feature Packages (Math, GFM, etc) │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│      核心层（Core Layer）            │
│      @supramark/core                │
│  AST | Parser | Plugin System       │
└─────────────────────────────────────┘
```

## Diagram Engine 目标

图表链路正在从“平台各自渲染”收敛到“统一 engine 产出 SVG，再由各平台消费”。

- 架构目标文档见 [../architecture/DIAGRAM_ENGINE_TARGET.md](../architecture/DIAGRAM_ENGINE_TARGET.md)
- 图表渲染不再通过额外执行容器，RN 与 Web 都消费 `@supramark/engines` 输出的 SVG

## 核心层（@supramark/core）

### 职责

- 定义统一的 AST 节点类型
- 提供 Markdown 解析器
- 管理 Feature 注册和配置
- 提供插件系统接口

### 关键组件

#### 1. AST 类型系统

```typescript
// 所有节点的联合类型
type SupramarkNode =
  | ParagraphNode
  | HeadingNode
  | CodeBlockNode
  | MathNode
  | DiagramNode
  | ... // 80+ 种类型

// 节点接口
interface BaseNode {
  type: string
  position?: Position
  data?: Record<string, unknown>
}

interface ParagraphNode extends BaseNode {
  type: 'paragraph'
  children: InlineNode[]
}
```

#### 2. 解析器

解析层收敛为单一 public contract：

```typescript
import { parse } from '@supramark/core';

const ast = await parse(source);
```

`@supramark/core` 只是 facade：加载 Rust `supramark-markdown` 产物，并执行 AST 后处理插件。
真正的源码扫描、rule、source map 和 AST v2 构建在 `crates/supramark-markdown` 中完成。

#### 3. Feature 注册器

```typescript
class FeatureRegistry {
  private features = new Map<string, SupramarkFeature>();

  register(feature: SupramarkFeature) {
    this.validate(feature);
    this.features.set(feature.metadata.id, feature);
  }

  getFeature(id: string): SupramarkFeature | undefined {
    return this.features.get(id);
  }
}
```

## Feature 层

### Feature 接口设计

```typescript
interface SupramarkFeature {
  // 元信息
  metadata: FeatureMetadata;

  // 语法与 AST 合同
  syntax: {
    ast: ASTNodeDefinition;
  };

  // 渲染器
  renderers?: {
    web?: WebRenderer;
    rn?: RNRenderer;
  };

  // 文档
  documentation?: FeatureDocumentation;
}
```

### Feature 生命周期

1. **注册** - Feature 注册到 Registry
2. **初始化** - 解析器加载 Feature 插件
3. **解析** - 应用 Feature 的语法规则
4. **渲染** - 使用 Feature 的渲染器

### 内置 Features

每个 Feature 是独立的 npm 包：

- `@supramark/feature-core-markdown`
- `@supramark/feature-gfm`
- `@supramark/feature-math`
- `@supramark/feature-admonition`
- 等等...

优点：

- 按需安装，减小包体积
- 独立版本管理
- 便于第三方扩展

## 渲染层

### Web 渲染器（@supramark/web）

```typescript
// Web 渲染器将 AST 节点映射为 React 元素
function WebRenderer({ ast }: { ast: SupramarkNode }) {
  return renderNode(ast)
}

function renderNode(node: SupramarkNode): React.ReactNode {
  switch (node.type) {
    case 'paragraph':
      return <p>{node.children.map(renderNode)}</p>

    case 'heading':
      const Tag = `h${node.depth}` as const
      return <Tag>{node.children.map(renderNode)}</Tag>

    case 'math':
      return <KaTeX math={node.value} displayMode={node.display} />

    // ... 其他节点类型
  }
}
```

### RN 渲染器（@supramark/rn）

```typescript
// RN 渲染器将 AST 节点映射为 React Native 组件
function RNRenderer({ ast }: { ast: SupramarkNode }) {
  return renderNode(ast)
}

function renderNode(node: SupramarkNode): React.ReactNode {
  switch (node.type) {
    case 'paragraph':
      return <Text>{node.children.map(renderNode)}</Text>

    case 'heading':
      return (
        <Text style={headingStyles[node.depth]}>
          {node.children.map(renderNode)}
        </Text>
      )

    case 'math':
      return <MathRenderer math={node.value} display={node.display} />

    // ... 其他节点类型
  }
}
```

### 重度功能渲染策略

对于需要浏览器环境的功能（图表、复杂数学公式）：

**Web 端**：

```typescript
// 直接使用浏览器库
import mermaid from 'mermaid'

function DiagramRenderer({ code }: { code: string }) {
  const svg = useMemo(() => mermaid.render(code), [code])
  return <div dangerouslySetInnerHTML={{ __html: svg }} />
}
```

**RN 端**：

```typescript
import { createReactNativeDiagramEngine } from '@supramark/engines/rn'

const engine = createReactNativeDiagramEngine()

async function renderDiagram(code: string) {
  return engine.render({ engine: 'mermaid', code })
}
```

## RN SVG Engine

```
┌────────────────────────────────┐
│      RN Main Thread            │
│  ┌──────────────────────────┐  │
│  │  <Supramark>             │  │
│  │    ├─ Paragraph          │  │
│  │    ├─ Math ─────────┐    │  │
│  │    └─ Diagram ───┐   │    │  │
│  └──────────────────│───│────┘  │
│                     │   │        │
│                     ↓   ↓        │
│  ┌─────────────────────────────┐│
│  │ @supramark/engines/rn      ││
│  │ render({ engine, code })   ││
│  └─────────────────────────────┘│
│                ↓                 │
│ native FFI / JS SVG-string       │
│                ↓                │
│        Generate SVG             │
└─────────────────────────────────┘
```

### 工作流程

1. RN 组件请求渲染图表
2. `@supramark/engines/rn` 路由到 native FFI adapter 或 JS SVG-string engine
3. engine 返回 SVG
4. RN 组件通过 `react-native-svg` 展示结果

优点：

- 图表渲染入口统一
- RN 不需要额外执行容器
- renderer 只负责展示 SVG

## 数据流

### 完整流程

```
Markdown Text
    ↓
parse()
    ↓
Apply Feature Plugins
    ↓
AST (SupramarkNode[])
    ↓
Platform Renderer
    ↓
React Element Tree
    ↓
Platform-specific Output
(DOM on Web, Native Components on RN)
```

### 示例

```typescript
// 1. 输入
const markdown = `# Hello\n\nThis is **bold** text.`

// 2. 解析
const ast = await parse(markdown)
/*
{
  type: 'root',
  children: [
    {
      type: 'heading',
      depth: 1,
      children: [{ type: 'text', value: 'Hello' }]
    },
    {
      type: 'paragraph',
      children: [
        { type: 'text', value: 'This is ' },
        {
          type: 'strong',
          children: [{ type: 'text', value: 'bold' }]
        },
        { type: 'text', value: ' text.' }
      ]
    }
  ]
}
*/

// 3. 渲染（Web）
<h1>Hello</h1>
<p>This is <strong>bold</strong> text.</p>

// 3. 渲染（RN）
<Text style={h1Style}>Hello</Text>
<Text>
  This is <Text style={strongStyle}>bold</Text> text.
</Text>
```

## 性能优化

### 1. 解析缓存

```typescript
const cache = new Map<string, SupramarkNode>();

async function parse(markdown: string, config: Config) {
  const cacheKey = hash(markdown + JSON.stringify(config));

  if (cache.has(cacheKey)) {
    return cache.get(cacheKey)!;
  }

  const ast = await parse(markdown, config);
  cache.set(cacheKey, ast);
  return ast;
}
```

### 2. 虚拟化长列表

```typescript
// 使用 react-window 或 react-native-virtualized-list
<VirtualizedList
  data={sections}
  renderItem={({ item }) => <Supramark markdown={item} />}
/>
```

### 3. 图表延迟渲染

```typescript
// 只在进入视口时渲染
function LazyDiagram({ code }: { code: string }) {
  const ref = useRef<HTMLDivElement>(null)
  const isVisible = useIntersectionObserver(ref)

  return (
    <div ref={ref}>
      {isVisible ? <DiagramRenderer code={code} /> : <Skeleton />}
    </div>
  )
}
```

## 类型安全

### TypeScript 优先

所有包都用 TypeScript 编写：

```typescript
// 严格的类型定义
interface SupramarkConfig {
  features: SupramarkFeature[];
  lazyLoad?: boolean;
  cache?: boolean;
}

// 类型推导
const config = {
  features: [mathFeature], // 自动推导类型
};
```

### 运行时验证

```typescript
import { validateFeature } from '@supramark/core';

const result = validateFeature(myFeature, {
  mode: 'strict',
});

if (!result.valid) {
  console.error(result.errors);
}
```

## 测试策略

### 单元测试

```typescript
describe('parse', () => {
  it('should parse headings', async () => {
    const ast = await parse('# Title');
    expect(ast.children[0].type).toBe('heading');
  });
});
```

### 集成测试

```typescript
describe('Supramark Component', () => {
  it('should render markdown correctly', () => {
    render(<Supramark markdown="**bold**" />)
    expect(screen.getByText('bold')).toHaveStyle({ fontWeight: 'bold' })
  })
})
```

### 快照测试

```typescript
it('should match snapshot', () => {
  const { container } = render(<Supramark markdown={complexMarkdown} />)
  expect(container).toMatchSnapshot()
})
```

## 扩展点

### 1. 自定义 Feature

继承 `SupramarkFeature` 接口

### 2. 自定义渲染器

覆盖特定节点的渲染器

### 3. 自定义解析能力

扩展 AST v2 规范、Rust parser rule，或编写 AST 后处理插件

### 4. 自定义样式

通过 `style` prop 覆盖默认样式

详细指南请参考 [自定义 Feature 开发](/guide/custom-features)。

## 设计原则

### 1. 跨平台一致性

同一份代码，Web 和 RN 都能运行

### 2. 按需加载

只加载需要的 Features，减小包体积

### 3. 可扩展性

开放的接口，方便第三方扩展

### 4. 类型安全

TypeScript 优先，编译时检查

### 5. 性能优先

缓存、虚拟化、延迟加载

## 未来规划

- [ ] 增强缓存策略
- [ ] 支持流式渲染
- [ ] Web Worker 解析（Web 端）
- [ ] 更多内置 Features
- [ ] 插件市场

## 参考资料

- [核心概念](/guide/concepts)
- [Features 文档](/features/)
- [API 参考](/api/)
- [TypeDoc](/typedoc/)
