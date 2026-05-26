import React, { useEffect, useState } from 'react';
import { ActivityIndicator, Dimensions, LayoutChangeEvent, StyleSheet, Text, View } from 'react-native';
import { SvgXml } from 'react-native-svg';
import type { SupramarkMathBlockNode } from '@supramark/core';
import { normalizeSvgLight } from '../svgUtils';
import { getSvgViewBoxSize, renderMathJaxSvg } from './mathjax';

interface MathBlockProps {
  node: SupramarkMathBlockNode;
}

// 块级公式和图表一样采用固定预览框，避免异步 SVG 返回后再改高导致聊天列表抖动。
const PHONE_MATH_BLOCK_HEIGHT = 140;
const PAD_MATH_BLOCK_HEIGHT = 200;
// iPad 常见最小逻辑宽度为 768，使用窗口宽度区分 Phone / Pad，保证首帧就能选出稳定展示框。
const PAD_MIN_SCREEN_WIDTH = 768;

export const MathBlock: React.FC<MathBlockProps> = ({ node }) => {
  const [svg, setSvg] = useState<string | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [containerWidth, setContainerWidth] = useState<number>(0);
  const windowWidth = Dimensions.get('window').width;
  // 块级公式统一固定高度，宽度不超过父容器，让 SVG 只填充内容而不再改变外层布局。
  const previewHeight = windowWidth >= PAD_MIN_SCREEN_WIDTH
    ? PAD_MATH_BLOCK_HEIGHT
    : PHONE_MATH_BLOCK_HEIGHT;
  const fallbackWidth = windowWidth >= PAD_MIN_SCREEN_WIDTH ? 500 : 300;
  const handleLayout = (event: LayoutChangeEvent) => {
    const nextWidth = Math.max(0, Math.floor(event.nativeEvent.layout.width));
    setContainerWidth(prev => (prev === nextWidth ? prev : nextWidth));
  };

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setSvg(null);

    renderMathJaxSvg(node.value, { displayMode: true })
      .then(result => {
        if (cancelled) return;
        const normalized = normalizeSvgLight(result);
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
      <View style={[styles.placeholder, { minHeight: previewHeight }]} onLayout={handleLayout}>
        <ActivityIndicator size="small" />
        <Text style={styles.placeholderText}>正在渲染公式...</Text>
      </View>
    );
  }

  if (svg) {
    // 高度固定，宽度跟随父容器可用宽，避免异步 SVG 返回后通过改高撑动消息列表。
    const frameWidth = containerWidth > 0 ? containerWidth : fallbackWidth;

    return (
      <View style={[styles.mathContainer, { height: previewHeight }]} onLayout={handleLayout}>
        <SvgXml xml={svg} width={frameWidth} height={previewHeight} />
      </View>
    );
  }

  // 统一降级：源码文本
  return (
    <View style={[styles.codeBlock, { minHeight: previewHeight }]} onLayout={handleLayout}>
      <Text style={styles.codeText}>{node.value}</Text>
    </View>
  );
};

const styles = StyleSheet.create({
  mathContainer: {
    marginVertical: 8,
    width: '100%',
    justifyContent: 'center',
  },
  placeholder: {
    padding: 8,
    borderRadius: 4,
    borderWidth: 1,
    borderColor: '#ccc',
    marginVertical: 8,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
  },
  placeholderText: {
    fontSize: 14,
    color: '#666',
    marginLeft: 6,
  },
  codeBlock: {
    backgroundColor: '#f5f5f5',
    padding: 8,
    borderRadius: 4,
    marginVertical: 8,
    justifyContent: 'center',
  },
  codeText: {
    fontFamily: 'Menlo',
    fontSize: 12,
    color: '#262626',
  },
});
