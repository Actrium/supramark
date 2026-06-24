# @actrium/graphviz-anywhere-react-native

<!-- TODO (follow-up PR): migrate to the shared C++ CGraphviz wrapper
     introduced in `packages/web/src-cpp/main.cpp`. Plan: expose the same
     typed `layout(dot, format, engine)` method through JSI / TurboModule
     instead of the current C-ABI bridge, so RN and Web share one Graphviz
     core. Tracked alongside the rearch introduced in the
     `rearch/hpcc-js-aligned` branch. -->

React Native bindings for Graphviz. Not yet published to npm.

## Status

This package currently builds against the legacy C-ABI wrapper
(`capi/graphviz_api.{h,c}`) and is **not affected** by the web Wasm
rearch landed in `rearch/hpcc-js-aligned`. A follow-up PR will port the
native side to consume the same `CGraphviz` C++ class as the web package
so all platforms share one implementation.
