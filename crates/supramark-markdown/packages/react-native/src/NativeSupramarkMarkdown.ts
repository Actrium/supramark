/**
 * TurboModule spec for SupramarkMarkdownNative (new React Native architecture).
 *
 * Codegen picks this up to emit Objective-C / Java bindings. The
 * `import * as TurboModuleRegistry` form keeps the file harmless when
 * codegen isn't run (e.g. old-arch fallback in `index.ts`).
 */
import type { TurboModule } from 'react-native';
import { TurboModuleRegistry } from 'react-native';

export interface Spec extends TurboModule {
  /** Markdown source → AST v2 JSON string. Rejects on parse / FFI error. */
  parseJson(source: string): Promise<string>;
  /** Static version string of the linked libsupramark_markdown_native. */
  getVersion(): Promise<string>;
}

export default TurboModuleRegistry.getEnforcing<Spec>('SupramarkMarkdownNative');
