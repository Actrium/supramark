import type { SupramarkDiagramNode, SupramarkNode } from './ast.js';
import type {
  ExampleDefinition,
  InfrastructureRequirements,
  PlatformDependency,
  SupramarkFeature,
} from './feature.js';

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
  faq?: Array<{ question: string; answer: string }>;

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
   * to `false`; features opt in once their RN adapter or JS SVG renderer is
   * available in the test environment.
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
 *     web: { dependencies: [{ name: '@actrium/d2-little-web', version: 'workspace:*', type: 'npm', optional: false }] },
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
export function defineDiagramFeature(
  spec: DefineDiagramFeatureSpec
): SupramarkFeature<SupramarkDiagramNode> {
  const {
    id,
    engineId,
    engineAliases = [],
    name,
    version = '0.1.0',
    description,
    tags = ['diagram', engineId],
    web,
    rn,
    examples,
    exampleCode,
    readme,
    bestPractices,
    faq,
    apiPrefix,
    engineFieldDescription,
    codeFieldDescription,
    metaFieldDescription,
    rnRenderTest = false,
    integrationTestPlatforms = ['web'],
    coverageRequirements,
  } = spec;

  const acceptedEngines = [engineId, ...engineAliases].map(s => s.toLowerCase());

  const selector = (node: SupramarkNode): node is SupramarkDiagramNode => {
    if (node.type !== 'diagram') return false;
    const engine = (node as SupramarkDiagramNode).engine;
    return typeof engine === 'string' && acceptedEngines.includes(engine.toLowerCase());
  };

  const engineFieldDesc =
    engineFieldDescription ??
    (engineAliases.length > 0
      ? `Diagram engine identifier (one of: ${[engineId, ...engineAliases]
          .map(s => `"${s}"`)
          .join(', ')}).`
      : `Diagram engine identifier, expected to be "${engineId}".`);

  const codeFieldDesc =
    codeFieldDescription ?? `Raw ${name} source code from the fenced block.`;

  const metaFieldDesc =
    metaFieldDescription ??
    'Optional metadata passed through to the engine (theme / size / renderer hints).';

  const fence = '```';

  return {
    metadata: {
      id,
      name,
      version,
      author: 'Supramark Team',
      description,
      license: 'Apache-2.0',
      tags: [...tags],
      syntaxFamily: 'fence',
    },

    syntax: {
      ast: {
        type: 'diagram',
        selector,
        interface: {
          required: ['type', 'engine', 'code'],
          optional: ['meta'],
          fields: {
            type: {
              type: 'string',
              description: 'Node type identifier, always "diagram".',
            },
            engine: {
              type: 'string',
              description: engineFieldDesc,
            },
            code: {
              type: 'string',
              description: codeFieldDesc,
            },
            meta: {
              type: 'object',
              description: metaFieldDesc,
            },
          },
        },
        examples: [
          {
            type: 'diagram',
            engine: engineId,
            code: exampleCode,
          } as SupramarkDiagramNode,
        ],
      },
    },

    renderers: {
      web: {
        platform: 'web',
        infrastructure: web?.infrastructure ?? { needsCache: false },
        dependencies: web?.dependencies ?? [],
      },
      rn: {
        platform: 'rn',
        infrastructure: rn?.infrastructure ?? { needsCache: false },
        dependencies: rn?.dependencies ?? [],
      },
    },

    examples,

    testing: {
      syntaxTests: {
        cases: [
          {
            name: `Parse a ${fence}${engineId} fence into a diagram node`,
            input: [`${fence}${engineId}`, exampleCode, fence].join('\n'),
            expected: {
              type: 'diagram',
              engine: engineId,
            } as unknown as SupramarkDiagramNode,
            options: {
              typeOnly: true,
            },
          },
        ],
      },
      renderTests: {
        web: [
          {
            name: `Web ${engineId} render (smoke: output exists)`,
            input: {
              type: 'diagram',
              engine: engineId,
              code: exampleCode,
            } as SupramarkDiagramNode,
            expected: (output: unknown) => output !== null && output !== undefined,
            snapshot: false,
          },
        ],
        ...(rnRenderTest
          ? {
              rn: [
                {
                  name: `RN ${engineId} render (smoke: output exists)`,
                  input: {
                    type: 'diagram',
                    engine: engineId,
                    code: exampleCode,
                  } as SupramarkDiagramNode,
                  expected: (output: unknown) => output !== null && output !== undefined,
                  snapshot: false,
                },
              ],
            }
          : {}),
      },
      integrationTests: {
        cases: [
          {
            name: `End-to-end: a markdown doc containing a ${fence}${engineId} fence`,
            input: [`# ${name} demo`, '', `${fence}${engineId}`, exampleCode, fence].join('\n'),
            validate: (result: unknown) => {
              if (!result || typeof result !== 'object') return false;
              const root = result as { children?: unknown };
              const children = Array.isArray(root.children) ? root.children : [];
              return children.some(n => {
                if (!n || typeof n !== 'object') return false;
                const node = n as { type?: unknown; engine?: unknown };
                return (
                  node.type === 'diagram' &&
                  typeof node.engine === 'string' &&
                  acceptedEngines.includes(node.engine.toLowerCase())
                );
              });
            },
            platforms: [...integrationTestPlatforms],
          },
        ],
      },
      coverageRequirements: {
        statements: coverageRequirements?.statements ?? 50,
        branches: coverageRequirements?.branches ?? 40,
        functions: coverageRequirements?.functions ?? 40,
        lines: coverageRequirements?.lines ?? 50,
      },
    },

    documentation: {
      readme,
      api: {
        interfaces: [
          {
            name: `${apiPrefix}FeatureOptions`,
            description: `${name} feature options (currently empty; reserved).`,
            fields: [],
          },
        ],
        functions: [
          {
            name: `create${apiPrefix}FeatureConfig`,
            description: `Create a feature config entry for the ${name} feature.`,
            parameters: [
              {
                name: 'enabled',
                type: 'boolean',
                description: 'Enable / disable the feature.',
                optional: true,
              },
              {
                name: 'options',
                type: `${apiPrefix}FeatureOptions`,
                description: 'Optional feature options.',
                optional: true,
              },
            ],
            returns: `${apiPrefix}FeatureConfig`,
          },
          {
            name: `get${apiPrefix}FeatureOptions`,
            description: `Read this feature's options from the global SupramarkConfig.`,
            parameters: [
              {
                name: 'config',
                type: 'SupramarkConfig | undefined',
                description: 'Global supramark config.',
                optional: true,
              },
            ],
            returns: `${apiPrefix}FeatureOptions | undefined`,
          },
        ],
        types: [],
      },
      bestPractices: bestPractices ?? [],
      faq: faq ?? [],
    },
  };
}
