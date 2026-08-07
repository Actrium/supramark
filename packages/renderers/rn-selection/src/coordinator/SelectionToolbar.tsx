import React, { useState } from 'react';
import { Text, TouchableOpacity, View, type LayoutChangeEvent } from 'react-native';
import type { LocalRect } from '../metrics';
import { computeHandles, HANDLE_TOUCH_RADIUS } from './handles';
import { computeToolbarPlacement, type Size } from './toolbar';
import { useSelectionRects } from './SelectionOverlay';
import { useSelectionContext, useSelectionSnapshot } from './useDocumentSelection';

export interface SelectionToolbarProps {
  /** Visible area in `SelectionRoot` space, used to keep the bar on screen. */
  viewport: Size;
  backgroundColor?: string;
  textColor?: string;
  zIndex?: number;
}

function handleTouchAvoidRects(rects: readonly LocalRect[]): LocalRect[] {
  const handles = computeHandles(rects);
  if (handles === null) return [];
  const diameter = HANDLE_TOUCH_RADIUS * 2;
  return [handles.start, handles.end]
    .filter(handle => handle.visible)
    .map(handle => ({
      x: handle.knobX - HANDLE_TOUCH_RADIUS,
      y: handle.knobY - HANDLE_TOUCH_RADIUS,
      w: diameter,
      h: diameter,
    }));
}

/**
 * The action bar that appears over a completed selection — the thing that used
 * to be the platform's edit menu.
 *
 * Visibility is derived, not stored: the bar shows exactly while the store is
 * in `selected`. Dragging re-enters `selecting` and hides it, releasing commits
 * and brings it back, and a tap clears the selection and takes it away. There
 * is no separate "menu is open" state to fall out of sync — which is precisely
 * the failure mode the platform menu had, since it dismissed itself on events
 * we could not observe.
 *
 * Placement lives in `toolbar.ts`; this component only measures itself and
 * positions the result.
 */
export const SelectionToolbar: React.FC<SelectionToolbarProps> = ({
  viewport,
  backgroundColor = '#2b2b2b',
  textColor = '#ffffff',
  zIndex = 12,
}) => {
  const { store, toolbarItems, runToolbarItem } = useSelectionContext();
  const snapshot = useSelectionSnapshot(store);
  const rects = useSelectionRects();
  const [size, setSize] = useState<Size | null>(null);

  const onLayout = (e: LayoutChangeEvent) => {
    const { width, height } = e.nativeEvent.layout;
    // Only react to real changes: setting state from every layout pass would
    // re-render, re-measure and loop.
    setSize(prev =>
      prev !== null && prev.w === width && prev.h === height ? prev : { w: width, h: height }
    );
  };

  if (snapshot.phase !== 'selected' || rects.length === 0 || toolbarItems.length === 0) return null;

  // First pass: render off-screen to measure, then position on the next frame.
  const placement =
    size === null
      ? null
      : computeToolbarPlacement(rects, size, viewport, {
          avoidRects: handleTouchAvoidRects(rects),
        });

  return (
    <View
      onLayout={onLayout}
      style={{
        position: 'absolute',
        left: placement?.x ?? 0,
        top: placement?.y ?? 0,
        opacity: placement === null ? 0 : 1,
        flexDirection: 'row',
        alignItems: 'center',
        backgroundColor,
        borderRadius: 8,
        paddingHorizontal: 4,
        zIndex,
      }}
    >
      {toolbarItems.map(item => (
        <TouchableOpacity
          key={item.id}
          accessibilityRole="button"
          onPress={() => runToolbarItem(item)}
          style={{ paddingHorizontal: 12, paddingVertical: 8 }}
        >
          <Text style={{ color: textColor, fontSize: 14 }}>{item.title}</Text>
        </TouchableOpacity>
      ))}
    </View>
  );
};
