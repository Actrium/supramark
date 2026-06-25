import type { SupramarkNode, SupramarkParentNode, SupramarkRootNode } from './ast.js';
import { type SupramarkConfig } from './feature.js';
import { loadRustMarkdownModule } from './plugin-loader.js';

/**
 * 插件解析上下文，提供给插件访问原始数据和共享状态。
 */
export interface SupramarkParseContext {
  /** 原始 markdown 文本。 */
  source: string;

  /** 插件共享数据存储，用于插件间通信。 */
  data: Record<string, unknown>;
}

/**
 * AST 后处理插件。
 *
 * AST v2 的 canonical parse 由 Rust `supramark-markdown` 完成；TS 插件只允许在
 * 解析完成后做结构化转换，不再参与 Markdown tokenization。
 */
export interface SupramarkPlugin {
  /** 插件名称，必须唯一。 */
  name: string;

  /** 插件版本（可选）。 */
  version?: string;

  /** 插件依赖列表（可选）。 */
  dependencies?: string[];

  /** 解析后的 AST 转换钩子。 */
  transform?(root: SupramarkRootNode, context: SupramarkParseContext): void | Promise<void>;
}

/**
 * Markdown 解析选项。
 */
export interface SupramarkParseOptions {
  /** AST 后处理插件列表。 */
  plugins?: SupramarkPlugin[];

  /**
   * Feature 运行时配置（可选）。
   *
   * Rust parser 是 AST v2 的唯一入口。配置裁剪由 `features.manifest.json` 与构建脚本
   * 在打包期完成；运行时字段保留给宿主传递 feature 语义。
   */
  config?: SupramarkConfig;
}

/**
 * 解析 Markdown 为 Supramark AST v2。
 */
export async function parse(
  source: string,
  options: SupramarkParseOptions = {}
): Promise<SupramarkRootNode> {
  const root = await parseWithRustMarkdown(source);
  await expandOpaqueContainers(root);
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
export async function expandOpaqueContainers(node: SupramarkNode): Promise<void> {
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
      const sub = await parseWithRustMarkdown(child.value);
      child.children = sub.children;
      child.value = undefined;
    }
    await expandOpaqueContainers(child);
  }
}

async function parseWithRustMarkdown(source: string): Promise<SupramarkRootNode> {
  const mod = await loadRustMarkdownModule();
  if (typeof mod.parse === 'function') {
    return mod.parse(source) as SupramarkRootNode;
  }
  if (typeof mod.parseJson === 'function') {
    return JSON.parse(await mod.parseJson(source)) as SupramarkRootNode;
  }

  throw new Error('supramark-markdown module does not expose parse(source) or parseJson(source).');
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
 * 对插件进行拓扑排序，确保依赖的插件先执行。
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
 * Supramark 预设类型。
 */
export type SupramarkPreset = () => SupramarkParseOptions;

/**
 * 默认预设。GFM 基础能力由 `supramark-markdown` 默认启用。
 */
export function presetDefault(): SupramarkParseOptions {
  return {
    plugins: [],
  };
}

/**
 * GFM 预设。保留为语义化入口，实际能力由 AST v2 parser 提供。
 */
export function presetGFM(): SupramarkParseOptions {
  return {
    plugins: [],
  };
}
