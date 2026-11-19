import { useEffect, useRef } from 'react';
import { NativeModules } from 'react-native';
import type { RawTextLine } from '../metrics';

interface MetricsModule {
  measure(target: number): Promise<RawTextLine[] | null>;
}

export function useNativeLineMetrics(publish: (lines: readonly RawTextLine[]) => void) {
  const generation = useRef(0);
  useEffect(
    () => () => {
      generation.current += 1;
    },
    []
  );
  return (target: number, lines: readonly RawTextLine[]) => {
    const request = ++generation.current;
    publish(lines);
    const metrics = NativeModules.SupramarkSelectionMetrics as MetricsModule | undefined;
    if (!metrics) return;
    void metrics.measure(target).then(
      measured => {
        if (request !== generation.current || measured?.length !== lines.length) return;
        if (measured.every((line, i) => line.text === lines[i].text)) publish(measured);
      },
      (error: unknown) => {
        if (request === generation.current)
          console.error('[supramark] TextKit measurement failed', error);
      }
    );
  };
}
