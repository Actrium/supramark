# supramark-d2-native

Native FFI wrapper around [`d2-little`](../..) for React Native, iOS,
Android, and other native hosts. Mirrors the wasm-bindgen surface
exposed by [`d2-little-web`](../web) but speaks plain C ABI:
`supramark_d2_render` → SVG bytes, `supramark_d2_free` → release the
buffer, `supramark_d2_version` → static version string. The full
contract lives in [`include/supramark_d2.h`](./include/supramark_d2.h).

## Status

Pre-publish. This crate is **not** intended for crates.io / npm; it is
consumed only as a Cargo workspace member that produces:

- `libsupramark_d2_native.a` — staticlib for iOS `.xcframework` and
  Android NDK static linking
- `libsupramark_d2_native.{so,dylib,dll}` — cdylib for Android
  `jniLibs/`, desktop hosts, and any consumer that prefers dynamic
  linking
- `rlib` — so other in-workspace Rust crates can depend on it

Today only `x86_64-unknown-linux-gnu` is verified locally (this is the
machine the wave-2 native FFI rollout was bootstrapped on). The iOS /
Android cross targets land in a follow-up wave once the platform
toolchains are provisioned in CI.

## Building

### Linux (default verification target)

```bash
cargo build --release -p supramark-d2-native
ls target/release/libsupramark_d2_native.{a,so}
```

### iOS

Requires `aarch64-apple-ios` (device) and `aarch64-apple-ios-sim` /
`x86_64-apple-ios` (simulator) targets installed via `rustup target
add`. From the workspace root:

```bash
cargo build --release --target aarch64-apple-ios     -p supramark-d2-native
cargo build --release --target aarch64-apple-ios-sim -p supramark-d2-native
cargo build --release --target x86_64-apple-ios      -p supramark-d2-native

# Bundle the device + simulator slices into a single XCFramework so
# Xcode can pick the right slice automatically.
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libsupramark_d2_native.a \
    -headers crates/d2-little/packages/native/include \
  -library target/aarch64-apple-ios-sim/release/libsupramark_d2_native.a \
    -headers crates/d2-little/packages/native/include \
  -output  SupramarkD2.xcframework
```

Drop `SupramarkD2.xcframework` into the host RN module's
`ios/Frameworks/` and link it from the `.podspec`.

### Android

Easiest path is [`cargo-ndk`](https://github.com/bbqsrc/cargo-ndk) so
ABI selection and `--sysroot` plumbing happen automatically:

```bash
cargo install cargo-ndk
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

cargo ndk \
  -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o ./android/src/main/jniLibs \
  build --release -p supramark-d2-native
```

Each target ABI directory under `jniLibs/` will contain
`libsupramark_d2_native.so`. The Kotlin / Java side declares the
native methods with `System.loadLibrary("supramark_d2_native")` and
JNI-bridges to the C ABI declared in `include/supramark_d2.h`.

## Text-metrics callback

By default the d2 layout pipeline measures text using
`D2GoEmulationMetrics` — an embedded DejaVu Latin subset that
reproduces the Go d2 e2e baseline within a fraction of a pixel.
That is the right choice for "looks correct on first paint" but it
**does not** consult the host platform's installed fonts.

To wire a host-supplied measureText impl (RN Skia / UIKit / Android
Paint), call `supramark_install_metrics_callback` from the
`font-metrics` crate **once at module init** before the first render.
The C ABI for that symbol lives in
[`crates/font-metrics/src/ffi_callback.rs`](../../../font-metrics/src/ffi_callback.rs);
the symbol is automatically exported from
`libsupramark_d2_native.{a,so,dylib}` because `font-metrics` is a
direct dependency.

> **Caveat (wave 2):** today the d2 compile pipeline still builds
> `D2GoEmulationMetrics` internally regardless of the installed
> callback — wiring `FfiCallbackMetrics` into `d2-little`'s
> `default_d2_metrics()` requires the upstream metrics-trait
> indirection that lands in a follow-up phase. The callback is
> exported and installable today; it just doesn't yet flow into
> diagram geometry. See the project plan for the cutover step.

## License

MPL-2.0, matching the parent `d2-little` crate. The embedded DejaVu
fonts pulled in transitively via `font-metrics` are dual-licensed
DejaVu / Bitstream Vera / Public Domain — preserve their attribution
in any redistributed binary.
