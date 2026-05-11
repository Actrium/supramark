# graphviz-anywhere

Graphviz for native runtimes and the web.

This repository now covers three delivery paths:

- Native Graphviz shared libraries for Rust and React Native
- A safe Rust crate on top of the native C ABI
- A WebAssembly-powered web package for browsers, Web Workers, and edge runtimes

Web builds are powered by compiled WebAssembly. Native builds cover a range of target triples — see the [per-target status matrix](#native-target-status) below for an honest view of what is scripted, tested in CI, published as a Release asset, and auto-resolved by the Rust `build.rs`.

## Architecture

```
graphviz-anywhere/
├── capi/                     # C ABI wrapper (graphviz_api.h/.c)
├── packages/
│   ├── rust/                 # Safe Rust crate (graphviz-anywhere)
│   ├── react-native/         # React Native package
│   └── web/                  # Wasm-powered web package
├── scripts/                  # Per-platform native build scripts
├── examples/
│   ├── rust/                 # Rust usage example
│   ├── react-native/         # RN usage example
│   └── web/                  # Web usage notes
├── graphviz/                 # Graphviz source (git submodule)
└── .github/workflows/        # CI/CD automation
```

## Native Target Status

The table below reflects the state after concurrent PRs (Agents A–C) land.
Entries marked **NEW** are not yet in `main`; entries marked **⚠️** have
partial coverage only.

| Target | `scripts/build-*.sh` | CI matrix | GitHub Release asset | Rust `build.rs` auto-resolve |
| --- | --- | --- | --- | --- |
| x86_64-unknown-linux-gnu | ✅ | ✅ | ✅ `graphviz-native-linux-x86_64.tar.gz` | ✅ |
| aarch64-unknown-linux-gnu | ✅ | ✅ (ubuntu-24.04-arm, NEW) | ✅ `graphviz-native-linux-aarch64.tar.gz` (NEW) | ✅ (NEW) |
| aarch64-apple-darwin | ✅ (universal) | ✅ | ✅ `graphviz-native-macos-universal.tar.gz` | ✅ |
| x86_64-apple-darwin | ✅ (universal) | ✅ | ✅ `graphviz-native-macos-universal.tar.gz` | ✅ |
| aarch64-apple-ios | ✅ | ✅ | ✅ per-slice (NEW) | ✅ (NEW) |
| aarch64-apple-ios-sim | ✅ | ✅ | ✅ per-slice (NEW) | ✅ (NEW) |
| x86_64-apple-ios | ✅ (NEW) | ✅ (NEW) | ✅ per-slice (NEW) | ✅ (NEW) |
| aarch64-linux-android | ✅ | ✅ | ✅ `graphviz-native-android-arm64-v8a.tar.gz` | ✅ |
| armv7-linux-androideabi | ✅ | ✅ | ✅ `graphviz-native-android-armeabi-v7a.tar.gz` | ✅ |
| x86_64-linux-android | ✅ | ✅ | ✅ `graphviz-native-android-x86_64.tar.gz` | ✅ |
| i686-linux-android | ✅ (NEW) | ✅ (NEW) | ✅ `graphviz-native-android-x86.tar.gz` (NEW) | ✅ (NEW) |
| x86_64-pc-windows-msvc | ✅ | ✅ | ✅ `graphviz-native-windows-x86_64.zip` | ⚠️ env-override only (zip layout not auto-extracted) |
| aarch64-pc-windows-msvc | ⚠️ skeleton | ⚠️ continue-on-error | ⚠️ planned | ⚠️ env-override only |
| wasm32-unknown-unknown | — (JS bridge) | ✅ wasm build | — (bundled in npm) | — (no native link needed) |

## Cross-Compilation

For per-target toolchain requirements, expected output paths, release asset
names, override env vars, common errors, and notes on RN vs Rust path
divergence, see **[docs/cross-compile.md](docs/cross-compile.md)**.

## Quick Start

### Native prerequisites

- CMake 3.16+, bison, flex, pkg-config
- Platform-specific toolchains (Xcode, Android NDK, MSVC, etc.)

### Native build

```bash
git clone --recursive https://github.com/kookyleo/graphviz-anywhere.git
cd graphviz-anywhere

./scripts/build-linux.sh
./scripts/build-macos.sh
./scripts/build-ios.sh
./scripts/build-android.sh
./scripts/build-windows.sh
```

Build outputs land in `output/<platform>/`.

Prebuilt native binaries are published from the current repository namespace:
[GitHub Releases](https://github.com/kookyleo/graphviz-anywhere/releases).

## C API

```c
#include "graphviz_api.h"

gv_context_t *ctx = gv_context_new();

char *svg = NULL;
size_t len = 0;
gv_error_t err = gv_render(ctx, "digraph { a -> b }", "dot", "svg", &svg, &len);

if (err == GV_OK) {
    gv_free_render_data(svg);
}

gv_context_free(ctx);
```

## Rust

```toml
[dependencies]
graphviz-anywhere = { path = "packages/rust" }
```

```rust
use graphviz_anywhere::{Engine, Format, GraphvizContext};

let ctx = GraphvizContext::new().unwrap();
let svg = ctx
    .render_to_string("digraph { a -> b -> c }", Engine::Dot, Format::Svg)
    .unwrap();
println!("{svg}");
```

Highlights:

- Type-safe `Engine` and `Format` enums
- `GraphvizContext` with `Drop` for automatic cleanup
- `Result<T, GraphvizError>` error handling
- `!Send + !Sync` because Graphviz is not thread-safe
- `GRAPHVIZ_ANYWHERE_DIR` as the preferred native lookup variable, with `GRAPHVIZ_NATIVE_DIR` kept for compatibility

Build with:

```bash
GRAPHVIZ_ANYWHERE_DIR=output/linux-x86_64 cargo build
```

## React Native

```bash
npm install @kookyleo/graphviz-anywhere-rn
# or
yarn add @kookyleo/graphviz-anywhere-rn
```

```ts
import { renderDot, getVersion } from '@kookyleo/graphviz-anywhere-rn';

const svg = await renderDot('digraph { mobile -> native }');
const svg2 = await renderDot('graph { a -- b }', 'neato', 'svg');
```

Platform support:

| Platform | Bridge | Min Version |
|----------|--------|-------------|
| iOS | ObjC (`dispatch_async`) | iOS 15.1 |
| Android | Java + JNI | API 24 |
| macOS | ObjC | macOS 11.0 |
| Windows | C++/WinRT | Windows 10 v1903 |

RN compatibility: `react-native >= 0.71.0`, tested with `0.84.x`. `react-native-macos` and `react-native-windows` remain optional peer dependencies.

## Web Wasm

The `packages/web/` package adds Graphviz rendering in browsers and edge runtimes through WebAssembly.

```bash
cd packages/web
npm install
```

```ts
import { createLazyWasmRenderer } from '@kookyleo/graphviz-anywhere-web';

const renderer = createLazyWasmRenderer();
const svg = await renderer.render('digraph { web -> wasm }');
```

The runtime can self-report supported engines and formats:

```ts
const capabilities = await renderer.getCapabilities();
console.log(capabilities.engines, capabilities.formats);
```

## Three Deployment Profiles

### 1. Lazy client rendering

Use `createLazyWasmRenderer()` for docs sites, product pages, or pages where diagrams are occasional and startup size matters more than first-render latency.

### 2. Worker-backed rendering

Use `createWorkerWasmRenderer()` for diagram editors, whiteboards, or large-graph exploration so layout work stays off the main thread.

```ts
import { createWorkerWasmRenderer } from '@kookyleo/graphviz-anywhere-web';

const renderer = createWorkerWasmRenderer({ timeoutMs: 8000 });
const svg = await renderer.render('digraph { editor -> worker -> svg }');
```

### 3. Warm server / edge renderer

Use `createServerWasmRenderer()` for SSR, preview APIs, or edge handlers that render repeatedly and want to amortize Wasm startup cost across requests.

```ts
import { createServerWasmRenderer } from '@kookyleo/graphviz-anywhere-web';

const renderer = createServerWasmRenderer();
await renderer.preload();

const outputs = await renderer.renderMany(
  'digraph { api -> cache -> client }',
  ['svg', 'json']
);
```

## Supported Engines and Formats

### Native (C / Rust / React Native)

- Engines: `dot`, `neato`, `fdp`, `sfdp`, `circo`, `twopi`, `osage`, `patchwork`
- Formats: `svg`, `png`, `pdf`, `ps`, `json`, `dot`, `xdot`, `plain`

### Web (Wasm)

The exact engine and format list depends on the Wasm build and should be queried at runtime through `getCapabilities()`.

## Naming

Renaming the project to `graphviz-anywhere` is reasonable because the repository now spans:

- native shared libraries
- Rust bindings
- React Native bindings
- WebAssembly delivery for the web

The migration strategy in this repository is:

- the Rust crate is published as `graphviz-anywhere`
- the React Native npm package is published as `@kookyleo/graphviz-anywhere-rn`
- the Web npm package is published as `@kookyleo/graphviz-anywhere-web`

## Testing

### Web Unit Tests

Run vitest for the web package:

```bash
cd packages/web
npm install
npm test -- --run          # Run once
npm test                   # Watch mode
npm test:ui                # Open test UI
```

Test coverage includes:
- 30 tests for shared utilities and error handling
- 25 tests for renderer creation (lazy, server, worker)
- 20 tests for worker protocol and messaging

All tests use mock VizWasmInstance (no real Wasm dependency).

### Rust Unit Tests

Run cargo tests for Rust bindings:

```bash
# Build native C library first (macOS example)
./scripts/build-macos.sh

# Run tests from the consolidated Rust crate
cd packages/rust
GRAPHVIZ_ANYWHERE_DIR=/path/to/native/output cargo test --lib
```

Test coverage includes:
- tests for the Rust crate (Engine, Format, Error, GraphvizContext)

All Rust tests verify type safety, error handling, and trait implementations.

## Graphviz Version

Native builds bundle **Graphviz 2.44.0** from the pinned submodule.

## License

Apache License 2.0 - see [LICENSE](LICENSE).

Graphviz itself: [Eclipse Public License 1.0](https://www.eclipse.org/legal/epl-v10.html).
