import React, { useEffect, useState } from 'react';
import { ActivityIndicator, Dimensions, StyleSheet, Text, View } from 'react-native';
import { SvgXml } from 'react-native-svg';
import type { SupramarkDiagramNode, SupramarkDiagramConfig } from '@supramark/core';
import { type DiagramRenderResult } from '@supramark/engines';
import { createReactNativeDiagramEngine, getNativeEngineAdapter } from '@supramark/engines/rn';
import { normalizeSvg, normalizeSvgLight } from './svgUtils';

export interface DiagramNodeProps {
  node: SupramarkDiagramNode;
  /**
   * Diagram subsystem config.
   *
   * - Passed in via SupramarkConfig.diagram from the host;
   * - Used to inject per-engine defaults (server / timeout / etc.);
   * - Per-node `node.meta` still overrides these defaults.
   */
  diagramConfig?: SupramarkDiagramConfig;
}

const defaultDiagramEngine = createReactNativeDiagramEngine();

// RN engine support matrix:
//   - 'dot' / 'graphviz' → handled by createReactNativeDiagramEngine
//     (graphviz-anywhere-rn native FFI, no DOM, no WebView).
//   - 'd2' / 'mermaid' / 'plantuml' → available IF the host registers
//     the engine's native FFI adapter at startup via
//     `registerNativeEngineAdapter({ engine, render, ... })` from
//     `@supramark/engines/rn`. The native FFI crates live at
//     `crates/<engine>-little/packages/native/` and are consumed
//     through per-engine native-rn npm wrappers (planned).
//   - everything else (echarts / vega-lite) → unsupported on RN in
//     this build. The hidden-WebView worker
//     (@supramark/rn-diagram-worker) was retired in the 2026-05
//     cleanup; native FFI bindings are tracked per-engine in the
//     respective crates/<engine>/UPSTREAM.md.
const BUILTIN_SUPPORTED_ENGINES = new Set(['dot', 'graphviz']);

function isEngineSupported(normalizedEngine: string): boolean {
  if (BUILTIN_SUPPORTED_ENGINES.has(normalizedEngine)) return true;
  return getNativeEngineAdapter(normalizedEngine) !== undefined;
}

export const DiagramNode: React.FC<DiagramNodeProps> = ({ node, diagramConfig }) => {
  const [svg, setSvg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState<boolean>(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setSvg(null);

    const normalizedEngine = String(node.engine || '').toLowerCase();

    if (!isEngineSupported(normalizedEngine)) {
      setError(
        `Engine "${node.engine}" is not yet supported on React Native. ` +
          'Built-in support: dot / graphviz. For mermaid / plantuml / d2, ' +
          'install the corresponding @kookyleo/supramark-*-native-rn package ' +
          'and call registerNativeEngineAdapter() at startup.'
      );
      setLoading(false);
      return;
    }

    const options = buildRenderOptions(node.engine, node.meta, diagramConfig);
    const renderPromise: Promise<DiagramRenderResult> = defaultDiagramEngine.render({
      engine: normalizedEngine,
      code: node.code,
      options,
    });

    renderPromise
      .then(result => {
        if (cancelled) return;

        if (!result.success) {
          const errorMsg = result.error
            ? `${result.error.message}: ${result.error.details || result.payload}`
            : result.payload || 'Unknown error';
          setError(errorMsg);
          setLoading(false);
          return;
        }

        if (result.format === 'svg') {
          let normalized;
          try {
            normalized = result.payload.includes('<style')
              ? normalizeSvg(result.payload)
              : normalizeSvgLight(result.payload);
          } catch (err) {
            setError(`SVG normalization failed: ${err}`);
            setLoading(false);
            return;
          }

          setSvg(normalized);
          setLoading(false);
        } else {
          setError(`Unsupported diagram format: ${result.format}`);
          setLoading(false);
        }
      })
      .catch(err => {
        if (cancelled) return;
        setError(String(err));
        setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [node.engine, node.code, node.meta, diagramConfig]);

  if (loading && !svg && !error) {
    return (
      <View style={styles.placeholder}>
        <ActivityIndicator size="small" />
        <Text style={styles.placeholderText}>Rendering diagram ({node.engine})…</Text>
      </View>
    );
  }

  if (error) {
    return (
      <View style={styles.placeholder}>
        <Text style={styles.errorText}>Diagram error: {error}</Text>
      </View>
    );
  }

  if (svg) {
    const viewBoxMatch = svg.match(/viewBox="([^"]+)"/);
    const widthAttrMatch = svg.match(/<svg[^>]*\bwidth="([^"]+)"/);
    const heightAttrMatch = svg.match(/<svg[^>]*\bheight="([^"]+)"/);

    const { width: screenWidth } = Dimensions.get('window');
    const containerWidth = screenWidth - 32;
    let svgWidth = 0;
    let svgHeight = 0;

    if (viewBoxMatch) {
      const parts = viewBoxMatch[1].split(/[\s,]+/);
      if (parts.length === 4) {
        svgWidth = parseFloat(parts[2]);
        svgHeight = parseFloat(parts[3]);
      }
    }

    if (svgWidth <= 0 && widthAttrMatch) svgWidth = parseFloat(widthAttrMatch[1]);
    if (svgHeight <= 0 && heightAttrMatch) svgHeight = parseFloat(heightAttrMatch[1]);

    let height = 300;
    if (svgWidth > 0 && svgHeight > 0) {
      height = (svgHeight / svgWidth) * containerWidth;
      height = Math.min(height, 500);
    }

    let scalableSvg = svg;
    if (!viewBoxMatch && svgWidth > 0 && svgHeight > 0) {
      scalableSvg = scalableSvg.replace(
        /<svg([^>]*)>/,
        `<svg$1 viewBox="0 0 ${svgWidth} ${svgHeight}">`
      );
    }

    scalableSvg = scalableSvg
      .replace(/(<svg[^>]*)\bwidth="[^"]*"/, '$1')
      .replace(/(<svg[^>]*)\bheight="[^"]*"/, '$1');

    return (
      <View style={[styles.diagram, { width: containerWidth, height }]}>
        <SvgXml xml={scalableSvg} width={containerWidth} height={height} />
      </View>
    );
  }

  return (
    <View style={styles.placeholder}>
      <Text style={styles.placeholderText}>[diagram: {node.engine}]</Text>
    </View>
  );
};

/**
 * Compose render options from per-engine global defaults +
 * node-specific meta overrides.
 *
 * Resolution order:
 * - diagramConfig.engines[engine] supplies engine-level defaults
 *   (server / timeout / etc.);
 * - fields on `node.meta` override those defaults;
 * - returns `undefined` when neither carries any options.
 */
function buildRenderOptions(
  engine: string,
  meta: SupramarkDiagramNode['meta'],
  diagramConfig?: SupramarkDiagramConfig
): Record<string, unknown> | undefined {
  const base: Record<string, unknown> = {};

  const engineConfig = diagramConfig?.engines?.[engine];
  if (engineConfig) {
    if (typeof engineConfig.server === 'string') {
      base.server = engineConfig.server;
      base.plantumlServer = engineConfig.server;
    }
    if (typeof engineConfig.timeoutMs === 'number') {
      base.timeout = engineConfig.timeoutMs;
    }
    if (engineConfig.cache) {
      base.cache = engineConfig.cache;
    }

    for (const [key, value] of Object.entries(engineConfig as Record<string, unknown>)) {
      if (value === undefined) continue;
      if (key === 'enabled' || key === 'timeoutMs' || key === 'server' || key === 'cache') continue;
      base[key] = value;
    }
  }

  if (!meta) {
    return Object.keys(base).length > 0 ? base : undefined;
  }

  return { ...base, ...meta };
}

const styles = StyleSheet.create({
  diagram: {
    marginBottom: 8,
  },
  placeholder: {
    padding: 8,
    borderRadius: 4,
    borderWidth: 1,
    borderColor: '#ccc',
    marginBottom: 8,
    flexDirection: 'row',
    alignItems: 'center',
  },
  placeholderText: {
    fontSize: 12,
    color: '#666',
    marginLeft: 6,
  },
  errorText: {
    fontSize: 12,
    color: '#d4380d',
  },
});
