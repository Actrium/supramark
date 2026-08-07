#!/usr/bin/env node
import { spawn, spawnSync } from 'node:child_process';
import http from 'node:http';
import { existsSync, mkdirSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const projectDir = path.resolve(here, '..');
const maestroDir = path.join(projectDir, 'maestro');
const artifactDir = path.join(maestroDir, 'artifacts');
const gestureFlowFile = path.join(maestroDir, 'selection-ios.yaml');
const cjkFlowFile = path.join(maestroDir, 'selection-cjk-ios.yaml');
const flatListFlowFile = path.join(maestroDir, 'selection-flatlist-ios.yaml');
const scrollFlowFile = path.join(maestroDir, 'selection-scroll-ios.yaml');
const visualAssertScript = path.join(projectDir, 'scripts', 'assert-selection-visual.mjs');
const port = process.env.SUPRAMARK_RN_E2E_PORT ?? '8090';
const bundleURL =
  process.env.SUPRAMARK_RN_E2E_BUNDLE_URL ??
  `http://localhost:${port}/.expo/.virtual-metro-entry.bundle?platform=ios&dev=true&minify=false`;
const screenshotPath = (flowFile, name) =>
  path.join(artifactDir, path.basename(flowFile, '.yaml'), 'takeScreenshot', `${name}.png`);
const gestureScreenshot = screenshotPath(gestureFlowFile, 'selection-gesture');
const cjkScreenshot = screenshotPath(cjkFlowFile, 'selection-cjk-half');
const flatListScreenshot = screenshotPath(flatListFlowFile, 'selection-flatlist');
const generatedFiles = [
  'ios/Podfile.lock',
  'ios/supramarkexamplereactnative.xcodeproj/project.pbxproj',
];

const e2eEnv = {
  ...process.env,
  CI: process.env.CI ?? '1',
  SUPRAMARK_RN_E2E: 'selection',
  EXPO_PUBLIC_SUPRAMARK_RN_E2E: 'selection',
};

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? projectDir,
    env: options.env ?? e2eEnv,
    stdio: options.stdio ?? 'inherit',
    encoding: 'utf8',
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
  const found = spawnSync('which', [command], { encoding: 'utf8' });
  if (found.status === 0 && found.stdout.trim()) return found.stdout.trim();
  if (fallback && existsSync(fallback)) return fallback;
  throw new Error(`${command} was not found`);
}

function dirty(files) {
  const diff = spawnSync('git', ['diff', '--quiet', '--', ...files], { cwd: projectDir });
  const staged = spawnSync('git', ['diff', '--cached', '--quiet', '--', ...files], {
    cwd: projectDir,
  });
  return diff.status !== 0 || staged.status !== 0;
}

function restoreGeneratedFiles() {
  run('git', ['restore', '--', ...generatedFiles], { env: process.env });
}

function prepareArtifacts() {
  rmSync(artifactDir, { recursive: true, force: true });
  mkdirSync(artifactDir, { recursive: true });
}

function runMaestro(maestro, udid, flowFile) {
  run(maestro, [
    '--udid',
    udid,
    'test',
    '-e',
    `BUNDLE_URL=${bundleURL}`,
    '--debug-output',
    artifactDir,
    '--flatten-debug-output',
    flowFile,
  ]);
}

function assertScreenshot(mode, file) {
  run(process.execPath, [visualAssertScript, mode, file], { env: process.env });
}

function chooseUDID() {
  if (process.env.MAESTRO_DEVICE_UDID) return process.env.MAESTRO_DEVICE_UDID;
  if (process.env.SIMULATOR_UDID) return process.env.SIMULATOR_UDID;

  const json = JSON.parse(
    capture('xcrun', ['simctl', 'list', 'devices', 'booted', '--json'], {
      env: process.env,
    })
  );
  for (const devices of Object.values(json.devices ?? {})) {
    for (const device of devices) {
      if (device.isAvailable && device.state === 'Booted') return device.udid;
    }
  }
  throw new Error('No booted iOS simulator found. Boot one or set MAESTRO_DEVICE_UDID.');
}

function warnAboutProxyBypass() {
  if (process.platform !== 'darwin') return;
  const proxy = spawnSync('scutil', ['--proxy'], { encoding: 'utf8' });
  if (proxy.status !== 0) return;
  const text = proxy.stdout;
  const proxyEnabled = /HTTPEnable\s*:\s*1/.test(text) || /HTTPSEnable\s*:\s*1/.test(text);
  if (!proxyEnabled || /localhost/.test(text)) return;
  console.warn(
    '[selection-e2e] macOS proxy is enabled without localhost bypass; the simulator may fail to load Metro.'
  );
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

async function main() {
  if (process.platform !== 'darwin') {
    throw new Error('The iOS selection E2E runner must be run on macOS.');
  }
  if (dirty(generatedFiles)) {
    throw new Error(
      `Refusing to run with local changes in generated iOS files: ${generatedFiles.join(', ')}`
    );
  }

  const maestro = commandPath('maestro', path.join(os.homedir(), '.maestro', 'bin', 'maestro'));
  const udid = chooseUDID();
  prepareArtifacts();
  warnAboutProxyBypass();

  let metro;
  try {
    run('pod', ['install'], { cwd: path.join(projectDir, 'ios') });

    metro = spawn('bunx', ['expo', 'start', '--port', port, '--dev-client'], {
      cwd: projectDir,
      env: e2eEnv,
      stdio: 'inherit',
    });
    await waitForBundle(bundleURL);

    run('bunx', ['expo', 'run:ios', '--device', udid, '--no-bundler']);
    runMaestro(maestro, udid, gestureFlowFile);
    assertScreenshot('gesture', gestureScreenshot);

    runMaestro(maestro, udid, cjkFlowFile);
    assertScreenshot('cjk', cjkScreenshot);

    runMaestro(maestro, udid, flatListFlowFile);
    // A range edge near a viewport boundary may intentionally hide its handle
    // when the full touch target would escape the clip.
    assertScreenshot('viewport', flatListScreenshot);

    runMaestro(maestro, udid, scrollFlowFile);
  } finally {
    if (metro) metro.kill('SIGINT');
    restoreGeneratedFiles();
  }
}

main().catch(err => {
  console.error(`[selection-e2e] ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
});
