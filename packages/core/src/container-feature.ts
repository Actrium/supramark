/**
 * Unified Container Feature interface.
 *
 * Defines a lean, practical interface specification for Features that implement a
 * :::xxx container type. Consolidates definitions that used to be spread across
 * feature.ts / extension.ts / syntax.ts.
 *
 * ## Design principles
 * - Every field has a clear consumer
 * - No redundancy, no filler
 * - containerNames must be globally unique, checked by feature:lint
 *
 * @packageDocumentation
 */

import type { ExampleDefinition, SupramarkConfig } from './feature.js';
import type { SupramarkContainerNode, SupramarkNode } from './ast.js';

// ============================================================================
// ContainerFeature interface
// ============================================================================

/**
 * The unified interface for a container-type Feature.
 *
 * Every :::xxx container Feature must implement this interface.
 *
 * @example
 * ```typescript
 * export const admonitionFeature: ContainerFeature = {
 *   id: '@supramark/feature-admonition',
 *   name: 'Admonition',
 *   version: '0.1.0',
 *   description: 'Callout container (note/tip/warning, etc.)',
 *   containerNames: ['note', 'tip', 'info', 'warning', 'danger'],
 *   registerParser: () => { ... },
 *   webRendererExport: 'renderAdmonitionContainerWeb',
 *   rnRendererExport: 'renderAdmonitionContainerRN',
 * };
 * ```
 */
export interface ContainerFeature {
  // ============================================================================
  // Metadata (required)
  // ============================================================================

  /**
   * Unique Feature identifier.
   *
   * Format: @scope/feature-name
   * Example: @supramark/feature-admonition
   *
   * Consumers: feature:lint, FeatureRegistry, the configuration system
   */
  id: string;

  /**
   * Feature display name.
   *
   * Example: 'Admonition', 'Weather'
   *
   * Consumers: documentation generation, UI display
   */
  name: string;

  /**
   * Version number (semantic versioning).
   *
   * Format: x.y.z
   * Example: '0.1.0', '1.0.0'
   *
   * Consumers: version checks, documentation
   */
  version: string;

  /**
   * Short description (optional).
   *
   * Consumers: documentation generation, package.json description
   */
  description?: string;

  // ============================================================================
  // Container definition (required)
  // ============================================================================

  /**
   * The list of supported :::xxx container names.
   *
   * Example: ['note', 'tip', 'info', 'warning', 'danger']
   *
   * **Important**: these names must be globally unique and must not conflict with
   * other Features. feature:lint checks global uniqueness.
   *
   * Consumers: parser registration, feature:lint uniqueness checks
   */
  containerNames: string[];

  // ============================================================================
  // Parser registration (required)
  // ============================================================================

  /**
   * The function that registers the parser.
   *
   * Calling this function registers the parsing hooks for all containerNames.
   * Usually calls registerContainerHook() internally.
   *
   * Consumers: the generated registry file
   */
  registerParser: () => void;

  // ============================================================================
  // Renderer exports (optional, for registry generation)
  // ============================================================================

  /**
   * The export name of the Web render function.
   *
   * Example: 'renderAdmonitionContainerWeb'
   *
   * Consumers: feature-sync.ts, generating the web registry
   */
  webRendererExport?: string;

  /**
   * The export name of the React Native render function.
   *
   * Example: 'renderAdmonitionContainerRN'
   *
   * Consumers: feature-sync.ts, generating the rn registry
   */
  rnRendererExport?: string;
}

// ============================================================================
// Renderer interfaces
// ============================================================================

/**
 * Arguments for a Container Web render function.
 */
export interface ContainerWebRenderArgs {
  /** The AST node */
  node: SupramarkContainerNode;
  /** React key */
  key: number;
  /** CSS class name mapping */
  classNames: Record<string, string>;
  /** Supramark configuration */
  config?: SupramarkConfig;
  /** Function for rendering child nodes */
  renderChildren: (children: SupramarkNode[]) => unknown;
}

/**
 * The Container Web render function type.
 *
 * Every runtime.web.tsx render function must match this signature.
 */
export type ContainerWebRenderer = (args: ContainerWebRenderArgs) => unknown;

/**
 * Video tap event delivered to a host-supplied onVideoPress handler.
 *
 * Mirrors {@link SupramarkImagePressEvent} in shape but without gallery
 * merging — videos render as standalone cards.
 */
export interface SupramarkVideoPressEvent {
  /** The video source URL. */
  src: string;
  /** Poster image URL, if configured. */
  poster?: string;
  /** Title, if configured. */
  title?: string;
}

/**
 * Arguments for a Container RN render function.
 */
export interface ContainerRNRenderArgs {
  /** The AST node */
  node: SupramarkContainerNode;
  /** React key */
  key: number;
  /** RN style mapping */
  styles: Record<string, unknown>;
  /** Supramark configuration */
  config?: SupramarkConfig;
  /**
   * Host handler for video card taps, threaded from the root
   * `<Supramark onVideoPress={...}>` prop through the renderNode chain.
   */
  onVideoPress?: (event: SupramarkVideoPressEvent) => void;
  /** Function for rendering child nodes */
  renderChildren: (children: SupramarkNode[]) => unknown;
}

/**
 * The Container RN render function type.
 *
 * Every runtime.rn.tsx render function must match this signature.
 */
export type ContainerRNRenderer = (args: ContainerRNRenderArgs) => unknown;

// ============================================================================
// Examples interface (re-exported)
// ============================================================================

/**
 * Example definition.
 *
 * Every examples.ts must export an array of type ExampleDefinition[].
 *
 * Re-exported from feature.ts for compatibility.
 */
export type { ExampleDefinition };

// ============================================================================
// Validation function
// ============================================================================

/**
 * Validate the completeness of a ContainerFeature implementation.
 *
 * @param feature - the Feature definition
 * @returns the validation result
 */
export function validateContainerFeature(feature: Partial<ContainerFeature>): {
  valid: boolean;
  errors: Array<{ code: string; message: string }>;
} {
  const errors: Array<{ code: string; message: string }> = [];

  // Check required fields
  if (!feature.id) {
    errors.push({ code: 'id-required', message: 'Feature must have an id' });
  } else if (!/^@[\w-]+\/feature-[\w-]+$/.test(feature.id)) {
    errors.push({
      code: 'id-format',
      message: 'Feature id must match @scope/feature-name format',
    });
  }

  if (!feature.name || feature.name.trim().length === 0) {
    errors.push({ code: 'name-required', message: 'Feature must have a name' });
  }

  if (!feature.version) {
    errors.push({ code: 'version-required', message: 'Feature must have a version' });
  } else if (!/^\d+\.\d+\.\d+$/.test(feature.version)) {
    errors.push({
      code: 'version-format',
      message: 'Feature version must be semver format (x.y.z)',
    });
  }

  if (!feature.containerNames || feature.containerNames.length === 0) {
    errors.push({
      code: 'containerNames-required',
      message: 'Feature must define at least one containerName',
    });
  }

  if (typeof feature.registerParser !== 'function') {
    errors.push({
      code: 'registerParser-required',
      message: 'Feature must have a registerParser function',
    });
  }

  return {
    valid: errors.length === 0,
    errors,
  };
}
