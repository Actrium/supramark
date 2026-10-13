'use strict';
/*
 * windows-staging.js — Windows artifact staging logic for prepare-native.js.
 *
 * build-windows.sh cross-compiles one architecture at a time and installs to
 * `<crate>/output/windows-<arch>/` (`windows-x86_64`, `windows-arm64`). This
 * module stages those trees into the package layout consumed by the RNW
 * project (windows/SupramarkPlantumlNativeWinRT/SupramarkPlantumlNativeWinRT.vcxproj):
 *
 *   windows/Frameworks/
 *     include/<abi-header>            (architecture-independent C ABI header)
 *     x64/{bin,lib}/...               (from output/windows-x86_64)
 *     arm64/{bin,lib}/...             (from output/windows-arm64)
 *
 * The per-architecture directories let one installed package serve every
 * platform an RNW app builds ($(Platform)-conditioned includes/libs in the
 * vcxproj). A run stages every architecture whose artifacts exist, or only
 * the explicitly requested ones; staging never fails just because a second
 * architecture was not built.
 *
 * Pure path/filesystem logic (no repo-relative constants) so it can be unit
 * tested with isolated fixtures — see __tests__/prepare-native.windows.test.ts.
 */

const fs = require('fs');
const path = require('path');

// Staged directory name -> build-windows.sh output directory name.
const WINDOWS_ARCHES = [
  { name: 'x64', outputDir: 'windows-x86_64' },
  { name: 'arm64', outputDir: 'windows-arm64' },
];

function fileExists(p) {
  try {
    fs.accessSync(p);
    return true;
  } catch {
    return false;
  }
}

function isKnownArch(name) {
  return WINDOWS_ARCHES.some((a) => a.name === name);
}

/**
 * Parse a `--windows-arch` value ("x64", "arm64" or "x64,arm64").
 * Throws on an unknown architecture name.
 */
function parseArchSelection(value) {
  const arches = String(value)
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  if (arches.length === 0) {
    throw new Error(`--windows-arch needs at least one of: ${WINDOWS_ARCHES.map((a) => a.name).join(', ')}`);
  }
  for (const arch of arches) {
    if (!isKnownArch(arch)) {
      throw new Error(
        `Unknown --windows-arch '${arch}' (expected one of: ${WINDOWS_ARCHES.map((a) => a.name).join(', ')})`
      );
    }
  }
  return [...new Set(arches)];
}

/**
 * Which architectures have built artifacts under `outputDir`.
 * An architecture counts as present when its runtime DLL exists.
 */
function listAvailableArches(outputDir, dllName) {
  return WINDOWS_ARCHES.filter((a) => fileExists(path.join(outputDir, a.outputDir, 'bin', dllName))).map(
    (a) => a.name
  );
}

/**
 * Stage Windows artifacts into `frameworksDest`.
 *
 * @param {object} opts
 * @param {string} opts.outputDir      crate `output/` directory (contains windows-<arch>/ trees)
 * @param {string} opts.frameworksDest package `windows/Frameworks` directory
 * @param {string} opts.dllName        runtime DLL file name
 * @param {string} opts.libName        import library file name
 * @param {string} opts.headerName     C ABI header file name
 * @param {string} [opts.headerFallbackDir] directory to take the header from when
 *        the built tree does not ship one (`packages/native/include`); optional
 * @param {string[]|null} [opts.requestedArches] architectures to stage (names from
 *        WINDOWS_ARCHES). `null`/omitted = every architecture present in `outputDir`.
 * @returns {{staged: string[], missing: string[], headerStaged: boolean}}
 *        `staged` — architectures written; `missing` — requested architectures whose
 *        artifacts are absent; `headerStaged` — whether the C ABI header was copied.
 *
 * On success (at least one architecture staged) `frameworksDest` is rebuilt from
 * scratch, so the staged layout is exactly the requested set. When nothing can be
 * staged, the existing `frameworksDest` is left untouched.
 */
function stageWindowsFrameworks(opts) {
  const { outputDir, frameworksDest, dllName, libName, headerName, headerFallbackDir } = opts;
  const requested = opts.requestedArches == null ? null : opts.requestedArches;

  const available = listAvailableArches(outputDir, dllName);
  const toStage = (requested == null ? WINDOWS_ARCHES.map((a) => a.name) : requested).filter((name) =>
    available.includes(name)
  );
  const missing = (requested == null ? [] : requested).filter((name) => !available.includes(name));

  if (toStage.length === 0) {
    return { staged: [], missing, headerStaged: false };
  }

  fs.rmSync(frameworksDest, { recursive: true, force: true });

  let headerStaged = false;
  for (const name of toStage) {
    const arch = WINDOWS_ARCHES.find((a) => a.name === name);
    const srcDir = path.join(outputDir, arch.outputDir);
    const destDir = path.join(frameworksDest, name);

    fs.mkdirSync(path.join(destDir, 'bin'), { recursive: true });
    fs.mkdirSync(path.join(destDir, 'lib'), { recursive: true });
    fs.copyFileSync(path.join(srcDir, 'bin', dllName), path.join(destDir, 'bin', dllName));

    const libSrc = path.join(srcDir, 'lib', libName);
    if (fileExists(libSrc)) {
      fs.copyFileSync(libSrc, path.join(destDir, 'lib', libName));
    }

    if (!headerStaged) {
      const headerSrc = path.join(srcDir, 'include', headerName);
      const fallback = headerFallbackDir ? path.join(headerFallbackDir, headerName) : null;
      const headerFrom =
        fileExists(headerSrc) ? headerSrc : fallback && fileExists(fallback) ? fallback : null;
      if (headerFrom) {
        const headerDest = path.join(frameworksDest, 'include');
        fs.mkdirSync(headerDest, { recursive: true });
        fs.copyFileSync(headerFrom, path.join(headerDest, headerName));
        headerStaged = true;
      }
    }
  }

  return { staged: toStage, missing, headerStaged };
}

module.exports = {
  WINDOWS_ARCHES,
  parseArchSelection,
  listAvailableArches,
  stageWindowsFrameworks,
};
