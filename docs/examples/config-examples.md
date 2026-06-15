# 构建配置示例

本目录包含在不同构建工具中集成 Supramark 的配置示例。

## 目录结构

```
config-examples/
├── vite/              # Vite 配置示例
│   ├── vite.config.ts
│   └── README.md
├── webpack/           # Webpack 配置示例
│   ├── webpack.config.js
│   ├── package.json.example
│   └── README.md
└── README.md          # 本文件
```

## 快速导航

### [Vite 配置](./vite/)

适用于使用 Vite 的项目（推荐）。

**特点：**

- ⚡ 开箱即用，无需特殊配置
- 🚀 极快的开发服务器启动
- 📦 优化的生产构建

**查看示例：**

- [vite.config.ts](./vite/vite.config.ts) - 完整配置文件
- [README.md](./vite/README.md) - 详细使用说明

---

### [Webpack 配置](./webpack/)

适用于使用 Webpack 的项目（包括 Create React App）。

**特点：**

- 🔧 灵活的配置选项
- 📊 强大的代码分割
- 🛠️ 成熟的生态系统

**查看示例：**

- [webpack.config.js](./webpack/webpack.config.js) - 完整配置文件
- [package.json.example](./webpack/package.json.example) - 依赖列表
- [README.md](./webpack/README.md) - 详细使用说明

---

## 选择指南

### 推荐使用 Vite

如果你满足以下任一条件：

- ✅ 创建新项目
- ✅ 不需要兼容旧版浏览器（ES2015+）
- ✅ 需要快速的开发体验
- ✅ 项目规模较小到中等

### 推荐使用 Webpack

如果你满足以下任一条件：

- ✅ 需要兼容旧版浏览器
- ✅ 已有 Webpack 项目
- ✅ 需要高度定制化的构建流程
- ✅ 使用 Create React App（CRA）

---

## 通用配置要点

无论使用哪个构建工具，都需要注意以下要点：

### 1. 安装依赖

```bash
npm install @supramark/web @supramark/core react react-dom
```

### 2. 导入正确的入口

**客户端渲染（CSR）：**

```typescript
import { Supramark } from '@supramark/web/client';
```

> 当前版本仅支持客户端渲染。SSR 入口已下线 —— diagram 渲染需要浏览器
> canvas 做字体测量（layer-1 layout）+ 浏览器实际字体堆栈做绘制
> （layer-3 render），服务端无法完成端到端 byte-equal 输出。普通
> markdown 节点可在客户端 hydrate 时由 React 一次性渲染。

### 3. TypeScript 支持

确保 `tsconfig.json` 配置正确：

```json
{
  "compilerOptions": {
    "jsx": "react-jsx",
    "module": "ESNext",
    "moduleResolution": "node",
    "esModuleInterop": true
  }
}
```

---

## 完整示例项目

查看完整的可运行示例：

- **[React Web CSR](../react-web-csr/)** - 完整的客户端渲染示例
  - 使用 Vite 构建
  - 实时 Markdown 编辑器
  - 包含 Mermaid 图表演示

---

## 其他框架

### Next.js

Next.js 13+ (App Router) 把 supramark 放在客户端组件里使用：

```typescript
// components/Editor.tsx
'use client';
import { Supramark } from '@supramark/web/client';

export function Editor({ markdown }: { markdown: string }) {
  return <Supramark markdown={markdown} />;
}
```

### Remix / Astro

同样仅在客户端组件 / island 里使用 `Supramark`，服务端只承担页面骨架渲染。
Diagram 节点会在客户端 hydrate 后通过 host canvas 完成测量 + 渲染。

---

## 常见问题

### Q: 如何优化打包体积？

**A:** 使用代码分割：

```typescript
// 动态导入
const Supramark = lazy(() => import('@supramark/web/client').then(m => ({ default: m.Supramark })));
```

### Q: 如何在 TypeScript 中获得完整类型支持？

**A:** 确保安装了类型定义：

```bash
npm install --save-dev @types/react @types/react-dom
```

类型会自动从 `@supramark/web` 和 `@supramark/core` 导出。

### Q: 遇到 "Module not found" 错误？

**A:** 检查以下几点：

1. 确保安装了所有依赖
2. 检查导入路径（`/client` 或 `/server`）
3. 对于 Webpack，确保配置了 `resolve.conditionNames`

---

## 更多资源

- [Web 集成指南](../../docs/guides/web-integration.md) - 完整的集成文档
- [API 文档](../../docs/api/) - API 参考
- [插件开发](../../docs/plugins/plugin-api.md) - 自定义插件开发

---

**需要帮助？** 提交 Issue 到 GitHub 仓库或查阅文档。

## 快速开始

```bash
cd examples/config-examples
bun install
```

## 项目结构

```
examples/config-examples/
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

_此文档由 scripts/doc-gen-example.ts 自动生成_
