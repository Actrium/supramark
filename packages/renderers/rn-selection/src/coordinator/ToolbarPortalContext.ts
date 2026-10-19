import { createContext, type RefObject } from 'react';
import type { View } from 'react-native';
import type { LocalRect } from '../metrics';
import type { SelectionToolbarItem } from './toolbar';

/** A toolbar's selection geometry remains relative to its native document root. */
export interface ToolbarPortalEntry {
  anchor: RefObject<View | null>;
  rects: readonly LocalRect[];
  items: readonly SelectionToolbarItem[];
  run(this: void, item: SelectionToolbarItem): void;
  backgroundColor: string;
  textColor: string;
  /** Carried through the portal so stacked bars keep their publisher's stacking order. */
  zIndex: number;
}

/** Host-local portal; no module-level registry or cross-document state. */
export const ToolbarPortalContext = createContext<{
  set(id: symbol, entry: ToolbarPortalEntry | null): void;
} | null>(null);
