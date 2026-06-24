/*
 * Metro configuration for the Graphviz Anywhere example app.
 *
 * Configures Metro to resolve the local React Native package
 * from the parent directory so changes are picked up without publishing.
 *
 * Licensed under the Apache License, Version 2.0
 */

const path = require('path');
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');

const libraryRoot = path.resolve(__dirname, '../../packages/react-native');

const config = {
  watchFolders: [libraryRoot],
  resolver: {
    extraNodeModules: {
      '@actrium/graphviz-anywhere-rn': libraryRoot,
    },
    // Ensure the example app's node_modules take priority
    nodeModulesPaths: [
      path.resolve(__dirname, 'node_modules'),
    ],
  },
};

module.exports = mergeConfig(getDefaultConfig(__dirname), config);
