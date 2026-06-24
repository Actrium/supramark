# @actrium/supramark-mermaid-native-rn

React Native FFI wrapper around `supramark-mermaid-native` — a Rust staticlib
that turns Mermaid source into SVG. iOS via xcframework + ObjC bridge,
Android via JNI + `cargo ndk`-built `.so` per ABI.

This package side-registers a `d2` engine with `@supramark/engines/rn`
on import.

## Usage

```ts
import '@actrium/supramark-mermaid-native-rn';
import { createReactNativeDiagramEngine } from '@supramark/engines/rn';

const engine = createReactNativeDiagramEngine();
const svg = await engine.render('mermaid', 'a -> b -> c');
```

## Build prerequisites (monorepo dev)

This package consumes binary artefacts built by the
`crates/mermaid-little/packages/native` Cargo target. Before running
`pod install` / Android Gradle build:

```bash
# 1. iOS (3 slices + xcframework assembly)
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
for t in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
  cargo build --release --target $t -p supramark-mermaid-native
done
scripts/build-ios-xcframework.sh supramark-mermaid-native \
  crates/mermaid-little/packages/native/include libsupramark_mermaid_native.a \
  target/ios-xcframeworks/SupramarkMermaid.xcframework

# 2. Android (4 ABIs)
rustup target add aarch64-linux-android armv7-linux-androideabi \
                  x86_64-linux-android i686-linux-android
ANDROID_NDK_HOME=/opt/homebrew/share/android-ndk \
  cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 build --release \
    -p supramark-mermaid-native

# 3. Stage artefacts into this package
cd crates/mermaid-little/packages/react-native
node scripts/prepare-native.js
```

After that:
- iOS: `pod install` in your RN app's `ios/` finds the xcframework
- Android: `gradlew :supramark-mermaid-native:assembleDebug` (or via the RN
  CLI) picks up the per-ABI `.so` files in `android/src/main/jniLibs/`

## Notes

- iOS deployment target is **15.1** (matches the staticlib's
  cross-compile target — lowering it without a rebuild causes ABI
  mismatch at link time)
- Android NDK STL is `c++_shared`. RN ≥ 0.71 bundles
  `libc++_shared.so` automatically; standalone Android apps may need
  `packagingOptions { jniLibs.useLegacyPackaging = true }` or an
  explicit `include 'lib/.../libc++_shared.so'`.
- Both old (`NativeModules.SupramarkMermaidNative`) and new
  (`TurboModule`) RN architectures are supported via `index.ts`'s
  resolver.

## Out of scope (TODO)

- GitHub Release based postinstall download (today the prebuilt
  artefacts must be cargo-built locally — there's no published binary
  channel yet)
- text-metrics callback wiring (`supramark_install_metrics_callback`)
  is currently NOT installed; d2 falls back to its embedded
  `MermaidGoEmulationMetrics`. Wiring host fonts is a follow-up.
