#!/usr/bin/env node
/*
 * prepare-native.js — stage the locally built Graphviz native libraries into
 * the RN package's shipped layout so the podspec (iOS / macOS), Gradle + CMake
 * (Android) and the Windows project reference stable paths, and so `npm pack`
 * bundles the binaries directly into the tarball.
 *
 * This package has NO cargo crate of its own — the native core is built from
 * Graphviz *source* via scripts/build-<platform>.sh at the crate root. Build
 * the platforms you need first, e.g. from crates/graphviz-anywhere:
 *
 *   ./scripts/build-ios.sh                       -> output/ios/Graphviz.xcframework
 *   ./scripts/build-android.sh                   -> output/android/<abi>/lib/libgraphviz_api.so
 *   ./scripts/build-macos.sh                     -> output/macos-universal/lib/libgraphviz_api.dylib
 *   ./scripts/build-windows.sh                   -> output/windows-x86_64/{lib/graphviz_api.lib, bin/graphviz_api.dll}
 *
 * Then run `npm run prepare-native` (or `node scripts/prepare-native.js`).
 * Idempotent — re-running just refreshes. Missing platforms are skipped with a
 * warning; the run only fails if NOTHING was staged.
 *
 * NOTE: the staged binaries are gitignored (see .gitignore) but ARE published
 * because package.json `files[]` lists ios/android/macos/windows and `prepack`
 * runs the bob build. There is no postinstall download step — consumers get the
 * binaries straight from the npm tarball.
 */

const fs = require('fs');
const path = require('path');

// crates/graphviz-anywhere/packages/react-native/scripts/ -> crate root is 2 up.
const PKG_DIR = path.resolve(__dirname, '..');
const CRATE_ROOT = path.resolve(__dirname, '..', '..', '..');
const OUTPUT_DIR = path.join(CRATE_ROOT, 'output');
const CAPI_HEADER = path.join(CRATE_ROOT, 'capi', 'graphviz_api.h');

// ── Android: one unified libgraphviz_api.so per ABI, staged under jniLibs so
//    Gradle packages it into the APK and CMake imports it from the same path
//    (single-sourced — no separate android/libs copy). ───────────────────────
const ANDROID_ABIS = ['arm64-v8a', 'armeabi-v7a', 'x86_64', 'x86'];
const ANDROID_JNILIBS_DEST = path.join(PKG_DIR, 'android', 'src', 'main', 'jniLibs');

// ── iOS: the static XCFramework produced by build-ios.sh. ────────────────────
const IOS_XCFRAMEWORK_SRC = path.join(OUTPUT_DIR, 'ios', 'Graphviz.xcframework');
const IOS_FRAMEWORKS_DEST = path.join(PKG_DIR, 'ios', 'Frameworks');

// ── macOS: the universal dylib + header. ─────────────────────────────────────
const MACOS_LIB_SRC = path.join(OUTPUT_DIR, 'macos-universal', 'lib');
const MACOS_FRAMEWORKS_DEST = path.join(PKG_DIR, 'macos', 'Frameworks');

// ── Windows: the import lib (lib/) + runtime DLL (bin/) + header. ─────────────
const WINDOWS_LIB_SRC = path.join(OUTPUT_DIR, 'windows-x86_64', 'lib');
const WINDOWS_BIN_SRC = path.join(OUTPUT_DIR, 'windows-x86_64', 'bin');
const WINDOWS_INCLUDE_SRC = path.join(OUTPUT_DIR, 'windows-x86_64', 'include');
const WINDOWS_FRAMEWORKS_DEST = path.join(PKG_DIR, 'windows', 'Frameworks');

function fileExists(p) {
  try {
    fs.accessSync(p);
    return true;
  } catch {
    return false;
  }
}

function copyDirRecursive(src, dest) {
  fs.mkdirSync(dest, { recursive: true });
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const s = path.join(src, entry.name);
    const d = path.join(dest, entry.name);
    if (entry.isDirectory()) {
      copyDirRecursive(s, d);
    } else if (entry.isSymbolicLink()) {
      const link = fs.readlinkSync(s);
      try {
        fs.unlinkSync(d);
      } catch {
        /* not present */
      }
      fs.symlinkSync(link, d);
    } else {
      fs.copyFileSync(s, d);
    }
  }
}

function prepareAndroid() {
  let anyFound = false;
  for (const abi of ANDROID_ABIS) {
    const src = path.join(OUTPUT_DIR, 'android', abi, 'lib', 'libgraphviz_api.so');
    if (!fileExists(src)) {
      console.warn(`!  Android ${abi}: missing ${path.relative(CRATE_ROOT, src)} (skip)`);
      continue;
    }
    const destDir = path.join(ANDROID_JNILIBS_DEST, abi);
    fs.mkdirSync(destDir, { recursive: true });
    fs.copyFileSync(src, path.join(destDir, 'libgraphviz_api.so'));
    anyFound = true;
    console.log(`OK Android ${abi}: libgraphviz_api.so -> jniLibs/${abi}/`);
  }
  if (!anyFound) {
    console.warn('   Run `./scripts/build-android.sh` at the crate root first.');
  }
  return anyFound;
}

function prepareIOS() {
  if (!fileExists(IOS_XCFRAMEWORK_SRC)) {
    console.warn(`!  iOS: missing ${path.relative(CRATE_ROOT, IOS_XCFRAMEWORK_SRC)} (skip)`);
    console.warn('   Run `./scripts/build-ios.sh` at the crate root first.');
    return false;
  }
  const dest = path.join(IOS_FRAMEWORKS_DEST, 'Graphviz.xcframework');
  fs.rmSync(dest, { recursive: true, force: true });
  fs.mkdirSync(IOS_FRAMEWORKS_DEST, { recursive: true });
  copyDirRecursive(IOS_XCFRAMEWORK_SRC, dest);
  console.log('OK iOS: Graphviz.xcframework -> ios/Frameworks/');
  return true;
}

function prepareMacOS() {
  const dylib = path.join(MACOS_LIB_SRC, 'libgraphviz_api.dylib');
  if (!fileExists(dylib)) {
    console.warn(`!  macOS: missing ${path.relative(CRATE_ROOT, dylib)} (skip)`);
    return false;
  }
  const libDest = path.join(MACOS_FRAMEWORKS_DEST, 'lib');
  const incDest = path.join(MACOS_FRAMEWORKS_DEST, 'include');
  fs.mkdirSync(libDest, { recursive: true });
  fs.mkdirSync(incDest, { recursive: true });
  fs.copyFileSync(dylib, path.join(libDest, 'libgraphviz_api.dylib'));
  if (fileExists(CAPI_HEADER)) {
    fs.copyFileSync(CAPI_HEADER, path.join(incDest, 'graphviz_api.h'));
  }
  console.log('OK macOS: libgraphviz_api.dylib + header -> macos/Frameworks/');
  return true;
}

function prepareWindows() {
  if (!fileExists(WINDOWS_LIB_SRC)) {
    console.warn(`!  Windows: missing ${path.relative(CRATE_ROOT, WINDOWS_LIB_SRC)} (skip)`);
    return false;
  }
  const libDest = path.join(WINDOWS_FRAMEWORKS_DEST, 'lib');
  const incDest = path.join(WINDOWS_FRAMEWORKS_DEST, 'include');
  copyDirRecursive(WINDOWS_LIB_SRC, libDest);
  // build-windows.sh installs the runtime graphviz_api.dll under bin/ (CMake
  // RUNTIME DESTINATION), separate from the import lib in lib/. Stage it too —
  // otherwise a dynamic-link consumer ships the import lib without its DLL.
  if (fileExists(WINDOWS_BIN_SRC)) {
    copyDirRecursive(WINDOWS_BIN_SRC, path.join(WINDOWS_FRAMEWORKS_DEST, 'bin'));
  }
  if (fileExists(WINDOWS_INCLUDE_SRC)) {
    copyDirRecursive(WINDOWS_INCLUDE_SRC, incDest);
  } else if (fileExists(CAPI_HEADER)) {
    fs.mkdirSync(incDest, { recursive: true });
    fs.copyFileSync(CAPI_HEADER, path.join(incDest, 'graphviz_api.h'));
  }
  console.log('OK Windows: graphviz_api lib + dll + headers -> windows/Frameworks/');
  return true;
}

const results = [prepareAndroid(), prepareIOS(), prepareMacOS(), prepareWindows()];
if (!results.some(Boolean)) {
  console.error(
    'No native artefacts found. Build at least one platform with ' +
      'scripts/build-<platform>.sh at the crate root first.'
  );
  process.exit(1);
}
console.log('prepare-native: done.');
