# supramark-plantuml-native

Native FFI wrapper around [`plantuml-little`](../..) for React Native
(iOS / Android), desktop Swift / Kotlin / C / C++ hosts, and anything
else that can dlopen a `cdylib` or link a `staticlib`.

Counterpart to [`packages/web/`](../web), which is the wasm-bindgen
wrapper used in the browser / Hermes.

## Public C ABI

See [`include/supramark_plantuml.h`](include/supramark_plantuml.h) for
the canonical declarations. The Rust source in
[`src/lib.rs`](src/lib.rs) is the source of truth for the contract.

Three render-side entry points:

| Function                       | Purpose                                                |
|--------------------------------|--------------------------------------------------------|
| `supramark_plantuml_render`    | UTF-8 PlantUML source → SVG bytes (Rust-allocated).    |
| `supramark_plantuml_free`      | Return a render output buffer back to Rust.            |
| `supramark_plantuml_version`   | `'static` NUL-terminated `CARGO_PKG_VERSION`.          |

One metrics-bridge entry point (re-exported from `font-metrics`):

| Function                                  | Purpose                                          |
|-------------------------------------------|--------------------------------------------------|
| `supramark_install_metrics_callback`      | Wire the host's `measureText` into Rust layout.  |

### Error codes

```c
#define SUPRAMARK_PLANTUML_OK              0
#define SUPRAMARK_PLANTUML_ERR_PARSE       1  // input not valid UTF-8
#define SUPRAMARK_PLANTUML_ERR_RENDER      2  // plantuml-little returned Err
#define SUPRAMARK_PLANTUML_ERR_NULL_INPUT  3  // required pointer was null
```

v1 deliberately discards the underlying `Display` message from
`plantuml-little::convert`. If a future revision needs human-readable
diagnostics we'll add `supramark_plantuml_last_error() -> *const char`
without breaking the existing return codes.

### Allocator contract

`supramark_plantuml_render` writes a heap-allocated UTF-8 byte buffer
through `*out_buf` / `*out_len`. The buffer is allocated by Rust's
global allocator and **must** be returned via `supramark_plantuml_free`
— the host's `free()` would mis-match the allocator and corrupt the
heap. The buffer is **not** NUL-terminated; use `out_len`.

## Building

The crate is a workspace member of the supramark super-monorepo; build
from the repo root.

### Linux / macOS host build (smoke)

```bash
cargo build --release -p supramark-plantuml-native
# Artefacts land in target/release/
#   libsupramark_plantuml_native.a    (staticlib — link into XCFramework / NDK)
#   libsupramark_plantuml_native.so   (cdylib — Linux)
#   libsupramark_plantuml_native.dylib (cdylib — macOS)
```

### iOS (XCFramework)

```bash
# One-time toolchain install:
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

# Per-arch staticlibs:
cargo build --release -p supramark-plantuml-native --target aarch64-apple-ios
cargo build --release -p supramark-plantuml-native --target aarch64-apple-ios-sim
cargo build --release -p supramark-plantuml-native --target x86_64-apple-ios

# Bundle into an .xcframework via xcodebuild -create-xcframework, e.g.
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libsupramark_plantuml_native.a \
    -headers crates/plantuml-little/packages/native/include \
  -library target/aarch64-apple-ios-sim/release/libsupramark_plantuml_native.a \
    -headers crates/plantuml-little/packages/native/include \
  -output supramark_plantuml.xcframework
```

### Android (NDK)

```bash
# One-time toolchain install:
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

# Configure cc / linker via cargo-ndk or .cargo/config.toml, then:
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o ./jniLibs build --release -p supramark-plantuml-native
```

The header (`include/supramark_plantuml.h`) is platform-independent;
ship it alongside whichever artefact your build pipeline produces.

## Wiring the metrics bridge

Layer-1 layout inside `plantuml-little` measures text against a host-
supplied callback. Install it **once** at module init, before any
`supramark_plantuml_render` call:

### iOS (Swift, with a UIFont-backed measurer)

```swift
@_silgen_name("supramark_install_metrics_callback")
func supramark_install_metrics_callback(_ cb: @convention(c) (
    UnsafePointer<CChar>?, Int,
    UnsafePointer<CChar>?, Int,
    Double, UInt8, UInt8,
    UnsafeMutablePointer<Double>?,
    UnsafeMutablePointer<Double>?,
    UnsafeMutablePointer<Double>?
) -> Void) -> Void

let measure: @convention(c) (
    UnsafePointer<CChar>?, Int,
    UnsafePointer<CChar>?, Int,
    Double, UInt8, UInt8,
    UnsafeMutablePointer<Double>?,
    UnsafeMutablePointer<Double>?,
    UnsafeMutablePointer<Double>?
) -> Void = { family, familyLen, text, textLen, size, bold, italic,
              outW, outA, outD in
    // … wrap UIFont / NSAttributedString.size, write into out* …
}
supramark_install_metrics_callback(measure)
```

### Android (Kotlin via JNI / TurboModule, with a Paint-backed measurer)

```kotlin
external fun installMetricsCallback()  // wired in C++ that calls
                                        // supramark_install_metrics_callback
```

The C++ shim looks up a `Paint` per `(family, size, bold, italic)`
tuple, calls `Paint.measureText` and `Paint.getFontMetrics`, and
writes the results back through the three `double*` out-pointers.

### Threading

`supramark_plantuml_render` is safe to call from multiple host threads
concurrently. The metrics callback installed via
`supramark_install_metrics_callback` must therefore also be
thread-safe — the global slot is `AtomicPtr`-protected on the Rust
side, but the host implementation is on its own.

## Verifying the link

After bundling, the linked artefact exposes the following symbols
(check with `nm -gU` on macOS / `nm -D` on Linux / `objdump -t` on
Windows-via-MinGW):

```text
T supramark_plantuml_render
T supramark_plantuml_free
T supramark_plantuml_version
T supramark_install_metrics_callback
```

If `supramark_install_metrics_callback` is missing, your build is
linking `plantuml-little` without the `metrics-ffi-callback` feature
— double-check that the Cargo invocation went through this crate
(`supramark-plantuml-native`) and not the parent `plantuml-little`
crate's default feature set.

## Licence

Inherits the upstream `plantuml-little` multi-licence disjunction:
`GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0 OR EPL-2.0 OR
MIT`. supramark itself consumes plantuml-little under Apache-2.0; see
[`../../UPSTREAM.md`](../../UPSTREAM.md) for details.
