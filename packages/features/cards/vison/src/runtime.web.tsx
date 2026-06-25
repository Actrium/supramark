/**
 * Vison Web container renderer.
 *
 * Designed to be passed to `<Supramark containerRenderers={{ vison }} />`.
 * Lazy-imports `@actrium/vison-web` so consumers who never include a
 * `:::vison` block don't pay for the renderer bundle.
 */

import React, { useEffect, useState } from 'react';
import type { ContainerWebRenderArgs } from '@supramark/core';
import type { VisonContainerData, VisonSpec } from './feature.js';

type VisonWebRendererComponent = React.ComponentType<{ data: VisonSpec }>;

interface VisonWebModule {
  VisonWebRenderer?: VisonWebRendererComponent;
  default?: VisonWebRendererComponent;
}

let cachedRenderer: VisonWebRendererComponent | null = null;
let rendererPromise: Promise<VisonWebRendererComponent> | null = null;

async function loadRenderer(): Promise<VisonWebRendererComponent> {
  if (cachedRenderer) return cachedRenderer;
  if (!rendererPromise) {
    rendererPromise = import('@actrium/vison-web' as string).then((mod: VisonWebModule) => {
      const Component = mod.VisonWebRenderer ?? mod.default;
      if (!Component) {
        throw new Error('@actrium/vison-web did not export VisonWebRenderer');
      }
      cachedRenderer = Component;
      return cachedRenderer;
    });
  }
  return rendererPromise;
}

export function renderVisonContainerWeb({
  node,
  key,
}: ContainerWebRenderArgs): React.ReactNode {
  const data = (node?.data ?? {}) as VisonContainerData;

  if (data.parseError || !data.spec) {
    return (
      <pre
        key={key}
        style={{
          padding: 12,
          borderRadius: 8,
          border: '1px solid #f0a39c',
          background: '#fff5f4',
          color: '#a8071a',
          fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
          fontSize: 12,
          whiteSpace: 'pre-wrap',
        }}
      >
        Vison parse error: {data.parseError ?? 'no spec'}
        {'\n\n'}
        {data.source}
      </pre>
    );
  }

  return <VisonAsync key={key} spec={data.spec} />;
}

const VisonAsync: React.FC<{ spec: VisonSpec }> = ({ spec }) => {
  const [Renderer, setRenderer] = useState<VisonWebRendererComponent | null>(cachedRenderer);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (Renderer) return;
    let cancelled = false;
    loadRenderer().then(
      r => {
        if (!cancelled) setRenderer(() => r);
      },
      e => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      }
    );
    return () => {
      cancelled = true;
    };
  }, [Renderer]);

  if (error) {
    return (
      <div style={{ padding: 8, color: '#a8071a', fontSize: 12 }}>
        Failed to load Vison renderer: {error}
      </div>
    );
  }

  if (!Renderer) {
    return <div style={{ padding: 8, color: '#888', fontSize: 12 }}>Loading Vison card…</div>;
  }

  return <Renderer data={spec} />;
};
