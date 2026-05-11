# supramark-mermaid-native

C ABI wrapper around the [`mermaid-little`](../..) crate. Pairs a
small `extern "C"` surface (see [`include/supramark_mermaid.h`](include/supramark_mermaid.h))
with the `metrics-ffi-callback` impl from
[`font-metrics`](../../../font-metrics), so non-Rust hosts — React
Native TurboModules on iOS / Android, desktop Swift / Kotlin / C++
apps, etc. — can render Mermaid diagrams to SVG without spinning up
a JS engine.

Sibling of [`packages/web`](../web), which is the wasm-bindgen
wrapper. Layout and rendering are identical; only the host bridge
differs.

## Surface

Four entry points, all declared in `include/supramark_mermaid.h`:

| Function                              | Purpose                                                  |
| ------------------------------------- | -------------------------------------------------------- |
| `supramark_mermaid_render`            | Mermaid source → SVG.                                    |
| `supramark_mermaid_render_with_id`    | Same, with an explicit diagram id (stable element ids).  |
| `supramark_mermaid_free`              | Release a buffer returned by either render call.         |
| `supramark_mermaid_version`           | Static NUL-terminated crate version string.              |

Error codes:

```c
#define SUPRAMARK_MERMAID_OK              0
#define SUPRAMARK_MERMAID_ERR_PARSE       1
#define SUPRAMARK_MERMAID_ERR_RENDER      2
#define SUPRAMARK_MERMAID_ERR_NULL_INPUT  3
```

## Build

From the workspace root (single-crate build keeps the rest of the
monorepo out of the link graph):

```bash
cargo build --release -p supramark-mermaid-native
```

Output lives under `target/release/`:

- `libsupramark_mermaid_native.a` — static archive (Android / iOS
  toolchains, anything linking the Rust archive into a JNI / Swift
  static library).
- `libsupramark_mermaid_native.so` / `.dylib` / `.dll` — shared
  object (desktop Linux / macOS / Windows hosts loading via JNA or
  `dlopen`).

Both artefacts come out of a single build thanks to
`crate-type = ["staticlib", "cdylib"]` in `Cargo.toml`.

### iOS (device + simulator)

```bash
# device
cargo build --release -p supramark-mermaid-native --target aarch64-apple-ios
# simulator (Apple Silicon)
cargo build --release -p supramark-mermaid-native --target aarch64-apple-ios-sim
# simulator (Intel Macs)
cargo build --release -p supramark-mermaid-native --target x86_64-apple-ios

# combine into a universal simulator lib + XCFramework
lipo -create \
    target/aarch64-apple-ios-sim/release/libsupramark_mermaid_native.a \
    target/x86_64-apple-ios-sim/release/libsupramark_mermaid_native.a \
    -output target/universal-sim/libsupramark_mermaid_native.a

xcodebuild -create-xcframework \
    -library target/aarch64-apple-ios/release/libsupramark_mermaid_native.a \
        -headers crates/mermaid-little/packages/native/include \
    -library target/universal-sim/libsupramark_mermaid_native.a \
        -headers crates/mermaid-little/packages/native/include \
    -output SupramarkMermaid.xcframework
```

### Android (NDK, four ABIs)

```bash
# requires cargo-ndk: `cargo install cargo-ndk`
cargo ndk \
    -t armeabi-v7a -t arm64-v8a -t x86 -t x86_64 \
    -o ./jniLibs \
    build --release -p supramark-mermaid-native
```

This produces `jniLibs/<abi>/libsupramark_mermaid_native.so` ready
to drop into an Android Studio module's `src/main/jniLibs/`.

## Host integration

### 1. Wire up the metrics callback

`mermaid-little` does no text measurement of its own when this
binding is built; it defers every width / ascent / descent query to
a host-installed function pointer. The host calls
`supramark_install_metrics_callback` **once** at module init,
before any render call.

The installer is exported by the `font-metrics` crate (re-linked
into this library), so the symbol is visible alongside the
`supramark_mermaid_*` exports. Declare it locally on the C side:

```c
typedef struct supramark_measured {
    double width;
    double ascent;
    double descent;
} supramark_measured;

/* Called by Rust whenever a layout pass needs to know how wide a
 * label is. The host implementation must be reentrant and quick
 * (it's on the hot layout path). UTF-8, length is authoritative,
 * input is NOT NUL-terminated. */
typedef supramark_measured (*supramark_measure_text_fn)(
    const uint8_t *text_utf8,
    size_t         text_len,
    double         size_px,
    int32_t        bold,    /* 0 / 1 */
    int32_t        italic,  /* 0 / 1 */
    const uint8_t *family,
    size_t         family_len);

extern void supramark_install_metrics_callback(supramark_measure_text_fn cb);
```

On iOS the implementation typically calls `-[NSString
sizeWithAttributes:]` or `CTLineGetTypographicBounds`; on Android,
`Paint.measureText` + `FontMetrics`; on desktop, whatever the host
text stack provides. RN-Skia exposes `SkiaText.measureText` from
its TurboModule.

### 2. Drive the render

```c
#include "supramark_mermaid.h"

const char *src = "graph TD\nA-->B";
uint8_t *svg = NULL;
size_t   svg_len = 0;

int32_t rc = supramark_mermaid_render(
    (const uint8_t *)src, strlen(src),
    &svg, &svg_len);

if (rc == SUPRAMARK_MERMAID_OK) {
    /* use (svg, svg_len) */
    supramark_mermaid_free(svg, svg_len);
} else {
    /* rc is one of SUPRAMARK_MERMAID_ERR_* */
}
```

### Threading

All entry points are `Send + Sync`. Each render allocates its own
working set; you can drive several conversions concurrently from a
thread pool. The metrics callback must itself be thread-safe (it
is invoked from whichever thread called `supramark_mermaid_render`).

### Panics

The release profile sets `panic = "abort"`, so a Rust panic
terminates the process rather than unwinding across the C ABI
boundary (which would be undefined behaviour). As a belt-and-
braces measure each entry point also wraps its body in
`catch_unwind`, so debug builds (which use unwinding) won't tear
down the host either — caught panics surface as
`SUPRAMARK_MERMAID_ERR_RENDER`.

## Verifying locally

```bash
cargo check   -p supramark-mermaid-native
cargo build   -p supramark-mermaid-native --release
cargo test    -p mermaid-little --lib            # parent crate, must stay green
```
