/**
 * Rust markdown module 加载器入口（中转文件）。
 *
 * 实际实现拆成两个平台文件：
 *   - `plugin-loader-web.ts` —— Web / Node，import wasm
 *   - `plugin-loader-rn.ts` —— RN，走 native FFI
 *
 * `plugin.ts` 统一从这里 import `loadRustMarkdownModule`，metro 通过
 * 宿主 `metro.config.js` 的 sourceMap 把 `./plugin-loader.js` 解析到
 * 对应平台实现。这样 RN bundle 里根本不出现 `@supramark/markdown-web`
 * 的 import，避免 lazy bundle 与静态 require 冲突。
 *
 * 默认 re-export web 实现（供 Node / Bun / Web 直接使用，不依赖 metro 配置）。
 * RN 宿主必须在 metro.config.js 显式映射 `'./plugin-loader.js'` 或
 * `'@supramark/core/plugin-loader'` 到 `plugin-loader-rn.ts`。
 */
export { loadRustMarkdownModule } from './plugin-loader-web.js';
