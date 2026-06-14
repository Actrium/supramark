# Supramark 文档系统

本目录包含 Supramark 的自动化文档生成系统。

## 🎯 设计理念

**主体自动生成（~90%），框架手动维护（~10%）**

文档内容从源代码、Feature 定义和示例项目中自动提取，确保与代码保持 100% 同步。

## 📦 文档来源

### 1. ✅ 自动生成的文档（主体内容）

#### Feature 文档 (`docs/features/*.md`)

- **来源**: `packages/features/*/feature-*/src/feature.ts` 的 `documentation.api` 字段
- **生成脚本**: `scripts/generate-feature-docs.mjs`
- **包含内容**:
  - Feature 元信息
  - API 参考（functions, interfaces）
  - 最佳实践
  - FAQ

**生成命令**:

```bash
npm run docs:generate:features
```

#### API 文档 (`docs/api/*.md`)

- **来源**: `packages/*/src/index.ts` 的 JSDoc 注释
- **生成脚本**: `scripts/generate-api-docs.mjs`
- **包含内容**:
  - 包说明
  - 导出的函数和类型
  - 参数和返回值
  - 使用示例

**生成命令**:

```bash
npm run docs:generate:api
```

#### 示例文档 (`docs/examples/*.md`)

- **来源**: `examples/` 目录中的示例项目
- **生成脚本**: `scripts/generate-example-docs.mjs`
- **包含内容**:
  - 示例项目说明
  - 源代码片段
  - 运行方法
  - 依赖列表

**生成命令**:

```bash
npm run docs:generate:examples
```

#### TypeDoc 文档 (`docs/public/typedoc/`)

- **来源**: `@supramark/core` 的 TypeScript 源代码
- **生成工具**: TypeDoc
- **包含内容**:
  - 完整的类型定义（80+ 接口）
  - 类型层级关系
  - 详细的 API 参考

**生成命令**:

```bash
cd ../packages/core
npm run docs
cd ../../docs
npm run docs:copy-typedoc
```

### 2. ❌ 手动维护的框架（最小化）

以下文件需要手动维护（仅框架和导航）：

- `docs/.vitepress/config.mts` - VitePress 配置（导航、侧边栏）
- `docs/.vitepress/theme/` - 主题和样式
- `docs/index.md` - 首页（Hero 布局）
- `docs/guide/concepts.md` - 核心概念（可选，概览性内容）
- `docs/guide/architecture.md` - 架构设计（可选，设计思路）

## 🚀 使用方法

### 开发模式

```bash
cd docs
npm install

# 生成所有文档
npm run docs:generate

# 启动开发服务器
npm run docs:dev
# 访问 http://localhost:5173/supramark/
```

### 构建生产版本

```bash
cd docs

# 生成文档并构建
npm run docs:build

# 预览构建结果
npm run docs:preview
```

### 单独生成特定文档

```bash
# 只生成 Feature 文档
npm run docs:generate:features

# 只生成 API 文档
npm run docs:generate:api

# 只生成示例文档
npm run docs:generate:examples

# 只复制 TypeDoc
npm run docs:copy-typedoc
```

## 📁 目录结构

```
docs/
├── .vitepress/
│   ├── config.mts          # VitePress 配置 [手动]
│   └── theme/              # 主题配置 [手动]
│       ├── index.ts
│       └── custom.css
├── public/
│   └── typedoc/            # TypeDoc HTML [自动生成]
├── index.md                # 首页 [手动]
├── guide/                  # 用户指南
│   ├── getting-started.md  # 快速开始 [手动，简短]
│   ├── concepts.md         # 核心概念 [手动，概览]
│   └── architecture.md     # 架构设计 [手动，设计思路]
├── features/               # Feature 文档 [自动生成]
│   ├── index.md
│   ├── core-markdown.md
│   ├── gfm.md
│   ├── math.md
│   └── ... (8+ 文件)
├── api/                    # API 文档 [自动生成]
│   ├── index.md            # API 索引 [手动，简短]
│   ├── core.md
│   ├── web.md
│   └── rn.md
├── examples/               # 示例文档 [自动生成]
│   ├── index.md
│   ├── react-web.md
│   ├── react-web-csr.md
│   └── react-native.md
└── typedoc/                # TypeDoc 集成页 [手动，简短]
    └── index.md
```

## 🔄 文档更新流程

### 更新 Feature 文档

1. 修改 `packages/features/*/feature-*/src/feature.ts` 中的 `documentation.api` 字段
2. 运行 `npm run docs:generate:features`
3. 检查生成的 `docs/features/*.md` 文件
4. 提交变更

### 更新 API 文档

1. 在源代码中添加/更新 JSDoc 注释
   ````typescript
   /**
    * Parse Markdown text to AST
    * @param markdown - The markdown text
    * @param options - Parse options
    * @returns Parsed AST
    * @example
    * ```ts
    * const ast = await parse('# Hello')
    * ```
    */
   export async function parse(...)
   ````
2. 运行 `npm run docs:generate:api`
3. 检查生成的 `docs/api/*.md` 文件
4. 提交变更

### 更新示例文档

1. 修改 `examples/` 目录中的示例代码
2. 运行 `npm run docs:generate:examples`
3. 检查生成的 `docs/examples/*.md` 文件
4. 提交变更

### 更新 TypeDoc

1. 在 `@supramark/core` 源代码中添加/更新类型定义和注释
2. 运行:
   ```bash
   cd packages/core
   npm run docs
   cd ../../docs
   npm run docs:copy-typedoc
   ```
3. 提交变更

## 🎨 自定义和扩展

### 添加新的 Feature

1. 创建 Feature 包并完善 `documentation.api` 字段
2. 在 `scripts/generate-feature-docs.mjs` 的 `FEATURES` 数组中添加 Feature 名称
3. 运行 `npm run docs:generate:features`

### 添加新的示例项目

1. 在 `examples/` 创建新的示例项目
2. 在 `scripts/generate-example-docs.mjs` 的 `EXAMPLES` 数组中添加配置
3. 运行 `npm run docs:generate:examples`

### 自定义主题

编辑 `docs/.vitepress/theme/custom.css` 修改样式。

### 添加导航项

编辑 `docs/.vitepress/config.mts` 的 `nav` 和 `sidebar` 配置。

## 📊 文档覆盖率

当前文档覆盖：

- ✅ Feature 文档: 8 个包，100% 自动生成
- ✅ API 文档: 3 个包（core, web, rn），100% 自动生成
- ✅ 示例文档: 3 个项目，100% 自动生成
- ✅ TypeDoc: 1 个包（core），100% 自动生成
- ⚠️ 指南文档: 3 个页面，手动维护（框架性内容）

**自动化比例**: ~90%
**手动维护**: ~10% (仅框架和导航)

## 🔧 故障排查

### Feature 文档生成失败

- 检查 `src/feature.ts` 中的 `documentation.api` 字段格式
- 运行 `npm run lint:features` 检查 Feature 质量
- 查看生成脚本的错误输出

### API 文档缺失内容

- 确保源代码中有 JSDoc 注释
- 检查注释格式是否正确
- 验证 `@param` 和 `@returns` 标签

### 示例文档不完整

- 检查示例项目的 `package.json` 和 `README.md`
- 确保 `src/` 目录中有主要源文件
- 查看生成脚本的警告信息

### TypeDoc 未更新

- 确保运行了 `cd packages/core && npm run docs`
- 检查 `packages/core/docs/api` 目录是否存在
- 确保运行了 `npm run docs:copy-typedoc`

## 🚀 CI/CD 集成

建议在 CI 中添加文档生成检查：

```yaml
# .github/workflows/docs.yml
name: Docs

on: [push, pull_request]

jobs:
  generate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
      - run: npm install
      - run: cd docs && npm install
      - run: cd docs && npm run docs:generate
      - run: cd docs && npm run docs:build
```

## 📝 未来改进

- [ ] 从测试用例自动生成使用示例
- [ ] 添加交互式 API playground
- [ ] 改进 JSDoc 解析（使用 AST）
- [ ] 添加文档版本管理
- [ ] 集成 Algolia DocSearch

## 🎯 设计原则

1. **Single Source of Truth** - 文档内容来自源代码
2. **自动化优先** - 尽量减少手动维护
3. **保持同步** - 文档随代码自动更新
4. **类型安全** - 利用 TypeScript 类型系统
5. **易于扩展** - 便于添加新的文档来源
