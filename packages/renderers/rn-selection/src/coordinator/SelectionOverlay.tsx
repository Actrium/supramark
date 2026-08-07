import React, { useCallback, useSyncExternalStore } from 'react';
import { StyleSheet, View } from 'react-native';
import { computeSelectionRects, type OverlayRect } from './overlay';
import { useSelectionContext } from './useDocumentSelection';

export interface SelectionOverlayProps {
  color?: string; // default 'rgba(51,153,255,0.35)'
  zIndex?: number; // default 10
}

/**
 * Subscribe to everything that can move a highlight: the selection itself, and
 * the registry version (layout, unit and metrics changes do NOT alter the
 * `getBlocks()` array reference, so without this the overlay would keep
 * painting at stale coordinates after a reflow or a re-measure).
 */
export function useSelectionRects(): OverlayRect[] {
  const { registry, store } = useSelectionContext();
  const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
  const subscribeRegistry = useCallback(
    (cb: () => void) => registry.subscribe(() => cb()),
    [registry]
  );
  useSyncExternalStore(subscribeRegistry, registry.getVersion, registry.getVersion);
  return computeSelectionRects({
    blocks: registry.getBlocks(),
    range: snapshot.range,
    units: snapshot.units,
    index: registry.index,
  });
}

/**
 * Paints the selection highlight: one rectangle per line of selected text,
 * from each block's own metrics. Non-interactive (`pointerEvents="none"`) so it
 * never intercepts touches — the gestures live on `SelectionRoot`.
 *
 * This is now the only thing that draws a selection anywhere in the tree. It
 * used to share that job with the platform and had to be told which block to
 * skip; see `overlay.ts` for what that cost.
 */
export const SelectionOverlay: React.FC<SelectionOverlayProps> = ({
  color = 'rgba(51,153,255,0.35)',
  zIndex = 10,
}) => {
  const rects = useSelectionRects();
  return (
    <View pointerEvents="none" style={[StyleSheet.absoluteFill, { zIndex }]}>
      {rects.map((r, i) => (
        <View
          key={i}
          pointerEvents="none"
          style={{
            position: 'absolute',
            left: r.x,
            top: r.y,
            width: r.w,
            height: r.h,
            backgroundColor: color,
            borderRadius: 2,
          }}
        />
      ))}
    </View>
  );
};
