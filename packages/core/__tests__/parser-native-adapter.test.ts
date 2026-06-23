import {
  __resetNativeParserRegistryForTests,
  getNativeParserAdapter,
  listNativeParserAdapters,
  parseViaNative,
  registerNativeParserAdapter,
  type NativeParserAdapter,
} from '../src/parser-native-adapter';
import { loadRustMarkdownModule } from '../src/plugin-loader-rn';

describe('native markdown parser adapter', () => {
  beforeEach(() => {
    // 每个用例从空 registry 开始，避免 last-wins 状态串到下一个测试。
    __resetNativeParserRegistryForTests();
  });

  it('未注册 native adapter 时返回空路由结果', async () => {
    expect(getNativeParserAdapter()).toBeUndefined();
    expect(listNativeParserAdapters()).toEqual([]);
    expect(await parseViaNative('# title')).toBeNull();
  });

  it('注册 native adapter 后通过 parseViaNative 返回 AST JSON', async () => {
    // 模拟 RN native module 返回的 AST v2 JSON 字符串。
    const rootJson = JSON.stringify({ type: 'root', ast_version: 2, children: [] });

    // 注册一个最小 native adapter，验证 registry 能把 Markdown source 转发过去。
    const adapter: NativeParserAdapter = {
      parseJson: async source => JSON.stringify({ source, parsed: rootJson }),
      getVersion: async () => 'test-native',
    };

    registerNativeParserAdapter(adapter);

    expect(getNativeParserAdapter()).toBe(adapter);
    expect(listNativeParserAdapters()).toEqual([adapter]);
    expect(await adapter.getVersion?.()).toBe('test-native');
    expect(await parseViaNative('# title')).toBe(
      JSON.stringify({ source: '# title', parsed: rootJson })
    );
  });

  it('多次注册 native adapter 时 last-wins，但保留诊断列表顺序', async () => {
    // 第一个 adapter 模拟旧 native module 实例。
    const firstAdapter: NativeParserAdapter = {
      parseJson: async () => JSON.stringify({ type: 'root', from: 'first' }),
    };

    // 第二个 adapter 模拟 hot reload 或测试替换后的 native module 实例。
    const secondAdapter: NativeParserAdapter = {
      parseJson: async () => JSON.stringify({ type: 'root', from: 'second' }),
    };

    registerNativeParserAdapter(firstAdapter);
    registerNativeParserAdapter(secondAdapter);

    expect(getNativeParserAdapter()).toBe(secondAdapter);
    expect(listNativeParserAdapters()).toEqual([firstAdapter, secondAdapter]);
    expect(await parseViaNative('source')).toBe(JSON.stringify({ type: 'root', from: 'second' }));
  });

  it('RN loader 未注册 native adapter 时抛出明确接入错误', async () => {
    // 保存 RN loader 的错误对象，断言它没有静默回退到 wasm。
    let thrown: unknown;

    try {
      await loadRustMarkdownModule();
    } catch (error) {
      thrown = error;
    }

    expect(thrown).toBeInstanceOf(Error);
    expect((thrown as Error).message).toContain('RN runtime requires native markdown parser adapter');
    expect((thrown as Error).message).toContain("import '@supramark/markdown-native-rn'");
  });

  it('RN loader 已注册 native adapter 时暴露异步 parseJson', async () => {
    // 模拟 native bridge 的 Promise<string> 返回值，覆盖 RN TurboModule 异步路径。
    const nativeJson = JSON.stringify({ type: 'root', ast_version: 2, children: [] });

    registerNativeParserAdapter({
      parseJson: async source => JSON.stringify({ source, nativeJson }),
    });

    const mod = await loadRustMarkdownModule();

    expect(typeof mod.parseJson).toBe('function');
    expect(await mod.parseJson?.('hello')).toBe(JSON.stringify({ source: 'hello', nativeJson }));
  });
});
