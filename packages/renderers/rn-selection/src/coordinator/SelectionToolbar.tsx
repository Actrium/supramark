import React, { useState, useContext, useEffect, useMemo } from 'react';
import { Text, TouchableOpacity, View, type LayoutChangeEvent } from 'react-native';
import { ToolbarPortalContext } from './ToolbarPortalContext';
import type { LocalRect } from '../metrics';
import { computeHandles, HANDLE_TOUCH_RADIUS } from './handles';
import { computeToolbarPlacement, type Size, type SelectionToolbarItem } from './toolbar';
import { useSelectionRects } from './SelectionOverlay';
import { useSelectionContext, useSelectionSnapshot } from './useDocumentSelection';

export interface SelectionToolbarProps {
  /** Visible area in `SelectionRoot` space, used to keep the bar on screen. */
  viewport: Size;
  /** Native document root used to place the toolbar in a host portal. */
  anchor?: React.RefObject<View | null>;
  backgroundColor?: string;
  textColor?: string;
  zIndex?: number;
}

// Unique descriptions keep the host's portal frame keys distinct per publisher.
let toolbarIdSeq = 0;

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
  anchor,
  backgroundColor = '#2b2b2b',
  textColor = '#ffffff',
  zIndex = 12,
}) => {
  const { store, toolbarItems, runToolbarItem } = useSelectionContext();
  const snapshot = useSelectionSnapshot(store);
  const rects = useSelectionRects();
  // Publish the same selection into the host's visible area instead of the small bubble.
  const portal = useContext(ToolbarPortalContext);
  const id = useMemo(() => Symbol(`selection-toolbar-${++toolbarIdSeq}`), []);
  const visible = snapshot.phase === 'selected' && rects.length > 0 && toolbarItems.length > 0;
  useEffect(() => {
    // Standalone roots retain their local toolbar when no application host is mounted.
    if (!portal || !anchor) return;
    portal.set(
      id,
      visible
        ? {
            anchor,
            rects,
            items: toolbarItems,
            run: runToolbarItem,
            backgroundColor,
            textColor,
            zIndex,
          }
        : null
    );
    return () => portal.set(id, null);
  }, [
    portal,
    anchor,
    id,
    visible,
    rects,
    toolbarItems,
    runToolbarItem,
    backgroundColor,
    textColor,
    zIndex,
  ]);
  if (!visible || (portal && anchor)) return null;
  return (
    <SelectionToolbarView
      viewport={viewport}
      rects={rects}
      items={toolbarItems}
      run={runToolbarItem}
      backgroundColor={backgroundColor}
      textColor={textColor}
      zIndex={zIndex}
    />
  );
};

/** Measure menu content against the host width, independently of its final horizontal position. */
export function SelectionToolbarView({
  viewport,
  rects,
  items,
  run,
  backgroundColor = '#2b2b2b',
  textColor = '#ffffff',
  zIndex = 12,
}: {
  viewport: Size;
  rects: readonly LocalRect[];
  items: readonly SelectionToolbarItem[];
  run(item: SelectionToolbarItem): void;
  backgroundColor?: string;
  textColor?: string;
  zIndex?: number;
}) {
  // The native intrinsic width determines placement; a bubble never constrains this measurement.
  const [size, setSize] = useState<Size | null>(null);
  const onLayout = (e: LayoutChangeEvent) => {
    const { width, height } = e.nativeEvent.layout;
    // Only react to real changes: setting state from every layout pass would
    // re-render, re-measure and loop.
    setSize(prev =>
      prev !== null && prev.w === width && prev.h === height ? prev : { w: width, h: height }
    );
  };

  // An empty action list has no visual or touch surface.
  if (items.length === 0 || rects.length === 0) return null;

  // First pass: render off-screen to measure, then position on the next frame.
  const placement =
    size === null
      ? null
      : computeToolbarPlacement(rects, size, viewport, {
          avoidRects: handleTouchAvoidRects(rects),
        });

  return (
    <View
      pointerEvents={placement === null ? 'none' : 'box-none'}
      accessibilityElementsHidden={placement === null}
      importantForAccessibility={placement === null ? 'no-hide-descendants' : 'auto'}
      // Toolbar presses belong to actions, not the host's background dismissal handler.
      onTouchStart={event => event.stopPropagation()}
      onTouchEnd={event => event.stopPropagation()}
      style={{
        position: 'absolute',
        left: placement?.x ?? 0,
        top: placement?.y ?? 0,
        width: Math.max(0, viewport.w - 16),
        zIndex,
        opacity: placement === null ? 0 : 1,
      }}
    >
      <View
        onLayout={onLayout}
        style={{
          alignSelf: 'flex-start',
          maxWidth: Math.max(0, viewport.w - 16),
          flexWrap: 'wrap',
          flexDirection: 'row',
          alignItems: 'center',
          backgroundColor,
          borderRadius: 8,
          paddingHorizontal: 4,
          zIndex,
        }}
      >
        {items.map(item => (
          <TouchableOpacity
            key={item.id}
            accessibilityRole="button"
            onPress={() => run(item)}
            style={{ paddingHorizontal: 12, paddingVertical: 8, flexShrink: 0 }}
          >
            <Text numberOfLines={1} style={{ color: textColor, fontSize: 14 }}>
              {item.title}
            </Text>
          </TouchableOpacity>
        ))}
      </View>
    </View>
  );
}
