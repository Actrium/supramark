/**
 * Web / Node 的 Rust markdown module 加载器。
 *
 * 通过 wasm-bindgen 产物 `@supramark/markdown-web` 加载 Rust parser。
 * 此文件只被 `@supramark/core` 的 web 入口（`index.ts`）引用，
 * RN 入口（`index.rn.ts`）走 `plugin-loader-rn.ts`，完全不 import 此文件，
 * 因此 metro 打包 RN bundle 时不会扫描到 `@supramark/markdown-web`。
 */

type RustMarkdownModule = {
  parse?: (source: string) => unknown;
  parseJson?: (source: string) => string | Promise<string>;
};

// Node package subpath：只在服务端运行时尝试，不能让浏览器 bundler 静态解析。
const MARKDOWN_WEB_NODE_PACKAGE = '@supramark/markdown-web/node';
// Node fallback 产物路径：只在 package subpath 加载失败时运行时尝试。
const MARKDOWN_WEB_NODE_DIST = '../../../crates/supramark-markdown/packages/web/dist/node.js';
// Browser fallback 产物路径：只在 package main 加载失败时运行时尝试。
const MARKDOWN_WEB_BROWSER_DIST = '../../../crates/supramark-markdown/packages/web/dist/index.js';

type RuntimeGlobal = typeof globalThis & {
  Bun?: unknown;
  process?: {
    versions?: { node?: string };
    env?: Record<string, string | undefined>;
    cwd?: () => string;
  };
};

export async function loadRustMarkdownModule(): Promise<RustMarkdownModule> {
  const errors: unknown[] = [];

  if (!isServerRuntime()) {
    try {
      return (await import('@supramark/markdown-web')) as RustMarkdownModule;
    } catch (error) {
      errors.push(error);
    }
  }

  // Server runtime 候选：每个候选都写成静态字符串字面量，让 Metro 在
  // 静态分析时走 resolveRequest（宿主 metro.config.js 可 stub）。
  if (isServerRuntime()) {
    try {
      return await importRustMarkdownModule(MARKDOWN_WEB_NODE_PACKAGE);
    } catch (error) {
      errors.push(error);
    }
    try {
      return await importRustMarkdownModule(MARKDOWN_WEB_NODE_DIST);
    } catch (error) {
      errors.push(error);
    }
  }

  // 兜底候选：直接路径（Web/Node）。
  try {
    return await importRustMarkdownModule(MARKDOWN_WEB_BROWSER_DIST);
  } catch (error) {
    errors.push(error);
  }

  throw new Error(
    `Unable to load supramark-markdown parser. Build @supramark/markdown-web first. Tried ${errors.length} module candidates.`
  );
}

function isServerRuntime(): boolean {
  const runtime = globalThis as RuntimeGlobal;
  return runtime.Bun !== undefined || Boolean(runtime.process?.versions?.node);
}

/**
 * 运行时加载 dist fallback，避免 TypeScript 把构建产物路径当成源码依赖解析。
 */
async function importRustMarkdownModule(specifier: string): Promise<RustMarkdownModule> {
  return (await import(/* @vite-ignore */ specifier)) as RustMarkdownModule;
}
