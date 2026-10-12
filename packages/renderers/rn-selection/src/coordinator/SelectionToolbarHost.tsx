import React, {
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from 'react';
import { View, type ViewProps } from 'react-native';
import type { LocalRect } from '../metrics';
import { SelectionToolbarView } from './SelectionToolbar';
import { ToolbarPortalContext, type ToolbarPortalEntry } from './ToolbarPortalContext';

/** Refresh after native scrolling moves document roots without a layout event. */
export interface SelectionToolbarHostHandle {
  refresh(): void;
}

/** Bound a measurement that a platform may never answer (a detached native view). */
const MEASURE_TIMEOUT_MS = 500;

/** Measure both surfaces in window coordinates before translating a toolbar anchor. */
function measure(view: View | null): Promise<LocalRect | null> {
  return new Promise(resolve => {
    // Unmounted roots do not retain an actionable floating toolbar.
    if (!view?.measureInWindow) {
      resolve(null);
      return;
    }
    let settled = false;
    const settle = (rect: LocalRect | null) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(rect);
    };
    // A recycled native view can swallow the callback and leave the refresh pending.
    const timer = setTimeout(() => settle(null), MEASURE_TIMEOUT_MS);
    view.measureInWindow((x, y, w, h) => settle(w > 0 && h > 0 ? { x, y, w, h } : null));
  });
}

/** Place one portal above the chat viewport, independent of individual message sizes and clipping. */
export function SelectionToolbarHost({
  children,
  hostRef,
  onLayout,
  ...props
}: ViewProps & {
  hostRef?: React.Ref<SelectionToolbarHostHandle>;
}) {
  // Native host bounds are the actual visible area supplied by the application.
  const viewRef = useRef<View | null>(null);
  // Entries belong to mounted SelectionToolbar instances, keyed independently of message ids.
  const entries = useRef(new Map<symbol, ToolbarPortalEntry>());
  // Discard measurements overtaken by scroll, selection changes, or unmounts.
  const generation = useRef(0);
  // Render only after both root and host geometry have been measured together.
  const [frames, setFrames] = useState<
    Array<{ id: symbol; entry: ToolbarPortalEntry; viewport: LocalRect }>
  >([]);
  // Remeasure all visible toolbars without causing their publishers to remount.
  const refresh = useCallback(async () => {
    const version = ++generation.current;
    // Ordinary chat scrolling must not measure or rerender an unused toolbar host.
    if (entries.current.size === 0) {
      // Deferred by one microtask: a toolbar that clears and republishes within the
      // same commit (registry bump re-running its publish effect) bumps the version
      // first, so this stale clear is dropped instead of unmounting the bar.
      void Promise.resolve().then(() => {
        if (version !== generation.current || entries.current.size !== 0) return;
        setFrames(previous => (previous.length === 0 ? previous : []));
      });
      return;
    }
    const host = await measure(viewRef.current);
    const pending = await Promise.all(
      [...entries.current].map(async ([id, entry]) => {
        const root = await measure(entry.anchor.current);
        // Missing geometry cannot produce a valid portal position.
        if (!host || !root) return null;
        return {
          id,
          entry: {
            ...entry,
            rects: entry.rects.map(rect => ({
              ...rect,
              x: rect.x + root.x - host.x,
              y: rect.y + root.y - host.y,
            })),
          },
          viewport: { x: 0, y: 0, w: host.w, h: host.h },
        };
      })
    );
    // A later request owns the current selection and geometry.
    if (version !== generation.current) return;
    setFrames(pending.filter((frame): frame is NonNullable<typeof frame> => frame !== null));
  }, []);
  // A stable provider prevents the portal's layout from republishing every document.
  const portal = useMemo(
    () => ({
      set(id: symbol, entry: ToolbarPortalEntry | null) {
        // Removing a publisher also removes its pending measurement from the next frame.
        if (entry === null) entries.current.delete(id);
        else entries.current.set(id, entry);
        void refresh();
      },
    }),
    [refresh]
  );
  // Invalidate native callbacks when the whole chat viewport unmounts.
  useEffect(
    () => () => {
      generation.current += 1;
      entries.current.clear();
    },
    []
  );
  useImperativeHandle(
    hostRef,
    () => ({
      refresh: () => {
        void refresh();
      },
    }),
    [refresh]
  );
  return (
    <ToolbarPortalContext.Provider value={portal}>
      <View
        {...props}
        ref={viewRef}
        collapsable={false}
        onLayout={event => {
          onLayout?.(event);
          void refresh();
        }}
      >
        {children}
        <View
          pointerEvents="box-none"
          style={{ position: 'absolute', left: 0, right: 0, top: 0, bottom: 0, zIndex: 1000 }}
        >
          {frames.map(({ id, entry, viewport }) => (
            <SelectionToolbarView
              // The publisher's stable symbol keeps sibling toolbars from remounting
              // when another entry leaves the frame list.
              key={id.toString()}
              viewport={viewport}
              rects={entry.rects}
              items={entry.items}
              run={entry.run}
              backgroundColor={entry.backgroundColor}
              textColor={entry.textColor}
              zIndex={entry.zIndex}
            />
          ))}
        </View>
      </View>
    </ToolbarPortalContext.Provider>
  );
}
