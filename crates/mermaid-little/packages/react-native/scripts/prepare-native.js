#!/usr/bin/env node
/*
 * prepare-native.js — bring the cargo-produced d2 native libs into
 * the RN package's local layout so podspec / Gradle / CMake reference
 * stable paths.
 *
 * Run this AFTER:
 *   - `cargo build --release --target <ios-triple>  -p supramark-mermaid-native`
 *   - `scripts/build-ios-xcframework.sh ...` (at repo root, produces
 *     target/ios-xcframeworks/SupramarkMermaid.xcframework)
 *   - `cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 build --release
 *      -p supramark-mermaid-native`
 *
 * Then run `npm run prepare-native` (or `node scripts/prepare-native.js`).
 * Idempotent — re-running just refreshes.
 */

const fs = require('fs');
const path = require('path');

// crates/mermaid-little/packages/react-native/scripts/ → 5 levels up to repo root
const REPO_ROOT = path.resolve(__dirname, '..', '..', '..', '..', '..');
const PKG_DIR = path.resolve(__dirname, '..');
const TARGET_DIR = path.join(REPO_ROOT, 'target');

const IOS_XCFRAMEWORK_SRC = path.join(
  TARGET_DIR,
  'ios-xcframeworks',
  'SupramarkMermaid.xcframework'
);
const IOS_FRAMEWORKS_DEST = path.join(PKG_DIR, 'ios', 'Frameworks');

const ANDROID_ABIS = {
  'arm64-v8a':    'aarch64-linux-android',
  'armeabi-v7a':  'armv7-linux-androideabi',
  'x86_64':       'x86_64-linux-android',
  'x86':          'i686-linux-android',
};
const ANDROID_JNILIBS_DEST = path.join(PKG_DIR, 'android', 'src', 'main', 'jniLibs');

// C ABI header — staged into the package so the Android CMake build is
// self-contained. A `file:` install copies the package into the consumer's
// node_modules, which breaks any relative path that pointed back into the
// monorepo's native crate, so we vendor the header here instead.
const NATIVE_HEADER_SRC = path.join(
  REPO_ROOT,
  'crates',
  'mermaid-little',
  'packages',
  'native',
  'include'
);
const ANDROID_JNI_INCLUDE_DEST = path.join(PKG_DIR, 'android', 'src', 'main', 'jni', 'include');

function copyDirRecursive(src, dest) {
  fs.mkdirSync(dest, { recursive: true });
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const s = path.join(src, entry.name);
    const d = path.join(dest, entry.name);
    if (entry.isDirectory()) copyDirRecursive(s, d);
    else if (entry.isSymbolicLink()) {
      const link = fs.readlinkSync(s);
      fs.symlinkSync(link, d);
    } else fs.copyFileSync(s, d);
  }
}

function fileExists(p) {
  try { fs.accessSync(p); return true; } catch { return false; }
}

function prepareIOS() {
  if (!fileExists(IOS_XCFRAMEWORK_SRC)) {
    console.warn(`⚠  iOS xcframework not found at:\n   ${IOS_XCFRAMEWORK_SRC}`);
    console.warn(`   Run scripts/build-ios-xcframework.sh from the repo root first.`);
    return false;
  }
  fs.rmSync(IOS_FRAMEWORKS_DEST, { recursive: true, force: true });
  fs.mkdirSync(IOS_FRAMEWORKS_DEST, { recursive: true });
  copyDirRecursive(IOS_XCFRAMEWORK_SRC, path.join(IOS_FRAMEWORKS_DEST, 'SupramarkMermaid.xcframework'));
  console.log(`✓ iOS: copied SupramarkMermaid.xcframework → ${path.relative(REPO_ROOT, IOS_FRAMEWORKS_DEST)}`);
  return true;
}

function prepareAndroid() {
  let anyFound = false;
  for (const [abi, rustTriple] of Object.entries(ANDROID_ABIS)) {
    const src = path.join(TARGET_DIR, rustTriple, 'release', 'libsupramark_mermaid_native.so');
    if (!fileExists(src)) {
      console.warn(`⚠  Android ${abi}: missing ${path.relative(REPO_ROOT, src)} (skip)`);
      continue;
    }
    const destDir = path.join(ANDROID_JNILIBS_DEST, abi);
    fs.mkdirSync(destDir, { recursive: true });
    fs.copyFileSync(src, path.join(destDir, 'libsupramark_mermaid_native.so'));
    anyFound = true;
    console.log(`✓ Android ${abi}: copied .so → jniLibs/${abi}/`);
  }
  if (!anyFound) {
    console.warn(`   Run \`cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 build --release -p supramark-mermaid-native\` first.`);
  }
  return anyFound;
}

const ios = prepareIOS();
const android = prepareAndroid();

// Stage the C ABI header into the package (used by the Android CMake build;
// iOS gets its headers from inside the xcframework).
function prepareHeader() {
  if (!fileExists(NATIVE_HEADER_SRC)) {
    console.warn(`⚠  Native header not found at:\n   ${NATIVE_HEADER_SRC}`);
    return false;
  }
  fs.rmSync(ANDROID_JNI_INCLUDE_DEST, { recursive: true, force: true });
  copyDirRecursive(NATIVE_HEADER_SRC, ANDROID_JNI_INCLUDE_DEST);
  console.log(`✓ Android: copied headers → ${path.relative(REPO_ROOT, ANDROID_JNI_INCLUDE_DEST)}/`);
  return true;
}
const header = prepareHeader();

if (!ios && !android) {
  console.error('No native artefacts found. Build the Rust crate first.');
  process.exit(1);
}
if (!header) {
  console.error('Native header missing. Cannot proceed.');
  process.exit(1);
}
