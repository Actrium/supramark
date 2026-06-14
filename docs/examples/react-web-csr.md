# React Web CSR 示例

这是一个使用 **Supramark** 的浏览器端（CSR - Client-Side Rendering）示例应用，展示如何在 React Web 应用中实现实时 Markdown 编辑器。

## 功能特性

- ✅ **实时预览**：编辑 Markdown 时实时渲染预览
- ✅ **GFM 支持**：完整支持 GitHub Flavored Markdown
  - 删除线（`~~text~~`）
  - 任务列表（`- [ ]` / `- [x]`）
  - 表格
- ✅ **代码高亮**：支持代码块渲染
- ✅ **响应式设计**：适配桌面和移动端

## 技术栈

- **构建工具**: Vite
- **框架**: React + TypeScript
- **Markdown 渲染**: @supramark/web (client 入口)

## 快速开始

### 安装依赖

\`\`\`bash
npm install
\`\`\`

### 开发模式

\`\`\`bash
npm run dev
\`\`\`

然后打开浏览器访问 \`http://localhost:5173\`

### 生产构建

\`\`\`bash
npm run build
\`\`\`

构建产物将输出到 \`dist/\` 目录。

### 预览构建结果

\`\`\`bash
npm run preview
\`\`\`

## 使用方法

### 基础用法

在 React 组件中导入并使用 Supramark：

\`\`\`typescript
import { Supramark } from '@supramark/web/client';

function App() {
const [markdown, setMarkdown] = useState('# Hello World');

return (
<div>
<textarea
value={markdown}
onChange={(e) => setMarkdown(e.target.value)}
/>
<Supramark markdown={markdown} />
</div>
);
}
\`\`\`

### 预解析优化

如果需要更好的性能，可以预先解析 AST：

\`\`\`typescript
import { Supramark, parse } from '@supramark/web/client';

// 在组件外或 useEffect 中解析
const ast = await parse('# Hello World');

function App() {
return <Supramark ast={ast} markdown="" />;
}
\`\`\`

## Vite 配置

本示例使用标准的 Vite 配置，无需特殊设置。\`@supramark/web\` 已通过 package.json 的 \`exports\` 字段正确配置，可以直接导入 \`/client\` 入口。

## 项目结构

\`\`\`
react-web-csr/
├── src/
│ ├── App.tsx # 主应用组件
│ ├── App.css # 样式文件
│ ├── main.tsx # 入口文件
│ └── index.css # 全局样式
├── package.json
├── vite.config.ts
└── tsconfig.json
\`\`\`

## 了解更多

- [Supramark 文档](../../README.md)
- [Vite 文档](https://vitejs.dev/)
- [React 文档](https://react.dev/)


## 快速开始

```bash
cd examples/react-web-csr
bun install
bun run dev
```

## Supramark 依赖

- `@supramark/web` - file:../../packages/renderers/web
- `@supramark/core` - file:../../packages/core
- `@supramark/feature-weather` - file:../../packages/features/feature-weather

## 源代码

### App.tsx

```tsx
const INITIAL_MARKDOWN = `# Supramark Live Editor

欢迎使用 **Supramark** 的实时 Markdown 编辑器！

## 功能特性

### GFM 扩展支持

- **粗体文本**
- *斜体文本*
- \`内联代码\`
- ~~删除线~~

### 任务列表

- [x] 支持 GFM 任务列表
- [x] 实时预览
- [x] 主题切换
- [ ] 更多功能开发中

### 表格示例

| 功能 | 状态 | 说明 |
| --- | :---: | ---: |
| 删除线 | ✅ | 使用 \`~~\` 语法 |
| 任务列表 | ✅ | \`[ ]\` 和 \`[x]\` |
| 表格 | ✅ | 标准 GFM 表格 |
| 主题系统 | ✅ | 支持自定义 className |

### 代码块

\`\`\`javascript
function hello(name) {
  console.log('Hello, ' + name);
}

hello('Supramark');
\`\`\`

### 链接和图片

这是一个 [链接示例](https://github.com)

### Weather 卡片

:::weather
location: Shanghai
condition: Cloudy
tempC: 22
:::
```

### main.tsx

```tsx
const params = new URLSearchParams(window.location.search);
const featureParam = params.get('feature');

if (featureParam) {
  const featureModule = () => import(`@supramark/feature-${featureParam}`);
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <FeaturePreview featureName={featureParam} featureImport={featureModule} />
    </StrictMode>
  );
} else {
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <App />
    </StrictMode>
  );
}

```

## 项目结构

```
examples/react-web-csr/
├── src/
├── public/
├── package.json
└── README.md
```

## 相关资源

- [快速开始](/guide/getting-started)
- [API 参考](/api/)
- [其他示例](/examples/)

---
*此文档由 scripts/doc-gen-example.ts 自动生成*
