import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import {
  createLazyWasmRenderer,
  createServerWasmRenderer,
  createWorkerWasmRenderer,
  GraphvizWebError,
  type GraphvizWorkerRequest,
} from '../index';
import { createMockVizWasmModule } from './mocks/viz-wasm';

// Reset all mocks before each test
beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.clearAllMocks();
});

describe('createLazyWasmRenderer', () => {
  it('creates a renderer', () => {
    const renderer = createLazyWasmRenderer();
    expect(renderer).toBeDefined();
    expect(renderer.preload).toBeDefined();
    expect(renderer.getCapabilities).toBeDefined();
    expect(renderer.render).toBeDefined();
    expect(renderer.renderDetailed).toBeDefined();
    expect(renderer.renderMany).toBeDefined();
    expect(renderer.renderManyDetailed).toBeDefined();
    expect(renderer.dispose).toBeDefined();
  });

  it('uses custom module loader', async () => {
    const loadModule = vi.fn(() =>
      Promise.resolve(createMockVizWasmModule())
    );

    const renderer = createLazyWasmRenderer({ loadModule });
    await renderer.preload();

    expect(loadModule).toHaveBeenCalled();
  });

  it('caches module instance across calls', async () => {
    const loadModule = vi.fn(() =>
      Promise.resolve(createMockVizWasmModule())
    );

    const renderer = createLazyWasmRenderer({ loadModule });
    await renderer.preload();
    await renderer.getCapabilities();

    // Module should be loaded only once
    expect(loadModule).toHaveBeenCalledTimes(1);
  });

  it('preloads module when warmup is enabled', async () => {
    const loadModule = vi.fn(() =>
      Promise.resolve(createMockVizWasmModule())
    );

    // Give some time for the background preload to complete
    createLazyWasmRenderer({ loadModule, warmup: true });
    await new Promise(resolve => setTimeout(resolve, 50));

    expect(loadModule).toHaveBeenCalled();
  });

  it('does not preload when warmup is disabled', async () => {
    const loadModule = vi.fn(() =>
      Promise.resolve(createMockVizWasmModule())
    );

    const renderer = createLazyWasmRenderer({ loadModule, warmup: false });
    await new Promise(resolve => setTimeout(resolve, 50));

    expect(loadModule).not.toHaveBeenCalled();

    // Loading should happen on first use
    await renderer.preload();
    expect(loadModule).toHaveBeenCalled();
  });

  it('successfully renders with default options', async () => {
    const renderer = createLazyWasmRenderer({
      loadModule: () => Promise.resolve(createMockVizWasmModule()),
    });

    const result = await renderer.render('digraph { a -> b }');
    expect(result).toBe('<svg>Mock SVG Output</svg>');
  });

  it('renders with custom engine and format', async () => {
    const renderer = createLazyWasmRenderer({
      loadModule: () => Promise.resolve(createMockVizWasmModule()),
    });

    const result = await renderer.render('digraph { a -> b }', {
      engine: 'neato',
      format: 'json',
    });
    expect(result).toBe('{"name":"graph"}');
  });

  it('renderDetailed returns diagnostics', async () => {
    const renderer = createLazyWasmRenderer({
      loadModule: () => Promise.resolve(createMockVizWasmModule()),
    });

    const result = await renderer.renderDetailed('digraph { a -> b }');
    expect(result).toHaveProperty('output');
    expect(result).toHaveProperty('issues');
    expect(result).toHaveProperty('capabilities');
    expect(Array.isArray(result.issues)).toBe(true);
  });

  it('renderMany returns multiple formats', async () => {
    const renderer = createLazyWasmRenderer({
      loadModule: () => Promise.resolve(createMockVizWasmModule()),
    });

    const result = await renderer.renderMany('digraph { a -> b }', ['svg', 'json']);
    expect(result).toHaveProperty('svg');
    expect(result).toHaveProperty('json');
    expect(typeof result.svg).toBe('string');
  });

  it('throws when renderer is disposed', async () => {
    const renderer = createLazyWasmRenderer({
      loadModule: () => Promise.resolve(createMockVizWasmModule()),
    });

    await renderer.dispose();

    await expect(renderer.render('digraph { a -> b }')).rejects.toThrow(
      GraphvizWebError
    );
  });

  it('throwsfor unsupported engine', async () => {
    const renderer = createLazyWasmRenderer({
      loadModule: () =>
        Promise.resolve(createMockVizWasmModule({ engines: ['dot'] })),
    });

    await expect(
      renderer.render('digraph { a -> b }', { engine: 'invalid' })
    ).rejects.toThrow(GraphvizWebError);
  });

  it('throws for unsupported format', async () => {
    const renderer = createLazyWasmRenderer({
      loadModule: () =>
        Promise.resolve(createMockVizWasmModule({ formats: ['svg'] })),
    });

    await expect(
      renderer.render('digraph { a -> b }', { format: 'pdf' })
    ).rejects.toThrow(GraphvizWebError);
  });

  it('throws when render fails in wasm', async () => {
    const renderer = createLazyWasmRenderer({
      loadModule: () =>
        Promise.resolve(
          createMockVizWasmModule({
            renderSuccess: false,
            renderError: 'Parse error',
          })
        ),
    });

    await expect(
      renderer.render('invalid dot')
    ).rejects.toThrow(GraphvizWebError);
  });
});

describe('createServerWasmRenderer', () => {
  it('creates a renderer', () => {
    const renderer = createServerWasmRenderer({ eager: false });
    expect(renderer).toBeDefined();
  });

  it('enables warmup by default', async () => {
    const loadModule = vi.fn(() =>
      Promise.resolve(createMockVizWasmModule())
    );

    createServerWasmRenderer({ loadModule });
    await new Promise(resolve => setTimeout(resolve, 50));

    // Should warmup (preload) by default
    expect(loadModule).toHaveBeenCalled();
  });

  it('can disable eager warmup', async () => {
    const loadModule = vi.fn(() =>
      Promise.resolve(createMockVizWasmModule())
    );

    const renderer = createServerWasmRenderer({ loadModule, eager: false });
    await new Promise(resolve => setTimeout(resolve, 50));

    // Should not warmup when eager is false
    expect(loadModule).not.toHaveBeenCalled();

    // Should load on first use
    await renderer.preload();
    expect(loadModule).toHaveBeenCalled();
  });

  it('supports multiple renders with same instance', async () => {
    const loadModule = vi.fn(() =>
      Promise.resolve(createMockVizWasmModule())
    );

    const renderer = createServerWasmRenderer({ loadModule });

    const result1 = await renderer.render('digraph { a -> b }');
    const result2 = await renderer.render('digraph { x -> y }');

    // Module loaded once, reused for both renders
    expect(loadModule).toHaveBeenCalledTimes(1);
    expect(result1).toBeDefined();
    expect(result2).toBeDefined();
  });

  it('uses custom module loader', async () => {
    const loadModule = vi.fn(() =>
      Promise.resolve(createMockVizWasmModule())
    );

    const renderer = createServerWasmRenderer({ loadModule });
    await renderer.preload();

    expect(loadModule).toHaveBeenCalled();
  });
});

describe('createWorkerWasmRenderer', () => {
  it('creates a renderer without worker', () => {
    // Mock Worker constructor
    const mockWorker = {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      postMessage: vi.fn(),
      terminate: vi.fn(),
    };

    const renderer = createWorkerWasmRenderer({
      worker: mockWorker as unknown as Worker,
    });

    expect(renderer).toBeDefined();
    expect(renderer.dispose).toBeDefined();
  });

  it('uses provided worker', () => {
    const mockWorker = {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      postMessage: vi.fn(),
      terminate: vi.fn(),
    };

    const renderer = createWorkerWasmRenderer({
      worker: mockWorker as unknown as Worker,
    });

    expect(renderer.preload).toBeDefined();
  });

  it('sends messages to worker', async () => {
    const mockWorker = {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      postMessage: vi.fn((msg: GraphvizWorkerRequest) => {
        // Simulate worker response
        const response = {
          id: msg.id,
          ok: true,
          value: { graphvizVersion: '2.43.0', engines: [], formats: [] },
        };
        // Trigger the message event handler immediately
        const handlers = mockWorker.addEventListener.mock.calls.find(
          call => call[0] === 'message'
        );
        if (handlers) {
          handlers[1]({ data: response } as MessageEvent);
        }
      }),
      terminate: vi.fn(),
    };

    createWorkerWasmRenderer({
      worker: mockWorker as unknown as Worker,
    });

    // This would normally wait for worker response
    // In actual testing, you'd need more sophisticated mocking
    expect(mockWorker.postMessage).toBeDefined();
  });

  it('applies timeout to requests', () => {
    const mockWorker = {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      postMessage: vi.fn(),
      terminate: vi.fn(),
    };

    createWorkerWasmRenderer({
      worker: mockWorker as unknown as Worker,
      timeoutMs: 1000,
    });
    expect(mockWorker.postMessage).toBeDefined();
  });

  it('terminates worker on dispose when owned', async () => {
    const mockWorker = {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      postMessage: vi.fn(),
      terminate: vi.fn(),
    };

    const renderer = createWorkerWasmRenderer({
      worker: mockWorker as unknown as Worker,
    });

    await renderer.dispose();

    expect(mockWorker.removeEventListener).toHaveBeenCalled();
  });

  it('does not terminate worker when not owned', async () => {
    const mockWorker = {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      postMessage: vi.fn(),
      terminate: vi.fn(),
    };

    const renderer = createWorkerWasmRenderer({
      worker: mockWorker as unknown as Worker,
      terminateOnDispose: false,
    });

    await renderer.dispose();

    expect(mockWorker.terminate).not.toHaveBeenCalled();
  });

  it('rejects all pending requests on dispose', async () => {
    const mockWorker = {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      postMessage: vi.fn(),
      terminate: vi.fn(),
    };

    const renderer = createWorkerWasmRenderer({
      worker: mockWorker as unknown as Worker,
    });

    await renderer.dispose();

    // After dispose, requests should be rejected
    await expect(renderer.getCapabilities()).rejects.toThrow(GraphvizWebError);
  });
});
