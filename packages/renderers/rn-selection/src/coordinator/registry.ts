import type { SelectionNodeId, SelectionUnit } from '../model';
import type { SegmentTextMetrics } from '../metrics';
import { buildUnitIndex, type SelectionUnitIndex } from '../resolve';

/** A block's laid-out box in the `SelectionRoot`'s coordinate space. */
export interface LayoutRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Offset of a block's text content box from the block's own top-left. */
export interface ContentOffset {
  x: number;
  y: number;
}

/**
 * A rendered document block registered upward into the registry.
 *
 * `metrics` and `contentOffset` are what a text block contributes to the
 * self-drawn selection: the line table it laid out, and where that table's
 * origin sits inside the block's box. Both are absent until the block's text
 * has been measured (and always absent for atoms/boundaries), so every consumer
 * has to degrade gracefully — hit-testing falls back to before/after, and the
 * highlight falls back to the whole block rect.
 */
export interface RegisteredBlock {
  nodeId: SelectionNodeId;
  unitIds: readonly SelectionNodeId[];
  kind: 'text' | 'atom' | 'boundary';
  rect?: LayoutRect;
  metrics?: SegmentTextMetrics;
  contentOffset?: ContentOffset;
  /** Nearest nested scroll viewport that owns this block, when any. */
  viewportId?: string;
  /** Visible clip in SelectionRoot coordinates, supplied by the viewport. */
  clipRect?: LayoutRect;
  /** Viewport offset at which `rect` was last measured or translated. */
  viewportOffset?: ContentOffset;
}

interface RegisteredViewport {
  rect?: LayoutRect;
  offset: ContentOffset;
}

export type RegistryChange =
  | 'register'
  | 'unregister'
  | 'layout'
  | 'units'
  | 'metrics'
  | 'viewport';

/** Intersection in root coordinates, or null when the rectangles do not overlap. */
export function intersectLayoutRects(a: LayoutRect, b: LayoutRect): LayoutRect | null {
  const x = Math.max(a.x, b.x);
  const y = Math.max(a.y, b.y);
  const right = Math.min(a.x + a.w, b.x + b.w);
  const bottom = Math.min(a.y + a.h, b.y + b.h);
  if (right <= x || bottom <= y) return null;
  return { x, y, w: right - x, h: bottom - y };
}

/** The part of a block that can currently receive touches or be painted. */
export function visibleBlockRect(block: RegisteredBlock): LayoutRect | null {
  if (block.rect === undefined) return null;
  if (block.clipRect === undefined) return block.rect;
  return intersectLayoutRects(block.rect, block.clipRect);
}

/** Blocks whose units are absent from the index sort after every indexed one. */
const ORDER_LAST = Number.MAX_SAFE_INTEGER;

/**
 * Document-order key for a block: the smallest linearized-unit index across the
 * block's units. Units missing from the index contribute `ORDER_LAST` so a block
 * built only of unknown ids sorts last. Exported for tests.
 */
export function orderKey(block: RegisteredBlock, index: SelectionUnitIndex): number {
  let min = ORDER_LAST;
  for (const unitId of block.unitIds) {
    const idx = index.byUnitId.get(unitId);
    if (idx !== undefined && idx < min) min = idx;
  }
  return min;
}

/**
 * Pure, React-free registry of rendered blocks. Owns block layout plus a
 * document-ordered iteration derived from the linearized unit stream (NOT
 * registration order). Streaming markdown updates re-index via `setUnits`.
 */
export class SelectionRegistry {
  private _index: SelectionUnitIndex;
  private blocks = new Map<SelectionNodeId, RegisteredBlock>();
  private viewports = new Map<string, RegisteredViewport>();
  private listeners = new Set<(change: RegistryChange, nodeId: SelectionNodeId) => void>();
  private orderCache: RegisteredBlock[] | null = null;
  private unitToNode: Map<SelectionNodeId, SelectionNodeId> | null = null;
  private _version = 0;

  constructor(units: readonly SelectionUnit[]) {
    this._index = buildUnitIndex(units);
  }

  /** The current unit index; rebuilt by `setUnits`, read externally only. */
  get index(): SelectionUnitIndex {
    return this._index;
  }

  /**
   * Monotonic revision bumped on every mutation notification. A React overlay
   * subscribes to `subscribe` and reads this as its `useSyncExternalStore`
   * snapshot so that layout/unit changes (which do NOT change the `getBlocks`
   * array reference) still trigger a repaint. Arrow property so it can be passed
   * as a bare `getSnapshot` without losing `this`.
   */
  getVersion = (): number => this._version;

  /**
   * Rebuild the index for a new unit stream (streaming markdown). Registered
   * blocks are kept; the document-order cache is invalidated because unit
   * positions may have shifted. The unitId -> nodeId map is derived from block
   * registrations, not units, so it stays valid.
   */
  setUnits(units: readonly SelectionUnit[]): void {
    this._index = buildUnitIndex(units);
    this.orderCache = null;

    // Drop blocks whose units no longer exist in the new stream. A re-parse
    // reassigns unit ids (`linearize.ts` derives them as `pos:<start>-<end>`),
    // so a block left behind by a removed paragraph would keep its entry
    // forever: `orderKey` gives it ORDER_LAST so it bunches at the end of
    // `getBlocks()` and corrupts the document ordering `chooseBlock` relies on,
    // and `getBlockForUnit` still resolves its dead unit ids into
    // `planNativeSelection`'s ownership vote.
    //
    // A block with no unit ids at all is kept: React children effects run
    // before the parent's, so a freshly mounted block has already called
    // `updateUnits` by the time this runs — but a block mid-registration may
    // legitimately still be empty, and dropping it would unregister a live one.
    let dropped = false;
    for (const [nodeId, block] of this.blocks) {
      if (block.unitIds.length === 0) continue;
      const alive = block.unitIds.some(id => this._index.byUnitId.has(id));
      if (!alive) {
        this.blocks.delete(nodeId);
        dropped = true;
      }
    }
    if (dropped) this.unitToNode = null;
  }

  /**
   * Register a block and return the object actually stored, which callers pass
   * back to `unregister` as an identity guard — see that method.
   */
  register(block: RegisteredBlock): RegisteredBlock {
    // Preserve previously measured geometry when a re-registration carries
    // none. A React block re-registers (effect cleanup + re-run) with no rect
    // and no metrics while its `onLayout` / `onTextLayout` do not re-fire on an
    // unchanged layout; without this the measured box and line table would be
    // lost and the overlay/hit-test would go blank.
    const existing = this.blocks.get(block.nodeId);
    const next = { ...block };
    if (existing !== undefined) {
      if (next.rect === undefined && next.viewportId === existing.viewportId) {
        next.rect = existing.rect;
        next.viewportOffset = existing.viewportOffset;
      }
      if (next.metrics === undefined) next.metrics = existing.metrics;
      if (next.contentOffset === undefined) next.contentOffset = existing.contentOffset;
    }
    if (next.viewportId !== undefined) {
      const viewport = this.viewports.get(next.viewportId);
      next.clipRect = viewport?.rect;
      if (next.viewportOffset === undefined && viewport !== undefined) {
        next.viewportOffset = { ...viewport.offset };
      }
    } else {
      next.clipRect = undefined;
      next.viewportOffset = undefined;
    }
    this.blocks.set(block.nodeId, next);
    this.orderCache = null;
    this.unitToNode = null;
    this.notify('register', block.nodeId);
    return next;
  }

  /**
   * Replace a registered block's `unitIds` in place, preserving its measured
   * `rect` and native `handle`. Used when a block keeps its `nodeId` but its
   * unit stream grows (streaming markdown); re-registering instead would drop
   * the rect between unregister and register.
   */
  updateUnits(nodeId: SelectionNodeId, unitIds: readonly SelectionNodeId[]): void {
    const block = this.blocks.get(nodeId);
    if (!block) return;
    block.unitIds = unitIds;
    this.orderCache = null;
    this.unitToNode = null;
    this.notify('units', nodeId);
  }

  /**
   * Remove a block. When `expected` is given, the entry is removed only if it
   * is still the one that was registered — pass the value `register` returned.
   *
   * React mounts a replacement before unmounting the component it replaces, so
   * two instances sharing a `nodeId` produce the order
   * register(new) -> unregister(old). Without the guard, the old instance's
   * cleanup deletes the LIVE registration, leaving the mounted block with no
   * rect and no handle. This is not hypothetical: `linearize.ts` derives block
   * ids as `pos:<start>-<end>`, which a re-parse readily reassigns to a
   * different component instance.
   */
  unregister(nodeId: SelectionNodeId, expected?: RegisteredBlock): void {
    if (expected !== undefined && this.blocks.get(nodeId) !== expected) return;
    if (!this.blocks.delete(nodeId)) return;
    this.orderCache = null;
    this.unitToNode = null;
    this.notify('unregister', nodeId);
  }

  updateLayout(nodeId: SelectionNodeId, rect: LayoutRect): void {
    const block = this.blocks.get(nodeId);
    if (!block) return;
    block.rect = rect;
    if (block.viewportId !== undefined) {
      const viewport = this.viewports.get(block.viewportId);
      block.viewportOffset = viewport === undefined ? undefined : { ...viewport.offset };
      block.clipRect = viewport?.rect;
    }
    this.notify('layout', nodeId);
  }

  /** Register a nested scroll viewport and attach any already-mounted blocks. */
  registerViewport(viewportId: string, initialOffset: ContentOffset = { x: 0, y: 0 }): () => void {
    const viewport: RegisteredViewport = { offset: { ...initialOffset } };
    this.viewports.set(viewportId, viewport);
    for (const block of this.blocks.values()) {
      if (block.viewportId !== viewportId) continue;
      block.clipRect = viewport.rect;
      block.viewportOffset = { ...viewport.offset };
    }
    this.notify('viewport', viewportId);
    return () => {
      if (this.viewports.get(viewportId) !== viewport) return;
      this.viewports.delete(viewportId);
      for (const block of this.blocks.values()) {
        if (block.viewportId !== viewportId) continue;
        block.clipRect = undefined;
        block.viewportOffset = undefined;
      }
      this.notify('viewport', viewportId);
    };
  }

  /** Update the viewport's visible bounds in SelectionRoot coordinates. */
  updateViewportLayout(viewportId: string, rect: LayoutRect): void {
    const viewport = this.viewports.get(viewportId);
    if (viewport === undefined) return;
    viewport.rect = rect;
    for (const block of this.blocks.values()) {
      if (block.viewportId === viewportId) block.clipRect = rect;
    }
    this.notify('viewport', viewportId);
  }

  /**
   * Move all mounted blocks synchronously with their scroll viewport.
   *
   * Native measurement is asynchronous and one callback per FlatList cell can
   * arrive several frames late. A scroll offset is already the exact movement,
   * so translate each cached root-space rect by its delta and publish once.
   */
  updateViewportScroll(viewportId: string, offset: ContentOffset): void {
    const viewport = this.viewports.get(viewportId);
    if (viewport === undefined) return;
    if (viewport.offset.x === offset.x && viewport.offset.y === offset.y) return;
    viewport.offset = { ...offset };
    for (const block of this.blocks.values()) {
      if (block.viewportId !== viewportId || block.rect === undefined) continue;
      const previous = block.viewportOffset ?? offset;
      block.rect = {
        ...block.rect,
        x: block.rect.x + previous.x - offset.x,
        y: block.rect.y + previous.y - offset.y,
      };
      block.viewportOffset = { ...offset };
      block.clipRect = viewport.rect;
    }
    this.notify('viewport', viewportId);
  }

  /**
   * Record the line table a text block laid out. Re-measured on every text or
   * width change, so this is the hot path for streaming Markdown: it mutates in
   * place and bumps the version rather than re-registering, keeping the block's
   * rect and unit ids intact.
   */
  setMetrics(nodeId: SelectionNodeId, metrics: SegmentTextMetrics): void {
    const block = this.blocks.get(nodeId);
    if (!block) return;
    block.metrics = metrics;
    this.notify('metrics', nodeId);
  }

  /**
   * Record where a block's text content box starts relative to the block's own
   * box. Line coordinates are relative to the text, block rects to the root, so
   * without this every highlight would be off by the block's padding.
   */
  setContentOffset(nodeId: SelectionNodeId, offset: ContentOffset): void {
    const block = this.blocks.get(nodeId);
    if (!block) return;
    block.contentOffset = offset;
    this.notify('metrics', nodeId);
  }

  getBlock(nodeId: SelectionNodeId): RegisteredBlock | undefined {
    return this.blocks.get(nodeId);
  }

  getBlockForUnit(unitId: SelectionNodeId): RegisteredBlock | undefined {
    if (!this.unitToNode) {
      this.unitToNode = new Map();
      for (const block of this.blocks.values()) {
        for (const id of block.unitIds) this.unitToNode.set(id, block.nodeId);
      }
    }
    const nodeId = this.unitToNode.get(unitId);
    return nodeId === undefined ? undefined : this.blocks.get(nodeId);
  }

  /**
   * Registered blocks in document order (by `orderKey`), cached until the block
   * set or index changes.
   */
  getBlocks(): RegisteredBlock[] {
    if (this.orderCache) return this.orderCache;
    const sorted = [...this.blocks.values()].sort(
      (a, b) => orderKey(a, this._index) - orderKey(b, this._index)
    );
    this.orderCache = sorted;
    return sorted;
  }

  subscribe(listener: (change: RegistryChange, nodeId: SelectionNodeId) => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private notify(change: RegistryChange, nodeId: SelectionNodeId): void {
    this._version++;
    for (const listener of this.listeners) listener(change, nodeId);
  }
}
