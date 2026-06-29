const { getDefaultConfig } = require('expo/metro-config');
const path = require('path');

const config = getDefaultConfig(__dirname);

// 配置 Metro 解析 monorepo 中的包
const projectRoot = __dirname;
const workspaceRoot = path.resolve(projectRoot, '../..');

// 配置 watchFolders 以包含 monorepo 根目录
config.watchFolders = [workspaceRoot];

// 配置 nodeModulesPath
config.resolver.nodeModulesPaths = [
  path.resolve(projectRoot, 'node_modules'),
  path.resolve(workspaceRoot, 'node_modules'),
];

// 获取原始的 resolveRequest
const defaultResolver = config.resolver.resolveRequest;

// 配置模块解析,处理 package.json exports 和 imports 字段
config.resolver.resolveRequest = (context, moduleName, platform) => {
  // RN must not load the wasm web parser. plugin-loader.ts re-exports the web
  // loader whose `await import(specifier)` wasm fallback Metro cannot bundle;
  // route every plugin-loader / plugin-loader-web request to the native-adapter
  // loader so parse() goes through the linked libsupramark_markdown_native.
  if (
    platform !== 'web' &&
    /(^|\/)plugin-loader(-web)?(\.(js|ts))?$/.test(moduleName)
  ) {
    return {
      filePath: path.resolve(workspaceRoot, 'packages/core/src/plugin-loader-rn.ts'),
      type: 'sourceFile',
    };
  }

  // workspace 内多个 TS 源文件用 Node-ESM 风格的 `./foo.js` 导入兄弟 `.ts`。
  // Metro 默认不会把 `.js` 重映射到 `.ts` —— 我们对仅相对路径 + .js 后缀的
  // 失败 case 退一步尝试同名 .ts / .tsx，保持源码端的 ESM 风格不变。
  if (
    (moduleName.startsWith('./') || moduleName.startsWith('../')) &&
    moduleName.endsWith('.js')
  ) {
    const stripped = moduleName.slice(0, -3);
    try {
      return context.resolveRequest(context, stripped, platform);
    } catch {
      // fall through to other resolvers
    }
  }

  // RN 不加载 wasm web 包。D2 / Mermaid / PlantUML 走 native FFI adapter；
  // ECharts / Vega-Lite 走纯 JS SVG-string engine。@supramark/engines/src/*
  // 仍静态引用了部分 *-web 包名，Metro 不会跳过未调用的 `await import(...)`，
  // 所以仅把这些 wasm/web 入口短路到空 stub。
  if (/^@(kookyleo|actrium)\/(d2|mermaid|plantuml)-little-web$|^@(kookyleo|actrium)\/graphviz-anywhere-web$/.test(moduleName)) {
    return {
      filePath: path.resolve(projectRoot, 'stubs/empty.js'),
      type: 'sourceFile',
    };
  }

  // 处理 @supramark/core 包的 react-native 入口
  // Metro 不支持 package.json 的 exports 条件导出,需要手动指定 RN 入口。
  // 早期版本指向 dist/index.rn.js（已不存在；core 现在源码直出），改用源
  // 文件直供 Metro，避免依赖一次额外的 tsc --emit 步骤。
  if (moduleName === '@supramark/core') {
    return {
      filePath: path.resolve(workspaceRoot, 'packages/core/src/index.rn.ts'),
      type: 'sourceFile',
    };
  }

  // @supramark/engines 的 ./rn subpath — Metro 同样不支持 package.json exports
  // 的 subpath；手动映射到 source 文件。
  // @supramark/core/rn subpath — Metro ignores package.json exports, so map it
  // to the same RN entry as the bare specifier (index.rn.ts re-exports the
  // native parser registry @supramark/markdown-native-rn registers into).
  if (moduleName === '@supramark/core/rn') {
    return {
      filePath: path.resolve(workspaceRoot, 'packages/core/src/index.rn.ts'),
      type: 'sourceFile',
    };
  }

  if (moduleName === '@supramark/engines/rn') {
    return {
      filePath: path.resolve(workspaceRoot, 'packages/engines/src/rn.ts'),
      type: 'sourceFile',
    };
  }

  // 处理 devlop 包的 exports 字段
  if (moduleName === 'devlop') {
    return {
      filePath: path.resolve(
        workspaceRoot,
        'node_modules/devlop/lib/default.js'
      ),
      type: 'sourceFile',
    };
  }

  // 处理 vfile 包的 subpath imports (以 # 开头)
  if (moduleName === '#minpath') {
    return {
      filePath: path.resolve(
        workspaceRoot,
        'node_modules/vfile/lib/minpath.browser.js'
      ),
      type: 'sourceFile',
    };
  }

  if (moduleName === '#minproc') {
    return {
      filePath: path.resolve(
        workspaceRoot,
        'node_modules/vfile/lib/minproc.browser.js'
      ),
      type: 'sourceFile',
    };
  }

  if (moduleName === '#minurl') {
    return {
      filePath: path.resolve(
        workspaceRoot,
        'node_modules/vfile/lib/minurl.browser.js'
      ),
      type: 'sourceFile',
    };
  }

  // 处理 unist-util-visit-parents 的 subpath exports
  if (moduleName === 'unist-util-visit-parents/do-not-use-color') {
    return {
      filePath: path.resolve(
        workspaceRoot,
        'node_modules/unist-util-visit-parents/lib/color.js'
      ),
      type: 'sourceFile',
    };
  }

  // 使用默认解析器
  if (defaultResolver) {
    return defaultResolver(context, moduleName, platform);
  }
  return context.resolveRequest(context, moduleName, platform);
};

module.exports = config;
