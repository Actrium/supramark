// Host-level RN autolinking config.
//
// @actrium/graphviz-anywhere-rn@0.1.3's postinstall downloads prebuilt
// graphviz native libs from an out-of-date GitHub repo URL, so its
// jniLibs (Android) and ios/Frameworks (iOS) end up empty and the
// per-ABI CMakeLists / podspec FATAL_ERROR at build time. We don't use
// graphviz/dot from this demo, so suppress autolinking until the
// package is bumped to a release with a working postinstall (or pinned
// to the in-tree subtree wrapper).
const selectionE2E =
  process.env.SUPRAMARK_RN_E2E === 'selection' ||
  process.env.EXPO_PUBLIC_SUPRAMARK_RN_E2E === 'selection';

const disabledDependency = {
  platforms: {
    android: null,
    ios: null,
  },
};

const dependencies = {
  '@actrium/graphviz-anywhere-rn': disabledDependency,
};

if (selectionE2E) {
  Object.assign(dependencies, {
    '@actrium/supramark-d2-native-rn': disabledDependency,
    '@actrium/supramark-mermaid-native-rn': disabledDependency,
    '@actrium/supramark-plantuml-native-rn': disabledDependency,
    '@supramark/markdown-native-rn': disabledDependency,
  });
}

module.exports = {
  dependencies,
};
