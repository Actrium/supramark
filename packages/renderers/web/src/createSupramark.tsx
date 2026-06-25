import React from 'react';
import type {
  RenderFn,
  DiagramRenderService,
  DiagramRenderResult,
} from '@supramark/engines';
import type { renderToString as RenderToString } from 'react-dom/server';
import { DiagramEngineContext } from './DiagramEngineProvider.js';
import { Supramark, type SupramarkWebProps } from './Supramark.js';

/**
 * v0.2 Supramark 工厂 —— 由 `@supramark/cli` 生成的文件消费。
 *
 * 接收 `{ engines, features }` spec，闭包捕获后返回一个 `Supramark` React 组件。
 * 业务代码只看到 `<Supramark markdown={...} />`，engines / features 全隐形。
 *
 * @example（生成文件里会自动写这个用法）
 * ```ts
 * import mermaid from '@supramark/engines/mermaid';
 * import mathjax from '@supramark/engines/mathjax';
 * import { createSupramark } from '@supramark/web/createSupramark';
 *
 * export const Supramark = createSupramark({
 *   engines: { mermaid: mermaid(), math: mathjax() },
 *   features: { gfm: true, math: true }
 * });
 * ```
 */
export interface RenderSpec {
  engines: Record<string, RenderFn>;
  features?: Record<string, unknown>;
}

/** createSupramark 返回的 FC 对外暴露的 props。 */
export interface SupramarkFCProps
  extends Omit<SupramarkWebProps, 'config' | 'containerRenderers'> {
  // spec 已经固化在工厂闭包中，这里不再接收 engines/features。
}

/**
 * 从 spec 生成一个 markdown → HTML string 的纯函数。
 *
 * 内部走 react-dom/server.renderToString，engines / features 继承自 spec。
 */
export function createRender(
  spec: RenderSpec
): (markdown: string, options?: { theme?: 'light' | 'dark' }) => Promise<string> {
  const Comp = createSupramark(spec);
  let renderToStringPromise: Promise<typeof RenderToString> | null = null;
  const loadRts = () => {
    if (!renderToStringPromise) {
      renderToStringPromise = import('react-dom/server').then(m => m.renderToString);
    }
    return renderToStringPromise;
  };
  return async (markdown, _options) => {
    const renderToString = await loadRts();
    return renderToString(React.createElement(Comp, { markdown }) as never);
  };
}

export function createSupramark(spec: RenderSpec): React.FC<SupramarkFCProps> {
  const service: DiagramRenderService = {
    async render(params): Promise<DiagramRenderResult> {
      const { engine, code, options } = params;
      const fn = spec.engines[engine];
      if (!fn) {
        return {
          id: `render_${Date.now()}`,
          engine,
          success: false,
          format: 'error',
          payload: `Engine not configured: ${engine}`,
          error: { code: 'unsupported_engine', message: `Engine not in spec: ${engine}` },
        };
      }
      try {
        const payload = await fn(code, options as never);
        return {
          id: `render_${Date.now()}`,
          engine,
          success: true,
          format: 'svg',
          payload,
        };
      } catch (e) {
        return {
          id: `render_${Date.now()}`,
          engine,
          success: false,
          format: 'error',
          payload: e instanceof Error ? e.message : String(e),
          error: { code: 'render_error', message: e instanceof Error ? e.message : String(e) },
        };
      }
    },
  };

  // 将 spec.features 转成内部 SupramarkConfig（给 parser 用）。
  // 简化处理：features 形如 { gfm: true, math: { engine: 'mathjax' } }，
  // 转成 `config.features = [{ id: '@supramark/feature-gfm', enabled: true, options }, ...]`.
  const config = buildSupramarkConfig(spec.features);

  const SupramarkComponent: React.FC<SupramarkFCProps> = props => {
    return (
      <DiagramEngineContext.Provider value={service}>
        <Supramark {...props} config={config} />
      </DiagramEngineContext.Provider>
    );
  };
  SupramarkComponent.displayName = 'Supramark';

  return SupramarkComponent;
}

// ----------------------------------------------------------------------------
// features 简化映射：{gfm:true} → SupramarkConfig 的 features 数组
// ----------------------------------------------------------------------------
const FEATURE_ID_MAP: Record<string, string> = {
  gfm: '@supramark/feature-gfm',
  math: '@supramark/feature-math',
  footnote: '@supramark/feature-footnote',
  emoji: '@supramark/feature-emoji',
  admonition: '@supramark/feature-admonition',
  'definition-list': '@supramark/feature-definition-list',
};

function buildSupramarkConfig(
  features?: Record<string, unknown>
): { features: Array<{ id: string; enabled: boolean; options?: unknown }> } | undefined {
  if (!features) return undefined;
  const entries: Array<{ id: string; enabled: boolean; options?: unknown }> = [];
  for (const [key, value] of Object.entries(features)) {
    const id = FEATURE_ID_MAP[key];
    if (!id) continue;
    if (value === true) {
      entries.push({ id, enabled: true });
    } else if (value === false) {
      entries.push({ id, enabled: false });
    } else if (typeof value === 'object' && value !== null) {
      entries.push({ id, enabled: true, options: value });
    }
  }
  return entries.length > 0 ? { features: entries } : undefined;
}

