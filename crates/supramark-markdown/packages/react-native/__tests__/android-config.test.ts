import { describe, expect, it } from 'bun:test';
import { readFileSync } from 'node:fs';

const buildGradle = readFileSync(new URL('../android/build.gradle', import.meta.url), 'utf8');

describe('Android ABI configuration', () => {
  it('honors reactNativeArchitectures while preserving the four-ABI fallback', () => {
    expect(buildGradle).toContain('rootProject.findProperty("reactNativeArchitectures")');
    expect(buildGradle).toContain(
      'abiFilters.addAll(nativeArchitectures(["arm64-v8a", "armeabi-v7a", "x86_64", "x86"]))'
    );
    expect(buildGradle).not.toMatch(/abiFilters\s+["']/);
  });
});
