import { UIManager, type GestureResponderEvent } from 'react-native';
import type { Point } from './hitTest';

interface ViewportView {
  measureInWindow?: (callback: (x: number, y: number) => void) => void;
}

/** Fabric runs these layout queries synchronously against the current shadow tree. */
export function macMousePoint(
  event: GestureResponderEvent,
  root: ViewportView | null
): Point | null {
  if (!root?.measureInWindow) return null;
  const native = event.nativeEvent;
  // A nested Text can be the event emitter, while locationX/Y belong to the physical paragraph.
  const touch = native.changedTouches?.find(t => t.identifier === native.identifier) ?? native;
  const tag = Number(touch.target);
  if (!tag || !Number.isFinite(touch.locationX) || !Number.isFinite(touch.locationY)) return null;

  let point: Point | null = null;
  root.measureInWindow((rootX, rootY) => {
    UIManager.measureInWindow(tag, (x, y, width, height) => {
      if (!(width > 0 && height > 0)) return;
      const localX = touch.locationX + x - rootX;
      const localY = touch.locationY + y - rootY;
      if (Number.isFinite(localX) && Number.isFinite(localY)) point = { x: localX, y: localY };
    });
  });
  return point;
}
