/**
 * Vison React Native container renderer.
 *
 * Designed to be passed to `<Supramark containerRenderers={{ vison }} />`
 * on the RN side. Lazy-imports `@actrium/vison-rn` so consumers who
 * never include a `:::vison` block don't pay for the renderer bundle.
 */

import React, { useEffect, useState } from 'react';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';
import type { VisonContainerData, VisonSpec } from './feature.js';

// Loose typing for the RN renderer args — the core ContainerRendererRN
// signature lives in @supramark/rn but we don't want a hard dep here.
interface RNContainerArgs {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  node: any;
  key: string | number;
}

type VisonRNRendererComponent = React.ComponentType<{ data: VisonSpec }>;

let cachedRenderer: VisonRNRendererComponent | null = null;
let rendererPromise: Promise<VisonRNRendererComponent> | null = null;

async function loadRenderer(): Promise<VisonRNRendererComponent> {
  if (cachedRenderer) return cachedRenderer;
  if (!rendererPromise) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    rendererPromise = import('@actrium/vison-rn' as string).then((mod: any) => {
      const Component = mod.VisonRNRenderer ?? mod.default;
      if (!Component) {
        throw new Error('@actrium/vison-rn did not export VisonRNRenderer');
      }
      cachedRenderer = Component as VisonRNRendererComponent;
      return cachedRenderer;
    });
  }
  return rendererPromise;
}

export function renderVisonContainerRN({ node, key }: RNContainerArgs): React.ReactNode {
  const data = (node?.data ?? {}) as VisonContainerData;

  if (data.parseError || !data.spec) {
    return (
      <View key={key} style={styles.errorBox}>
        <Text style={styles.errorTitle}>Vison parse error: {data.parseError ?? 'no spec'}</Text>
        <Text style={styles.errorBody}>{data.source}</Text>
      </View>
    );
  }

  return <VisonAsync key={key} spec={data.spec} />;
}

const VisonAsync: React.FC<{ spec: VisonSpec }> = ({ spec }) => {
  const [Renderer, setRenderer] = useState<VisonRNRendererComponent | null>(cachedRenderer);
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
      <View style={styles.loadingBox}>
        <Text style={styles.errorTitle}>Failed to load Vison renderer: {error}</Text>
      </View>
    );
  }

  if (!Renderer) {
    return (
      <View style={styles.loadingBox}>
        <ActivityIndicator size="small" />
        <Text style={styles.loadingText}>Loading Vison card…</Text>
      </View>
    );
  }

  return <Renderer data={spec} />;
};

const styles = StyleSheet.create({
  errorBox: {
    padding: 12,
    borderRadius: 8,
    borderWidth: 1,
    borderColor: '#f0a39c',
    backgroundColor: '#fff5f4',
    marginBottom: 8,
  },
  errorTitle: {
    color: '#a8071a',
    fontSize: 12,
    fontWeight: '600',
    marginBottom: 4,
  },
  errorBody: {
    color: '#a8071a',
    fontSize: 11,
    fontFamily: 'monospace',
  },
  loadingBox: {
    padding: 8,
    flexDirection: 'row',
    alignItems: 'center',
    marginBottom: 8,
  },
  loadingText: {
    fontSize: 12,
    color: '#888',
    marginLeft: 6,
  },
});
