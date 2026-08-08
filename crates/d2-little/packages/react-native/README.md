# @actrium/supramark-d2-native-rn

React Native FFI wrapper around `supramark-d2-native` that turns D2 source
into SVG. iOS uses a static XCFramework, macOS uses a universal dylib, and
Android uses JNI with a `cargo ndk`-built `.so` per ABI.

This package side-registers a `d2` engine with `@supramark/engines/rn`
on import.

## Usage

```ts
import '@actrium/supramark-d2-native-rn';
import { createReactNativeDiagramEngine } from '@supramark/engines/rn';

const engine = createReactNativeDiagramEngine();
const svg = await engine.render('d2', 'a -> b -> c');
```

## Build prerequisites (monorepo dev)

This package consumes binary artefacts built by the
`crates/d2-little/packages/native` Cargo target. Repository maintainers build
and stage them before publishing:

```bash
# 1. iOS (3 slices + xcframework assembly)
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
for t in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
  cargo build --release --target $t -p supramark-d2-native
done
scripts/build-ios-xcframework.sh supramark-d2-native \
  crates/d2-little/packages/native/include libsupramark_d2_native.a \
  target/ios-xcframeworks/SupramarkD2.xcframework

# 2. macOS (arm64 + x86_64 dylib; also stages all Apple package artefacts)
bun run native:macos:build

# 3. Android (4 ABIs)
rustup target add aarch64-linux-android armv7-linux-androideabi \
                  x86_64-linux-android i686-linux-android
ANDROID_NDK_HOME=/opt/homebrew/share/android-ndk \
  cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 build --release \
    -p supramark-d2-native

# 4. Stage every available artefact (safe to rerun)
cd crates/d2-little/packages/react-native
node scripts/prepare-native.js
```

After that:

- iOS: `pod install` in your RN app's `ios/` finds the xcframework
- macOS: the standard CocoaPods/autolinking flow finds the package-root
  podspec and embeds the universal dylib; no manual Podfile entry or binary
  patch is required
- Android: `gradlew :supramark-d2-native:assembleDebug` (or via the RN
  CLI) picks up the per-ABI `.so` files in `android/src/main/jniLibs/`

## Notes

- iOS deployment target is **15.1** (matches the staticlib's
  cross-compile target — lowering it without a rebuild causes ABI
  mismatch at link time)
- macOS deployment target is **11.0**. Published dylibs contain both arm64 and
  x86_64 and use an `@rpath` install name.
- Android NDK STL is `c++_shared`. RN ≥ 0.71 bundles
  `libc++_shared.so` automatically; standalone Android apps may need
  `packagingOptions { jniLibs.useLegacyPackaging = true }` or an
  explicit `include 'lib/.../libc++_shared.so'`.
- The Android library's `abiFilters` honour the app-level
  `reactNativeArchitectures` gradle property, and otherwise default to
  the ABIs `prepare-native.js` stages (`arm64-v8a`, `x86_64`). The
  default build needs **both** staged — a selected ABI whose `.so` is
  missing triggers a CMake `FATAL_ERROR`. For a faster single-ABI build
  (local or CI), set `reactNativeArchitectures=arm64-v8a` in the app's
  `android/gradle.properties`.
- Both old (`NativeModules.SupramarkD2Native`) and new
  (`TurboModule`) RN architectures are supported via `index.ts`'s
  resolver.

Published npm tarballs contain the staged native binaries directly. Consumers
do not run the monorepo build commands or a postinstall download.

## Out of scope (TODO)

- text-metrics callback wiring (`supramark_install_metrics_callback`)
  is currently NOT installed; d2 falls back to its embedded
  `D2GoEmulationMetrics`. Wiring host fonts is a follow-up.
