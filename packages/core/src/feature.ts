/**
 * Supramark Feature Interface System
 *
 * Defines the top-level interface for each Supramark feature, expanding into nested
 * sub-interfaces layer by layer.
 *
 * ## Interface positioning and evolution path
 *
 * **Current stage (v0.x)**:
 * - Primarily used at the **documentation, specification, and type definition** level
 * - Not required to be wired into the runtime parse/render pipeline immediately
 * - Represents an "ideal target architecture" rather than something that "must be
 *   implemented right away"
 *
 * **Core design principles**:
 * 1. **Incremental implementation**: define the interface first, then wire it into
 *    the runtime step by step
 * 2. **Separation of spec and implementation**: a Feature describes "what it should
 *    be", not "how it must be implemented"
 * 3. **Backward compatibility**: the existing parse/render pipeline takes priority;
 *    Features gradually replace it
 *
 * **Key constraints**:
 * - The core package is **pure TypeScript type definitions** with no dependency on
 *   React/RN
 * - A renderer's `render` function should be a **type reference only**, without a
 *   JSX implementation
 * - The real React component implementation belongs in the `@supramark/rn` and
 *   `@supramark/web` packages
 *
 * **Feature vs. AST granularity**:
 * - In some cases multiple Features may share the same AST node type
 * - Example: Vega-Lite, Mermaid, and PlantUML all use `type: 'diagram'`, distinguished
 *   by the `engine` field
 * - The Feature interface supports matching a subset of nodes via a `selector` function
 *
 * @example
 * @example
 * // Example Feature definition (including parser, renderers, and tests)
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

import type { SupramarkNode, SupramarkDiagramConfig, SupramarkDiagramEngineId } from './ast';
import { warnIfUnknownDiagramEngine } from './ast';

// ============================================================================
// Top-level interface: SupramarkFeature
// ============================================================================

/**
 * The top-level interface for a Supramark feature (production).
 *
 * Every feature extension (e.g. Math, Diagram, Admonition) should implement this
 * interface.
 *
 * **Mandatory rules**:
 * - metadata: required; all fields should be filled in completely
 * - syntax: required; must include a complete AST definition and interface
 * - renderers: required; at least one platform renderer must be defined
 * - examples: required; every Feature must provide at least one complete usage example
 * - testing: required; a test definition must be provided to guarantee Feature quality
 * - documentation: required; documentation must be provided for users to reference
 */
export interface SupramarkFeature<TNode extends SupramarkNode = SupramarkNode> {
  /**
   * Feature metadata.
   *
   * Required; all fields should be filled in completely.
   */
  metadata: FeatureMetadata;

  /**
   * Syntax definition (Markdown → AST).
   *
   * Required; a production Feature should include a complete AST interface definition.
   */
  syntax: SyntaxDefinition<TNode>;

  /**
   * Renderer definitions (AST → per-platform components).
   *
   * Required; at least one platform renderer (rn or web) should be defined.
   */
  renderers: RendererDefinitions<TNode>;

  /**
   * Usage examples (required).
   *
   * Every Feature must provide at least one complete markdown example,
   * used for documentation, testing, and the demo app.
   *
   * Example data should be self-contained within the Feature package, not depend on
   * externally shared data.
   */
  examples: ExampleDefinition[];

  /**
   * Test definition (required).
   *
   * A test definition must be provided to guarantee Feature quality.
   */
  testing: TestingDefinition<TNode>;

  /**
   * Documentation definition (required).
   *
   * Documentation must be provided for users to reference.
   */
  documentation: DocumentationDefinition;

  /**
   * Feature self-describing prompt information (optional).
   *
   * Used to generate the System Prompt that helps an AI Agent understand and use this
   * Feature. Includes a description, syntax structure, and examples.
   */
  prompt?: FeaturePromptDefinition;

  /**
   * Other features this one depends on.
   *
   * If this Feature depends on other Features, declare them here.
   */
  dependencies?: string[];

  /**
   * Compile-time capability hints (optional).
   *
   * At runtime, `config.features` decides whether a Feature is enabled; build tooling
   * can read the `compile` info of enabled Features to produce a runtime that only
   * includes the heavy assets actually needed — e.g. the syntect/two_face syntax and
   * theme assets for code highlighting.
   */
  compile?: FeatureCompileHints;

  /**
   * Lifecycle hooks (optional).
   *
   * Used to run custom logic during Feature registration, parsing, rendering, etc.
   */
  hooks?: FeatureHooks<TNode>;
}

export interface FeatureCompileHints {
  codeHighlight?: CodeHighlightCompileHints;
}

export interface CodeHighlightCompileHints {
  /**
   * Whether to enable the base highlighting runtime capability. Language/theme assets
   * are still contributed by the fields below or by other Features, so hosts can trim
   * bundle size via their Feature set.
   */
  runtime?: boolean;

  /**
   * syntect/two_face syntax names that need to be compiled into the highlighting
   * runtime.
   *
   * `'*'` means use the full two_face syntax set; build tooling can use this to select
   * the full artifact.
   */
  languages?: string[];

  /**
   * Alias mapping from Markdown fence lang to syntax name.
   */
  languageAliases?: Record<string, string>;

  /**
   * Theme names that need to be compiled into the highlighting runtime.
   *
   * `'*'` means use the full two_face theme set.
   */
  themes?: string[];

  /**
   * Default light/dark themes. Build tooling only aggregates these; the renderer or
   * host decides which one is actually used.
   */
  defaultThemes?: {
    light?: string;
    dark?: string;
  };
}

export interface CodeHighlightCompileManifest {
  runtime: boolean;
  languages: string[];
  languageAliases: Record<string, string>;
  themes: string[];
  defaultThemes: {
    light?: string;
    dark?: string;
  };
  fullLanguages: boolean;
  fullThemes: boolean;
  featureIds: string[];
}

// ============================================================================
// Layer 2: Prompt definition
// ============================================================================

/**
 * Feature self-describing prompt information definition.
 */
export interface FeaturePromptDefinition {
  /**
   * Feature description.
   * Briefly explains the purpose of this Feature, for the AI to understand.
   */
  description: string;

  /**
   * Syntax structure.
   * Describes the Markdown syntax format.
   */
  syntax: string;

  /**
   * Usage examples.
   * Provide 1-3 typical use cases.
   */
  examples: Array<{
    /** Example explanation */
    desc: string;
    /** Markdown code */
    code: string;
  }>;
}

// ============================================================================
// Layer 2: Feature metadata
// ============================================================================

/**
 * Feature metadata.
 *
 * **Mandatory rules**:
 * - id: must match the `@scope/feature-name` format (e.g. `@supramark/feature-math`)
 * - version: must match the semantic versioning format x.y.z (e.g. `1.0.0`)
 * - name: must not be empty
 * - description: strongly recommended (required in production)
 * - author: strongly recommended
 * - license: should be set to 'Apache-2.0' (Supramark's unified license)
 */
export interface FeatureMetadata {
  /**
   * Unique feature identifier.
   *
   * Format: @scope/feature-name
   * Example: @supramark/feature-math
   *
   * @pattern ^@[\w-]+\/feature-[\w-]+$
   */
  id: string;

  /**
   * Feature name.
   *
   * Must not be empty; should be concise and clear.
   * Example: 'Math Formula', 'Footnote', 'Diagram'
   */
  name: string;

  /**
   * Version number (semantic versioning).
   *
   * Format: x.y.z
   * Example: 1.0.0, 0.1.0
   *
   * @pattern ^\d+\.\d+\.\d+$
   */
  version: string;

  /**
   * Author.
   *
   * Recommended, used to identify the maintainer of the Feature.
   */
  author: string;

  /**
   * Short description.
   *
   * Should clearly describe the purpose and functionality of this Feature.
   * Recommended; strongly recommended in production.
   */
  description: string;

  /**
   * License.
   *
   * Supramark uniformly uses Apache-2.0.
   * Recommended to be set to 'Apache-2.0'.
   */
  license: string;

  /** Homepage URL */
  homepage?: string;

  /** Repository URL */
  repository?: string;

  /**
   * Tags (for categorization).
   *
   * Recommended to add at least one tag, used for categorizing and searching Features.
   * Example: ['math', 'latex', 'formula']
   */
  tags?: string[];

  /**
   * Syntax family (optional).
   *
   * Used to coarsely categorize Features by syntax form, for documentation, matrix
   * views, and future tooling.
   *
   * - 'main'      : the main Markdown syntax (the original spec; GFM / Math / Emoji /
   *   footnotes etc. all use this family);
   * - 'container' : container syntax based on :::name ... ::: (admonition / html / map
   *   etc.);
   * - 'fence'     : syntax based on ```lang code fences (diagram / various code-fence
   *   extensions etc.).
   *
   * Currently for documentation and classification purposes only; runtime logic does
   * not depend on this field.
   */
  syntaxFamily?: 'main' | 'container' | 'fence';
}

// ============================================================================
// Layer 2: Syntax definition
// ============================================================================

/**
 * Syntax definition (Markdown input → AST output).
 *
 * **Positioning of the parser**:
 * - At the current stage (v0.x), the parser is used mainly for **documentation and
 *   specification**
 * - The actual parsing logic may still be hardcoded in `parse()`
 * - When a Feature reuses existing parsing logic (e.g. the various diagram engines),
 *   the parser can be omitted
 * - Future versions will gradually support driving the parsing pipeline via Feature
 *   registration
 */
export interface SyntaxDefinition<TNode extends SupramarkNode> {
  /** AST node definition */
  ast: ASTNodeDefinition<TNode>;

  /**
   * Parsing rules (optional).
   *
   * - Can be omitted if this Feature reuses existing parsing logic (e.g. diagram)
   * - Must be provided if a custom parser is needed (e.g. a new syntax extension)
   */
  parser?: ParserRules;

  /** Validation rules (optional) */
  validator?: ValidatorRules<TNode>;
}

// ----------------------------------------------------------------------------
// Layer 3: AST node definition
// ----------------------------------------------------------------------------

/**
 * AST node definition.
 *
 * **Node selector**:
 * - In some cases multiple Features may share the same AST node type
 * - Example: Vega-Lite, Mermaid, and PlantUML all use `type: 'diagram'`, distinguished
 *   by the `engine` field
 * - Use a `selector` function to match a subset of nodes, rather than relying on
 *   `type` alone
 *
 * **Mandatory rules**:
 * - type: must be defined and must not be empty
 * - interface: strongly recommended for a production Feature
 * - examples: recommended to provide at least one example node
 *
 * @example
 * // The Vega-Lite Feature only cares about diagram nodes whose engine is 'vega-lite'
 * const vegaLiteAST: ASTNodeDefinition<DiagramNode> = {
 *   type: 'diagram',
 *   selector: (node) => node.type === 'diagram' && ['vega-lite', 'vega'].includes(node.engine),
 *   interface: { ... }
 * };
 */
export interface ASTNodeDefinition<TNode extends SupramarkNode> {
  /**
   * Node type name.
   *
   * Required, must not be empty.
   * Example: 'math_inline', 'diagram', 'footnote_reference'
   */
  type: string;

  /**
   * Node selector (optional).
   *
   * Used to precisely match the subset of nodes this Feature cares about.
   * When multiple Features share the same `type`, this function distinguishes between
   * them.
   * If a Feature handles multiple node types, it should provide a selector function.
   *
   * @param node - the AST node to match against
   * @returns true if the node belongs to this Feature, false otherwise
   *
   * @example
   * // Match all diagram nodes whose engine is 'plantuml'
   * selector: (node) => node.type === 'diagram' && node.engine === 'plantuml'
   *
   * @example
   * // Match both footnote_reference and footnote_definition types
   * selector: (node) =>
   *   node.type === 'footnote_reference' || node.type === 'footnote_definition'
   */
  selector?: (node: SupramarkNode) => boolean;

  /**
   * Node interface (TypeScript type).
   *
   * Strongly recommended for a production Feature; used for documentation,
   * validation, and type safety.
   */
  interface?: NodeInterface<TNode>;

  /** Positional constraints for the node within the AST tree */
  constraints?: NodeConstraints;

  /**
   * Example nodes.
   *
   * Recommended to provide at least one example node, used for documentation and
   * testing.
   */
  examples?: TNode[];

  /**
   * Multi-node-type note (optional).
   *
   * If this Feature handles multiple node types, explain it here.
   * Example: 'Note: the Footnote Feature usually needs to handle both
   * footnote_reference and footnote_definition node types'
   */
  multiNodeNote?: string;
}

/**
 * Node interface definition.
 *
 * **Mandatory rules**:
 * - required: should not contain only 'type'; should include the node's key fields
 * - fields: should define the type and description for every required field
 */
export interface NodeInterface<TNode> {
  /**
   * Required fields of the node.
   *
   * Should not contain only 'type'; should include the node's key fields.
   * Example: ['type', 'index', 'label'] rather than ['type']
   */
  required: Array<keyof TNode>;

  /**
   * Optional fields of the node.
   */
  optional?: Array<keyof TNode>;

  /**
   * Field type descriptions.
   *
   * Should define the type and description for every required field.
   */
  fields: Record<string, FieldDefinition>;
}

/**
 * Field definition.
 */
export interface FieldDefinition {
  /** Field type */
  type: 'string' | 'number' | 'boolean' | 'object' | 'array' | 'node' | 'nodes';

  /** Field description */
  description: string;

  /** Default value */
  default?: unknown;

  /** Validation rule */
  validate?: (value: unknown) => boolean;
}

/**
 * Node constraints.
 */
export interface NodeConstraints {
  /** Allowed parent node types */
  allowedParents?: string[];

  /** Allowed child node types */
  allowedChildren?: string[];

  /** Whether the node can nest itself */
  allowSelfNesting?: boolean;

  /** Whether the node must have children */
  requireChildren?: boolean;
}

// ----------------------------------------------------------------------------
// Layer 3: Parsing rules
// ----------------------------------------------------------------------------

/**
 * Parsing rules.
 *
 * Since AST v2, Markdown tokenization is handled uniformly by the Rust
 * `supramark-markdown` crate; the Feature layer only describes custom post-processing
 * or host-side extended semantics.
 */
export interface ParserRules {
  /** Parser type */
  engine: 'supramark-markdown' | 'custom';

  /** Custom parser */
  custom?: CustomParserRules;
}

/**
 * Parser context.
 */
export interface ParserContext {
  /** Raw Markdown source text */
  source: string;

  /** Parent node stack */
  stack: SupramarkNode[];

  /** Current parent node */
  parent: SupramarkNode;
}

/**
 * Custom parsing rules.
 */
export interface CustomParserRules {
  /** Regular expression match */
  pattern?: RegExp;

  /** Custom parsing function */
  parse: (input: string, context: ParserContext) => SupramarkNode | null;
}

// ----------------------------------------------------------------------------
// Layer 3: Validation rules
// ----------------------------------------------------------------------------

/**
 * Validation rules.
 */
export interface ValidatorRules<TNode extends SupramarkNode> {
  /** Node validation function */
  validate: (node: TNode) => ValidationResult;

  /** Strict mode (whether to throw on validation failure) */
  strict?: boolean;
}

/**
 * Validation result.
 */
export interface ValidationResult {
  /** Whether validation passed */
  valid: boolean;

  /** List of error messages */
  errors?: ValidationError[];

  /** List of warning messages */
  warnings?: ValidationWarning[];
}

/**
 * Validation error.
 */
export interface ValidationError {
  /** Error code */
  code: string;

  /** Error message */
  message: string;

  /** Error location (node path) */
  path?: string;

  /** Related data */
  data?: unknown;
}

/**
 * Validation warning.
 */
export interface ValidationWarning {
  /** Warning code */
  code: string;

  /** Warning message */
  message: string;

  /** Suggested fix */
  suggestion?: string;
}

// ============================================================================
// Layer 2: Renderer definitions
// ============================================================================

/**
 * Renderer definitions (AST → per-platform components).
 *
 * **Important constraints**:
 * - The core package is **pure TypeScript type definitions** with no dependency on
 *   React/RN
 * - A renderer's `render` function should be a **type reference and signature only**,
 *   without a JSX implementation
 * - The real React component implementation belongs in the `@supramark/rn` and
 *   `@supramark/web` packages
 *
 * **Current-stage positioning**:
 * - The `renderers` field on a Feature is mainly used to:
 *   1. Declare which platforms this feature needs support on
 *   2. Describe the renderer's infrastructure requirements
 *   3. List the external library dependencies
 * - The actual rendering logic is still implemented in each platform package
 *   (@supramark/rn, @supramark/web)
 *
 * **For complex features (e.g. diagrams)**:
 * - The `infrastructure` field can declare the need for a worker, client-side script,
 *   native adapter, etc.
 * - The actual worker/script implementation still lives in each platform package
 *
 * @example
 * // A simplified renderer definition (only declares platform support and dependencies)
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
  /** React Native renderer */
  rn?: PlatformRenderer<TNode, Platform>;

  /** Web (React) renderer */
  web?: PlatformRenderer<TNode, Platform>;

  /** CLI (terminal) renderer */
  cli?: PlatformRenderer<TNode, Platform>;

  /** Custom platform renderer */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  [platform: string]: PlatformRenderer<TNode, any> | undefined;
}

// ----------------------------------------------------------------------------
// Layer 3: Platform renderer
// ----------------------------------------------------------------------------

/**
 * Platform renderer.
 *
 * **Positioning of the render function**:
 * - At the current stage (v0.x), `render` is mainly used for **type signature
 *   definitions**
 * - The core package should not contain an actual JSX implementation
 * - Can be omitted if the Feature reuses existing rendering logic (e.g. diagram)
 * - The actual render component should be implemented in @supramark/rn and
 *   @supramark/web
 */
export interface PlatformRenderer<TNode extends SupramarkNode, TPlatform extends Platform> {
  /** Platform identifier (kept for compatibility with existing feature definitions) */
  platform?: TPlatform;

  /**
   * Render function (optional).
   *
   * - If provided, it is only a type-signature reference and does not include a JSX
   *   implementation
   * - Can be omitted if this Feature reuses an existing renderer (e.g. diagram)
   */
  render?: RenderFunction<TNode, TPlatform>;

  /** Style definition (optional) */
  styles?: StyleDefinition<TPlatform>;

  /** Rendering infrastructure requirements (optional but recommended) */
  infrastructure?: InfrastructureRequirements;

  /** External library dependencies (optional but recommended) */
  dependencies?: PlatformDependency[];
}

/**
 * Platform type.
 */
export type Platform = string;

/**
 * Render function.
 */
export interface RenderFunction<TNode extends SupramarkNode, TPlatform> {
  (node: TNode, context: RenderContext<TPlatform>): RenderOutput<TPlatform>;
}

/**
 * Render context.
 */
export interface RenderContext<TPlatform> {
  /** The current platform */
  platform: TPlatform;

  /** The node's index within the list (used as the React key) */
  key: number;

  /** The style system */
  styles: TPlatform extends 'rn' ? ReactNativeStyles : WebStyles;

  /** Helper function for rendering child nodes */
  renderChildren: (children: SupramarkNode[]) => RenderOutput<TPlatform>[];

  /** Custom data */
  data?: Record<string, unknown>;
}

/**
 * Render output.
 *
 * The core package stays framework-agnostic and does not depend on React types
 * directly. Upstream applications instantiate this type as React.ReactNode.
 */
export type RenderOutput<TPlatform> = TPlatform extends 'cli' ? string : unknown;

/**
 * React Native style type (simplified).
 */
export type ReactNativeStyles = Record<string, unknown>;

/**
 * Web style type (CSS class names).
 */
export type WebStyles = Record<string, string>;

// ----------------------------------------------------------------------------
// Layer 3: Style definition
// ----------------------------------------------------------------------------

/**
 * Style definition.
 */
export interface StyleDefinition<TPlatform> {
  /** Default style */
  default: TPlatform extends 'rn' ? ReactNativeStyles : WebStyles;

  /** Theme variants */
  themes?: Record<string, TPlatform extends 'rn' ? ReactNativeStyles : WebStyles>;

  /** Style variables */
  variables?: StyleVariables;
}

/**
 * Style variables.
 */
export interface StyleVariables {
  /** Colors */
  colors?: Record<string, string>;

  /** Sizes */
  sizes?: Record<string, number>;

  /** Fonts */
  fonts?: Record<string, string>;

  /** Other custom variables */
  [key: string]: unknown;
}

// ----------------------------------------------------------------------------
// Layer 3: Infrastructure requirements
// ----------------------------------------------------------------------------

/**
 * Renderer infrastructure requirements declared by a feature so hosts
 * can decide whether to wire it up.
 */
export interface InfrastructureRequirements {
  /** Whether a background worker is needed for rendering. */
  needsWorker?: boolean;

  /**
   * Worker type. Rendering engines now use Web workers, service workers,
   * JS SVG-string exporters, or native FFI adapters depending on platform.
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
 * Cache configuration.
 */
export interface CacheConfig {
  /** Maximum number of cache entries */
  maxSize?: number;

  /** TTL (milliseconds) */
  ttl?: number;

  /** Cache key generator */
  keyGenerator?: (node: SupramarkNode) => string;
}

// ----------------------------------------------------------------------------
// Layer 3: Platform dependency
// ----------------------------------------------------------------------------

/**
 * Platform dependency.
 */
export interface PlatformDependency {
  /** Dependency name */
  name: string;

  /** Dependency version */
  version: string;

  /** Dependency type */
  type: 'npm' | 'cdn' | 'system';

  /** CDN URL (if it's a CDN dependency) */
  cdnUrl?: string;

  /** Whether it's optional */
  optional?: boolean;
}

// ============================================================================
// Layer 2: Test definition
// ============================================================================

/**
 * Test definition.
 */
export interface TestingDefinition<TNode extends SupramarkNode> {
  /** Syntax tests (Markdown → AST) */
  syntaxTests?: SyntaxTestSuite<TNode>;

  /** Render tests (AST → component) */
  renderTests?: RenderTestSuite<TNode>;

  /** Integration tests */
  integrationTests?: IntegrationTestSuite;

  /** Test coverage requirements */
  coverageRequirements?: CoverageRequirements;
}

// ----------------------------------------------------------------------------
// Layer 3: Test suites
// ----------------------------------------------------------------------------

/**
 * Syntax test suite.
 */
export interface SyntaxTestSuite<TNode> {
  /** Test cases */
  cases: SyntaxTestCase<TNode>[];
}

/**
 * Syntax test case.
 */
export interface SyntaxTestCase<TNode> {
  /** Test name */
  name: string;

  /** Input Markdown */
  input: string;

  /** Expected AST node(s) */
  expected: TNode | TNode[];

  /** Test options */
  options?: {
    /** Whether to check only the node type */
    typeOnly?: boolean;

    /** Fields to ignore */
    ignoreFields?: string[];
  };
}

/**
 * Render test suite.
 */
export interface RenderTestSuite<TNode> {
  /** Test cases (grouped by platform) */
  rn?: RenderTestCase<TNode, 'rn'>[];
  web?: RenderTestCase<TNode, 'web'>[];
  cli?: RenderTestCase<TNode, 'cli'>[];
}

/**
 * Render test case.
 */
export interface RenderTestCase<TNode, TPlatform> {
  /** Test name */
  name: string;

  /** Input AST node */
  input: TNode;

  /** Expected render output (or a validation function) */
  expected: RenderOutput<TPlatform> | ((output: RenderOutput<TPlatform>) => boolean);

  /** Snapshot test */
  snapshot?: boolean;
}

/**
 * Integration test suite.
 */
export interface IntegrationTestSuite {
  /** End-to-end test cases */
  cases: IntegrationTestCase[];
}

/**
 * Integration test case.
 */
export interface IntegrationTestCase {
  /** Test name */
  name: string;

  /** Input Markdown */
  input: string;

  /** Validation function */
  validate: (result: unknown) => boolean;

  /** Test platforms */
  platforms?: Platform[];
}

/**
 * Coverage requirements.
 */
export interface CoverageRequirements {
  /** Statement coverage */
  statements?: number;

  /** Branch coverage */
  branches?: number;

  /** Function coverage */
  functions?: number;

  /** Line coverage */
  lines?: number;
}

// ============================================================================
// Layer 2: Documentation definition
// ============================================================================

/**
 * Documentation definition.
 */
export interface DocumentationDefinition {
  /** README content */
  readme: string;

  /** API documentation */
  api?: APIDocumentation;

  /** Best practices */
  bestPractices?: string[];

  /** Frequently asked questions */
  faq?: FAQItem[];
}

// ----------------------------------------------------------------------------
// Layer 3: Documentation sub-items
// ----------------------------------------------------------------------------

/**
 * API documentation.
 */
export interface APIDocumentation {
  /** Interface documentation */
  interfaces: InterfaceDoc[];

  /** Function documentation */
  functions?: FunctionDoc[];

  /** Type documentation */
  types?: TypeDoc[];
}

/**
 * Interface documentation.
 */
export interface InterfaceDoc {
  /** Interface name */
  name: string;

  /** Description */
  description: string;

  /** Field list */
  fields: FieldDoc[];
}

/**
 * Field documentation.
 */
export interface FieldDoc {
  /** Field name */
  name: string;

  /** Type */
  type: string;

  /** Description */
  description: string;

  /** Whether it's required */
  required: boolean;

  /** Default value */
  default?: string;
}

/**
 * Function documentation.
 */
export interface FunctionDoc {
  /** Function name */
  name: string;

  /** Description */
  description: string;

  /** Parameter list */
  parameters: ParameterDoc[];

  /** Return value */
  returns: string;

  /** Examples */
  examples?: string[];
}

/**
 * Parameter documentation.
 */
export interface ParameterDoc {
  /** Parameter name */
  name: string;

  /** Type */
  type: string;

  /** Description */
  description: string;

  /** Whether it's optional */
  optional?: boolean;
}

/**
 * Type documentation.
 */
export interface TypeDoc {
  /** Type name */
  name: string;

  /** Description */
  description: string;

  /** Type definition */
  definition: string;
}

/**
 * Example definition.
 */
export interface ExampleDefinition {
  /** Example name */
  name: string;

  /** Description */
  description: string;

  /** Markdown input */
  markdown: string;

  /** Expected output (optional) */
  output?: string;

  /** Code example (how to use it) */
  code?: string;

  /** Online demo URL */
  demoUrl?: string;
}

/**
 * FAQ item.
 */
export interface FAQItem {
  /** Question */
  question: string;

  /** Answer */
  answer: string;

  /** Related links */
  links?: string[];
}

// ============================================================================
// Layer 2: Lifecycle hooks
// ============================================================================

/**
 * Feature lifecycle hooks.
 */
export interface FeatureHooks<TNode extends SupramarkNode> {
  /** Before feature registration */
  beforeRegister?: () => void | Promise<void>;

  /** After feature registration */
  afterRegister?: () => void | Promise<void>;

  /** Before parsing */
  beforeParse?: (markdown: string) => string;

  /** After parsing */
  afterParse?: (ast: TNode[]) => TNode[];

  /** Before rendering */
  beforeRender?: (node: TNode) => TNode;

  /** After rendering */
  afterRender?: (output: unknown) => unknown;

  /** Feature unregistration */
  onUnregister?: () => void | Promise<void>;
}

// ============================================================================
// Utility types
// ============================================================================

/**
 * Recursively expand all fields to required.
 */
export type DeepRequired<T> = {
  [P in keyof T]-?: T[P] extends object ? DeepRequired<T[P]> : T[P];
};

/**
 * Recursively expand all fields to optional.
 */
export type DeepPartial<T> = {
  [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};

/**
 * Extract a feature's AST node type.
 */
export type FeatureNodeType<F extends SupramarkFeature<SupramarkNode>> =
  F extends SupramarkFeature<infer TNode> ? TNode : never;

// ============================================================================
// Minimal Feature interface
// ============================================================================

/**
 * Minimal Feature definition.
// ============================================================================
// Feature registration and discovery mechanism
// ============================================================================

/**
 * Feature registry.
 *
 * Used to collect and manage all defined Features.
 */
export class FeatureRegistry {
  // The registry uses `any` internally to avoid over-constraining node types.
  // This allows registering a Feature that targets a specific node subtype (e.g.
  // SupramarkDiagramNode) without affecting external callers that query uniformly
  // via SupramarkNode.
  private static features = new Map<string, SupramarkFeature<SupramarkNode>>();

  /**
   * Register a Feature.
   *
   * @param feature - the Feature definition
   * @throws if the Feature ID already exists
   */
  static register<TNode extends SupramarkNode>(feature: SupramarkFeature<TNode>): void {
    const id = feature.metadata.id;

    if (this.features.has(id)) {
      throw new Error(`Feature "${id}" is already registered`);
    }

    // A feature is registered for a specific node subtype (e.g. SupramarkDiagramNode),
    // but the registry stores them under the common SupramarkNode contract. The widening
    // is sound because the registry never calls node-specific functions with a wider node.
    this.features.set(id, feature as unknown as SupramarkFeature<SupramarkNode>);
  }

  /**
   * Get the Feature with the given ID.
   *
   * @param id - the Feature ID
   * @returns the Feature definition, or undefined if it doesn't exist
   */
  static get(id: string): SupramarkFeature<SupramarkNode> | undefined {
    return this.features.get(id);
  }

  /**
   * List all registered Features.
   *
   * @returns the list of Features
   */
  static list(): Array<SupramarkFeature<SupramarkNode>> {
    return Array.from(this.features.values());
  }

  /**
   * Find Features by tag.
   *
   * @param tag - the tag name
   * @returns all Features that include this tag
   */
  static findByTag(tag: string): Array<SupramarkFeature<SupramarkNode>> {
    return this.list().filter(
      feature => 'tags' in feature.metadata && feature.metadata.tags?.includes(tag)
    );
  }

  /**
   * Find Features that match the given AST node.
   *
   * @param node - the AST node
   * @returns the matching Features
   */
  static findByNode(node: SupramarkNode): Array<SupramarkFeature<SupramarkNode>> {
    return this.list().filter(feature => {
      const ast = feature.syntax.ast;

      // Check the node type
      if (ast.type !== node.type) {
        return false;
      }

      // If a selector is present, use it to filter further
      if (ast.selector) {
        return ast.selector(node);
      }

      return true;
    });
  }

  /**
   * Clear the registry (mainly for tests).
   */
  static clear(): void {
    this.features.clear();
  }
}

// ============================================================================
// Helper functions
// ============================================================================

/**
 * Create a complete Feature.
 *
 * A helper function that provides better type inference.
 *
 * @param feature - the Feature definition
 * @returns the typed SupramarkFeature
 */
export function defineFeature<TNode extends SupramarkNode>(
  feature: SupramarkFeature<TNode>
): SupramarkFeature<TNode> {
  return feature;
}

/**
 * Validate the completeness of a Feature definition.
 *
 * Mirrors the Feature Linter rules and provides runtime validation.
 *
 * @param feature - the Feature definition
 * @param options - validation options
 * @returns the validation result
 */
export function validateFeature<TNode extends SupramarkNode = SupramarkNode>(
  feature: Partial<SupramarkFeature<TNode>> & {
    metadata?: Partial<FeatureMetadata>;
    syntax?: { ast?: Partial<ASTNodeDefinition<TNode>> } & Record<string, unknown>;
  },
  options: {
    /** Strict mode (treat warnings as errors) */
    strict?: boolean;
    /** Whether this is production (stricter requirements) */
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
  // Critical Rules (error severity) - must pass
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
      message: 'Feature ID must match the @scope/feature-name format (e.g. @supramark/feature-math)',
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
      message: 'Version must match the semantic versioning format x.y.z (e.g. 1.0.0)',
      severity: 'error',
    });
  }

  // metadata-name-required
  if (!metadata.name || metadata.name.trim().length === 0) {
    errors.push({
      code: 'metadata-name-required',
      message: 'Feature name must not be empty',
      severity: 'error',
    });
  }

  // ast-type-required
  if (!ast.type || String(ast.type).trim().length === 0) {
    errors.push({
      code: 'ast-type-required',
      message: 'Feature must define an AST node type',
      severity: 'error',
    });
  }

  // ============================================================================
  // Warning Rules (warning severity) - strongly recommended
  // ============================================================================

  // metadata-description-required
  if (!metadata.description || metadata.description.trim().length === 0) {
    errors.push({
      code: 'metadata-description-required',
      message: 'Feature description must not be empty',
      severity: 'warning',
    });
  }

  // metadata-author-required
  if (!metadata.author || metadata.author.trim().length === 0) {
    errors.push({
      code: 'metadata-author-required',
      message: 'Feature author should be provided',
      severity: 'warning',
    });
  }

  // metadata-license-required
  if (!metadata.license) {
    errors.push({
      code: 'metadata-license-required',
      message: 'Feature license should be set',
      severity: 'warning',
    });
  } else if (metadata.license !== 'Apache-2.0') {
    errors.push({
      code: 'metadata-license-apache',
      message: 'Feature license should be set to Apache-2.0 (Supramark\'s unified license)',
      severity: 'info',
    });
  }

  // ast-interface-required-nonempty
  if (ast.interface) {
    const required = ast.interface.required;
    if (!Array.isArray(required) || required.length <= 1) {
      errors.push({
        code: 'ast-interface-required-nonempty',
        message: 'AST interface.required should not contain only type; it should include the node\'s key fields',
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
        message: `AST interface.fields should define all required fields; missing: ${missingFields.join(', ')}`,
        severity: 'warning',
      });
    }
  }

  // selector-multi-node-with-function
  if (ast.multiNodeNote && !ast.selector) {
    errors.push({
      code: 'selector-multi-node-with-function',
      message: 'If a Feature handles multiple node types (has multiNodeNote), it should provide a selector function',
      severity: 'warning',
    });
  }

  // ============================================================================
  // Info Rules (info severity) - best practices
  // ============================================================================

  // metadata-tags-nonempty
  if (!metadata.tags || metadata.tags.length === 0) {
    errors.push({
      code: 'metadata-tags-nonempty',
      message: 'Feature tags should include at least one tag, for categorization and search',
      severity: 'info',
    });
  }

  // ast-examples-provided
  if (!ast.examples || ast.examples.length === 0) {
    errors.push({
      code: 'ast-examples-provided',
      message: 'AST examples should include at least one example node, for documentation and testing',
      severity: 'info',
    });
  }

  // ============================================================================
  // Production Mode Extra Checks
  // ============================================================================

  if (options.production) {
    // In production mode, interface should be required
    if (!ast.interface) {
      errors.push({
        code: 'ast-interface-required-production',
        message: 'A production Feature must define a complete AST interface',
        severity: 'error',
      });
    }

    // In production mode, at least one renderer should be defined
    if ('renderers' in feature) {
      const renderers = feature.renderers as RendererDefinitions<SupramarkNode>;
      const hasRenderer = renderers && (renderers.rn || renderers.web || renderers.cli);
      if (!hasRenderer) {
        errors.push({
          code: 'renderers-required-production',
          message: 'A production Feature must define at least one platform renderer (rn, web, or cli)',
          severity: 'error',
        });
      }
    }

    // In production mode, tests are recommended
    if (!('testing' in feature) || !feature.testing) {
      errors.push({
        code: 'testing-recommended-production',
        message: 'A production Feature is strongly recommended to provide a test definition',
        severity: 'warning',
      });
    }
  }

  // ============================================================================
  // Compute the final result
  // ============================================================================

  // In strict mode, warnings also count as errors
  const criticalErrors = errors.filter(e =>
    options.strict ? e.severity !== 'info' : e.severity === 'error'
  );

  return {
    valid: criticalErrors.length === 0,
    errors,
  };
}

// ============================================================================
// Feature configuration system
// ============================================================================

/**
 * Feature runtime configuration.
 *
 * Used at runtime to control whether a Feature is enabled/disabled and its behavior.
 */
export interface FeatureConfig {
  /** Feature ID */
  id: string;

  /** Whether this Feature is enabled */
  enabled: boolean;

  /** Feature-specific configuration options (optional) */
  options?: unknown;
}

/**
 * A FeatureConfig with strongly-typed options.
 *
 * - Used by each Feature package to define its own XXXFeatureConfig type;
 * - Still treated as a plain FeatureConfig (options: unknown) at the core layer, but
 *   provides full type hints in business code.
 */
export type FeatureConfigWithOptions<TOptions> = Omit<FeatureConfig, 'options'> & {
  options?: TOptions;
};

/**
 * Supramark runtime configuration.
 *
 * Used to configure the behavior of an entire Supramark instance.
 */
export interface SupramarkConfig {
  /** List of enabled Features */
  features?: FeatureConfig[];

  /** Global configuration options */
  options?: {
    /**
     * Whether to enable the runtime cache.
     *
     * Used as the default when a specific subsystem hasn't declared its own cache
     * policy; subsystem- or engine-level policies can explicitly override it.
     */
    cache?: boolean;

    /** Whether to enable strict mode (stricter validation) */
    strict?: boolean;

    /**
     * Whether to allow rendering raw HTML (HTML blocks / inline raw HTML).
     *
     * - Default `false`: raw nodes are dropped, equivalent to the node not existing
     *   (matching behavior from before raw rendering was introduced), avoiding
     *   untrusted markdown executing scripts.
     * - Set to `true`: enables raw HTML passthrough (matching CommonMark spec
     *   expectations), but the host must ensure the markdown source is trusted —
     *   the output is not sanitized at all, so `<img onerror>`, `<iframe srcdoc>`,
     *   etc. get written into the DOM verbatim.
     */
    allowDangerousHtml?: boolean;

    /**
     * GFM "Disallowed Raw HTML" (tagfilter): drop a small allowlist of unsafe
     * tags (`<title>`, `<textarea>`, `<style>`, `<xmp>`, `<iframe>`,
     * `<noembed>`, `<noframes>`, `<script>`, `<plaintext>`) when rendering
     * raw HTML.
     *
     * - Default `false`: raw HTML (when `allowDangerousHtml` is on) is passed
     *   through verbatim.
     * - Set to `true`: the tagfilter rewrites those tags so the browser does
     *   not treat them as raw markup. cmark-gfm applies this only in its
     *   "Disallowed Raw HTML" / "HTML tag filter" spec sections; hosts
     *   rendering untrusted markdown with raw HTML on should opt in.
     */
    gfmTagfilter?: boolean;

    /**
     * GFM footnote section rendering: emit the trailing `<section>` with
     * back-references, mirroring cmark-gfm's footnote extension output.
     *
     * - Default `false`: footnote definitions render as plain blocks.
     * - Set to `true`: the GFM footnote section format is emitted. CommonMark
     *   has no footnotes extension, so the default stays off.
     */
    gfmFootnoteStyle?: boolean;

    /**
     * Flatten a `<strong>` whose parent is also `<strong>` (cmark-gfm 0.29's
     * `html.c` `CMARK_NODE_STRONG` rule), so `__foo, __bar__, baz__` renders
     * as a single `<strong>foo, bar, baz</strong>` instead of the nested
     * `<strong>foo, <strong>bar</strong>, baz</strong>` the delimiter-run
     * algorithm produces.
     *
     * - Default `false`: keep the nesting (CommonMark 0.31 behavior — the two
     *   references diverge on the same input).
     * - Set to `true`: flatten, matching cmark-gfm 0.29.
     */
    flattenNestedStrong?: boolean;

    /** Other global configuration */
    [key: string]: unknown;
  };

  /**
   * Diagram subsystem configuration.
   *
   * - Used to control global diagram-rendering-related behavior (timeouts, caching,
   *   extra per-engine parameters, etc.);
   * - Only defines the structure; actually consumed by the upstream runtime
   *   (@supramark/rn / @supramark/web);
   * - If unset, each runtime falls back to its own defaults (backward compatible).
   */
  diagram?: SupramarkDiagramConfig;
}

/**
 * Generate a default configuration from the FeatureRegistry.
 *
 * @param enabledByDefault - whether all Features are enabled by default (default true)
 * @returns a Supramark configuration object
 *
 * Behavior note: the returned config always has `options.cache: true`, meaning the
 * host runtime (@supramark/rn / @supramark/web) enables a process-level runtime cache
 * — reused by parsed documents and normalized diagram SVGs across scenarios like
 * virtual-list remounts. This cache is bounded by default (by entry count) and can be
 * adjusted or disabled via `diagram.defaultCache` / `diagram.engines[engine].cache`.
 * Hosts built on {@link createConfigFromRegistry} therefore get this caching behavior
 * by default; to disable it, pass a config that explicitly overrides
 * `options.cache: false`.
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
 * Get the list of enabled Feature IDs from a configuration.
 *
 * @param config - the Supramark configuration
 * @returns an array of enabled Feature IDs
 */
export function getEnabledFeatureIds(config: SupramarkConfig): string[] {
  return (config.features || []).filter(f => f.enabled).map(f => f.id);
}

/**
 * Get the list of enabled Feature definitions.
 *
 * @param config - the Supramark configuration
 * @returns an array of enabled Feature definitions
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
 * Aggregate the code-highlighting compile assets declared by the enabled Features.
 *
 * This function only performs a deterministic merge; it does not load any
 * highlighting implementation. Build tooling can write the output as a
 * JSON/TS/Rust manifest, which the syntect + two_face runtime then uses to produce a
 * trimmed-down artifact.
 */
export function createCodeHighlightCompileManifest(
  features: readonly SupramarkFeature<SupramarkNode>[]
): CodeHighlightCompileManifest {
  const languages = new Set<string>();
  const themes = new Set<string>();
  const languageAliases: Record<string, string> = {};
  const featureIds: string[] = [];

  let runtime = false;
  let fullLanguages = false;
  let fullThemes = false;
  let defaultLight: string | undefined;
  let defaultDark: string | undefined;

  for (const feature of features) {
    const hints = feature.compile?.codeHighlight;
    if (!hints) continue;

    featureIds.push(feature.metadata.id);
    runtime = runtime || hints.runtime === true;

    for (const lang of hints.languages ?? []) {
      if (lang === '*') {
        fullLanguages = true;
      } else {
        languages.add(lang);
      }
    }

    for (const theme of hints.themes ?? []) {
      if (theme === '*') {
        fullThemes = true;
      } else {
        themes.add(theme);
      }
    }

    for (const [alias, target] of Object.entries(hints.languageAliases ?? {})) {
      languageAliases[alias] = target;
    }

    defaultLight = hints.defaultThemes?.light ?? defaultLight;
    defaultDark = hints.defaultThemes?.dark ?? defaultDark;
  }

  return {
    runtime,
    languages: fullLanguages ? ['*'] : [...languages].sort(),
    languageAliases: Object.fromEntries(
      Object.entries(languageAliases).sort(([a], [b]) => a.localeCompare(b))
    ),
    themes: fullThemes ? ['*'] : [...themes].sort(),
    defaultThemes: {
      light: defaultLight,
      dark: defaultDark,
    },
    fullLanguages,
    fullThemes,
    featureIds: featureIds.sort(),
  };
}

/**
 * Check whether a specific Feature is enabled.
 *
 * @param config - the Supramark configuration
 * @param featureId - the Feature ID
 * @returns whether it's enabled
 */
export function isFeatureEnabled(config: SupramarkConfig, featureId: string): boolean {
  const featureConfig = config.features?.find(f => f.id === featureId);
  return featureConfig?.enabled ?? false;
}

export type DiagramFeatureFamilyId = 'mermaid' | 'vega-family' | 'echarts' | 'graphviz-family';

const DIAGRAM_FEATURE_IDS_BY_FAMILY: Record<DiagramFeatureFamilyId, readonly string[]> = {
  mermaid: ['@supramark/feature-mermaid'],
  'vega-family': ['@supramark/feature-diagram-vega-lite'],
  echarts: ['@supramark/feature-diagram-echarts'],
  'graphviz-family': ['@supramark/feature-diagram-dot'],
};

/**
 * Classify a diagram engine into one of the feature families currently supported.
 *
 * Current convention:
 * - mermaid
 * - plantuml
 * - vega-family (vega / vega-lite / chart / chartjs)
 * - echarts
 * - graphviz-family (dot / graphviz)
 */
export function getDiagramFeatureFamily(
  engine: SupramarkDiagramEngineId
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
 * Map a diagram engine to the corresponding list of feature IDs.
 */
export function getDiagramFeatureIdsForEngine(engine: SupramarkDiagramEngineId): string[] {
  const family = getDiagramFeatureFamily(engine);
  if (!family) {
    return [];
  }

  return [...DIAGRAM_FEATURE_IDS_BY_FAMILY[family]];
}

/**
 * Determine whether a group of Feature IDs is enabled.
 *
 * Convention:
 * - No config provided, or config.features is empty → treated as all enabled;
 * - If config doesn't mention any of these IDs at all → treated as default behavior
 *   (enabled);
 * - Once any of these IDs is explicitly configured, the config takes precedence —
 *   enabled if at least one has enabled:true.
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
 * Determine whether a given diagram engine is enabled based on the configuration.
 */
export function isDiagramFeatureEnabled(
  config: SupramarkConfig | undefined,
  engine: SupramarkDiagramEngineId,
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
 * Get a Feature's configuration options.
 *
 * @param config - the Supramark configuration
 * @param featureId - the Feature ID
 * @returns the Feature's configuration options, or an empty object if not configured
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
 * Get a Feature's configuration options in a strongly-typed way.
 *
 * - The return type is determined by the caller via the generic parameter;
 * - Returns undefined if the corresponding Feature is not configured.
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
  // Arrow-typed properties (not method signatures) so callers can safely
  // re-export `helpers.create` / `helpers.getOptions` without `this` binding.
  create: (enabled?: boolean, options?: TOptions) => FeatureConfigWithOptions<TOptions>;
  getOptions: (config?: SupramarkConfig) => TOptions | undefined;
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
    create: (enabled = true, options) => {
      return { id: featureId, enabled, options };
    },
    getOptions: (config) => {
      return getFeatureOptionsAs<TOptions>(config, featureId);
    },
  };
}
