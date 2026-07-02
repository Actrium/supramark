import { describe, expect, it, mock } from 'bun:test';

// d2 wasm 在加载时就会通过 globalThis.supramark.measureText 量字宽。
// 历史 bug: loadWebD2Render 忘记调用 installHostMetricsBridge()，
// 导致 d2 单独渲染时 fallback 到 size*0.6 启发式，而 mermaid→d2
// 路径下 bridge 已被 mermaid 装上、走真实测量，两条路径产出不一致。
// 本测试守住 "d2 loader 必须先装 bridge" 这一不变量。

const installMock = mock(() => {});

mock.module('../src/host-bridge.js', () => ({
  __esModule: true,
  installHostMetricsBridge: installMock,
}));

mock.module('@actrium/d2-little-web', () => ({
  __esModule: true,
  default: async () => {},
  convert: (code: string) => `<svg data-stub>${code}</svg>`,
}));

const { loadWebD2Render } = await import('../src/web');

describe('loadWebD2Render', () => {
  it('installs host metrics bridge before invoking d2 wasm', async () => {
    installMock.mockClear();
    await loadWebD2Render();
    expect(installMock).toHaveBeenCalledTimes(1);
  });

  it('still returns a working render fn after bridge install', async () => {
    const render = await loadWebD2Render();
    const svg = await render('a -> b');
    expect(svg).toContain('<svg');
  });
});
