import { describe, expect, it } from 'bun:test';
import { createDiagramEngine } from '../src/engine';

// Issue #162: `engineConfig.timeoutMs` is forwarded as `options.timeout`, but
// no engine consumed it — a hung wasm render left the whole document's
// `Promise.all` pending forever. These tests guard that the engine now races
// the render against the configured timeout and surfaces a `render_error`
// instead of hanging.

const never = () => new Promise<string>(() => {});

describe('engine render timeout (options.timeout)', () => {
  it('returns render_timeout when an engine hangs past options.timeout', async () => {
    const engine = createDiagramEngine({
      d2: { render: never },
    });

    const result = await engine.render({
      engine: 'd2',
      code: 'a -> b',
      options: { timeout: 50 },
    });

    expect(result.success).toBe(false);
    expect(result.format).toBe('error');
    expect(result.error?.code).toBe('render_timeout');
    expect(result.payload).toMatch(/timed out/i);
    expect(result.error?.message).toBe('d2 render timed out');
    expect(result.engine).toBe('d2');
  });

  it('honors timeoutMs as an alias for timeout', async () => {
    const engine = createDiagramEngine({
      d2: { render: never },
    });

    const result = await engine.render({
      engine: 'd2',
      code: 'a -> b',
      options: { timeoutMs: 50 },
    });

    expect(result.success).toBe(false);
    expect(result.error?.code).toBe('render_timeout');
  });

  it('still returns svg when render completes before options.timeout', async () => {
    const engine = createDiagramEngine({
      d2: { render: async () => '<svg></svg>' },
    });

    const result = await engine.render({
      engine: 'd2',
      code: 'a -> b',
      options: { timeout: 1000 },
    });

    expect(result.success).toBe(true);
    expect(result.format).toBe('svg');
    expect(result.payload).toContain('<svg');
  });

  it('does not apply a timeout when none is configured (a slow render resolves normally)', async () => {
    // A render that takes ~30ms must resolve when no timeout is configured —
    // discriminating against a hypothetical regression that always races.
    const slow = () =>
      new Promise<string>(resolve => setTimeout(() => resolve('<svg></svg>'), 30));
    const engine = createDiagramEngine({ d2: { render: slow } });

    const result = await engine.render({ engine: 'd2', code: 'a -> b' });

    expect(result.success).toBe(true);
    expect(result.format).toBe('svg');
    expect(result.payload).toContain('<svg');
  });

  it('ignores non-positive timeout values even against a slow render', async () => {
    // A ~30ms render distinguishes "non-positive values are ignored" from
    // "values are handed straight to setTimeout" (which would fire instantly
    // for 0 and reject before the render resolves).
    const slow = () =>
      new Promise<string>(resolve => setTimeout(() => resolve('<svg></svg>'), 30));
    const engine = createDiagramEngine({ d2: { render: slow } });

    for (const timeout of [0, -1, NaN]) {
      const result = await engine.render({
        engine: 'd2',
        code: 'a -> b',
        options: { timeout },
      });
      expect(result.success).toBe(true);
      expect(result.format).toBe('svg');
    }
  });

  it('clears the timer on the success path (no late unhandled rejection)', async () => {
    // On a fast successful render, `Promise.race` settles as fulfilled. If
    // the timer were not cleared in `finally`, it would later fire
    // `reject(new RenderTimeoutError)` against an already-settled race —
    // surfacing as an unhandledRejection. We assert none surfaces.
    const fast = async () => '<svg></svg>';
    const engine = createDiagramEngine({ d2: { render: fast } });

    const rejections: unknown[] = [];
    const handler = (reason: unknown) => rejections.push(reason);
    process.on('unhandledRejection', handler);

    const result = await engine.render({
      engine: 'd2',
      code: 'a -> b',
      options: { timeout: 20 },
    });

    expect(result.success).toBe(true);
    // Wait well past the timer deadline so a leaked timer would have fired.
    await new Promise(resolve => setTimeout(resolve, 50));

    process.off('unhandledRejection', handler);
    expect(rejections).toEqual([]);
  });

  it('does not raise an unhandled rejection when the abandoned work rejects late', async () => {
    // On timeout, `work` is left pending. If it later rejects, the
    // Promise.race reaction is a no-op and must not become an
    // unhandledRejection. We instrument process.on('unhandledRejection').
    const rejections: unknown[] = [];
    const handler = (reason: unknown) => rejections.push(reason);
    process.on('unhandledRejection', handler);

    let rejectWork: ((err: unknown) => void) | null = null;
    const hangThenReject = () =>
      new Promise<string>((_, reject) => {
        rejectWork = reject;
      });
    const engine = createDiagramEngine({ d2: { render: hangThenReject } });

    const result = await engine.render({
      engine: 'd2',
      code: 'a -> b',
      options: { timeout: 20 },
    });

    expect(result.error?.code).toBe('render_timeout');

    // Reject the abandoned work after the race has settled.
    await new Promise(resolve => setTimeout(resolve, 40));
    rejectWork?.(new Error('late work failure'));
    // Let the microtask drain so the rejection reaction (if any) runs.
    await new Promise(resolve => setTimeout(resolve, 10));

    process.off('unhandledRejection', handler);
    expect(rejections).toEqual([]);
  });

  it('treats a thrown render error normally even with a timeout set', async () => {
    const engine = createDiagramEngine({
      d2: { render: async () => {
        throw new Error('boom');
      } },
    });

    const result = await engine.render({
      engine: 'd2',
      code: 'a -> b',
      options: { timeout: 1000 },
    });

    expect(result.success).toBe(false);
    expect(result.error?.code).toBe('render_error');
    expect(result.payload).toBe('boom');
  });
});
