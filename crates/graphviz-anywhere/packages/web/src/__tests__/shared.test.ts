import { describe, it, expect } from 'vitest';
import {
  GraphvizWebError,
  normalizeRenderOptions,
  assertEngineSupported,
  assertFormatSupported,
  assertFormatsSupported,
  assertActive,
  snapshotCapabilities,
  issuesToMessage,
  DEFAULT_ENGINE,
  DEFAULT_FORMAT,
} from '../shared';
import { createMockVizWasmInstance } from './mocks/viz-wasm';

describe('GraphvizWebError', () => {
  it('creates error with code and message', () => {
    const error = new GraphvizWebError('RENDER_FAILED', 'Test error');
    expect(error.message).toBe('Test error');
    expect(error.code).toBe('RENDER_FAILED');
    expect(error.name).toBe('GraphvizWebError');
    expect(error.issues).toEqual([]);
  });

  it('creates error with issues', () => {
    const issues = [{ message: 'Issue 1' }, { message: 'Issue 2', level: 'warning' as const }];
    const error = new GraphvizWebError('RENDER_FAILED', 'Test error', { issues });
    expect(error.issues).toEqual(issues);
  });

  it('creates error with cause', () => {
    const cause = new Error('Original error');
    const error = new GraphvizWebError('RENDER_FAILED', 'Wrapped error', { cause });
    expect((error as { cause?: unknown }).cause).toBe(cause);
  });

  it('has correct error codes', () => {
    const codes: Array<ConstructorParameters<typeof GraphvizWebError>[0]> = [
      'UNSUPPORTED_ENGINE',
      'UNSUPPORTED_FORMAT',
      'RENDER_FAILED',
      'WORKER_UNAVAILABLE',
      'TIMEOUT',
      'DISPOSED',
    ];

    for (const code of codes) {
      const error = new GraphvizWebError(code, 'Test');
      expect(error.code).toBe(code);
    }
  });
});

describe('normalizeRenderOptions', () => {
  it('returns empty object with defaults when no options', () => {
    const result = normalizeRenderOptions();
    expect(result.engine).toBe(DEFAULT_ENGINE);
    expect(result.format).toBe(DEFAULT_FORMAT);
  });

  it('uses provided options', () => {
    const result = normalizeRenderOptions({
      engine: 'neato',
      format: 'png',
      yInvert: true,
    });
    expect(result.engine).toBe('neato');
    expect(result.format).toBe('png');
    expect(result.yInvert).toBe(true);
  });

  it('fills in defaults for missing engine and format', () => {
    const result = normalizeRenderOptions({
      yInvert: false,
    });
    expect(result.engine).toBe(DEFAULT_ENGINE);
    expect(result.format).toBe(DEFAULT_FORMAT);
    expect(result.yInvert).toBe(false);
  });

  it('preserves all attributes in options', () => {
    const options = {
      engine: 'dot',
      format: 'svg',
      graphAttributes: { label: 'Test' },
      nodeAttributes: { shape: 'circle' },
      edgeAttributes: { color: 'blue' },
    };
    const result = normalizeRenderOptions(options);
    expect(result.graphAttributes).toEqual(options.graphAttributes);
    expect(result.nodeAttributes).toEqual(options.nodeAttributes);
    expect(result.edgeAttributes).toEqual(options.edgeAttributes);
  });
});

describe('assertEngineSupported', () => {
  it('does not throw for supported engine', () => {
    const viz = createMockVizWasmInstance({ engines: ['dot', 'neato'] });
    expect(() => assertEngineSupported(viz, 'dot')).not.toThrow();
    expect(() => assertEngineSupported(viz, 'neato')).not.toThrow();
  });

  it('does not throw when engine is undefined', () => {
    const viz = createMockVizWasmInstance({ engines: ['dot'] });
    expect(() => assertEngineSupported(viz, undefined)).not.toThrow();
  });

  it('throws for unsupported engine', () => {
    const viz = createMockVizWasmInstance({ engines: ['dot', 'neato'] });
    try {
      assertEngineSupported(viz, 'sfdp');
      expect.fail('Should have thrown');
    } catch (error) {
      if (error instanceof GraphvizWebError) {
        expect(error.code).toBe('UNSUPPORTED_ENGINE');
      }
    }
  });

  it('includes supported engines in error message', () => {
    const viz = createMockVizWasmInstance({ engines: ['dot', 'neato'] });
    try {
      assertEngineSupported(viz, 'invalid');
      expect.fail('Should have thrown');
    } catch (error) {
      if (error instanceof GraphvizWebError) {
        expect(error.message).toContain('dot');
        expect(error.message).toContain('neato');
      }
    }
  });
});

describe('assertFormatSupported', () => {
  it('does not throw for supported format', () => {
    const viz = createMockVizWasmInstance({ formats: ['svg', 'png'] });
    expect(() => assertFormatSupported(viz, 'svg')).not.toThrow();
    expect(() => assertFormatSupported(viz, 'png')).not.toThrow();
  });

  it('does not throw when format is undefined', () => {
    const viz = createMockVizWasmInstance({ formats: ['svg'] });
    expect(() => assertFormatSupported(viz, undefined)).not.toThrow();
  });

  it('throws for unsupported format', () => {
    const viz = createMockVizWasmInstance({ formats: ['svg', 'png'] });
    expect(() => assertFormatSupported(viz, 'pdf')).toThrow(GraphvizWebError);
  });

  it('throws with correct error code', () => {
    const viz = createMockVizWasmInstance({ formats: ['svg'] });
    try {
      assertFormatSupported(viz, 'invalid');
      expect.fail('Should have thrown');
    } catch (error) {
      if (error instanceof GraphvizWebError) {
        expect(error.code).toBe('UNSUPPORTED_FORMAT');
      }
    }
  });
});

describe('assertFormatsSupported', () => {
  it('does not throw when all formats supported', () => {
    const viz = createMockVizWasmInstance({ formats: ['svg', 'png', 'pdf'] });
    expect(() => assertFormatsSupported(viz, ['svg', 'png'])).not.toThrow();
  });

  it('does not throw for empty formats list', () => {
    const viz = createMockVizWasmInstance({ formats: ['svg'] });
    expect(() => assertFormatsSupported(viz, [])).not.toThrow();
  });

  it('throws when any format unsupported', () => {
    const viz = createMockVizWasmInstance({ formats: ['svg', 'png'] });
    expect(() => assertFormatsSupported(viz, ['svg', 'pdf'])).toThrow(GraphvizWebError);
  });

  it('includes all unsupported formats in error', () => {
    const viz = createMockVizWasmInstance({ formats: ['svg', 'png'] });
    try {
      assertFormatsSupported(viz, ['svg', 'pdf', 'ps']);
      expect.fail('Should have thrown');
    } catch (error) {
      if (error instanceof GraphvizWebError) {
        expect(error.message).toContain('pdf');
        expect(error.message).toContain('ps');
        expect(error.code).toBe('UNSUPPORTED_FORMAT');
      }
    }
  });
});

describe('assertActive', () => {
  it('does not throw when not disposed', () => {
    expect(() => assertActive(false, 'Test surface')).not.toThrow();
  });

  it('throws when disposed', () => {
    expect(() => assertActive(true, 'Test surface')).toThrow(GraphvizWebError);
  });

  it('includes surface name in error message', () => {
    try {
      assertActive(true, 'My Renderer');
      expect.fail('Should have thrown');
    } catch (error) {
      if (error instanceof GraphvizWebError) {
        expect(error.message).toContain('My Renderer');
        expect(error.code).toBe('DISPOSED');
      }
    }
  });
});

describe('snapshotCapabilities', () => {
  it('returns capabilities object from viz instance', () => {
    const viz = createMockVizWasmInstance({
      graphvizVersion: '2.44.1',
      engines: ['dot', 'neato', 'fdp'],
      formats: ['svg', 'png'],
    });

    const snapshot = snapshotCapabilities(viz);

    expect(snapshot.graphvizVersion).toBe('2.44.1');
    expect(snapshot.engines).toEqual(['dot', 'neato', 'fdp']);
    expect(snapshot.formats).toEqual(['svg', 'png']);
  });

  it('creates a copy of arrays', () => {
    const viz = createMockVizWasmInstance();
    const snapshot1 = snapshotCapabilities(viz);
    const snapshot2 = snapshotCapabilities(viz);

    expect(snapshot1.engines).toEqual(snapshot2.engines);
    expect(snapshot1.engines).not.toBe(snapshot2.engines);
    expect(snapshot1.formats).not.toBe(snapshot2.formats);
  });
});

describe('issuesToMessage', () => {
  it('returns fallback for empty issues', () => {
    const result = issuesToMessage([], 'Fallback message');
    expect(result).toBe('Fallback message');
  });

  it('joins multiple issue messages with semicolon', () => {
    const issues = [{ message: 'Error 1' }, { message: 'Error 2' }];
    const result = issuesToMessage(issues, 'Fallback');
    expect(result).toBe('Error 1; Error 2');
  });

  it('trims and filters empty messages', () => {
    const issues = [
      { message: '  Trimmed  ' },
      { message: '   ' }, // should be filtered
      { message: 'Second' },
    ];
    const result = issuesToMessage(issues, 'Fallback');
    expect(result).toBe('Trimmed; Second');
  });

  it('returns fallback when all messages empty after trim', () => {
    const issues = [{ message: '  ' }, { message: '\t' }];
    const result = issuesToMessage(issues, 'Fallback message');
    expect(result).toBe('Fallback message');
  });

  it('handles single issue', () => {
    const result = issuesToMessage([{ message: 'Single issue' }], 'Fallback');
    expect(result).toBe('Single issue');
  });
});
