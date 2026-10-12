#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

const REPO_ROOT = path.resolve(__dirname, '..');
const REQUIRED_ARCHITECTURES = new Set([0x01000007, 0x0100000c]);

// These entries define the complete macOS payload each published RN package
// must contain before npm is allowed to create its tarball.
const NATIVE_PACKAGES = [
  {
    root: 'crates/supramark-markdown/packages/react-native',
    dylib: 'libsupramark_markdown_native.dylib',
    header: 'supramark_markdown.h',
    podspec: 'SupramarkMarkdownNative.podspec',
  },
  {
    root: 'crates/d2-little/packages/react-native',
    dylib: 'libsupramark_d2_native.dylib',
    header: 'supramark_d2.h',
    podspec: 'SupramarkD2Native.podspec',
  },
  {
    root: 'crates/mermaid-little/packages/react-native',
    dylib: 'libsupramark_mermaid_native.dylib',
    header: 'supramark_mermaid.h',
    podspec: 'SupramarkMermaidNative.podspec',
  },
  {
    root: 'crates/plantuml-little/packages/react-native',
    dylib: 'libsupramark_plantuml_native.dylib',
    header: 'supramark_plantuml.h',
    podspec: 'SupramarkPlantumlNative.podspec',
  },
  {
    root: 'crates/graphviz-anywhere/packages/react-native',
    dylib: 'libgraphviz_api.dylib',
    header: 'graphviz_api.h',
    podspec: 'GraphvizNative.podspec',
  },
];

// Parse the Mach-O fat header directly so package verification also works on
// Linux release runners where Apple's `lipo` command is unavailable.
function readFatSlices(binaryPath) {
  const binary = fs.readFileSync(binaryPath);
  if (binary.length < 8) {
    throw new Error('file is too small to contain a Mach-O fat header');
  }

  const magic = binary.readUInt32BE(0);
  const isFat32 = magic === 0xcafebabe;
  const isFat64 = magic === 0xcafebabf;
  if (!isFat32 && !isFat64) {
    throw new Error('binary is not a universal Mach-O file');
  }

  const architectureCount = binary.readUInt32BE(4);
  const entrySize = isFat64 ? 32 : 20;
  const requiredBytes = 8 + architectureCount * entrySize;
  if (binary.length < requiredBytes) {
    throw new Error('Mach-O fat header is truncated');
  }

  const slices = [];
  for (let index = 0; index < architectureCount; index += 1) {
    const entryOffset = 8 + index * entrySize;
    const cpuType = binary.readUInt32BE(entryOffset);
    const sliceOffset = isFat64
      ? Number(binary.readBigUInt64BE(entryOffset + 8))
      : binary.readUInt32BE(entryOffset + 8);
    const sliceSize = isFat64
      ? Number(binary.readBigUInt64BE(entryOffset + 16))
      : binary.readUInt32BE(entryOffset + 12);
    if (sliceOffset + sliceSize > binary.length) {
      throw new Error('Mach-O slice extends past the end of the file');
    }
    slices.push({ cpuType, offset: sliceOffset, size: sliceSize });
  }
  return { binary, slices };
}

// Read LC_ID_DYLIB from one 64-bit Mach-O slice and return its install name.
function readDylibInstallName(binary, slice) {
  const headerOffset = slice.offset;
  if (binary.readUInt32LE(headerOffset) !== 0xfeedfacf) {
    throw new Error('Mach-O slice is not a 64-bit little-endian binary');
  }

  const commandCount = binary.readUInt32LE(headerOffset + 16);
  let commandOffset = headerOffset + 32;
  const sliceEnd = headerOffset + slice.size;
  for (let index = 0; index < commandCount; index += 1) {
    if (commandOffset + 8 > sliceEnd) throw new Error('Mach-O load commands are truncated');
    const command = binary.readUInt32LE(commandOffset);
    const commandSize = binary.readUInt32LE(commandOffset + 4);
    if (commandSize < 8 || commandOffset + commandSize > sliceEnd) {
      throw new Error('Mach-O load command has an invalid size');
    }
    if (command === 0x0d) {
      const nameOffset = binary.readUInt32LE(commandOffset + 8);
      const nameStart = commandOffset + nameOffset;
      const nameEndLimit = commandOffset + commandSize;
      if (nameOffset < 24 || nameStart >= nameEndLimit) {
        throw new Error('LC_ID_DYLIB has an invalid name offset');
      }
      const terminator = binary.indexOf(0, nameStart);
      if (terminator === -1 || terminator >= nameEndLimit) {
        throw new Error('LC_ID_DYLIB install name is not terminated');
      }
      return binary.toString('utf8', nameStart, terminator);
    }
    commandOffset += commandSize;
  }
  throw new Error('Mach-O slice has no LC_ID_DYLIB command');
}

// Resolve either the one package invoking prepack or every native package for
// the root-level release verification command.
function selectPackages() {
  const packageRootIndex = process.argv.indexOf('--package-root');
  if (packageRootIndex === -1) return NATIVE_PACKAGES;

  const requestedValue = process.argv[packageRootIndex + 1];
  if (!requestedValue) throw new Error('--package-root requires a path');
  const requestedRoot = path.resolve(process.cwd(), requestedValue);
  const selected = NATIVE_PACKAGES.find(
    entry => path.resolve(REPO_ROOT, entry.root) === requestedRoot
  );
  if (!selected) throw new Error(`unknown native package root: ${requestedRoot}`);
  return [selected];
}

// Validate files, universal architectures, package inclusion and the single
// root-level podspec that React Native autolinking discovers.
function verifyPackage(entry) {
  const packageRoot = path.join(REPO_ROOT, entry.root);
  const dylibPath = path.join(packageRoot, 'macos', 'Frameworks', 'lib', entry.dylib);
  const headerPath = path.join(packageRoot, 'macos', 'Frameworks', 'include', entry.header);
  const podspecPath = path.join(packageRoot, entry.podspec);
  const packageJsonPath = path.join(packageRoot, 'package.json');

  for (const requiredPath of [dylibPath, headerPath, podspecPath, packageJsonPath]) {
    if (!fs.existsSync(requiredPath)) {
      throw new Error(`${entry.root}: missing ${path.relative(packageRoot, requiredPath)}`);
    }
  }

  const { binary, slices } = readFatSlices(dylibPath);
  const architectures = new Set(slices.map(slice => slice.cpuType));
  for (const architecture of REQUIRED_ARCHITECTURES) {
    if (!architectures.has(architecture)) {
      throw new Error(`${entry.root}: ${entry.dylib} is missing arm64 or x86_64`);
    }
  }
  const expectedInstallName = `@rpath/${entry.dylib}`;
  for (const slice of slices) {
    const installName = readDylibInstallName(binary, slice);
    if (installName !== expectedInstallName) {
      throw new Error(
        `${entry.root}: ${entry.dylib} uses non-portable install name ${installName}`
      );
    }
  }

  const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
  if (!Array.isArray(packageJson.files) || !packageJson.files.includes('macos')) {
    throw new Error(`${entry.root}: package.json files must include macos`);
  }

  const podspec = fs.readFileSync(podspecPath, 'utf8');
  const dylibReference = `macos/Frameworks/lib/${entry.dylib}`;
  if (!podspec.includes(dylibReference)) {
    throw new Error(`${entry.root}: ${entry.podspec} does not vend ${dylibReference}`);
  }
  const shipsPodspec =
    packageJson.files.includes('*.podspec') || packageJson.files.includes(entry.podspec);
  if (!shipsPodspec) {
    throw new Error(`${entry.root}: package.json files must include ${entry.podspec}`);
  }

  const legacyPodspecPath = path.join(packageRoot, 'ios', entry.podspec);
  if (fs.existsSync(legacyPodspecPath)) {
    throw new Error(`${entry.root}: duplicate legacy podspec remains at ios/${entry.podspec}`);
  }

  console.log(`OK ${packageJson.name}: macOS arm64 + x86_64 payload verified`);
}

try {
  for (const entry of selectPackages()) verifyPackage(entry);
} catch (error) {
  console.error(`macOS native package verification failed: ${error.message}`);
  process.exit(1);
}
