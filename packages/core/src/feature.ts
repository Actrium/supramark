/**
 * Supramark Feature Interface System
 *
 * 为每个 Supramark 功能定义顶层接口，递归逐层展开子接口
 *
 * ## 接口定位与演进路径
 *
 * **当前阶段（v0.x）**：
 * - 主要用于**文档、规范、类型定义**层面
 * - 不要求立即接入运行时解析/渲染流程
 * - 是"理想目标架构"，而非"必须立即实现"
 *
 * **核心设计原则**：
 * 1. **渐进式实现**：先定义接口，再逐步接入运行时
 * 2. **规范与实现分离**：Feature 描述"应该是什么"，不强制"如何实现"
 * 3. **向后兼容**：现有 parseMarkdown/render 流程优先，Feature 慢慢替换
 *
 * **关键限制**：
 * - core 包是**纯 TypeScript 类型定义**，不依赖 React/RN
 * - 渲染器的 `render` 函数应该是**类型引用**，不包含 JSX 实现
 * - 真实的 React 组件实现应该在 `@supramark/rn` 和 `@supramark/web` 包中
 *
 * **Feature vs AST 粒度问题**：
 * - 某些场景下，多个 Feature 可能共享同一个 AST 节点类型
 * - 例如：Vega-Lite、Mermaid、PlantUML 都使用 `type: 'diagram'`，通过 `engine` 字段区分
 * - Feature 接口支持通过 `selector` 函数来匹配节点子集
 *
 * @example
 * @example
 * // Feature 定义示例（包括解析器、渲染器、测试）
 * const myFeature: SupramarkFeature<MyNode> = {
 *   metadata: { ... },
 *   syntax: { ast: { ... }, parser: { ... } },
 *   renderers: { rn: { ... }, web: { ... } },
 *   testing: { ... },
 *   documentation: { ... }
 * };
 *
 * @license Apache-2.0
 */

import type {
  SupramarkNode,
  SupramarkDiagramConfig,
  SupramarkDiagramEngineId,
} from './ast';
import { warnIfUnknownDiagramEngine } from './ast';
import type MarkdownIt from 'markdown-it';
import type Token from 'markdown-it/lib/token.mjs';

// ============================================================================
// 顶层接口：SupramarkFeature
// ============================================================================

/**
 * Supramark 功能的顶层接口（生产环境）
 *
 * 每个功能扩展（如 Math、Diagram、Admonition）都应实现此接口
 *
 * **强制规范**：
 * - metadata: 必需，所有字段都应填写完整
 * - syntax: 必需，必须包含完整的 AST 定义和 interface
 * - renderers: 必需，至少应该定义一个平台的渲染器
 * - examples: 必需，每个 Feature 必须提供至少一个完整的使用示例
 * - testing: 必需，必须提供测试定义以保障 Feature 质量
 * - documentation: 必需，必须提供文档以便用户参考
 */
export interface SupramarkFeature<TNode extends SupramarkNode = SupramarkNode> {
  /**
   * 功能元信息
   *
   * 必需，所有字段都应填写完整
   */
  metadata: FeatureMetadata;

  /**
   * 语法定义（Markdown → AST）
   *
   * 必需，对于生产 Feature 应包含完整的 AST interface 定义
   */
  syntax: SyntaxDefinition<TNode>;

  /**
   * 渲染器定义（AST → 各平台组件）
   *
   * 必需，至少应该定义一个平台的渲染器（rn 或 web）
   */
  renderers: RendererDefinitions<TNode>;

  /**
   * 使用示例（必需）
   *
   * 每个 Feature 必须提供至少一个完整的 markdown 示例
   * 用于文档、测试和演示应用
   *
   * 示例数据应该自包含在 Feature 包中，不依赖外部共享数据
   */
  examples: ExampleDefinition[];

  /**
   * 测试定义（必需）
   *
   * 必须提供测试定义以保障 Feature 质量
   */
  testing: TestingDefinition<TNode>;

  /**
   * 文档定义（必需）
   *
   * 必须提供文档以便用户参考
   */
  documentation: DocumentationDefinition;

  /**
   * Feature 自描述提示信息（可选）
   *
   * 用于生成 System Prompt，帮助 AI Agent 理解和使用此 Feature。
   * 包含功能描述、语法结构和示例。
   */
  prompt?: FeaturePromptDefinition;

  /**
   * 依赖的其他功能
   *
   * 如果此 Feature 依赖其他 Feature，在这里声明
   */
  dependencies?: string[];

  /**
   * 生命周期钩子（可选）
   *
   * 用于在 Feature 注册、解析、渲染等阶段执行自定义逻辑
   */
  hooks?: FeatureHooks<TNode>;
}

// ============================================================================
// 第二层：Prompt 定义
// ============================================================================

/**
 * Feature 自描述提示信息定义
 */
export interface FeaturePromptDefinition {
  /**
   * 功能描述
   * 简要说明此 Feature 的用途，供 AI 理解。
   */
  description: string;

  /**
   * 语法结构
   * 描述 Markdown 语法格式。
   */
  syntax: string;

  /**
   * 使用示例
   * 提供 1-3 个典型的使用案例。
   */
  examples: Array<{
    /** 示例说明 */
    desc: string;
    /** Markdown 代码 */
    code: string;
  }>;
}

// ============================================================================
// 第二层：功能元信息
// ============================================================================

/**
 * 功能元信息
 *
 * **强制规范**：
 * - id: 必须符合 `@scope/feature-name` 格式（如 `@supramark/feature-math`）
 * - version: 必须符合语义化版本格式 x.y.z（如 `1.0.0`）
 * - name: 不能为空
 * - description: 强烈建议填写（生产环境必需）
 * - author: 强烈建议填写
 * - license: 应该设置为 'Apache-2.0'（Supramark 统一许可证）
 */
export interface FeatureMetadata {
  /**
   * 功能唯一标识符
   *
   * 格式: @scope/feature-name
   * 示例: @supramark/feature-math
   *
   * @pattern ^@[\w-]+\/feature-[\w-]+$
   */
  id: string;

  /**
   * 功能名称
   *
   * 不能为空，应该简洁明了
   * 示例: 'Math Formula', 'Footnote', 'Diagram'
   */
  name: string;

  /**
   * 版本号（语义化版本）
   *
   * 格式: x.y.z
   * 示例: 1.0.0, 0.1.0
   *
   * @pattern ^\d+\.\d+\.\d+$
   */
  version: string;

  /**
   * 作者
   *
   * 建议填写，用于标识 Feature 的维护者
   */
  author: string;

  /**
   * 简短描述
   *
   * 应该清晰描述此 Feature 的用途和功能
   * 建议填写，生产环境强烈建议
   */
  description: string;

  /**
   * 许可证
   *
   * Supramark 统一使用 Apache-2.0
   * 建议设置为 'Apache-2.0'
   */
  license: string;

  /** 主页 URL */
  homepage?: string;

  /** 仓库 URL */
  repository?: string;

  /**
   * 标签（用于分类）
   *
   * 建议至少添加一个标签，用于 Feature 的分类和搜索
   * 示例: ['math', 'latex', 'formula']
   */
  tags?: string[];

  /**
   * 语法家族（可选）
   *
   * 用于从语法形式角度对 Feature 做粗粒度归类，便于文档、矩阵视图和后续工具化。
   *
   * - 'main'      : 主体 Markdown 语法（原始规范，GFM / Math / Emoji / 脚注等均使用该族）；
   * - 'container' : 基于 :::name ... ::: 的容器语法（admonition / html / map 等）；
   * - 'fence'     : 基于 ```lang 代码块的语法（diagram / 各类代码块扩展等）。
   *
   * 当前仅作为文档与分类用途，运行时逻辑不会依赖此字段。
   */
  syntaxFamily?: 'main' | 'container' | 'fence';
}

// ============================================================================
// 第二层：语法定义
// ============================================================================

/**
 * 语法定义（Markdown 输入 → AST 输出）
 *
 * **解析器（Parser）的定位**：
 * - 在当前阶段（v0.x），parser 主要用于**文档和规范**
 * - 实际解析逻辑可能仍在 `parseMarkdown()` 中硬编码
 * - 当 Feature 复用现有解析逻辑时（如 diagram 的各种 engine），parser 可以省略
 * - 未来会逐步支持通过 Feature 注册来驱动解析流程
 */
export interface SyntaxDefinition<TNode extends SupramarkNode> {
  /** AST 节点定义 */
  ast: ASTNodeDefinition<TNode>;

  /**
   * 解析规则（可选）
   *
   * - 如果此 Feature 复用现有的解析逻辑（如 diagram），可以省略
   * - 如果需要自定义解析器（如新增语法扩展），则必须提供
   */
  parser?: ParserRules;

  /** 验证规则（可选） */
  validator?: ValidatorRules<TNode>;
}

// ----------------------------------------------------------------------------
// 第三层：AST 节点定义
// ----------------------------------------------------------------------------

/**
 * AST 节点定义
 *
 * **节点选择器（Selector）**：
 * - 某些场景下，多个 Feature 可能共享同一个 AST 节点类型
 * - 例如：Vega-Lite、Mermaid、PlantUML 都使用 `type: 'diagram'`，通过 `engine` 字段区分
 * - 使用 `selector` 函数来匹配节点子集，而不仅仅依赖 `type`
 *
 * **强制规范**：
 * - type: 必须定义，不能为空
 * - interface: 对于生产 Feature 强烈建议定义
 * - examples: 建议提供至少一个示例节点
 *
 * @example
 * // Vega-Lite Feature 只关心 diagram 节点中 engine 为 'vega-lite' 的
 * const vegaLiteAST: ASTNodeDefinition<DiagramNode> = {
 *   type: 'diagram',
 *   selector: (node) => node.type === 'diagram' && ['vega-lite', 'vega'].includes(node.engine),
 *   interface: { ... }
 * };
 */
export interface ASTNodeDefinition<TNode extends SupramarkNode> {
  /**
   * 节点类型名称
   *
   * 必需，不能为空
   * 示例: 'math_inline', 'diagram', 'footnote_reference'
   */
  type: string;

  /**
   * 节点选择器（可选）
   *
   * 用于精确匹配此 Feature 关心的节点子集。
   * 当多个 Feature 共享同一 `type` 时，通过此函数区分。
   * 如果 Feature 处理多节点类型，应该提供 selector 函数。
   *
   * @param node - 待匹配的 AST 节点
   * @returns 如果节点属于此 Feature，返回 true；否则返回 false
   *
   * @example
   * // 匹配所有 diagram 节点中 engine 为 'plantuml' 的
   * selector: (node) => node.type === 'diagram' && node.engine === 'plantuml'
   *
   * @example
   * // 匹配 footnote_reference 和 footnote_definition 两种类型
   * selector: (node) =>
   *   node.type === 'footnote_reference' || node.type === 'footnote_definition'
   */
  selector?: (node: SupramarkNode) => boolean;

  /**
   * 节点接口（TypeScript 类型）
   *
   * 对于生产 Feature 强烈建议定义，用于文档、验证和类型安全
   */
  interface?: NodeInterface<TNode>;

  /** 节点在 AST 树中的位置约束 */
  constraints?: NodeConstraints;

  /**
   * 示例节点
   *
   * 建议提供至少一个示例节点，用于文档和测试
   */
  examples?: TNode[];

  /**
   * 多节点类型提示（可选）
   *
   * 如果此 Feature 处理多种节点类型，在这里说明
   * 示例: '注意: Footnote Feature 通常需要处理 footnote_reference 和 footnote_definition 两种节点类型'
   */
  multiNodeNote?: string;
}

/**
 * 节点接口定义
 *
 * **强制规范**：
 * - required: 不应该只包含 'type'，应该包含节点的关键字段
 * - fields: 应该定义所有 required 字段的类型和描述
 */
export interface NodeInterface<TNode> {
  /**
   * 节点的必需字段
   *
   * 不应该只包含 'type'，应该包含节点的关键字段
   * 示例: ['type', 'index', 'label'] 而不是 ['type']
   */
  required: Array<keyof TNode>;

  /**
   * 节点的可选字段
   */
  optional?: Array<keyof TNode>;

  /**
   * 字段类型描述
   *
   * 应该定义所有 required 字段的类型和描述
   */
  fields: Record<string, FieldDefinition>;
}

/**
 * 字段定义
 */
export interface FieldDefinition {
  /** 字段类型 */
  type: 'string' | 'number' | 'boolean' | 'object' | 'array' | 'node' | 'nodes';

  /** 字段描述 */
  description: string;

  /** 默认值 */
  default?: unknown;

  /** 验证规则 */
  validate?: (value: unknown) => boolean;
}

/**
 * 节点约束
 */
export interface NodeConstraints {
  /** 允许的父节点类型 */
  allowedParents?: string[];

  /** 允许的子节点类型 */
  allowedChildren?: string[];

  /** 是否可以嵌套自身 */
  allowSelfNesting?: boolean;

  /** 是否必须有子节点 */
  requireChildren?: boolean;
}

// ----------------------------------------------------------------------------
// 第三层：解析规则
// ----------------------------------------------------------------------------

/**
 * 解析规则（支持多种解析器）
 */
export interface ParserRules {
  /** 解析器类型 */
  engine: 'markdown-it' | 'remark' | 'custom';

  /** markdown-it 解析规则 */
  markdownIt?: MarkdownItRules;

  /** remark 解析规则 */
  remark?: RemarkRules;

  /** 自定义解析器 */
  custom?: CustomParserRules;
}

/**
 * markdown-it 解析规则
 */
export interface MarkdownItRules {
  /** 使用的 markdown-it 插件 */
  plugin: MarkdownItPlugin;

  /** 插件配置选项 */
  options?: Record<string, unknown>;

  /** Token → AST 映射函数 */
  tokenMapper: TokenMapper;
}

/**
 * markdown-it 插件接口
 */
export interface MarkdownItPlugin {
  /** 插件函数 */
  (md: MarkdownIt, options?: unknown): void;
}

/**
 * Token 映射器
 */
export interface TokenMapper {
  /** 映射函数 */
  (token: Token, context: ParserContext): SupramarkNode | null;
}

/**
 * 解析器上下文
 */
export interface ParserContext {
  /** 当前处理的 token 列表 */
  tokens: Token[];

  /** 当前 token 索引 */
  index: number;

  /** 父节点栈 */
  stack: SupramarkNode[];

  /** 当前父节点 */
  parent: SupramarkNode;
}

/**
 * remark 解析规则
 */
export interface RemarkRules {
  /** remark 插件 */
  plugin: unknown;

  /** 插件选项 */
  options?: Record<string, unknown>;
}

/**
 * 自定义解析规则
 */
export interface CustomParserRules {
  /** 正则表达式匹配 */
  pattern?: RegExp;

  /** 自定义解析函数 */
  parse: (input: string, context: ParserContext) => SupramarkNode | null;
}

// ----------------------------------------------------------------------------
// 第三层：验证规则
// ----------------------------------------------------------------------------

/**
 * 验证规则
 */
export interface ValidatorRules<TNode extends SupramarkNode> {
  /** 节点验证函数 */
  validate: (node: TNode) => ValidationResult;

  /** 严格模式（是否在验证失败时抛出错误） */
  strict?: boolean;
}

/**
 * 验证结果
 */
export interface ValidationResult {
  /** 是否通过验证 */
  valid: boolean;

  /** 错误信息列表 */
  errors?: ValidationError[];

  /** 警告信息列表 */
  warnings?: ValidationWarning[];
}

/**
 * 验证错误
 */
export interface ValidationError {
  /** 错误代码 */
  code: string;

  /** 错误信息 */
  message: string;

  /** 错误位置（节点路径） */
  path?: string;

  /** 相关数据 */
  data?: unknown;
}

/**
 * 验证警告
 */
export interface ValidationWarning {
  /** 警告代码 */
  code: string;

  /** 警告信息 */
  message: string;

  /** 建议的修复方式 */
  suggestion?: string;
}

// ============================================================================
// 第二层：渲染器定义
// ============================================================================

/**
 * 渲染器定义（AST → 各平台组件）
 *
 * **重要限制**：
 * - core 包是**纯 TypeScript 类型定义**，不依赖 React/RN
 * - 渲染器的 `render` 函数应该是**类型引用和签名**，不包含 JSX 实现
 * - 真实的 React 组件实现应该在 `@supramark/rn` 和 `@supramark/web` 包中
 *
 * **当前阶段的定位**：
 * - Feature 中的 renderers 主要用于：
 *   1. 声明此功能需要哪些平台支持
 *   2. 描述渲染器的基础设施需求（infrastructure）
 *   3. 列出依赖的外部库（dependencies）
 * - 实际的渲染逻辑仍在各平台包中实现（@supramark/rn、@supramark/web）
 *
 * **对于复杂功能（如图表）**：
 * - 可以通过 `infrastructure` 字段声明需要 WebView、Worker、客户端脚本等
 * - 实际的 Worker/Script 实现仍在各平台包中
 *
 * @example
 * // 简化的渲染器定义（仅声明平台支持和依赖）
 * renderers: {
 *   rn: {
 *     platform: 'rn',
 *     infrastructure: { needsWorker: true },
 *     dependencies: [{ name: 'react-native-svg', version: '^13.0.0' }]
 *   },
 *   web: {
 *     platform: 'web',
 *     infrastructure: { needsClientScript: true }
 *   }
 * }
 */
export interface RendererDefinitions<TNode extends SupramarkNode> {
  /** React Native 渲染器 */
  rn?: PlatformRenderer<TNode, Platform>;

  /** Web (React) 渲染器 */
  web?: PlatformRenderer<TNode, Platform>;

  /** CLI (终端) 渲染器 */
  cli?: PlatformRenderer<TNode, Platform>;

  /** 自定义平台渲染器 */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  [platform: string]: PlatformRenderer<TNode, any> | undefined;
}

// ----------------------------------------------------------------------------
// 第三层：平台渲染器
// ----------------------------------------------------------------------------

/**
 * 平台渲染器
 *
 * **render 函数的定位**：
 * - 在当前阶段（v0.x），render 主要用于**类型签名定义**
 * - 不应在 core 包中包含实际的 JSX 实现
 * - 如果 Feature 复用现有渲染逻辑（如diagram），render 可以省略
 * - 实际渲染组件应该在 @supramark/rn 和 @supramark/web 包中实现
 */
export interface PlatformRenderer<TNode extends SupramarkNode, TPlatform extends Platform> {
  /** 平台标识（兼容当前 feature 定义写法） */
  platform?: TPlatform;

  /**
   * 渲染函数（可选）
   *
   * - 如果提供，仅作为类型签名参考，不包含 JSX 实现
   * - 如果此 Feature 复用现有渲染器（如 diagram），可以省略
   */
  render?: RenderFunction<TNode, TPlatform>;

  /** 样式定义（可选） */
  styles?: StyleDefinition<TPlatform>;

  /** 渲染基础设施需求（可选但推荐填写） */
  infrastructure?: InfrastructureRequirements;

  /** 依赖的外部库（可选但推荐填写） */
  dependencies?: PlatformDependency[];
}

/**
 * 平台类型
 */
export type Platform = 'rn' | 'web' | 'cli' | string;

/**
 * 渲染函数
 */
export interface RenderFunction<TNode extends SupramarkNode, TPlatform> {
  (node: TNode, context: RenderContext<TPlatform>): RenderOutput<TPlatform>;
}

/**
 * 渲染上下文
 */
export interface RenderContext<TPlatform> {
  /** 当前平台 */
  platform: TPlatform;

  /** 节点在列表中的索引（用作 React key） */
  key: number;

  /** 样式系统 */
  styles: TPlatform extends 'rn' ? ReactNativeStyles : WebStyles;

  /** 渲染子节点的辅助函数 */
  renderChildren: (children: SupramarkNode[]) => RenderOutput<TPlatform>[];

  /** 自定义数据 */
  data?: Record<string, unknown>;
}

/**
 * 渲染输出
 *
 * Core 包保持框架无关性，不直接依赖 React 类型。
 * 上层应用会将此类型实例化为 React.ReactNode。
 */
export type RenderOutput<TPlatform> = TPlatform extends 'cli' ? string : any;

/**
 * React Native 样式类型（简化）
 */
export type ReactNativeStyles = Record<string, unknown>;

/**
 * Web 样式类型（CSS 类名）
 */
export type WebStyles = Record<string, string>;

// ----------------------------------------------------------------------------
// 第三层：样式定义
// ----------------------------------------------------------------------------

/**
 * 样式定义
 */
export interface StyleDefinition<TPlatform> {
  /** 默认样式 */
  default: TPlatform extends 'rn' ? ReactNativeStyles : WebStyles;

  /** 主题变体 */
  themes?: Record<string, TPlatform extends 'rn' ? ReactNativeStyles : WebStyles>;

  /** 样式变量 */
  variables?: StyleVariables;
}

/**
 * 样式变量
 */
export interface StyleVariables {
  /** 颜色 */
  colors?: Record<string, string>;

  /** 尺寸 */
  sizes?: Record<string, number>;

  /** 字体 */
  fonts?: Record<string, string>;

  /** 其他自定义变量 */
  [key: string]: unknown;
}

// ----------------------------------------------------------------------------
// 第三层：基础设施需求
// ----------------------------------------------------------------------------

/**
 * Renderer infrastructure requirements declared by a feature so hosts
 * can decide whether to wire it up.
 */
export interface InfrastructureRequirements {
  /** Whether a background worker is needed for rendering. */
  needsWorker?: boolean;

  /**
   * Worker type. Note: 'webview' was removed in the 2026-05 cleanup —
   * supramark no longer ships a hidden-WebView worker. RN engines that
   * historically relied on one (mermaid / plantuml / d2 / echarts /
   * vega-lite) now go through native FFI bindings or are documented as
   * Web-only. New features should pick 'web-worker' or
   * 'service-worker' (or omit `workerType` entirely if no worker is
   * needed).
   */
  workerType?: 'web-worker' | 'service-worker';

  /** Whether output caching is desired. */
  needsCache?: boolean;

  /** Cache configuration. */
  cacheConfig?: CacheConfig;

  /** Whether the feature needs a client-side script (Web). */
  needsClientScript?: boolean;

  /** Client-side script generator. */
  clientScriptBuilder?: () => string;
}

/**
 * 缓存配置
 */
export interface CacheConfig {
  /** 最大缓存条目数 */
  maxSize?: number;

  /** TTL（毫秒） */
  ttl?: number;

  /** 缓存键生成器 */
  keyGenerator?: (node: SupramarkNode) => string;
}

// ----------------------------------------------------------------------------
// 第三层：平台依赖
// ----------------------------------------------------------------------------

/**
 * 平台依赖
 */
export interface PlatformDependency {
  /** 依赖名称 */
  name: string;

  /** 依赖版本 */
  version: string;

  /** 依赖类型 */
  type: 'npm' | 'cdn' | 'system';

  /** CDN URL（如果是 CDN 依赖） */
  cdnUrl?: string;

  /** 是否可选 */
  optional?: boolean;
}

// ============================================================================
// 第二层：测试定义
// ============================================================================

/**
 * 测试定义
 */
export interface TestingDefinition<TNode extends SupramarkNode> {
  /** 语法测试（Markdown → AST） */
  syntaxTests?: SyntaxTestSuite<TNode>;

  /** 渲染测试（AST → 组件） */
  renderTests?: RenderTestSuite<TNode>;

  /** 集成测试 */
  integrationTests?: IntegrationTestSuite<TNode>;

  /** 测试覆盖率要求 */
  coverageRequirements?: CoverageRequirements;
}

// ----------------------------------------------------------------------------
// 第三层：测试套件
// ----------------------------------------------------------------------------

/**
 * 语法测试套件
 */
export interface SyntaxTestSuite<TNode> {
  /** 测试用例 */
  cases: SyntaxTestCase<TNode>[];
}

/**
 * 语法测试用例
 */
export interface SyntaxTestCase<TNode> {
  /** 测试名称 */
  name: string;

  /** 输入 Markdown */
  input: string;

  /** 期望的 AST 节点 */
  expected: TNode | TNode[];

  /** 测试选项 */
  options?: {
    /** 是否只检查节点类型 */
    typeOnly?: boolean;

    /** 是否忽略某些字段 */
    ignoreFields?: string[];
  };
}

/**
 * 渲染测试套件
 */
export interface RenderTestSuite<TNode> {
  /** 测试用例（按平台分组） */
  rn?: RenderTestCase<TNode, 'rn'>[];
  web?: RenderTestCase<TNode, 'web'>[];
  cli?: RenderTestCase<TNode, 'cli'>[];
}

/**
 * 渲染测试用例
 */
export interface RenderTestCase<TNode, TPlatform> {
  /** 测试名称 */
  name: string;

  /** 输入 AST 节点 */
  input: TNode;

  /** 期望的渲染输出（或验证函数） */
  expected: RenderOutput<TPlatform> | ((output: RenderOutput<TPlatform>) => boolean);

  /** 快照测试 */
  snapshot?: boolean;
}

/**
 * 集成测试套件
 */
export interface IntegrationTestSuite<TNode> {
  /** 端到端测试用例 */
  cases: IntegrationTestCase<TNode>[];
}

/**
 * 集成测试用例
 */
export interface IntegrationTestCase<TNode> {
  /** 测试名称 */
  name: string;

  /** 输入 Markdown */
  input: string;

  /** 验证函数 */
  validate: (result: unknown) => boolean;

  /** 测试平台 */
  platforms?: Platform[];
}

/**
 * 覆盖率要求
 */
export interface CoverageRequirements {
  /** 语句覆盖率 */
  statements?: number;

  /** 分支覆盖率 */
  branches?: number;

  /** 函数覆盖率 */
  functions?: number;

  /** 行覆盖率 */
  lines?: number;
}

// ============================================================================
// 第二层：文档定义
// ============================================================================

/**
 * 文档定义
 */
export interface DocumentationDefinition {
  /** README 内容 */
  readme: string;

  /** API 文档 */
  api?: APIDocumentation;

  /** 最佳实践 */
  bestPractices?: string[];

  /** 常见问题 */
  faq?: FAQItem[];
}

// ----------------------------------------------------------------------------
// 第三层：文档子项
// ----------------------------------------------------------------------------

/**
 * API 文档
 */
export interface APIDocumentation {
  /** 接口文档 */
  interfaces: InterfaceDoc[];

  /** 函数文档 */
  functions?: FunctionDoc[];

  /** 类型文档 */
  types?: TypeDoc[];
}

/**
 * 接口文档
 */
export interface InterfaceDoc {
  /** 接口名称 */
  name: string;

  /** 描述 */
  description: string;

  /** 字段列表 */
  fields: FieldDoc[];
}

/**
 * 字段文档
 */
export interface FieldDoc {
  /** 字段名 */
  name: string;

  /** 类型 */
  type: string;

  /** 描述 */
  description: string;

  /** 是否必需 */
  required: boolean;

  /** 默认值 */
  default?: string;
}

/**
 * 函数文档
 */
export interface FunctionDoc {
  /** 函数名 */
  name: string;

  /** 描述 */
  description: string;

  /** 参数列表 */
  parameters: ParameterDoc[];

  /** 返回值 */
  returns: string;

  /** 示例 */
  examples?: string[];
}

/**
 * 参数文档
 */
export interface ParameterDoc {
  /** 参数名 */
  name: string;

  /** 类型 */
  type: string;

  /** 描述 */
  description: string;

  /** 是否可选 */
  optional?: boolean;
}

/**
 * 类型文档
 */
export interface TypeDoc {
  /** 类型名 */
  name: string;

  /** 描述 */
  description: string;

  /** 类型定义 */
  definition: string;
}

/**
 * 示例定义
 */
export interface ExampleDefinition {
  /** 示例名称 */
  name: string;

  /** 描述 */
  description: string;

  /** Markdown 输入 */
  markdown: string;

  /** 期望输出（可选） */
  output?: string;

  /** 代码示例（如何使用） */
  code?: string;

  /** 在线演示 URL */
  demoUrl?: string;
}

/**
 * FAQ 条目
 */
export interface FAQItem {
  /** 问题 */
  question: string;

  /** 答案 */
  answer: string;

  /** 相关链接 */
  links?: string[];
}

// ============================================================================
// 第二层：生命周期钩子
// ============================================================================

/**
 * 功能生命周期钩子
 */
export interface FeatureHooks<TNode extends SupramarkNode> {
  /** 功能注册前 */
  beforeRegister?: () => void | Promise<void>;

  /** 功能注册后 */
  afterRegister?: () => void | Promise<void>;

  /** 解析前 */
  beforeParse?: (markdown: string) => string;

  /** 解析后 */
  afterParse?: (ast: TNode[]) => TNode[];

  /** 渲染前 */
  beforeRender?: (node: TNode) => TNode;

  /** 渲染后 */
  afterRender?: (output: unknown) => unknown;

  /** 功能卸载 */
  onUnregister?: () => void | Promise<void>;
}

// ============================================================================
// 工具类型
// ============================================================================

/**
 * 递归展开所有必需字段
 */
export type DeepRequired<T> = {
  [P in keyof T]-?: T[P] extends object ? DeepRequired<T[P]> : T[P];
};

/**
 * 递归展开所有可选字段
 */
export type DeepPartial<T> = {
  [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};

/**
 * 提取功能的 AST 节点类型
 */
export type FeatureNodeType<F extends SupramarkFeature<SupramarkNode>> =
  F extends SupramarkFeature<infer TNode> ? TNode : never;

// ============================================================================
// 最小化 Feature 接口
// ============================================================================

/**
 * 最小化 Feature 定义
// ============================================================================
// Feature 注册与发现机制
// ============================================================================

/**
 * Feature 注册表
 *
 * 用于收集和管理所有已定义的 Feature
 */
export class FeatureRegistry {
  // 注册表内部使用 any，避免对节点类型做过度约束。
  // 这样既可以注册针对特定节点子集的 Feature（如 SupramarkDiagramNode），
  // 也不会影响外部通过 SupramarkNode 做统一查询。
  private static features = new Map<string, SupramarkFeature<any>>();

  /**
   * 注册一个 Feature
   *
   * @param feature - Feature 定义
   * @throws 如果 Feature ID 已存在
   */
  static register(feature: SupramarkFeature<any>): void {
    const id = feature.metadata.id;

    if (this.features.has(id)) {
      throw new Error(`Feature "${id}" is already registered`);
    }

    this.features.set(id, feature);
  }

  /**
   * 获取指定 ID 的 Feature
   *
   * @param id - Feature ID
   * @returns Feature 定义，如果不存在则返回 undefined
   */
  static get(id: string): SupramarkFeature<any> | undefined {
    return this.features.get(id);
  }

  /**
   * 列出所有已注册的 Feature
   *
   * @returns Feature 列表
   */
  static list(): Array<SupramarkFeature<any>> {
    return Array.from(this.features.values());
  }

  /**
   * 按标签查找 Feature
   *
   * @param tag - 标签名
   * @returns 包含该标签的所有 Feature
   */
  static findByTag(tag: string): Array<SupramarkFeature<SupramarkNode>> {
    return this.list().filter(
      feature => 'tags' in feature.metadata && feature.metadata.tags?.includes(tag)
    );
  }

  /**
   * 查找匹配指定 AST 节点的 Feature
   *
   * @param node - AST 节点
   * @returns 匹配的 Feature 列表
   */
  static findByNode(node: SupramarkNode): Array<SupramarkFeature<any>> {
    return this.list().filter(feature => {
      const ast = feature.syntax.ast;

      // 检查节点类型
      if (ast.type !== node.type) {
        return false;
      }

      // 如果有 selector，使用 selector 进一步过滤
      if (ast.selector) {
        return ast.selector(node);
      }

      return true;
    });
  }

  /**
   * 清空注册表（主要用于测试）
   */
  static clear(): void {
    this.features.clear();
  }
}

// ============================================================================
// 辅助函数
// ============================================================================

/**
 * 创建一个完整 Feature
 *
 * 辅助函数，提供更好的类型推导
 *
 * @param feature - Feature 定义
 * @returns 类型化的 SupramarkFeature
 */
export function defineFeature<TNode extends SupramarkNode>(
  feature: SupramarkFeature<TNode>
): SupramarkFeature<TNode> {
  return feature;
}

/**
 * 验证 Feature 定义的完整性
 *
 * 对齐 Feature Linter 的规则，提供运行时验证
 *
 * @param feature - Feature 定义
 * @param options - 验证选项
 * @returns 验证结果
 */
export function validateFeature<TNode extends SupramarkNode = SupramarkNode>(
  feature: Partial<SupramarkFeature<TNode>> & {
    metadata?: Partial<FeatureMetadata>;
    syntax?: { ast?: Partial<ASTNodeDefinition<TNode>> } & Record<string, unknown>;
  },
  options: {
    /** 严格模式（将警告视为错误） */
    strict?: boolean;
    /** 是否为生产环境（更严格的要求） */
    production?: boolean;
  } = {}
): {
  valid: boolean;
  errors: Array<{ code: string; message: string; severity: 'error' | 'warning' | 'info' }>;
} {
  const errors: Array<{ code: string; message: string; severity: 'error' | 'warning' | 'info' }> =
    [];

  const metadata: Partial<FeatureMetadata> = feature.metadata ?? {};
  const syntax = feature.syntax ?? {};
  const ast = (syntax as { ast?: Partial<ASTNodeDefinition<TNode>> }).ast ?? {};

  // ============================================================================
  // Critical Rules（错误级别）- 必须通过
  // ============================================================================

  // metadata-id-format
  if (!metadata.id) {
    errors.push({
      code: 'metadata-id-required',
      message: 'Feature must have an id',
      severity: 'error',
    });
  } else if (!/^@[\w-]+\/feature-[\w-]+$/.test(metadata.id)) {
    errors.push({
      code: 'metadata-id-format',
      message: 'Feature ID 必须符合 @scope/feature-name 格式（如 @supramark/feature-math）',
      severity: 'error',
    });
  }

  // metadata-version-semver
  if (!metadata.version) {
    errors.push({
      code: 'metadata-version-required',
      message: 'Feature must have a version',
      severity: 'error',
    });
  } else if (!/^\d+\.\d+\.\d+$/.test(metadata.version)) {
    errors.push({
      code: 'metadata-version-semver',
      message: '版本号必须符合语义化版本格式 x.y.z（如 1.0.0）',
      severity: 'error',
    });
  }

  // metadata-name-required
  if (!metadata.name || metadata.name.trim().length === 0) {
    errors.push({
      code: 'metadata-name-required',
      message: 'Feature name 不能为空',
      severity: 'error',
    });
  }

  // ast-type-required
  if (!ast.type || String(ast.type).trim().length === 0) {
    errors.push({
      code: 'ast-type-required',
      message: 'Feature 必须定义 AST 节点 type',
      severity: 'error',
    });
  }

  // ============================================================================
  // Warning Rules（警告级别）- 强烈建议通过
  // ============================================================================

  // metadata-description-required
  if (!metadata.description || metadata.description.trim().length === 0) {
    errors.push({
      code: 'metadata-description-required',
      message: 'Feature description 不能为空',
      severity: 'warning',
    });
  }

  // metadata-author-required
  if (!metadata.author || metadata.author.trim().length === 0) {
    errors.push({
      code: 'metadata-author-required',
      message: 'Feature author 建议填写',
      severity: 'warning',
    });
  }

  // metadata-license-required
  if (!metadata.license) {
    errors.push({
      code: 'metadata-license-required',
      message: 'Feature license 应该设置',
      severity: 'warning',
    });
  } else if (metadata.license !== 'Apache-2.0') {
    errors.push({
      code: 'metadata-license-apache',
      message: 'Feature license 应该设置为 Apache-2.0（Supramark 统一许可证）',
      severity: 'info',
    });
  }

  // ast-interface-required-nonempty
  if (ast.interface) {
    const required = ast.interface.required;
    if (!Array.isArray(required) || required.length <= 1) {
      errors.push({
        code: 'ast-interface-required-nonempty',
        message: 'AST interface.required 不应只包含 type，应该包含节点的关键字段',
        severity: 'warning',
      });
    }
  }

  // ast-interface-fields-defined
  if (ast.interface) {
    const required = ast.interface.required || [];
    const fields = ast.interface.fields || {};
    const missingFields = required.filter(field => !(String(field) in fields));
    if (missingFields.length > 0) {
      errors.push({
        code: 'ast-interface-fields-defined',
        message: `AST interface.fields 应该定义所有 required 字段，缺失: ${missingFields.join(', ')}`,
        severity: 'warning',
      });
    }
  }

  // selector-multi-node-with-function
  if (ast.multiNodeNote && !ast.selector) {
    errors.push({
      code: 'selector-multi-node-with-function',
      message: '如果 Feature 处理多节点类型（有 multiNodeNote），应该提供 selector 函数',
      severity: 'warning',
    });
  }

  // ============================================================================
  // Info Rules（建议级别）- 最佳实践
  // ============================================================================

  // metadata-tags-nonempty
  if (!metadata.tags || metadata.tags.length === 0) {
    errors.push({
      code: 'metadata-tags-nonempty',
      message: 'Feature tags 建议添加至少一个标签，用于分类和搜索',
      severity: 'info',
    });
  }

  // ast-examples-provided
  if (!ast.examples || ast.examples.length === 0) {
    errors.push({
      code: 'ast-examples-provided',
      message: 'AST examples 建议提供至少一个示例节点，用于文档和测试',
      severity: 'info',
    });
  }

  // ============================================================================
  // Production Mode Extra Checks
  // ============================================================================

  if (options.production) {
    // 生产模式下，interface 应该是必需的
    if (!ast.interface) {
      errors.push({
        code: 'ast-interface-required-production',
        message: '生产环境的 Feature 必须定义完整的 AST interface',
        severity: 'error',
      });
    }

    // 生产模式下，至少应该有一个渲染器
    if ('renderers' in feature) {
      const renderers = feature.renderers as RendererDefinitions<SupramarkNode>;
      const hasRenderer = renderers && (renderers.rn || renderers.web || renderers.cli);
      if (!hasRenderer) {
        errors.push({
          code: 'renderers-required-production',
          message: '生产环境的 Feature 必须定义至少一个平台的渲染器（rn, web, 或 cli）',
          severity: 'error',
        });
      }
    }

    // 生产模式下，建议有测试
    if (!('testing' in feature) || !feature.testing) {
      errors.push({
        code: 'testing-recommended-production',
        message: '生产环境的 Feature 强烈建议提供测试定义',
        severity: 'warning',
      });
    }
  }

  // ============================================================================
  // 计算最终结果
  // ============================================================================

  // 严格模式下，警告也算错误
  const criticalErrors = errors.filter(e =>
    options.strict ? e.severity !== 'info' : e.severity === 'error'
  );

  return {
    valid: criticalErrors.length === 0,
    errors,
  };
}

// ============================================================================
// Feature 配置系统
// ============================================================================

/**
 * Feature 运行时配置
 *
 * 用于在运行时控制 Feature 的启用/禁用和行为
 */
export interface FeatureConfig {
  /** Feature ID */
  id: string;

  /** 是否启用此 Feature */
  enabled: boolean;

  /** Feature 特定的配置选项（可选） */
  options?: unknown;
}

/**
 * 带有强类型 options 的 FeatureConfig。
 *
 * - 用于各 Feature 包定义自己的 XXXFeatureConfig 类型；
 * - 在核心层仍然视为 FeatureConfig（options: unknown），
 *   但在业务代码中可以获得完整的类型提示。
 */
export type FeatureConfigWithOptions<TOptions> = Omit<FeatureConfig, 'options'> & {
  options?: TOptions;
};

/**
 * Supramark 运行时配置
 *
 * 用于配置整个 Supramark 实例的行为
 */
export interface SupramarkConfig {
  /** 启用的 Feature 列表 */
  features?: FeatureConfig[];

  /** 全局配置选项 */
  options?: {
    /** 是否启用缓存 */
    cache?: boolean;

    /** 是否启用严格模式（更严格的验证） */
    strict?: boolean;

    /** 其他全局配置 */
    [key: string]: unknown;
  };

  /**
   * 图表子系统配置
   *
   * - 用于控制 diagram 渲染相关的全局行为（超时、缓存、各引擎附加参数等）；
   * - 仅定义结构，由上层运行时（@supramark/rn / @supramark/web）实际消费；
   * - 如果未设置，则由各运行时采用各自的默认值（向后兼容）。
   */
  diagram?: SupramarkDiagramConfig;
}

/**
 * 从 FeatureRegistry 生成默认配置
 *
 * @param enabledByDefault - 默认是否启用所有 Feature（默认为 true）
 * @returns Supramark 配置对象
 */
export function createConfigFromRegistry(enabledByDefault = true): SupramarkConfig {
  const features = FeatureRegistry.list().map(feature => ({
    id: feature.metadata.id,
    enabled: enabledByDefault,
  }));

  return {
    features,
    options: {
      cache: true,
      strict: false,
    },
  };
}

/**
 * 从配置中获取启用的 Feature ID 列表
 *
 * @param config - Supramark 配置
 * @returns 启用的 Feature ID 数组
 */
export function getEnabledFeatureIds(config: SupramarkConfig): string[] {
  return (config.features || []).filter(f => f.enabled).map(f => f.id);
}

/**
 * 获取启用的 Feature 定义列表
 *
 * @param config - Supramark 配置
 * @returns 启用的 Feature 定义数组
 */
export function getEnabledFeatures(
  config: SupramarkConfig
): Array<SupramarkFeature<SupramarkNode>> {
  const enabledIds = getEnabledFeatureIds(config);
  return enabledIds
    .map(id => FeatureRegistry.get(id))
    .filter((f): f is SupramarkFeature<SupramarkNode> => f !== undefined);
}

/**
 * 检查特定 Feature 是否已启用
 *
 * @param config - Supramark 配置
 * @param featureId - Feature ID
 * @returns 是否启用
 */
export function isFeatureEnabled(config: SupramarkConfig, featureId: string): boolean {
  const featureConfig = config.features?.find(f => f.id === featureId);
  return featureConfig?.enabled ?? false;
}

export type DiagramFeatureFamilyId =
  | 'mermaid'
  | 'vega-family'
  | 'echarts'
  | 'graphviz-family';

const DIAGRAM_FEATURE_IDS_BY_FAMILY: Record<DiagramFeatureFamilyId, readonly string[]> = {
  mermaid: ['@supramark/feature-mermaid'],
  'vega-family': ['@supramark/feature-diagram-vega-lite'],
  echarts: ['@supramark/feature-diagram-echarts'],
  'graphviz-family': ['@supramark/feature-diagram-dot'],
};

/**
 * 将 diagram engine 归类到当前支持的 feature family。
 *
 * 当前约定：
 * - mermaid
 * - plantuml
 * - vega-family（vega / vega-lite / chart / chartjs）
 * - echarts
 * - graphviz-family（dot / graphviz）
 */
export function getDiagramFeatureFamily(
  engine: SupramarkDiagramEngineId | string
): DiagramFeatureFamilyId | null {
  const normalized = String(engine).toLowerCase();

  if (normalized === 'mermaid') {
    return 'mermaid';
  }

  if (
    normalized === 'vega' ||
    normalized === 'vega-lite' ||
    normalized === 'chart' ||
    normalized === 'chartjs'
  ) {
    return 'vega-family';
  }

  if (normalized === 'echarts') {
    return 'echarts';
  }

  if (normalized === 'dot' || normalized === 'graphviz') {
    return 'graphviz-family';
  }

  return null;
}

/**
 * 将 diagram engine 映射到对应的 feature id 列表。
 */
export function getDiagramFeatureIdsForEngine(
  engine: SupramarkDiagramEngineId | string
): string[] {
  const family = getDiagramFeatureFamily(engine);
  if (!family) {
    return [];
  }

  return [...DIAGRAM_FEATURE_IDS_BY_FAMILY[family]];
}

/**
 * 判断一组 Feature ID 是否被启用。
 *
 * 约定：
 * - 未提供 config 或 config.features 为空 → 视为全部启用；
 * - 如果 config 中根本没有提到这些 ID → 视为使用默认行为（启用）；
 * - 一旦显式配置了其中任意一个 ID，则以配置为准，只要有一个 enabled:true 就认为启用。
 */
export function isFeatureGroupEnabled(
  config: SupramarkConfig | undefined,
  ids: readonly string[]
): boolean {
  if (!config || !config.features || config.features.length === 0) {
    return true;
  }

  const hasAny = ids.some(id => config.features!.some(f => f.id === id));
  if (!hasAny) {
    return true;
  }

  return ids.some(id => isFeatureEnabled(config, id));
}

/**
 * 根据配置判断某个 diagram engine 是否被启用。
 */
export function isDiagramFeatureEnabled(
  config: SupramarkConfig | undefined,
  engine: SupramarkDiagramEngineId | string,
  context?: string
): boolean {
  const ids = getDiagramFeatureIdsForEngine(engine);
  if (!ids.length) {
    warnIfUnknownDiagramEngine(engine, context);
    return true;
  }

  return isFeatureGroupEnabled(config, ids);
}

/**
 * 获取 Feature 的配置选项
 *
 * @param config - Supramark 配置
 * @param featureId - Feature ID
 * @returns Feature 配置选项，如果未配置则返回空对象
 */
export function getFeatureOptions(
  config: SupramarkConfig,
  featureId: string
): Record<string, unknown> {
  const featureConfig = config.features?.find(f => f.id === featureId);
  const raw = featureConfig?.options;

  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    return {};
  }

  return raw as Record<string, unknown>;
}

/**
 * 以强类型方式获取 Feature 配置选项。
 *
 * - 返回值类型由调用方通过泛型参数决定；
 * - 如果未配置对应 Feature，返回 undefined。
 */
export function getFeatureOptionsAs<TOptions>(
  config: SupramarkConfig | undefined,
  featureId: string
): TOptions | undefined {
  if (!config || !config.features || config.features.length === 0) {
    return undefined;
  }
  const featureConfig = config.features.find(f => f.id === featureId);
  return (featureConfig?.options ?? undefined) as TOptions | undefined;
}

/**
 * Helpers returned by `makeFeatureConfigHelpers`:
 * - `create(enabled?, options?)` — build a strongly-typed FeatureConfig for this feature.
 * - `getOptions(config)` — read the strongly-typed options for this feature from a SupramarkConfig.
 */
export interface FeatureConfigHelpers<TOptions> {
  create(enabled?: boolean, options?: TOptions): FeatureConfigWithOptions<TOptions>;
  getOptions(config?: SupramarkConfig): TOptions | undefined;
}

/**
 * Generate the standard `(createConfig, getOptions)` helper pair for a single feature.
 *
 * Replaces the hand-written `createXFeatureConfig` + `getXFeatureOptions` boilerplate
 * at the end of every feature package. `enabled` defaults to `true` (convention: an
 * explicitly-declared feature is on by default).
 *
 * @example
 *   const { create, getOptions } = makeFeatureConfigHelpers<MathFeatureOptions>(
 *     '@supramark/feature-math'
 *   );
 *   export const createMathFeatureConfig = create;
 *   export const getMathFeatureOptions = getOptions;
 */
export function makeFeatureConfigHelpers<TOptions>(
  featureId: string
): FeatureConfigHelpers<TOptions> {
  return {
    create(enabled = true, options) {
      return { id: featureId, enabled, options };
    },
    getOptions(config) {
      return getFeatureOptionsAs<TOptions>(config, featureId);
    },
  };
}
