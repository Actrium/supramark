# Supramark 项目结构报告

## 仓库概览

- **定位与目标**：面向 React Native / 小程序宿主的 Markdown 扩展和图表渲染集成库，统一解析、插件与重图表引擎后台渲染。
- **包管理方式**：根目录使用 npm workspaces（`packages/core`、`packages/renderers/*`、`packages/features/*/*`、`examples/*`），便于多包协同开发与构建。
- **TypeScript 基线**：`tsconfig.base.json` 定义统一的严格编译选项（ES2019、ESNext modules、`react-jsx`、声明/映射输出等），所有包继承该配置。

## 核心包与分层

### `packages/core`

- 功能：AST v2 类型定义、插件接口、Rust `supramark-markdown` 解析 facade，以及 KaTeX 等依赖。
- 工具链：提供 `tsc` 构建、Jest 测试、TypeDoc 文档生成任务，是其他渲染层与 Feature 包的基础。

### 渲染器层 `packages/renderers/`

渲染器相关包按逻辑分组放置在 `renderers/` 目录下，便于统一管理与扩展。

#### `packages/renderers/rn`

- 功能：React Native 渲染层，把 supramark AST 映射成 RN 组件并内置 Markdown / Math / Diagram 等渲染。
- 依赖：直接依赖 `@supramark/core` 与 `@supramark/engines`，并通过 `react-native-svg` 展示 SVG 图表。

#### `packages/renderers/web`

- 功能：提供 `<Supramark />` React 组件（client/server），以及与 Web 图表产物的集成。
- 依赖：依赖 `@supramark/core` 与 `@supramark/engines`，暴露多入口 exports 适应浏览器与 SSR。

## Feature 扩展体系

- 存放位置：`packages/features/{main|container|fence}/feature-*`，按语法家族分组。
- 已含能力：核心 Markdown、GFM、Math、Footnote、Definition List、Admonition、Emoji、HTML Page、Map 等。
- 每个 Feature 包：独立 npm 包，包含 `dist/src/README`、`tsc` 构建、Jest 测试脚本，并将 `@supramark/core` 作为 workspace peer 依赖。

## 示例与演示

- React Native 示例：`examples/react-native` 使用 “目录 + 示例详情” 布局展示所有能力。
- React Web 示例：`examples/react-web`（SSR）与 `examples/react-web-csr`（Vite CSR）展示 `<Supramark />` 在浏览器端的渲染。
- 示例数据聚合：`examples/demos.ts` 从各 Feature 包导入示例并生成统一 `DEMOS` 数据，供多端示例共享。
- 配置示例：`examples/config-examples` 提供 bundler 配置模板，`examples/demos.(ts|mjs)` 提供脚本化数据聚合。

## 文档体系

- 文档站：`docs/` 使用 VitePress，脚本 `docs:generate:*` 先生成 Feature/示例/API 文档，再通过 `vitepress` 构建/预览。
- 内容：包含架构说明、插件指南、Feature 生命周期、质量保障策略、CI 设置等文件以及生成的 TypeDoc。
- 静态资源：`docs/public` 承载站点资源，`docs/typedoc` 存放从 `packages/core` 构建的 API 文档。

## 工具链与质量保障

- 脚本：`scripts/` 目录提供 Feature 创建/更新、lint、质量检查、示例与文档生成等自动化脚本，配合 npm scripts 统一执行。
- 测试与配置：根级 Jest 配置（`jest-environment.cjs`、`jest.preset.cjs`）及各包 `jest.config.cjs` 支持多包单测；`tsconfig.base.json` 与质量检查脚本保持类型一致性。
- 质量体系：README 中描述的三层保障（TypeScript 强类型、运行时 `validateFeature`、Feature Linter + CI）确保 Feature 规范性与稳定性。
