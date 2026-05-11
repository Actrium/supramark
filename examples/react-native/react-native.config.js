// Host-level RN autolinking config.
//
// @kookyleo/graphviz-anywhere-rn@0.1.3's postinstall downloads prebuilt
// graphviz native libs from an out-of-date GitHub repo URL, so its
// jniLibs (Android) and ios/Frameworks (iOS) end up empty and the
// per-ABI CMakeLists / podspec FATAL_ERROR at build time. We don't use
// graphviz/dot from this demo, so suppress autolinking until the
// package is bumped to a release with a working postinstall (or pinned
// to the in-tree subtree wrapper).
module.exports = {
  dependencies: {
    '@kookyleo/graphviz-anywhere-rn': {
      platforms: {
        android: null,
        ios: null,
      },
    },
  },
};
