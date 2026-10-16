/**
 * The Rust markdown module loader for Web / Node.
 *
 * Loads the Rust parser via the wasm-bindgen artifact `@supramark/markdown-web`.
 * This file is referenced only by the web entry point of `@supramark/core`
 * (`index.ts`); the RN entry point (`index.rn.ts`) goes through
 * `plugin-loader-rn.ts` and never imports this file, so Metro never scans
 * `@supramark/markdown-web` while bundling the RN bundle.
 */

type RustMarkdownModule = {
  parse?: (source: string) => unknown;
  parseJson?: (source: string) => string | Promise<string>;
  parseWithOptions?: (source: string, options: { wikilink?: boolean }) => unknown;
  parseJsonWithOptions?: (source: string, options: { wikilink?: boolean }) => string | Promise<string>;
};

// Node package subpath: only attempted at server runtime; must not be statically
// resolved by a browser bundler.
const MARKDOWN_WEB_NODE_PACKAGE = '@supramark/markdown-web/node';
// Node fallback artifact path: only attempted at runtime if the package subpath fails to load.
const MARKDOWN_WEB_NODE_DIST = '../../../crates/supramark-markdown/packages/web/dist/node.js';
// Browser fallback artifact path: only attempted at runtime if the package main fails to load.
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

  // Server runtime candidates: each candidate is written as a static string literal
  // so Metro's static analysis routes it through resolveRequest (which the host's
  // metro.config.js can stub).
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

  // Fallback candidate: a direct path (Web/Node).
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
 * Load the dist fallback at runtime, so TypeScript doesn't try to resolve the build
 * artifact path as a source dependency.
 */
async function importRustMarkdownModule(specifier: string): Promise<RustMarkdownModule> {
  return (await import(/* @vite-ignore */ specifier)) as RustMarkdownModule;
}
