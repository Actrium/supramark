import React, { useEffect, useState } from 'react';
import { Text, type TextStyle } from 'react-native';
import { SvgXml } from 'react-native-svg';
import { getSvgViewBoxSize } from '@supramark/engines';
import { createReactNativeDiagramEngine } from '@supramark/engines/rn';
import { normalizeSvgLight } from './svgUtils';

function buildMathRenderOptions(
  displayMode: boolean,
  timeoutMs?: number
): Record<string, unknown> {
  const options: Record<string, unknown> = { displayMode };
  if (typeof timeoutMs === 'number' && timeoutMs > 0 && Number.isFinite(timeoutMs)) {
    options.timeout = timeoutMs;
  }
  return options;
}

interface MathInlineProps {
  value: string;
  textStyle?: TextStyle;
  timeoutMs?: number;
}

const defaultDiagramEngine = createReactNativeDiagramEngine();

export const MathInline: React.FC<MathInlineProps> = ({ value, textStyle, timeoutMs }) => {
  const fontSize = textStyle?.fontSize ?? 16;
  const [dimensions, setDimensions] = useState<{ width: number; height: number } | null>(null);
  const [svg, setSvg] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setFailed(false);
    setDimensions(null);
    setSvg(null);

    defaultDiagramEngine.render({
      engine: 'math',
      code: value,
      options: buildMathRenderOptions(false, timeoutMs),
    })
      .then(result => {
        if (cancelled) return;
        if (!result.success || result.format !== 'svg') {
          throw new Error(result.error?.details || result.payload);
        }

        const normalized = normalizeSvgLight(result.payload);
        const size = getSvgViewBoxSize(normalized);
        if (!size) {
          throw new Error('Math SVG viewBox is missing');
        }

        const targetHeight = fontSize * 1.2;
        const scale = targetHeight / size.height;
        setSvg(normalized);
        setDimensions({
          width: Math.max(1, Math.ceil(size.width * scale)),
          height: Math.max(1, Math.ceil(targetHeight)),
        });
      })
      .catch(() => {
        if (cancelled) return;
        setFailed(true);
      });

    return () => {
      cancelled = true;
    };
  }, [value, fontSize, timeoutMs]);

  if (failed || !dimensions || !svg) {
    return (
      <Text
        style={{
          fontFamily: 'Menlo',
          fontSize: fontSize * 0.8,
          backgroundColor: '#f0eaff',
          color: '#6b21a8',
          paddingHorizontal: 3,
          borderRadius: 2,
        }}
      >
        {value}
      </Text>
    );
  }

  return (
    <SvgXml
      xml={svg}
      width={dimensions.width}
      height={dimensions.height}
    />
  );
};
