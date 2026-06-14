# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## 仓库概览

Supramark 是面向 React Native / 小程序宿主的 Markdown 扩展与图表渲染**集成库**。使用 Bun 作为运行时与包管理器，采用 workspace 组织多包（见 `pnpm-workspace.yaml` 与 `package.json#workspaces`）。

- 项目尚未发布，不需要任何向后兼容（继承自 `../AGENTS.md`）。
- 默认使用中文对话；代码与提交信息使用英文。

## 常用命令

所有命令均在仓库根目录用 `bun` 执行。

```bash
# 构建 / 测试
bun run build                     # 对所有 workspace 执行 build（主要是 tsc --emitDeclarationOnly）
bun run test                      # 对所有 workspace 执行 test（若有）
bun run test:core                 # 仅 @supramark/core
bun run test:features             # 所有 @supramark/feature-*
bun --filter <pkg-name> test      # 单包：如 bun --filter @supramark/feature-math test
cd packages/<pkg> && bun test <file>   # 单个 feature 包内跑单测；ts-jest 也可用，preset 见 jest.preset.cjs

# Lint / 格式化
bun run lint                      # eslint + features:lint
bun run lint:fix
bun run features:lint             # 所有 feature 的静态质量检查（scripts/features-lint.ts）
bun run feature:lint <name>       # 单个 feature 严格模式
bun run format                    # prettier 写回

# Feature 脚手架 / 维护
bun run feature:create            # 交互式创建 feature 包，详见 docs/guide/CREATE_FEATURE_GUIDE.md
bun run feature:update
bun run feature:del
bun run features:sync             # 同步 feature 元数据 + 生成 bundle + 重建文档

# 文档与预览
bun run docs:generate             # 生成 features / api / example 文档
bun run feature:preview:web       # 启动 web 预览 sandbox
bun run feature:preview:ios|android|macos|windows    # examples/react-native 原生预览

# 质量聚合报告
bun run quality                   # 运行所有质量检查，CI 也会调用
```

注意：`@supramark/core` 走 `bun test`（Jest 兼容模式）；部分历史 feature 包可能仍然带 `jest.config.cjs`，它们继承 `jest.preset.cjs`。首选 `bun test`。

## 架构分层

顶层目录 `packages/` 分为三个逻辑层：

### 1. `packages/core` — `@supramark/core`

- **AST v2**：与 mdast 尽量兼容、但以 source map / 协作批注为一等目标的 `SupramarkNode`（见 `src/ast.ts`、`docs/architecture/ast-spec.md`）。
- **单一解析入口**：`parse(source)` 调用 Rust `supramark-markdown` canonical parser，公开合同固定为 `source -> AST v2`。旧 TS 解析线路已移除；TS 插件只做 AST 后处理。
- **Feature Interface**：`SupramarkFeature` 把 metadata / syntax / renderers / examples / testing / documentation / prompt 封装为一个**完整产品单元**。参见 `docs/architecture/PLUGIN_SYSTEM.md` 的「7 个核心 Trait」。
- **语法家族运行时**：`src/syntax/{main,container,fence,input}.ts` 给 feature 提供 `registerContainerHook` / `registerInputHook` 等可扩展点。
- **Container 扩展**：`container-feature.ts` 是新的精简版统一接口，用于实现 `:::` 型 container。

### 2. `packages/renderers/` — 渲染层

- `engines` (`@supramark/engines`) — **所有图表 / 公式渲染的唯一出口**。统一接口 `render({ engine, code }) => Promise<{ format: 'svg' | 'error', payload }>`。支持 mermaid / dot / vega / vega-lite / echarts / plantuml / mathjax（LaTeX 也走相同路径）。目录 `src/{web,rn}.ts` 是平台入口。
- `rn` (`@supramark/rn`) — React Native 渲染层。组件结构：`Supramark.tsx`（入口）、`DiagramNode.tsx`、`MathBlock/Inline.tsx`、`ErrorBoundary.tsx`、`styles.ts`、`svgUtils.ts`。
- `web` (`@supramark/web`) — React Web 渲染层，多入口（`.`、`./server`、`./client`）。

**关键架构约束**：renderer 自身**不直接**调用 Mermaid / Vega / ECharts 等图表库；它们只消费 `@supramark/engines` 返回的 SVG。修改/新增 diagram 能力时优先在 `packages/engines` 内完成。

### 3. `packages/features/` — Feature 扩展

扁平布局（除 `container/` 子目录外，feature 都直接在 `features/` 下）。每个 feature 包都实现 `SupramarkFeature`，并自带：
- `metadata` — id / version / tags / `syntaxFamily`（`main` | `container` | `fence`）
- `syntax.ast` — 通过 `type` 或 `selector` 匹配 AST 节点；parser rule 属于 `supramark-markdown` 内部实现，不作为 public API 暴露。
- `renderers.{web,rn}` — 声明 `infrastructure`（`needsWorker` / `needsCache` / `needsClientScript`）与 `dependencies`
- `examples` / `testing` / `documentation` — 被脚本 `doc-gen-*.ts` 与 CI 消费

当前已实现：core-markdown、gfm、math、footnote、definition-list、admonition、emoji、mermaid、diagram-dot、diagram-echarts、diagram-plantuml、diagram-vega-lite、weather；`container/` 下：feature-html-page、feature-map。

## Passive Rendering 模型

Core 不维护全局注册表；宿主通过 `<Supramark config={{ features: [...] }} />` 显式注入 feature 数组。渲染器在运行时动态构建 `node.type -> Component` 映射。因此：

- **不要添加全局副作用**（如模块级注册）。
- **不要**在渲染组件内直接 `import` 具体图表库；用 `useDiagramRender()` / `DiagramRenderContext`。
- 添加新 feature 时同步写 `examples`、`testing.syntaxTests` 与 `documentation.api`——CI 与文档生成都依赖这些字段。

## TypeScript 路径与导入

`tsconfig.base.json#paths` 把所有 `@supramark/*` 映射到各包的 `src/`，因此源码里直接 `import from '@supramark/core'` 即可，**不要**引 dist。`main/types/exports` 在各 `package.json` 里同样都指向 `src/`。

Feature 跨包导入同理——用 `@supramark/core` / `@supramark/feature-*` 别名，不要写相对路径跨越 `packages/`。

## 质量保障三层

1. **TypeScript** 严格模式（`tsconfig.base.json` 开启 `strict`）。
2. **运行时**：`validateFeature()`（core 导出），支持基本 / 严格 / 生产模式。
3. **静态 + CI**：`scripts/features-lint.ts`（`bun run lint:features`）打质量分；`.github/workflows/ci.yml` 在每次 PR 跑 `test:core --coverage`、`build`、`quality`。

详见 `docs/FEATURE_QUALITY_ASSURANCE.md`。

## 文档生成管线

`docs/` 是 VitePress 站点，不手写 feature 文档：

1. `scripts/features-sync.ts` 从各 feature 的 `metadata` / `examples` 同步到索引；
2. `scripts/features-gen-bundle.ts` 打出运行时 bundle；
3. `scripts/doc-gen-features.ts` / `doc-gen-api.ts` / `doc-gen-example.ts` 产出 markdown。

修改 feature 后运行 `bun run features:sync` 以保持文档一致。
