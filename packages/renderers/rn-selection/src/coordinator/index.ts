export * from './registry';
export * from './hitTest';
export * from './state';
// Re-exported under an alias at the barrel: the model-level `SelectionContext`
// (the provider behavior context in `../model`) already owns that name at the
// package's public surface. The React context object itself is rarely needed
// directly — `useSelectionContext()` is the intended consumer entrypoint.
export {
  SelectionContext as SelectionUIContext,
  type SelectionContextValue,
} from './SelectionContext';
export * from './useDocumentSelection';
export * from './overlay';
export * from './handles';
export * from './toolbar';
export * from './actions';
export * from './gesture';
export {
  SelectionRoot,
  pointToSelectionForRoot,
  type SelectionRootProps,
  type SelectionToolbarRenderProps,
} from './SelectionRoot';
export {
  SelectionOverlay,
  useSelectionRects,
  type SelectionOverlayProps,
} from './SelectionOverlay';
export { SelectionHandles, type SelectionHandlesProps } from './SelectionHandles';
export { SelectionToolbar, type SelectionToolbarProps } from './SelectionToolbar';
export { SelectableBlock, type SelectableBlockProps } from './SelectableBlock';
export { SelectionViewport, type SelectionViewportProps } from './SelectionViewport';
