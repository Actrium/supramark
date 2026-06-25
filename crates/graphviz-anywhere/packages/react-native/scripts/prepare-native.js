#!/usr/bin/env node
/**
 * prepare-native.js — stage the build-android.sh outputs (output/android/<abi>/)
 * into the package so that a `file:` install carries the .so + headers and a
 * fresh `yarn install` does not drop them.
 *
 * Each artifact is staged to two destinations for two distinct uses:
 *   1. android/libs/<abi>/{lib,include}/  — CMake link-time inputs for the JNI
 *      build (CMakeLists.txt's GRAPHVIZ_PREBUILT points at ../../libs/<abi>)
 *   2. android/src/main/jniLibs/<abi>/    — packaged into the APK by Gradle for
 *      runtime loading (build.gradle's jniLibs.srcDirs = ["src/main/jniLibs"])
 *
 * Run this AFTER:
 *   - ANDROID_NDK_HOME=... ./scripts/build-android.sh  (produces output/android/)
 *
 * Idempotent — re-running just refreshes.
 */

const fs = require('fs');
const path = require('path');

// crates/graphviz-anywhere/packages/react-native/scripts/ → 5 levels up
const REPO_ROOT = path.resolve(__dirname, '..', '..', '..', '..', '..');
const PKG_DIR = path.resolve(__dirname, '..');
const PROJECT_ROOT = path.resolve(PKG_DIR, '..', '..'); // crates/graphviz-anywhere
const TARGET_DIR = path.join(REPO_ROOT, 'target');

const ANDROID_OUTPUT = path.join(PROJECT_ROOT, 'output', 'android');
const ABIS = ['arm64-v8a', 'armeabi-v7a', 'x86_64', 'x86'];

// iOS xcframework (build-ios.sh output, staged under target/ios-xcframeworks/)
const IOS_XCFRAMEWORK_SRC = path.join(TARGET_DIR, 'ios-xcframeworks', 'GraphvizApi.xcframework');
const IOS_FRAMEWORKS_DEST = path.join(PKG_DIR, 'ios', 'Frameworks');

// CMake link-time inputs (CMakeLists.txt's GRAPHVIZ_PREBUILT)
const LIBS_DEST = path.join(PKG_DIR, 'android', 'libs');
// Packaged into the APK by Gradle (build.gradle's jniLibs.srcDirs)
const JNILIBS_DEST = path.join(PKG_DIR, 'android', 'src', 'main', 'jniLibs');

function fileExists(p) {
  try { fs.accessSync(p); return true; } catch { return false; }
}

function copyDirRecursive(src, dest) {
  fs.mkdirSync(dest, { recursive: true });
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const s = path.join(src, entry.name);
    const d = path.join(dest, entry.name);
    if (entry.isDirectory()) copyDirRecursive(s, d);
    else fs.copyFileSync(s, d);
  }
}

function prepareIOS() {
  if (!fileExists(IOS_XCFRAMEWORK_SRC)) {
    console.warn(`⚠  iOS xcframework not found at:\n   ${IOS_XCFRAMEWORK_SRC}`);
    console.warn(`   Run scripts/build-ios.sh, then assemble GraphvizApi.xcframework into target/ios-xcframeworks/.`);
    return false;
  }
  fs.rmSync(IOS_FRAMEWORKS_DEST, { recursive: true, force: true });
  fs.mkdirSync(IOS_FRAMEWORKS_DEST, { recursive: true });
  copyDirRecursive(IOS_XCFRAMEWORK_SRC, path.join(IOS_FRAMEWORKS_DEST, 'GraphvizApi.xcframework'));
  console.log(`✓ iOS: copied GraphvizApi.xcframework → ${path.relative(REPO_ROOT, IOS_FRAMEWORKS_DEST)}`);
  return true;
}

function prepareAndroid() {
  let anyFound = false;
  for (const abi of ABIS) {
    const srcSo = path.join(ANDROID_OUTPUT, abi, 'lib', 'libgraphviz_api.so');
    const srcHeader = path.join(ANDROID_OUTPUT, abi, 'include', 'graphviz_api.h');
    if (!fileExists(srcSo)) {
      console.warn(`⚠  Android ${abi}: missing ${path.relative(REPO_ROOT, srcSo)} (skip)`);
      continue;
    }

    // 1. libs/<abi>/lib + libs/<abi>/include (CMake link-time inputs)
    const libsAbiDir = path.join(LIBS_DEST, abi);
    fs.mkdirSync(path.join(libsAbiDir, 'lib'), { recursive: true });
    fs.mkdirSync(path.join(libsAbiDir, 'include'), { recursive: true });
    fs.copyFileSync(srcSo, path.join(libsAbiDir, 'lib', 'libgraphviz_api.so'));
    if (fileExists(srcHeader)) {
      fs.copyFileSync(srcHeader, path.join(libsAbiDir, 'include', 'graphviz_api.h'));
    }

    // 2. jniLibs/<abi> (packaged into the APK by Gradle)
    const jniLibsAbiDir = path.join(JNILIBS_DEST, abi);
    fs.mkdirSync(jniLibsAbiDir, { recursive: true });
    fs.copyFileSync(srcSo, path.join(jniLibsAbiDir, 'libgraphviz_api.so'));

    anyFound = true;
    console.log(`✓ Android ${abi}: copied .so → libs/${abi}/lib + jniLibs/${abi}/`);
  }
  return anyFound;
}

const ios = prepareIOS();
const android = prepareAndroid();
if (!ios && !android) {
  console.error('No native artefacts found. Run scripts/build-android.sh / build-ios.sh first.');
  process.exit(1);
}
