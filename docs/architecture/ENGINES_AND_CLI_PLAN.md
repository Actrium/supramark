# Supramark · Engines & CLI 重构方案

> 本文是 `@supramark/engines` 从"service-based wiring"迁移到
> "纯函数 engine + config 驱动 codegen"的总设计文档。它**取代** 
> `DIAGRAM_ENGINE_TARGET.md` 作为后续实施依据。
>
> 目标版本：v0.2。状态：设计定稿，待实施。

## 1 · 动因

当前架构有三个核心问题，单独看都能修，合起来要求整体重塑：

1. **使用方接入成本高**。`createWebDiagramEngine()` / `DiagramEngineProvider` /
   `containerRenderers` / `features` 四处散落的装配代码在 CSR / SSR / RN 三端各写一遍。
2. **Diagram engine 扩展不对称**。`engine.ts` 的 switch 只识别
   mermaid / math / graphviz，ECharts / PlantUML / Vega-Lite 在 parser 层被当 diagram
   节点产出，但 renderer 层 fallback 为原样代码块——默默失败，难以诊断。
3. **Bundle 控制粒度粗**。引入一个 engine 就是全量，ECharts 无法"只打 LineChart"。
   大多数生产项目实际只用 2–3 个 chart type，浪费 700KB+。

## 2 · 设计目标

| 目标 | 体现 |
|---|---|
| 使用方侧 API 最小化 | 99% 场景只接触 `render(md)` / `<Supramark markdown />` 两个符号 |
| 单一事实来源（SSOT） | 一份 `supramark.config.json`，所有装配从它派生 |
| 完美 tree-shake | 顶层静态 default import + `sideEffects:false`；编译器能砍到单个 chart type |
| 零反向依赖 | engine 是纯函数不 care 在哪跑；worker / quickjs / 主线程由 host 决定 |
| 统一的 engine 形态 | 每个 engine `(modules?) => (code, options) => Promise<string>` |
| 错误可统一处理 | 一个 `DiagramRenderError` 类，`e.code` 离散化错误类别 |
| 扩展不波及下游 | 加 chart type / 新 engine 只改 config + 跑 codegen，业务代码不改 |

## 3 · 架构总览

```
┌─────────────────────────────────────────────────┐
│                    宿主 app                      │
│  ─ supramark.config.json  (自己写)              │
│  ─ src/generated/supramark.ts  (CLI 生成, commit)│
│  ─ import { render, Supramark } from ./generated │
└───────────────────────────┬─────────────────────┘
                            │ static default imports
                            ↓
┌─────────────────────────────────────────────────┐
│  @supramark/engines                       │
│    runtime/{createRender,createSupramark}        │
│    types.ts                                      │
│    mermaid / mathjax / graphviz / echarts /      │
│    vega-lite  (每个一个 default export 工厂)     │
│    echarts/<SubType>  (codegen 产出的一行 re-export)│
└─────────────────────────────────────────────────┘
                            ↑
                            │ (build-time only)
┌─────────────────────────────────────────────────┐
│  @supramark/cli  (devDep, bin)                   │
│    读 supramark.config.json → 写 generated 文件  │
│    schema/config.v1.json（$schema 目标）         │
│    presets/{minimal,blog,docs,data-viz,ai-chat}  │
└─────────────────────────────────────────────────┘
```

`@supramark/cli` 只出现在 devDependency + `prebuild` 脚本中，**不进运行时 bundle**。

## 4 · `@supramark/engines` 设计

### 4.1 · 目录结构

```
packages/renderers/diagram-engine/src/
├── runtime/
│   ├── createRender.ts              ← (spec) → (md, opts) => Promise<string>
│   ├── createSupramark.tsx          ← (spec) → React.FC
│   └── index.ts
├── types.ts                          ← RenderOptions, DiagramRenderError, ErrorCode
│
├── mermaid/
│   └── index.ts                      ← default: factory(modules?) → RenderFn
├── mathjax/
│   └── index.ts                      ← default: factory(modules?) → RenderFn
│
├── graphviz/
│   ├── index.ts                      ← default: factory(modules) → RenderFn
│   ├── web-adapter.ts                ← default: WebAdapter (懒加载 wasm)
│   └── rn-adapter.ts                 ← default: RNAdapter (native binding)
│
├── echarts/
│   ├── index.ts                      ← default: factory(modules) → RenderFn
│   ├── LineChart.ts                  ← codegen 产物：export { LineChart as default } from 'echarts/charts'
│   ├── BarChart.ts
│   ├── ...(~40 个 chart/component/renderer/theme subtypes)
├── vega-lite/
│   ├── index.ts                      ← default: factory(modules) → RenderFn
│   ├── vega.ts                       ← export { default } from 'vega'
│   └── compile.ts                    ← export { compile as default } from 'vega-lite'
│
├── scripts/
│   └── gen-subpaths.ts               ← 内部 codegen：扫 echarts 导出列表，生成 subtype 文件
└── index.ts                          ← 仅导类型；业务不从顶层引
```

### 4.2 · 统一 Engine 契约

**每个 engine 模块默认导出一个工厂函数，签名完全一致：**

```ts
// src/<engine>/index.ts
export default function (modules?: unknown[]):
  (code: string, options?: Options) => Promise<string>;
```

- `modules` 是可选数组，装配期依赖由 host 静态 import 后传入（chart type、adapter、
  vega runtime 等）。
- 返回值是 `render` 函数，异步，输出 SVG 字符串。
- 错误通过 `throw new DiagramRenderError(...)` 抛出，不用 `{ success, payload }` 包装。

**每个 engine 自己定义 `Options`（extends 公共 `RenderOptions`）**：

```ts
// src/mermaid/index.ts
import { type RenderOptions, DiagramRenderError } from '../types.js';

export interface Options extends RenderOptions {
  theme?: 'default' | 'dark' | 'neutral' | 'forest';
  fontFamily?: string;
}

export default function mermaid(_modules?: unknown[]) {
  return async (code: string, options?: Options): Promise<string> => {
    options?.signal?.throwIfAborted();
    try {
      // ... renderMermaidSvg 核心逻辑
    } catch (e) {
      throw new DiagramRenderError(
        `Mermaid render failed: ${extractMessage(e)}`,
        { engine: 'mermaid', code: 'render_error', input: code.slice(0, 200), cause: e }
      );
    }
  };
}
```

### 4.3 · 通用类型

```ts
// src/types.ts

/** 所有 engine 都识别的渲染选项 */
export interface RenderOptions {
  signal?: AbortSignal;
  width?: number;
  height?: number;
  theme?: 'light' | 'dark' | string;
}

/** 所有 engine 失败时抛的统一错误类型 */
export class DiagramRenderError extends Error {
  readonly engine: string;
  readonly code: ErrorCode;
  readonly input?: string;
  constructor(message: string, init: {
    engine: string;
    code: ErrorCode;
    input?: string;
    cause?: unknown;
  }) {
    super(message, { cause: init.cause });
    this.name = 'DiagramRenderError';
    this.engine = init.engine;
    this.code = init.code;
    this.input = init.input;
  }
}

export type ErrorCode =
  | 'parse_error'         // 输入格式非法（JSON/DOT 解析失败）
  | 'render_error'        // engine 运行期失败
  | 'engine_unavailable'  // 依赖未安装 / 环境不支持
  | 'aborted'             // 被 AbortSignal 取消
  | 'unsupported';        // engine 不认该 code 种类
```

### 4.4 · Runtime：`createRender` / `createSupramark`

```ts
// src/runtime/createRender.ts
export interface RenderSpec {
  engines: Record<string, (code: string, options?: RenderOptions) => Promise<string>>;
  features: FeatureConfig;
}

export function createRender(spec: RenderSpec) {
  return async (markdown: string, options?: RenderOptions): Promise<string> => {
    const ast = await parse(markdown, { config: toSupramarkConfig(spec.features) });
    // 预渲染所有 diagram/math 节点
    // 序列化为 HTML string
    // 返回
  };
}
```

```ts
// src/runtime/createSupramark.tsx
export function createSupramark(spec: RenderSpec): React.FC<SupramarkProps> {
  return function Supramark({ markdown, theme, className }) {
    // 内部调 parse + 预渲染 + 遍历 AST 出 React 树
    // 所有 engines / features 已经固化在 spec 里
  };
}
```

生成的文件调用这两个工厂后**闭包捕获 spec**，业务代码 `import { render, Supramark } from './generated'` 看不到任何 spec / engines / features 的概念。

### 4.5 · package.json `exports`

```jsonc
{
  "exports": {
    ".":                       "./src/index.ts",
    "./types":                 "./src/types.ts",
    "./runtime/*":             "./src/runtime/*.ts",

    "./mermaid":               "./src/mermaid/index.ts",
    "./mathjax":               "./src/mathjax/index.ts",

    "./graphviz":              "./src/graphviz/index.ts",
    "./graphviz/*":            "./src/graphviz/*.ts",

    "./echarts":               "./src/echarts/index.ts",
    "./echarts/*":             "./src/echarts/*.ts",

    "./vega-lite":             "./src/vega-lite/index.ts",
    "./vega-lite/*":           "./src/vega-lite/*.ts"
  },
  "sideEffects": false
}
```

`sideEffects: false` 是 tree-shake 的**关键开关**——bundler 收到这个声明后能放心砍掉
没引用的默认导出。

### 4.6 · Subtype Subpath Codegen

ECharts 的 `engines/echarts/<Name>.ts` 是每个一行的 re-export 文件：

```ts
// echarts/LineChart.ts
export { LineChart as default } from 'echarts/charts';
```

这些文件**不手写**，由 `scripts/gen-subpaths.ts` 生成：

```ts
import * as echartsCharts from 'echarts/charts';
import * as echartsComponents from 'echarts/components';
import * as echartsRenderers from 'echarts/renderers';

const TABLE = {
  echarts: {
    'echarts/charts':     Object.keys(echartsCharts),
    'echarts/components': Object.keys(echartsComponents),
    'echarts/renderers':  Object.keys(echartsRenderers),
  },
};

for (const [engine, groups] of Object.entries(TABLE)) {
  for (const [srcPkg, names] of Object.entries(groups)) {
    for (const name of names) {
      fs.writeFileSync(
        `src/${engine}/${name}.ts`,
        `export { ${name} as default } from '${srcPkg}';\n`
      );
    }
  }
}
```

在 `package.json#scripts` 挂 `prebuild` 和 `postinstall`，产物 **commit 进仓库**，
下游 `pnpm install` 不跑脚本。

## 5 · `@supramark/cli` 设计

### 5.1 · 目录结构

```
packages/cli/
├── bin/
│   └── supramark-gen                    ← #!/usr/bin/env node shebang
├── src/
│   ├── cli.ts                            ← argv 解析（用 mri 或 cac）
│   ├── generator.ts                      ← 核心：config → generated.ts 字符串
│   ├── validator.ts                      ← ajv 校验 + 语义检查
│   ├── presets/
│   │   ├── minimal.json
│   │   ├── blog.json
│   │   ├── docs.json
│   │   ├── data-viz.json
│   │   └── ai-chat.json
│   └── schema/
│       └── config.v1.json                ← JSON Schema (draft-07)
├── package.json                          ← "bin": { "supramark-gen": "./bin/supramark-gen" }
└── README.md
```

### 5.2 · 命令规格

```
supramark-gen [options]

  -c, --config <path>     配置文件路径      [default: ./supramark.config.json]
  -o, --out <path>        生成文件路径      [default: ./src/generated/supramark.ts]
  -p, --preset <name>     使用内置 preset    [与 --config 互斥]
      --check             仅校验，漂移即失败（CI 用）
      --list-presets      列出可用 preset
  -h, --help
  -v, --version
```

**行为约定**：

- 不传 `--config` 也不传 `--preset` → 默认找 `./supramark.config.{json,js,ts}`。
- `--check` 模式：读 config → 在内存里生成一份 → 对比 `--out` 路径上已有文件的
  sha256，不一致则非零退出。
- config 里可以写 `"out"` 字段覆盖 CLI 默认输出路径。

### 5.3 · Config JSON Schema (v1)

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://supramark.dev/schema/config.v1.json",
  "title": "SupramarkConfig",
  "type": "object",
  "properties": {
    "out": {
      "type": "string",
      "description": "Output path for generated file. CLI --out overrides this."
    },
    "features": {
      "type": "object",
      "properties": {
        "gfm":             { "type": ["boolean", "object"] },
        "math":            { "type": ["boolean", "object"] },
        "footnote":        { "type": "boolean" },
        "emoji":           { "type": ["boolean", "object"] },
        "admonition":      { "type": ["boolean", "object"] },
        "definition-list": { "type": "boolean" }
      }
    },
    "mermaid":    { "type": ["boolean", "object"] },
    "mathjax":    { "type": ["boolean", "object"] },
    "graphviz":   { "enum": [false, "web", "rn"] },
    "echarts": {
      "type": "object",
      "properties": {
        "charts": {
          "oneOf": [
            { "const": "*" },
            { "type": "array", "items": { "enum": [
              "Line", "Bar", "Pie", "Scatter", "Radar", "Map", "Tree", "TreeMap",
              "Sunburst", "Boxplot", "Candlestick", "Heatmap", "Parallel",
              "Lines", "Graph", "Sankey", "Funnel", "Gauge", "PictorialBar",
              "ThemeRiver", "Effect", "Lines", "Custom"
            ] } }
          ]
        },
        "components": {
          "oneOf": [
            { "const": "*" },
            { "type": "array", "items": { "enum": [
              "Grid", "Polar", "GeoComponent", "SingleAxis", "Parallel",
              "Calendar", "Graphic", "Toolbox", "Tooltip", "AxisPointer",
              "Brush", "Title", "Timeline", "MarkPoint", "MarkLine", "MarkArea",
              "Legend", "DataZoom", "VisualMap", "Dataset", "Transform", "Aria"
            ] } }
          ]
        },
        "renderers": {
          "type": "array",
          "items": { "enum": ["svg", "canvas"] },
          "default": ["svg"]
        }
      },
      "required": ["charts"]
    },
    "vega-lite": { "type": ["boolean", "object"] }
  }
}
```

完整 chart / component 枚举由 codegen 基于 echarts 实际导出再同步（schema 文件
也随版本更新）。

### 5.4 · Preset 参考

**`minimal.json`**（裸 Markdown）：
```json
{
  "$schema": "https://supramark.dev/schema/config.v1.json",
  "features": { "gfm": true }
}
```

**`blog.json`**（博客）：
```json
{
  "$schema": "https://supramark.dev/schema/config.v1.json",
  "features": {
    "gfm": true, "math": true, "footnote": true, "emoji": true
  }
}
```

**`docs.json`**（技术文档）：
```json
{
  "$schema": "https://supramark.dev/schema/config.v1.json",
  "features": {
    "gfm": true, "math": true, "footnote": true, "emoji": true,
    "admonition": true, "definition-list": true
  },
  "mermaid":  true,
  "mathjax":  true,
  "graphviz": "web"
}
```

**`data-viz.json`**（数据可视化重载）：
```json
{
  "$schema": "https://supramark.dev/schema/config.v1.json",
  "features": { "gfm": true, "math": true },
  "echarts": {
    "charts": ["Line", "Bar", "Pie", "Scatter", "Heatmap"],
    "components": ["Grid", "Tooltip", "Legend", "DataZoom"]
  },
  "vega-lite": true,
  "mermaid":   true
}
```

**`ai-chat.json`**（LLM 流式输出场景）：
```json
{
  "$schema": "https://supramark.dev/schema/config.v1.json",
  "features": {
    "gfm": true, "math": true, "footnote": true,
    "emoji": true, "admonition": true
  },
  "mermaid":  true,
  "mathjax":  true,
  "graphviz": "web",
  "echarts":  {
    "charts": ["Line", "Bar", "Pie"],
    "components": ["Grid", "Tooltip"]
  }
}
```

### 5.5 · Generator 输出规范

```ts
// AUTO-GENERATED by @supramark/cli v<VERSION>
// Source: <config-path> (sha256: <hash>)
// Do not edit manually. Re-run: bun x supramark-gen
/* eslint-disable */

import { createRender }    from '@supramark/engines/runtime/createRender';
import { createSupramark } from '@supramark/engines/runtime/createSupramark';

// engine factories
import mermaid    from '@supramark/engines/mermaid';
import mathjax    from '@supramark/engines/mathjax';
import graphviz   from '@supramark/engines/graphviz';
import echarts    from '@supramark/engines/echarts';
import vegaLite   from '@supramark/engines/vega-lite';

// adapters & subtypes
import webAdapter from '@supramark/engines/graphviz/web-adapter';
import LineChart        from '@supramark/engines/echarts/LineChart';
import BarChart         from '@supramark/engines/echarts/BarChart';
import GridComponent    from '@supramark/engines/echarts/GridComponent';
import TooltipComponent from '@supramark/engines/echarts/TooltipComponent';
import vega     from '@supramark/engines/vega-lite/vega';
import compile  from '@supramark/engines/vega-lite/compile';

const spec = {
  engines: {
    mermaid:     mermaid(),
    mathjax:     mathjax(),
    graphviz:    graphviz([webAdapter]),
    echarts:     echarts([LineChart, BarChart, GridComponent, TooltipComponent]),
    'vega-lite': vegaLite([vega, compile]),
  },
  features: {
    gfm: true, math: true, footnote: true, emoji: true, admonition: true,
  },
} as const;

export const render    = createRender(spec);
export const Supramark = createSupramark(spec);
```

**生成规则**：

- 按 config 字段排序输出，保证同配置稳定产物（便于 `--check` 比对）。
- 顶部写 source config 的 sha256，hash 漂移即失效。
- import 按"runtime → factory → subtype → adapter"顺序分组，加空行分隔。
- `--out` 文件所在目录自动创建；默认 `src/generated/supramark.ts`。

## 6 · 使用方工作流

**初次配置**：

```bash
# 1. 装
bun add @supramark/engines @supramark/web echarts vega vega-lite graphviz-anywhere-web
bun add -D @supramark/cli

# 2. 起初始 config
bun x supramark-gen --preset docs > supramark.config.json
# 或者从 0 写

# 3. 生成
bun x supramark-gen
```

**日常修改**：

1. 改 `supramark.config.json`（比如加个 `"PieChart"`）。
2. `bun x supramark-gen`。
3. `git add supramark.config.json src/generated/supramark.ts` 一起 commit。

**CI 校验**：

```yaml
- run: bun x supramark-gen --check
```

漂移则构建失败，强制维护者先同步。

**业务调用**：

```ts
// 纯函数
import { render } from '@/generated/supramark';
const html = await render(markdown);

// React
import { Supramark } from '@/generated/supramark';
<Supramark markdown={md} theme="dark" />
```

## 7 · 对现有包的影响

| 包 | 影响 |
|---|---|
| `@supramark/core` | 无变动。AST / parser / feature 契约稳定 |
| `@supramark/engines` | **大重构**：删 `engine.ts` / `web.ts` / `rn.ts` / provider；加 runtime / types / 各 engine 独立目录 / subtype codegen |
| `@supramark/web` | **简化**：`Supramark.tsx` 不再自造 DiagramRenderService；runtime 层迁到 `@supramark/engines`；保留 `Supramark` 薄壳（由 `createSupramark` 生成） |
| `@supramark/rn` | **简化**：同上 |
| `@supramark/feature-*` | 不变。 |
| `@supramark/feature-diagram-plantuml` | **删除**。`DIAGRAM_ENGINE_TARGET.md` 早定的淘汰项 |
| 旧 RN diagram worker 包 | **删除**。旧 RN worker 过渡方案正式退场 |
| 旧 Web diagram helper 包 | **删除**。空壳包 |
| `@supramark/cli` | **新增** |
| `examples/react-web-csr` | 迁到 config + generated 流程 |
| `examples/react-native` | 同上 |

## 8 · 实施阶段（Phase 1–8）

| Phase | 内容 | 产出 | 可独立 revert |
|---|---|---|---|
| **1** | `engines` 骨架：types / runtime / 每 engine 空 factory | package 能 build，类型检查通过 | ✅ |
| **2** | 搬 mermaid / graphviz / mathjax 进新签名 | 3 个 engine 在新结构下跑通 | ✅ |
| **3** | 从 `origin/fix/graph` 移植 ECharts + Vega-Lite；subtype codegen 脚本 | 5 个 engine 全部可用；`echarts/<N>.ts` 自动产出 | ✅ |
| **4** | `@supramark/cli` 包 | bin 可跑，5 个 preset 可用 | ✅ |
| **5** | `web`/`rn` 的 Supramark 简化 | 组件变薄，接口迁到 spec 驱动 | ⚠️ 会影响 examples |
| **6** | 删 PlantUML 旧实现 + 旧 RN worker 包 + 旧 Web helper 包 | 三个包在仓库消失 | ✅ |
| **7** | examples 迁到 config 流程 | 所有示例跑新架构 | ⚠️ 需要 Phase 5 先完成 |
| **8** | 目录重组（见 §10） | 纯 `git mv` + workspace 配置更新 | ✅ 独立 PR |

**推荐合并策略**：Phase 1–7 合在一个大 PR（内容变更）；Phase 8 单独 PR（纯重命名）。

## 9 · 风险与缓解

| 风险 | 缓解 |
|---|---|
| ECharts 版本升级导致 subtype 导出列表变化 | codegen 脚本基于运行时 `Object.keys(echarts/charts)`，跟着版本走；CI 跑一遍 codegen 发现 diff 就报错 |
| Mermaid 依赖 `document` 导致 SSR / worker 失败 | `beautiful-mermaid` 已处理；暂不做 worker 化（列为 Phase 9 的后续） |
| Generated 文件漂移 | CI 跑 `--check`；hash 写进文件头 |
| `@supramark/cli` 需要读运行时包的类型 | 用 JSON Schema 声明，CLI 不 import runtime 代码，仅读字符串 |
| `$schema` URL 未上线期间 | 先指向 `./node_modules/@supramark/cli/config.schema.json`；等 supramark.dev 域名上了再切 |

## 10 · 后续工作（不在本次范围内）

| 项 | 说明 |
|---|---|
| **目录重组 PR** | `renderers/diagram-engine/` → `engines/`（顶层）；`features/feature-xxx/` → `features/<family>/xxx/`；详见前期讨论 |
| **Web Worker preset** | host 侧可选把 `render` 包一层 worker；supramark 不强绑 |
| **Mermaid tree-shake** | Mermaid 10+ 的 `registerExternalDiagrams` 做 subpath 化，类比 ECharts |
| **KaTeX 替代 MathJax** | `fix/graph` 已实现，体积显著更小；后续合入 |

## 11 · 验收标准

Phase 1–7 合并前：

- [ ] `bun run build` 全包通过
- [ ] `bun run test` 所有 feature 单测绿
- [ ] 三个 examples（CSR / SSR / RN）通过 `supramark-gen` 生成 + 启动无报错
- [ ] Chrome 端手测 13 个 feature 全部渲染（同本次回归测试用例）
- [ ] `ECharts bundle size`：选 `docs.json` preset 时，LineChart+BarChart+Grid+Tooltip 组合的 echarts 相关 chunk < 300KB gzipped
- [ ] `supramark-gen --check` 在 CI 通过
- [ ] 文档：`docs/guide/config-cli.md` 完整使用者手册

---

**设计定稿日期**：2026-04-19  
**预计实施周期**：1–2 周（Phase 1–7）+ 1 日（Phase 8）  
**本文件取代**：`DIAGRAM_ENGINE_TARGET.md`（保留作为历史记录，不再更新）
