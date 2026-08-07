#!/usr/bin/env node
import { spawn, spawnSync } from 'node:child_process';
import http from 'node:http';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const { PNG } = createRequire(import.meta.url)('pngjs');

const here = path.dirname(fileURLToPath(import.meta.url));
const projectDir = path.resolve(here, '..');
const maestroDir = path.join(projectDir, 'maestro');
const artifactDir = path.join(maestroDir, 'artifacts');
const visualAssertScript = path.join(projectDir, 'scripts', 'assert-selection-visual.mjs');
const port = process.env.SUPRAMARK_RN_E2E_PORT ?? '8090';
const bundleURL =
  process.env.SUPRAMARK_RN_E2E_BUNDLE_URL ??
  `http://localhost:${port}/.expo/.virtual-metro-entry.bundle?platform=android&dev=true&minify=false`;
const gestureStartScreenshot = path.join(artifactDir, 'selection-gesture-start-android.png');
const gestureScreenshot = path.join(artifactDir, 'selection-gesture-android.png');
const cjkScreenshot = path.join(artifactDir, 'selection-cjk-half-android.png');
const flatListScreenshot = path.join(artifactDir, 'selection-flatlist-android.png');
const androidAssetsDir = path.join(projectDir, 'android', 'app', 'src', 'main', 'assets');
const androidResDir = path.join(projectDir, 'android', 'app', 'src', 'main', 'res');
const androidEntryFile = path.join(projectDir, 'index.js');
const androidBundleFile = path.join(androidAssetsDir, 'index.android.bundle');
const generatedAndroidPaths = ['android/app/src/main/assets', 'android/app/src/main/res'];
const appId = 'com.supramarkrndemo';
const activityName = 'com.supramarkrndemo/.MainActivity';
const baseScreen = { w: 1080, h: 2400 };

const sdkRoot =
  process.env.ANDROID_HOME ??
  process.env.ANDROID_SDK_ROOT ??
  path.join(os.homedir(), 'Library', 'Android', 'sdk');

const e2eEnv = {
  ...process.env,
  ANDROID_HOME: sdkRoot,
  ANDROID_SDK_ROOT: sdkRoot,
  PATH: [
    path.join(sdkRoot, 'platform-tools'),
    path.join(sdkRoot, 'emulator'),
    process.env.PATH ?? '',
  ].join(path.delimiter),
  CI: process.env.CI ?? '1',
  SUPRAMARK_RN_E2E: 'selection',
  EXPO_PUBLIC_SUPRAMARK_RN_E2E: 'selection',
};

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? projectDir,
    env: options.env ?? e2eEnv,
    stdio: options.stdio ?? 'inherit',
    encoding: options.encoding ?? 'utf8',
    maxBuffer: 50 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(' ')} failed with status ${result.status}`);
  }
  return result.stdout ?? '';
}

function capture(command, args, options = {}) {
  return run(command, args, { ...options, stdio: 'pipe' });
}

function commandPath(command, fallback) {
  const locator = process.platform === 'win32' ? 'where' : 'which';
  const found = spawnSync(locator, [command], { env: e2eEnv, encoding: 'utf8' });
  if (found.status === 0 && found.stdout.trim()) return found.stdout.trim();
  if (fallback && existsSync(fallback)) return fallback;
  throw new Error(`${command} was not found`);
}

const adb = commandPath(
  'adb',
  path.join(sdkRoot, 'platform-tools', process.platform === 'win32' ? 'adb.exe' : 'adb')
);

let emulatorPath = null;

function getEmulator() {
  emulatorPath ??= commandPath(
    'emulator',
    path.join(sdkRoot, 'emulator', process.platform === 'win32' ? 'emulator.exe' : 'emulator')
  );
  return emulatorPath;
}

function sleepMs(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function boxWidth(box) {
  return box.x1 - box.x0 + 1;
}

function boxHeight(box) {
  return box.y1 - box.y0 + 1;
}

function endKnobCenter(box) {
  const radius = boxWidth(box) / 2;
  return {
    x: Math.round((box.x0 + box.x1) / 2),
    y: Math.round(box.y1 - radius),
  };
}

function summarizeBox(box) {
  return `${boxWidth(box)}x${boxHeight(box)}@${box.x0},${box.y0}`;
}

function dirty(paths) {
  const status = spawnSync('git', ['status', '--porcelain', '--', ...paths], {
    cwd: projectDir,
    env: process.env,
    encoding: 'utf8',
  });
  if (status.status !== 0) {
    throw new Error(`git status failed with status ${status.status}`);
  }
  return status.stdout.trim().length > 0;
}

function restoreGeneratedAndroidFiles() {
  run('git', ['restore', '--', 'android/app/src/main/res'], { env: process.env });
  run('git', ['clean', '-fd', '--', ...generatedAndroidPaths], { env: process.env });
}

function adbArgs(serial, args) {
  return ['-s', serial, ...args];
}

function sanitizeArtifactName(value) {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
}

function escapeXmlAttribute(value) {
  return value
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

function listDevices() {
  const output = capture(adb, ['devices']);
  return output
    .split('\n')
    .slice(1)
    .map(line => line.trim().split(/\s+/))
    .filter(parts => parts.length >= 2 && parts[1] === 'device')
    .map(parts => parts[0]);
}

function chooseBootedDevice() {
  if (process.env.MAESTRO_DEVICE_UDID) return process.env.MAESTRO_DEVICE_UDID;
  if (process.env.ANDROID_SERIAL) return process.env.ANDROID_SERIAL;
  return listDevices()[0] ?? null;
}

function chooseAvd() {
  if (process.env.SUPRAMARK_RN_E2E_AVD) return process.env.SUPRAMARK_RN_E2E_AVD;
  const avds = capture(getEmulator(), ['-list-avds'])
    .split('\n')
    .map(line => line.trim())
    .filter(line => /^[A-Za-z0-9_.-]+$/.test(line));
  if (avds.length === 0) throw new Error('No Android emulator or AVD found.');
  return avds[0];
}

function waitForBoot(serial, timeoutMs = 240000) {
  const started = Date.now();
  run(adb, adbArgs(serial, ['wait-for-device']));
  while (Date.now() - started < timeoutMs) {
    const booted = spawnSync(adb, adbArgs(serial, ['shell', 'getprop', 'sys.boot_completed']), {
      env: e2eEnv,
      encoding: 'utf8',
    });
    if (booted.status === 0 && booted.stdout.trim() === '1') {
      run(adb, adbArgs(serial, ['shell', 'input', 'keyevent', '82']));
      return;
    }
    sleepMs(2000);
  }
  throw new Error(`Timed out waiting for Android device ${serial} to boot.`);
}

function startEmulatorIfNeeded() {
  const existing = chooseBootedDevice();
  if (existing) {
    waitForBoot(existing, 60000);
    return { serial: existing, process: null };
  }

  const avd = chooseAvd();
  const emulator = getEmulator();
  const proc = spawn(
    emulator,
    [
      '-avd',
      avd,
      '-no-window',
      '-no-audio',
      '-no-boot-anim',
      '-gpu',
      'swiftshader_indirect',
      '-no-snapshot-save',
    ],
    {
      cwd: projectDir,
      env: e2eEnv,
      stdio: 'inherit',
    }
  );

  const started = Date.now();
  while (Date.now() - started < 60000) {
    const serial = listDevices().find(device => device.startsWith('emulator-'));
    if (serial) {
      waitForBoot(serial);
      return { serial, process: proc };
    }
    sleepMs(2000);
  }
  throw new Error(`Timed out waiting for AVD ${avd} to appear in adb devices.`);
}

function prepareArtifacts() {
  mkdirSync(artifactDir, { recursive: true });
  rmSync(gestureStartScreenshot, { force: true });
  rmSync(gestureScreenshot, { force: true });
  rmSync(cjkScreenshot, { force: true });
  rmSync(flatListScreenshot, { force: true });
}

function bundleAndroidApp() {
  mkdirSync(androidAssetsDir, { recursive: true });
  run('bunx', [
    'expo',
    'export:embed',
    '--entry-file',
    androidEntryFile,
    '--platform',
    'android',
    '--dev',
    'false',
    '--minify',
    'false',
    '--bundle-output',
    androidBundleFile,
    '--assets-dest',
    androidResDir,
  ]);
}

function waitForBundle(url, timeoutMs = 120000) {
  const started = Date.now();
  return new Promise((resolve, reject) => {
    const probe = () => {
      const req = http.get(url, res => {
        res.resume();
        if (res.statusCode && res.statusCode >= 200 && res.statusCode < 300) {
          resolve();
          return;
        }
        retry();
      });
      req.on('error', retry);
      req.setTimeout(5000, () => {
        req.destroy();
        retry();
      });
    };

    const retry = () => {
      if (Date.now() - started > timeoutMs) {
        reject(new Error(`Timed out waiting for Metro bundle at ${url}`));
        return;
      }
      setTimeout(probe, 1000);
    };

    probe();
  });
}

function adbReverse(serial) {
  run(adb, adbArgs(serial, ['reverse', `tcp:${port}`, `tcp:${port}`]));
  if (port !== '8081') {
    run(adb, adbArgs(serial, ['reverse', 'tcp:8081', `tcp:${port}`]));
  }
}

function screenSize(serial) {
  const out = capture(adb, adbArgs(serial, ['shell', 'wm', 'size']));
  const match =
    out.match(/Physical size:\s*(\d+)x(\d+)/) ?? out.match(/Override size:\s*(\d+)x(\d+)/);
  if (!match) throw new Error(`Could not read Android screen size from: ${out.trim()}`);
  return { w: Number(match[1]), h: Number(match[2]) };
}

function scalePoint(screen, x, y) {
  return {
    x: Math.round((x * screen.w) / baseScreen.w),
    y: Math.round((y * screen.h) / baseScreen.h),
  };
}

function adbTap(serial, screen, x, y) {
  const p = scalePoint(screen, x, y);
  run(adb, adbArgs(serial, ['shell', 'input', 'touchscreen', 'tap', String(p.x), String(p.y)]));
}

function adbSwipe(serial, screen, start, end, durationMs) {
  adbSwipeRaw(
    serial,
    scalePoint(screen, start.x, start.y),
    scalePoint(screen, end.x, end.y),
    durationMs
  );
}

function adbSwipeRaw(serial, start, end, durationMs) {
  run(
    adb,
    adbArgs(serial, [
      'shell',
      'input',
      'touchscreen',
      'swipe',
      String(Math.round(start.x)),
      String(Math.round(start.y)),
      String(Math.round(end.x)),
      String(Math.round(end.y)),
      String(durationMs),
    ])
  );
}

function adbLongPress(serial, screen, x, y, durationMs = 900) {
  const point = scalePoint(screen, x, y);
  adbSwipeRaw(serial, point, { x: point.x + 1, y: point.y }, durationMs);
}

function adbDragRaw(serial, start, end, durationMs = 900) {
  adbSwipeRaw(serial, start, end, durationMs);
}

function dumpUi(serial) {
  return capture(adb, adbArgs(serial, ['exec-out', 'uiautomator', 'dump', '/dev/tty']));
}

function nodeWithText(xml, text) {
  const escaped = escapeXmlAttribute(text);
  for (const match of xml.matchAll(/<node\b[^>]*>/g)) {
    const node = match[0];
    if (node.includes(`text="${escaped}"`) || node.includes(`content-desc="${escaped}"`)) {
      return node;
    }
  }
  return null;
}

function nodeBounds(node) {
  const match = node.match(/bounds="\[(\d+),(\d+)]\[(\d+),(\d+)]"/);
  if (!match) return null;
  const [, left, top, right, bottom] = match.map(Number);
  return { left, top, right, bottom };
}

function waitForUiText(serial, text, timeoutMs = 20000) {
  const started = Date.now();
  let latest = '';
  while (Date.now() - started < timeoutMs) {
    latest = dumpUi(serial);
    if (nodeWithText(latest, text) !== null) return latest;
    sleepMs(500);
  }
  const name = sanitizeArtifactName(text);
  const hierarchyFile = path.join(artifactDir, `android-ui-${name}.xml`);
  const screenshotFile = path.join(artifactDir, `android-ui-${name}.png`);
  const logcatFile = path.join(artifactDir, `android-ui-${name}.log`);
  writeFileSync(hierarchyFile, latest);
  takeScreenshot(serial, screenshotFile);
  captureLogcat(serial, logcatFile);
  throw new Error(`Timed out waiting for Android UI text: ${text}`);
}

function waitForUiTextMaybe(serial, text, timeoutMs = 5000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (nodeWithText(dumpUi(serial), text) !== null) return true;
    sleepMs(500);
  }
  return false;
}

function selectedUnitsFromUi(xml) {
  const match = xml.match(/(?:text|content-desc)="selected (\d+) units"/);
  return match ? Number(match[1]) : null;
}

function flatOffsetFromUi(xml) {
  const match = xml.match(/(?:text|content-desc)="flat offset (\d+)"/);
  return match ? Number(match[1]) : null;
}

function waitForSelectedUnits(serial, minUnits, timeoutMs = 20000) {
  const started = Date.now();
  let latest = '';
  while (Date.now() - started < timeoutMs) {
    latest = dumpUi(serial);
    const units = selectedUnitsFromUi(latest);
    if (units !== null && units >= minUnits) return units;
    sleepMs(500);
  }
  const name = `selected-at-least-${minUnits}-units`;
  const hierarchyFile = path.join(artifactDir, `android-ui-${name}.xml`);
  const screenshotFile = path.join(artifactDir, `android-ui-${name}.png`);
  const logcatFile = path.join(artifactDir, `android-ui-${name}.log`);
  writeFileSync(hierarchyFile, latest);
  takeScreenshot(serial, screenshotFile);
  captureLogcat(serial, logcatFile);
  throw new Error(`Timed out waiting for Android selected units >= ${minUnits}`);
}

function waitForFlatOffset(serial, minOffset, timeoutMs = 20000) {
  const started = Date.now();
  let latest = '';
  while (Date.now() - started < timeoutMs) {
    latest = dumpUi(serial);
    const offset = flatOffsetFromUi(latest);
    if (offset !== null && offset >= minOffset) return offset;
    sleepMs(500);
  }
  const name = `flat-offset-at-least-${minOffset}`;
  const hierarchyFile = path.join(artifactDir, `android-ui-${name}.xml`);
  const screenshotFile = path.join(artifactDir, `android-ui-${name}.png`);
  const logcatFile = path.join(artifactDir, `android-ui-${name}.log`);
  writeFileSync(hierarchyFile, latest);
  takeScreenshot(serial, screenshotFile);
  captureLogcat(serial, logcatFile);
  throw new Error(`Timed out waiting for Android FlatList offset >= ${minOffset}`);
}

function boundsForText(serial, text) {
  const xml = waitForUiText(serial, text);
  const node = nodeWithText(xml, text);
  const bounds = node === null ? null : nodeBounds(node);
  if (bounds === null) throw new Error(`Could not find bounds for Android UI text: ${text}`);
  return bounds;
}

function tapText(serial, screen, text) {
  const bounds = boundsForText(serial, text);
  adbTap(serial, screen, (bounds.left + bounds.right) / 2, (bounds.top + bounds.bottom) / 2);
}

function launchApp(serial) {
  run(adb, adbArgs(serial, ['shell', 'am', 'force-stop', appId]));
  run(
    adb,
    adbArgs(serial, [
      'shell',
      'am',
      'start',
      '-W',
      '-n',
      activityName,
      '--es',
      'supramarkBundleURL',
      bundleURL,
    ])
  );
  waitForUiText(serial, 'Selection Demo', 60000);
  sleepMs(1500);
}

function takeScreenshot(serial, file) {
  const result = spawnSync(adb, adbArgs(serial, ['exec-out', 'screencap', '-p']), {
    env: e2eEnv,
    encoding: 'buffer',
    maxBuffer: 50 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`adb screencap failed with status ${result.status}`);
  }
  writeFileSync(file, result.stdout);
}

function captureLogcat(serial, file) {
  const result = spawnSync(
    adb,
    adbArgs(serial, ['logcat', '-d', '-t', '400', 'ReactNativeJS:I', '*:S']),
    {
      env: e2eEnv,
      encoding: 'utf8',
      maxBuffer: 10 * 1024 * 1024,
    }
  );
  if (result.status === 0) writeFileSync(file, result.stdout);
}

function assertScreenshot(mode, file) {
  run(process.execPath, [visualAssertScript, mode, file], { env: process.env });
}

function readPng(file) {
  const png = PNG.sync.read(readFileSync(file));
  return { width: png.width, height: png.height, data: png.data };
}

function pixelAt(image, index) {
  const offset = index * 4;
  return {
    r: image.data[offset],
    g: image.data[offset + 1],
    b: image.data[offset + 2],
    a: image.data[offset + 3],
  };
}

function isHandleBlue({ r, g, b, a }) {
  return a > 200 && r >= 20 && r <= 90 && g >= 115 && g <= 190 && b >= 215;
}

function isHighlightBlue({ r, g, b, a }) {
  return a > 200 && r >= 135 && r <= 230 && g >= 180 && g <= 245 && b >= 225 && b - r >= 25;
}

function findComponents(image, predicate) {
  const total = image.width * image.height;
  const mask = new Uint8Array(total);
  for (let i = 0; i < total; i += 1) {
    if (predicate(pixelAt(image, i), i)) mask[i] = 1;
  }

  const components = [];
  const stack = new Int32Array(total);

  for (let i = 0; i < total; i += 1) {
    if (mask[i] === 0) continue;
    let top = 0;
    let area = 0;
    let x0 = image.width;
    let y0 = image.height;
    let x1 = 0;
    let y1 = 0;

    mask[i] = 0;
    stack[top] = i;
    top += 1;

    while (top > 0) {
      top -= 1;
      const at = stack[top];
      area += 1;

      const x = at % image.width;
      const y = Math.floor(at / image.width);
      if (x < x0) x0 = x;
      if (x > x1) x1 = x;
      if (y < y0) y0 = y;
      if (y > y1) y1 = y;

      const left = at - 1;
      const right = at + 1;
      const up = at - image.width;
      const down = at + image.width;

      if (x > 0 && mask[left] === 1) {
        mask[left] = 0;
        stack[top] = left;
        top += 1;
      }
      if (x < image.width - 1 && mask[right] === 1) {
        mask[right] = 0;
        stack[top] = right;
        top += 1;
      }
      if (y > 0 && mask[up] === 1) {
        mask[up] = 0;
        stack[top] = up;
        top += 1;
      }
      if (y < image.height - 1 && mask[down] === 1) {
        mask[down] = 0;
        stack[top] = down;
        top += 1;
      }
    }

    components.push({ area, x0, y0, x1, y1 });
  }

  return components.sort((a, b) => b.area - a.area);
}

function selectionHandles(file) {
  const image = readPng(file);
  const handles = findComponents(image, isHandleBlue).filter(
    component => component.area > 80 && boxWidth(component) >= 4 && boxHeight(component) >= 4
  );
  if (handles.length < 2) {
    throw new Error(
      `Expected at least two handle components in ${file}, found ${handles
        .map(summarizeBox)
        .join(' ')}`
    );
  }
  return handles.slice(0, 4);
}

function findEndHandlePoint(file) {
  const visible = selectionHandles(file).sort(
    (a, b) => b.x1 - a.x1 || b.y1 - a.y1 || b.area - a.area
  );
  const handle = visible[0];
  const fixed = [...visible].sort((a, b) => a.y0 - b.y0 || a.x0 - b.x0)[0];
  const point = endKnobCenter(handle);
  console.log(
    `[selection-e2e-android] dragging detected end handle ${summarizeBox(
      handle
    )} from ${point.x},${point.y}; candidates ${visible.map(summarizeBox).join(' ')}`
  );
  return { point, fixed };
}

function assertFixedHandleStayedPut(file, expected) {
  const nearest = selectionHandles(file)
    .map(handle => ({
      handle,
      distance: Math.hypot(handle.x0 - expected.x0, handle.y0 - expected.y0),
    }))
    .sort((a, b) => a.distance - b.distance)[0];
  if (nearest.distance > 24) {
    throw new Error(
      `Fixed selection handle drifted: before ${summarizeBox(expected)}, after ${summarizeBox(
        nearest.handle
      )}, distance ${nearest.distance.toFixed(1)}px`
    );
  }
}

function assertHighlightTracksText(file, bounds, label) {
  const image = readPng(file);
  const highlights = findComponents(image, isHighlightBlue).filter(
    component => component.area > 150 && boxWidth(component) > 12 && boxHeight(component) > 8
  );
  if (highlights.length === 0) {
    throw new Error(`Expected visible highlight near ${label} in ${file}`);
  }
  const highlight = highlights[0];
  const highlightCenterY = (highlight.y0 + highlight.y1) / 2;
  const textCenterY = (bounds.top + bounds.bottom) / 2;
  const verticalDrift = Math.abs(highlightCenterY - textCenterY);
  if (verticalDrift > Math.max(40, (bounds.bottom - bounds.top) * 1.2)) {
    throw new Error(
      `Selection highlight drifted away from ${label}: highlight ${summarizeBox(
        highlight
      )}, text [${bounds.left},${bounds.top}][${bounds.right},${bounds.bottom}]`
    );
  }
}

function runGestureFlow(serial, screen) {
  launchApp(serial);
  for (let attempt = 0; attempt < 3; attempt += 1) {
    adbLongPress(serial, screen, 151, 240, 1200);
    if (waitForUiTextMaybe(serial, 'selected 1 units', 8000)) break;
  }
  waitForUiText(serial, 'selected 1 units');
  waitForUiText(serial, 'Copy');
  takeScreenshot(serial, gestureStartScreenshot);
  const { point: dragStart, fixed: fixedHandle } = findEndHandlePoint(gestureStartScreenshot);
  const xml = waitForUiText(serial, 'Second paragraph for range selection.');
  const targetNode = nodeWithText(xml, 'Second paragraph for range selection.');
  const target = targetNode === null ? null : nodeBounds(targetNode);
  if (target === null) throw new Error('Could not find bounds for Android drag target text');
  adbDragRaw(
    serial,
    dragStart,
    {
      x: Math.round(target.right - boxWidth({ x0: target.left, x1: target.right - 1 }) * 0.08),
      y: Math.round((target.top + target.bottom) / 2),
    },
    900
  );
  waitForSelectedUnits(serial, 2);
  waitForUiText(serial, 'page offset 0');
  takeScreenshot(serial, gestureScreenshot);
  assertFixedHandleStayedPut(gestureScreenshot, fixedHandle);
  assertScreenshot('gesture', gestureScreenshot);
}

function runCjkFlow(serial, screen) {
  launchApp(serial);
  tapText(serial, screen, 'Select CJK half');
  waitForUiText(serial, 'cjk half');
  takeScreenshot(serial, cjkScreenshot);
  assertScreenshot('cjk', cjkScreenshot);
}

function runGapFlow(serial, screen) {
  launchApp(serial);
  tapText(serial, screen, 'Select in block');
  waitForUiText(serial, 'selected 2 units');
  adbLongPress(serial, screen, 900, 300);
  waitForUiText(serial, 'idle 0 units');
}

function runFlatListFlow(serial, screen) {
  launchApp(serial);
  sleepMs(1000);
  waitForUiText(serial, 'flat row 03 measured');
  const rowText = 'Flat row 03 selection target.';
  let rowBounds = boundsForText(serial, rowText);
  const press = {
    x: Math.round((rowBounds.left + rowBounds.right) / 2),
    y: Math.round((rowBounds.top + rowBounds.bottom) / 2),
  };
  adbSwipeRaw(serial, press, { x: press.x + 1, y: press.y }, 1200);
  waitForUiText(serial, 'selected 1 units');
  waitForUiText(serial, 'Copy');

  // Start inside an actual row rather than the FlatList's blank cross-axis
  // space. Android's same-axis nested scrolling routes that stream to the
  // child reliably; keep it shorter than the long-press threshold as a real
  // scroll gesture would be.
  const scrollX = Math.round((rowBounds.left + rowBounds.right) / 2);
  const scrollStartY = Math.min(screen.h - 200, rowBounds.bottom + 100);
  adbDragRaw(
    serial,
    { x: scrollX, y: scrollStartY },
    { x: scrollX, y: Math.max(0, scrollStartY - 140) },
    300
  );
  waitForFlatOffset(serial, 40);
  waitForUiText(serial, 'page offset 0');
  sleepMs(750);
  rowBounds = boundsForText(serial, rowText);
  takeScreenshot(serial, flatListScreenshot);
  assertHighlightTracksText(flatListScreenshot, rowBounds, rowText);

  adbDragRaw(
    serial,
    { x: scrollX, y: scrollStartY },
    { x: scrollX, y: Math.max(0, scrollStartY - 420) },
    300
  );
  waitForUiText(serial, 'visible selection rects 0');
  waitForUiText(serial, 'selected 1 units');
  // The toolbar is intentionally clipped with the offscreen selection. Clear
  // through the always-visible fixture control and verify the range can still
  // be dismissed without bringing leaked overlay UI back onscreen.
  tapText(serial, screen, 'Clear');
  waitForUiText(serial, 'idle 0 units');
}

function runScrollFlow(serial, screen) {
  launchApp(serial);
  for (let i = 0; i < 3; i += 1) {
    adbSwipe(serial, screen, { x: 540, y: 1900 }, { x: 540, y: 450 }, 800);
    sleepMs(300);
  }
  waitForUiText(serial, 'Scroll sentinel');
}

async function main() {
  if (dirty(generatedAndroidPaths)) {
    throw new Error(
      `Refusing to run with local changes in generated Android files: ${generatedAndroidPaths.join(
        ', '
      )}`
    );
  }

  prepareArtifacts();
  const started = startEmulatorIfNeeded();
  const serial = started.serial;
  const screen = screenSize(serial);
  adbReverse(serial);

  let metro;
  try {
    bundleAndroidApp();

    metro = spawn('bunx', ['expo', 'start', '--port', port, '--dev-client'], {
      cwd: projectDir,
      env: e2eEnv,
      stdio: 'inherit',
    });
    await waitForBundle(bundleURL);

    run('bunx', ['expo', 'run:android', '--no-bundler'], {
      env: { ...e2eEnv, ANDROID_SERIAL: serial },
    });
    adbReverse(serial);

    runGestureFlow(serial, screen);
    runCjkFlow(serial, screen);
    runGapFlow(serial, screen);
    runFlatListFlow(serial, screen);
    runScrollFlow(serial, screen);
  } finally {
    if (metro) metro.kill('SIGINT');
    restoreGeneratedAndroidFiles();
    if (started.process) {
      run(adb, adbArgs(serial, ['emu', 'kill']));
    }
  }
}

main().catch(err => {
  console.error(`[selection-e2e-android] ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
});
