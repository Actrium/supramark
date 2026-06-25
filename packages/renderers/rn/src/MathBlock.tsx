import React, { useEffect, useState } from 'react';
import { ActivityIndicator, type LayoutChangeEvent, StyleSheet, Text, View } from 'react-native';
import { SvgXml } from 'react-native-svg';
import type { SupramarkMathBlockNode } from '@supramark/core';
import { getSvgViewBoxSize } from '@supramark/engines';
import { createReactNativeDiagramEngine } from '@supramark/engines/rn';
import { normalizeSvgLight } from './svgUtils';

interface MathBlockProps {
  node: SupramarkMathBlockNode;
}

const defaultDiagramEngine = createReactNativeDiagramEngine();

export const MathBlock: React.FC<MathBlockProps> = ({ node }) => {
  const [svg, setSvg] = useState<string | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [containerWidth, setContainerWidth] = useState<number>(0);
  const handleLayout = (event: LayoutChangeEvent) => {
    const nextWidth = Math.max(0, Math.floor(event.nativeEvent.layout.width));
    setContainerWidth(prev => (prev === nextWidth ? prev : nextWidth));
  };

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setSvg(null);

    defaultDiagramEngine.render({
      engine: 'math',
      code: node.value,
      options: { displayMode: true },
    })
      .then(result => {
        if (cancelled) return;
        if (!result.success || result.format !== 'svg') {
          throw new Error(result.error?.details || result.payload);
        }
        const normalized = normalizeSvgLight(result.payload);
        setSvg(normalized);
        setLoading(false);
      })
      .catch(err => {
        if (cancelled) return;
        if (__DEV__) {
          console.error('[Supramark MathBlock] Local MathJax render failed, fallback to TeX:', err);
        }
        setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [node.value]);

  if (loading && !svg) {
    return (
      <View style={styles.placeholder}>
        <ActivityIndicator size="small" />
        <Text style={styles.placeholderText}>正在渲染公式...</Text>
      </View>
    );
  }

  if (svg) {
    const effectiveWidth = containerWidth > 0 ? containerWidth : 320;
    let height = 80;
    const size = getSvgViewBoxSize(svg);

    if (size) {
      height = Math.min((size.height / size.width) * effectiveWidth, 240);
      height += 8;
    }

    return (
      <View style={styles.mathContainer} onLayout={handleLayout}>
        <SvgXml xml={svg} width={effectiveWidth} height={height} />
      </View>
    );
  }

  return (
    <View style={styles.codeBlock} onLayout={handleLayout}>
      <Text style={styles.codeText}>{node.value}</Text>
    </View>
  );
};

const styles = StyleSheet.create({
  mathContainer: {
    marginVertical: 8,
  },
  placeholderText: {
    fontSize: 14,
    color: '#666',
    marginLeft: 6,
  },
  placeholder: {
    padding: 8,
    borderRadius: 4,
    borderWidth: 1,
    borderColor: '#ccc',
    marginVertical: 8,
    flexDirection: 'row',
    alignItems: 'center',
  },
  codeBlock: {
    backgroundColor: '#f5f5f5',
    padding: 8,
    borderRadius: 4,
    marginVertical: 8,
  },
  codeText: {
    fontFamily: 'Menlo',
    fontSize: 12,
    color: '#262626',
  },
});
