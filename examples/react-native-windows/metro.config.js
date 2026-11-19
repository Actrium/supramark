const path = require('node:path');
const fs = require('node:fs');
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');

const workspace = path.resolve(__dirname, '../..');
const defaults = getDefaultConfig(__dirname);
module.exports = mergeConfig(defaults, {
  watchFolders: [workspace],
  resolver: {
    // The demo and the library must share one React and one native renderer.
    disableHierarchicalLookup: true,
    nodeModulesPaths: [path.join(__dirname, 'node_modules'), path.join(workspace, 'node_modules')],
    resolveRequest(context, name, platform) {
      if (name === '@supramark/core' || name === '@supramark/core/rn') {
        return {
          type: 'sourceFile',
          filePath: path.join(workspace, 'packages/core/src/index.rn.ts'),
        };
      }
      if (
        name === './plugin-loader.js' &&
        context.originModulePath.startsWith(path.join(workspace, 'packages/core/src') + path.sep)
      ) {
        return {
          type: 'sourceFile',
          filePath: path.join(workspace, 'packages/core/src/plugin-loader-rn.ts'),
        };
      }
      if (/^@actrium\/(graphviz-anywhere|(?:d2|mermaid|plantuml)-little)-web$/.test(name)) {
        return { type: 'empty' };
      }
      if (name.startsWith('.') && name.endsWith('.js')) {
        const source = path.resolve(path.dirname(context.originModulePath), name.slice(0, -3));
        const filePath = [source + '.ts', source + '.tsx'].find(fs.existsSync);
        if (filePath) return { type: 'sourceFile', filePath };
      }
      const resolve = defaults.resolver.resolveRequest ?? context.resolveRequest;
      return resolve(context, name, platform);
    },
    blockList: [/[/\\]windows[/\\](?:x64|ARM64|Generated Files)[/\\]/, /\.ProjectImports\.zip$/],
  },
});
