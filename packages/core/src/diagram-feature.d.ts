import type { SupramarkDiagramNode } from './ast.js';
import type { ExampleDefinition, InfrastructureRequirements, PlatformDependency, SupramarkFeature } from './feature.js';
/**
 * Spec for `defineDiagramFeature`.
 *
 * Captures the per-engine variation across all in-tree diagram features
 * (mermaid / d2 / plantuml / dot / echarts / vega-lite). The factory fills
 * in the parts that are identical across engines (interface shape, smoke
 * test scaffold, integration test scaffold, default coverage requirements,
 * documentation API block, etc.).
 */
export interface DefineDiagramFeatureSpec {
    /** Unique feature ID, e.g. `'@supramark/feature-d2'`. */
    id: string;
    /** Canonical engine identifier matched in the AST, e.g. `'d2'`. */
    engineId: string;
    /** Optional aliases also matched (compared lower-case), e.g. `['graphviz']` for dot. */
    engineAliases?: readonly string[];
    /** Display name, e.g. `'Diagram (D2)'`. */
    name: string;
    /** Feature version. Defaults to `'0.1.0'`. */
    version?: string;
    /** Short description used in metadata. */
    description: string;
    /** Tags. Defaults to `['diagram', engineId]`. */
    tags?: readonly string[];
    /**
     * Web renderer overrides. The `platform` field is auto-filled. Defaults:
     * `{ infrastructure: { needsCache: false }, dependencies: [] }`.
     */
    web?: {
        infrastructure?: InfrastructureRequirements;
        dependencies?: PlatformDependency[];
    };
    /**
     * RN renderer overrides. The `platform` field is auto-filled. Defaults:
     * `{ infrastructure: { needsCache: false }, dependencies: [] }`.
     */
    rn?: {
        infrastructure?: InfrastructureRequirements;
        dependencies?: PlatformDependency[];
    };
    /** ExampleDefinition[] from the package's `examples.ts`. */
    examples: ExampleDefinition[];
    /** Canonical engine source snippet used for the AST example, smoke render, and integration test. */
    exampleCode: string;
    /** README markdown for `documentation.readme`. */
    readme: string;
    /** Best practices for `documentation.bestPractices`. */
    bestPractices?: string[];
    /** FAQ for `documentation.faq`. */
    faq?: Array<{
        question: string;
        answer: string;
    }>;
    /**
     * Prefix used to generate the documentation.api function names, e.g. `'D2'`
     * yields `createD2FeatureConfig` / `getD2FeatureOptions` / `D2FeatureOptions` /
     * `D2FeatureConfig`. The factory does NOT define those exports; each feature
     * package still wires them via `makeFeatureConfigHelpers`.
     */
    apiPrefix: string;
    /** Override the auto-generated description of the `engine` interface field. */
    engineFieldDescription?: string;
    /** Override the auto-generated description of the `code` interface field. */
    codeFieldDescription?: string;
    /** Override the auto-generated description of the `meta` interface field. */
    metaFieldDescription?: string;
    /**
     * Whether to also emit a smoke render test on the RN platform. Defaults
     * to `false` (most diagram engines are Web-only or have unsupported RN
     * paths in this build).
     */
    rnRenderTest?: boolean;
    /**
     * Platforms covered by the integration test. Defaults to `['web']`.
     * Engines with a working RN path (e.g. dot) can opt in via `['web', 'rn']`.
     */
    integrationTestPlatforms?: ReadonlyArray<'web' | 'rn'>;
    /**
     * Override the default `testing.coverageRequirements`
     * (`{ statements: 50, branches: 40, functions: 40, lines: 50 }`).
     */
    coverageRequirements?: {
        statements?: number;
        branches?: number;
        functions?: number;
        lines?: number;
    };
}
/**
 * Build a diagram `SupramarkFeature` from per-engine spec, filling in the
 * scaffolding that is identical across all in-tree diagram engines.
 *
 * @example
 *   export const d2Feature = defineDiagramFeature({
 *     id: '@supramark/feature-d2',
 *     engineId: 'd2',
 *     name: 'Diagram (D2)',
 *     description: 'D2 diagrams rendered to SVG ...',
 *     web: { dependencies: [{ name: '@kookyleo/d2-little-web', version: 'workspace:*', type: 'npm', optional: false }] },
 *     rn:  { dependencies: [{ name: 'react-native-svg', version: '^13.0.0', type: 'npm', optional: true }] },
 *     examples: d2Examples,
 *     exampleCode: 'a -> b',
 *     readme: '...',
 *     bestPractices: [...],
 *     faq: [...],
 *     apiPrefix: 'D2',
 *   });
 *   FeatureRegistry.register(d2Feature);
 */
export declare function defineDiagramFeature(spec: DefineDiagramFeatureSpec): SupramarkFeature<SupramarkDiagramNode>;
