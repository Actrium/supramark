import { useContext, useMemo, useSyncExternalStore } from 'react';
import type { SelectionPoint } from '../model';
import { SelectionContext, type SelectionContextValue } from './SelectionContext';
import type { SelectionSnapshot, SelectionStore } from './state';

const noopRefreshLayouts = () => {};

/**
 * Subscribe a component to a `SelectionStore` and expose its bound actions.
 * Pure wiring: `useSyncExternalStore` reads the store's cached snapshot (the
 * server-snapshot arg is the same getter, so it is SSR-safe). No logic here.
 */
export function useDocumentSelection(store: SelectionStore): {
  snapshot: SelectionSnapshot;
  beginAt(p: SelectionPoint): void;
  extendTo(p: SelectionPoint): void;
  commit(): void;
  clear(): void;
} {
  const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
  const actions = useMemo(
    () => ({
      beginAt: (p: SelectionPoint) => store.beginAt(p),
      extendTo: (p: SelectionPoint) => store.extendTo(p),
      commit: () => store.commit(),
      clear: () => store.clear(),
    }),
    [store]
  );
  return { snapshot, ...actions };
}

/**
 * Subscribe to just the snapshot. Same `useSyncExternalStore` contract as
 * `useDocumentSelection`, without minting the action object — for the several
 * internal components that only need to know what is selected.
 */
export function useSelectionSnapshot(store: SelectionStore): SelectionSnapshot {
  return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
}

/** Read the selection context; throws when used outside a `SelectionRoot`. */
export function useSelectionContext(): SelectionContextValue {
  const value = useContext(SelectionContext);
  if (value === null) {
    throw new Error('useSelectionContext must be used within a SelectionRoot');
  }
  return value;
}

/**
 * Read the `SelectionStore` off the context so the overlay and host controls
 * under the root can subscribe without prop-drilling.
 */
export function useSelectionStore(): SelectionStore {
  return useSelectionContext().store;
}

/**
 * Re-measure registered selectable blocks against the current SelectionRoot.
 * Use this from nested ScrollView / FlatList `onScroll` handlers.
 */
export function useSelectionLayoutRefresh(): () => void {
  return useSelectionContext().refreshLayouts ?? noopRefreshLayouts;
}
