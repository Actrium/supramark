import { useEffect, useRef } from 'react';
import { NativeModules } from 'react-native';
import type { RawTextLine } from '../metrics';
import { matchesTextLayout, type TextKitLine } from './matchesTextLayout';

interface MetricsModule {
  measure(target: number): Promise<TextKitLine[] | null>;
}

export function useNativeLineMetrics(publish: (lines: readonly RawTextLine[]) => void) {
  const generation = useRef(0);
  const warned = useRef(false);
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
        if (request !== generation.current || !measured) return;
        if (matchesTextLayout(measured, lines)) {
          publish(measured);
        } else if (typeof __DEV__ !== 'undefined' && __DEV__ && !warned.current) {
          warned.current = true;
          console.warn(
            '[supramark] TextKit and onTextLayout disagree; using React Native line geometry.'
          );
        }
      },
      (error: unknown) => {
        if (request === generation.current)
          console.error('[supramark] TextKit measurement failed', error);
      }
    );
  };
}
