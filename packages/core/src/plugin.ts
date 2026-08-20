import type { SupramarkNode, SupramarkParentNode, SupramarkRootNode } from './ast.js';
import { type SupramarkConfig, isFeatureEnabled } from './feature.js';
import { loadRustMarkdownModule } from './plugin-loader.js';

/**
 * Plugin parse context, giving plugins access to the raw data and shared state.
 */
export interface SupramarkParseContext {
  /** The raw markdown text. */
  source: string;

  /** Shared data store for plugins, used for inter-plugin communication. */
  data: Record<string, unknown>;
}

/**
 * An AST post-processing plugin.
 *
 * The canonical parse for AST v2 is done by the Rust `supramark-markdown` crate; TS
 * plugins are only allowed to perform structural transforms after parsing completes
 * and no longer participate in Markdown tokenization.
 */
export interface SupramarkPlugin {
  /** The plugin name, must be unique. */
  name: string;

  /** The plugin version (optional). */
  version?: string;

  /** The plugin's dependency list (optional). */
  dependencies?: string[];

  /** The post-parse AST transform hook. */
  transform?(root: SupramarkRootNode, context: SupramarkParseContext): void | Promise<void>;
}

/**
 * Markdown parse options.
 */
export interface SupramarkParseOptions {
  /** The list of AST post-processing plugins. */
  plugins?: SupramarkPlugin[];

  /**
   * Feature runtime configuration (optional).
   *
   * The Rust parser is the sole entry point for AST v2. Configuration-driven
   * trimming is handled by `features.manifest.json` and the build scripts at bundle
   * time; this runtime field is kept so hosts can still pass feature semantics
   * through.
   */
  config?: SupramarkConfig;

  /**
   * Enable WikiLink parsing (`[[target]]`, `[[target|label]]`, `[[target#section]]`).
   *
   * Off by default: `[[...]]` is not CommonMark/GFM syntax, so the default parse
   * is byte-identical to the CommonMark/GFM profiles. The flag turns on when
   * `@supramark/feature-wikilink` is explicitly present and enabled in
   * `config.features` (so enabling the feature is enough); an explicit
   * `wikilink` value always wins.
   */
  wikilink?: boolean;
}

/** Parser flags forwarded to the Rust `parse_with_options` entry point. */
export interface RustMarkdownParserOptions {
  wikilink?: boolean;
}

/**
 * Parse Markdown into Supramark AST v2.
 */
export async function parse(
  source: string,
  options: SupramarkParseOptions = {}
): Promise<SupramarkRootNode> {
  const wikilink =
    options.wikilink ??
    (options.config ? isFeatureEnabled(options.config, '@supramark/feature-wikilink') : false);
  const parserOptions: RustMarkdownParserOptions | undefined = wikilink
    ? { wikilink: true }
    : undefined;
  const root = await parseWithRustMarkdown(source, parserOptions);
  await expandOpaqueContainers(root, parserOptions);
  await applyPlugins(root, source, options.plugins ?? []);
  return root;
}

/**
 * Expand "transparent" containers: re-parse the body of an opaque container
 * (which the native parser leaves as a raw markdown string on `value`) into an
 * AST subtree and put it back on `children`.
 *
 * Background: in AST v2 all container scanning happens in the Rust parser, so
 * every `:::name` container is emitted as `mode: 'opaque'` — body on `value`,
 * `children` empty. The names the parser recognises (map / vison / html /
 * weather) also carry structured `data` (their body is YAML / HTML / JSON, not
 * markdown, and must be left untouched). Every other container (note and the
 * other admonitions, plus custom containers) has no `data`; its body is
 * markdown and must be expanded here, otherwise renderers that read `children`
 * silently drop the body.
 *
 * Discriminator: `mode === 'opaque'` and `data` empty and `value` non-empty →
 * a transparent container, expand it. A genuinely-opaque container (one that
 * carries `data`, e.g. map) is left exactly as-is — never re-parsed, never has
 * its `value` cleared.
 *
 * Idempotent: an already-expanded container has its `value` cleared, so a
 * second pass only walks the tree without re-parsing. This single entry point
 * lives in `parse()` so Web / RN / Node share it and renderers need no copy.
 */
export async function expandOpaqueContainers(
  node: SupramarkNode,
  parserOptions?: RustMarkdownParserOptions
): Promise<void> {
  const children = (node as Partial<SupramarkParentNode>).children;
  if (!Array.isArray(children)) {
    return;
  }
  for (const child of children) {
    if (
      child.type === 'container' &&
      child.mode === 'opaque' &&
      child.data == null &&
      typeof child.value === 'string' &&
      child.value.length > 0
    ) {
      const sub = await parseWithRustMarkdown(child.value, parserOptions);
      child.children = sub.children;
      child.value = undefined;
    }
    await expandOpaqueContainers(child, parserOptions);
  }
}

async function parseWithRustMarkdown(
  source: string,
  parserOptions?: RustMarkdownParserOptions
): Promise<SupramarkRootNode> {
  const mod = await loadRustMarkdownModule();
  if (parserOptions === undefined) {
    if (typeof mod.parse === 'function') {
      return mod.parse(source) as SupramarkRootNode;
    }
    if (typeof mod.parseJson === 'function') {
      return JSON.parse(await mod.parseJson(source)) as SupramarkRootNode;
    }
    throw new Error('supramark-markdown module does not expose parse(source) or parseJson(source).');
  }

  if (typeof mod.parseWithOptions === 'function') {
    return mod.parseWithOptions(source, parserOptions) as SupramarkRootNode;
  }
  if (typeof mod.parseJsonWithOptions === 'function') {
    return JSON.parse(await mod.parseJsonWithOptions(source, parserOptions)) as SupramarkRootNode;
  }

  // Not silently falling back: the caller asked for parser flags (e.g.
  // wikilink) that the loaded parser module cannot honor, and a default
  // parse would quietly drop the syntax.
  throw new Error(
    'supramark-markdown module does not expose parseWithOptions/parseJsonWithOptions; ' +
      'parser options such as wikilink require a newer @supramark/markdown-web build ' +
      '(or, on React Native, a native adapter with parseJsonWithOptions).'
  );
}

async function applyPlugins(
  root: SupramarkRootNode,
  source: string,
  plugins: SupramarkPlugin[]
): Promise<void> {
  if (plugins.length === 0) {
    return;
  }

  const context: SupramarkParseContext = {
    source,
    data: {},
  };

  for (const plugin of sortPluginsByDependencies(plugins)) {
    await plugin.transform?.(root, context);
  }
}

/**
 * Topologically sort plugins so that dependencies run before the plugins that
 * depend on them.
 */
function sortPluginsByDependencies(plugins: SupramarkPlugin[]): SupramarkPlugin[] {
  const pluginMap = new Map<string, SupramarkPlugin>();
  const visited = new Set<string>();
  const visiting = new Set<string>();
  const sorted: SupramarkPlugin[] = [];

  for (const plugin of plugins) {
    if (pluginMap.has(plugin.name)) {
      throw new Error(`Duplicate plugin name: ${plugin.name}`);
    }
    pluginMap.set(plugin.name, plugin);
  }

  function visit(pluginName: string, plugin: SupramarkPlugin) {
    if (visited.has(pluginName)) {
      return;
    }
    if (visiting.has(pluginName)) {
      throw new Error(`Circular dependency detected: ${pluginName}`);
    }

    visiting.add(pluginName);
    for (const dependencyName of plugin.dependencies ?? []) {
      const dependency = pluginMap.get(dependencyName);
      if (!dependency) {
        throw new Error(
          `Plugin "${pluginName}" depends on "${dependencyName}", but "${dependencyName}" is not provided`
        );
      }
      visit(dependencyName, dependency);
    }
    visiting.delete(pluginName);
    visited.add(pluginName);
    sorted.push(plugin);
  }

  for (const plugin of plugins) {
    visit(plugin.name, plugin);
  }

  return sorted;
}

/**
 * The Supramark preset type.
 */
export type SupramarkPreset = () => SupramarkParseOptions;

/**
 * The default preset. Base GFM capability is enabled by default by
 * `supramark-markdown`.
 */
export function presetDefault(): SupramarkParseOptions {
  return {
    plugins: [],
  };
}

/**
 * The GFM preset. Kept as a semantic entry point; the actual capability is provided
 * by the AST v2 parser.
 */
export function presetGFM(): SupramarkParseOptions {
  return {
    plugins: [],
  };
}
