import { describe, expect, it } from 'bun:test';
import { resolveGraphvizAnywhereRnExports } from '../src/graphviz';

// `@kookyleo/graphviz-anywhere-rn` ships a CommonJS main carrying both named
// exports and a default export. Metro's `await import(...)` may or may not
// hoist the named exports, so the adapter has to tolerate both namespace
// shapes. These tests pin that resolution down without a real RN runtime.

const renderDot = async (_dot: string, _engine: string, _format: string) => '<svg></svg>';
const getVersion = async () => '12.2.1';

describe('resolveGraphvizAnywhereRnExports', () => {
  it('reads named exports hoisted onto the namespace (ESM-style)', () => {
    const resolved = resolveGraphvizAnywhereRnExports({ renderDot, getVersion });
    expect(resolved.renderDot).toBe(renderDot);
    expect(resolved.getVersion).toBe(getVersion);
  });

  it('reaches through `default` when Metro does not hoist named exports', () => {
    const resolved = resolveGraphvizAnywhereRnExports({
      default: { renderDot, getVersion },
    });
    expect(resolved.renderDot).toBe(renderDot);
    expect(resolved.getVersion).toBe(getVersion);
  });

  it('prefers the named export when both shapes are present', () => {
    const defaultRenderDot = async () => '<svg data-from="default"></svg>';
    const resolved = resolveGraphvizAnywhereRnExports({
      renderDot,
      default: { renderDot: defaultRenderDot },
    });
    expect(resolved.renderDot).toBe(renderDot);
  });

  it('returns getVersion undefined when neither shape provides it', () => {
    const resolved = resolveGraphvizAnywhereRnExports({ renderDot });
    expect(resolved.renderDot).toBe(renderDot);
    expect(resolved.getVersion).toBeUndefined();
  });

  it('throws a clear error when renderDot is absent from both shapes', () => {
    expect(() => resolveGraphvizAnywhereRnExports({ default: { getVersion } })).toThrow(
      /renderDot/
    );
    expect(() => resolveGraphvizAnywhereRnExports({})).toThrow(/renderDot/);
    expect(() => resolveGraphvizAnywhereRnExports(null)).toThrow(/renderDot/);
  });
});
