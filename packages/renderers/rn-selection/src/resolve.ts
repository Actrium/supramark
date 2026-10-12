import type {
  SelectionPoint,
  SelectionRange,
  SelectionSourceRange,
  SelectionTextUnit,
  SelectionUnit,
} from './model';
import { snapToGraphemeBoundary } from './text';

export interface UnitIndexEntry {
  unit: SelectionUnit;
  unitIndex: number;
  /** Cumulative UTF-16 start of this unit in the global plain-text stream. */
  startOffset: number;
  /** `unit.text?.length ?? 0` — text/break advance the stream, atoms do not. */
  textLength: number;
}

export interface SelectionUnitIndex {
  entries: UnitIndexEntry[];
  byUnitId: Map<string, number>;
  byNodeId: Map<string, number[]>;
  /**
   * First and last unit index carrying each `structuralGroup` id. Derived in
   * the same pass as the rest of the index because it depends only on the unit
   * stream — `findPartialStructuralGroups` used to recompute it by walking all
   * N entries on every `beginAt` / `extendTo` / `commit`, i.e. on every frame
   * of a drag.
   */
  structuralGroupBounds: Map<string, { min: number; max: number }>;
  /**
   * The selectable region each unit belongs to, or -1 for a hard boundary
   * (atom / boundary units). Selection may not cross a hard boundary, so a
   * range whose endpoints land in different regions is clamped to the
   * anchor's; geometry, highlighting and copying all see the clamped range.
   */
  regionOf: number[];
  /** Inclusive unit-index spans of the selectable regions, in document order. */
  regions: Array<{ start: number; end: number }>;
}

/** Length a unit contributes to the global plain-text stream. */
function unitTextLength(unit: SelectionUnit): number {
  if (unit.kind === 'text' || unit.kind === 'break') return unit.text.length;
  return 0;
}

function clamp(value: number, min: number, max: number): number {
  if (value < min) return min;
  if (value > max) return max;
  return value;
}

/**
 * Index the unit stream in a single pass: accumulate global start offsets and
 * build the `unitId`/`nodeId` lookup maps used by point resolution.
 */
export function buildUnitIndex(units: readonly SelectionUnit[]): SelectionUnitIndex {
  const entries: UnitIndexEntry[] = [];
  const byUnitId = new Map<string, number>();
  const byNodeId = new Map<string, number[]>();
  const structuralGroupBounds = new Map<string, { min: number; max: number }>();
  const regionOf: number[] = [];
  const regions: Array<{ start: number; end: number }> = [];
  let startOffset = 0;
  units.forEach((unit, unitIndex) => {
    const textLength = unitTextLength(unit);
    entries.push({ unit, unitIndex, startOffset, textLength });
    // Unit ids must be unique per document. A collision would silently re-point
    // every offset past it, so the FIRST holder keeps the id (a later unit never
    // steals it) and dev builds report the collision — see detectDuplicateUnitIds.
    if (!byUnitId.has(unit.unitId)) {
      byUnitId.set(unit.unitId, unitIndex);
    }
    const existing = byNodeId.get(unit.nodeId);
    if (existing) existing.push(unitIndex);
    else byNodeId.set(unit.nodeId, [unitIndex]);
    const group = unit.kind === 'text' ? unit.structuralGroup : undefined;
    if (group !== undefined) {
      const bounds = structuralGroupBounds.get(group);
      if (bounds) {
        if (unitIndex < bounds.min) bounds.min = unitIndex;
        if (unitIndex > bounds.max) bounds.max = unitIndex;
      } else {
        structuralGroupBounds.set(group, { min: unitIndex, max: unitIndex });
      }
    }
    // Hard boundaries are not selectable and split the document into regions.
    if (unit.kind === 'atom' || unit.kind === 'boundary') {
      regionOf.push(-1);
    } else {
      const open = regions[regions.length - 1];
      if (open !== undefined && open.end === unitIndex - 1) {
        open.end = unitIndex;
        regionOf.push(regions.length - 1);
      } else {
        regions.push({ start: unitIndex, end: unitIndex });
        regionOf.push(regions.length - 1);
      }
    }
    startOffset += textLength;
  });
  reportDuplicateUnitIds(units);
  return { entries, byUnitId, byNodeId, structuralGroupBounds, regionOf, regions };
}

/**
 * Clamp a range so its endpoints stay inside one selectable region.
 *
 * Hard boundaries — inline images / math / diagrams, container and unsupported
 * boundaries — are selection limits, not gaps to skip over: hit testing,
 * highlighting and copying must all agree, so the clamp lives here and every
 * consumer (store, overlay, toolbar, serializer) derives from the result. The
 * anchor owns the region; a focus past it clamps to that region's edge, and an
 * anchor sitting on a boundary snaps toward the focus side.
 */
export function clampSelectionRange(
  index: SelectionUnitIndex,
  range: SelectionRange
): SelectionRange {
  const a = locateSelectionPoint(index, range.anchor);
  const f = locateSelectionPoint(index, range.focus);
  const regionA = index.regionOf[a.unitIndex];
  const regionF = index.regionOf[f.unitIndex];
  if (regionA === regionF && regionA !== -1) return range;
  if (regionA === -1 && regionF === -1) {
    // Neither endpoint is selectable: collapse so the range resolves to
    // nothing instead of spanning every boundary in between.
    return { anchor: range.anchor, focus: range.anchor };
  }

  const pointAtStart = (region: number): SelectionPoint => {
    const entry = index.entries[index.regions[region].start];
    return { nodeId: entry.unit.nodeId, unitId: entry.unit.unitId, offset: 0 };
  };
  const pointAtEnd = (region: number): SelectionPoint => {
    const entry = index.entries[index.regions[region].end];
    return { nodeId: entry.unit.nodeId, unitId: entry.unit.unitId, offset: entry.textLength };
  };

  let anchor = range.anchor;
  let owner = regionA;
  if (owner === -1) {
    // The anchor itself sits on a boundary: snap it to the nearest region on
    // the focus side. With no region there, nothing selectable is in reach.
    const forward = f.unitIndex > a.unitIndex;
    let found = -1;
    if (forward) {
      for (let region = 0; region < index.regions.length; region += 1) {
        if (index.regions[region].start > a.unitIndex) {
          found = region;
          break;
        }
      }
    } else {
      for (let region = index.regions.length - 1; region >= 0; region -= 1) {
        if (index.regions[region].end < a.unitIndex) {
          found = region;
          break;
        }
      }
    }
    if (found === -1) return range;
    owner = found;
    anchor = forward ? pointAtStart(owner) : pointAtEnd(owner);
  }

  const within = index.regions[owner];
  if (f.unitIndex < within.start) return { anchor, focus: pointAtStart(owner) };
  if (f.unitIndex > within.end) return { anchor, focus: pointAtEnd(owner) };
  return { anchor, focus: range.focus };
}

/** A unitId claimed by more than one unit — an identity bug upstream. */
export interface DuplicateUnitId {
  unitId: string;
  /** Index of the unit that keeps the id (the first in document order). */
  firstIndex: number;
  /** Index of the unit whose claim is ignored. */
  duplicateIndex: number;
}

/**
 * Every unitId claimed by more than one unit, in document order. Exported so
 * tests can assert uniqueness on real parser output directly, without depending
 * on `__DEV__` or console capture.
 */
export function detectDuplicateUnitIds(units: readonly SelectionUnit[]): DuplicateUnitId[] {
  const firstSeen = new Map<string, number>();
  const duplicates: DuplicateUnitId[] = [];
  units.forEach((unit, unitIndex) => {
    const first = firstSeen.get(unit.unitId);
    if (first === undefined) {
      firstSeen.set(unit.unitId, unitIndex);
      return;
    }
    duplicates.push({ unitId: unit.unitId, firstIndex: first, duplicateIndex: unitIndex });
  });
  return duplicates;
}

declare const __DEV__: boolean | undefined;

/** `__DEV__` in React Native bundles; off in tests, web and production alike. */
function isDevEnvironment(): boolean {
  return typeof __DEV__ !== 'undefined' && __DEV__ === true;
}

// One report per distinct collision set: the index is rebuilt on every unit
// stream change, and an unfixed collision would otherwise warn on every frame.
const reportedDuplicateSets = new Set<string>();

/** Dev-only: surface duplicate ids with their positions so they can be located. */
function reportDuplicateUnitIds(units: readonly SelectionUnit[]): void {
  if (!isDevEnvironment()) return;
  const duplicates = detectDuplicateUnitIds(units);
  if (duplicates.length === 0) return;
  const signature = duplicates.map(duplicate => duplicate.unitId).join('|');
  if (reportedDuplicateSets.has(signature)) return;
  reportedDuplicateSets.add(signature);
  const shown = duplicates
    .slice(0, 5)
    .map(
      duplicate =>
        `${JSON.stringify(duplicate.unitId)} (#${duplicate.firstIndex} vs #${duplicate.duplicateIndex})`
    )
    .join(', ');
  console.warn(
    `[supramark] ${duplicates.length} duplicate selection unitId(s): ${shown}` +
      `${duplicates.length > 5 ? ', ...' : ''}. ` +
      'Unit ids must be unique per document; every offset past a collision resolves wrongly.'
  );
}

/**
 * Resolve a `SelectionPoint` to a concrete `(unitIndex, intraOffset)`.
 *
 * A direct `unitId` hit wins. Otherwise the units sharing `nodeId` are walked
 * by their text lengths until the offset lands inside one; an offset past the
 * end clamps to the last content unit, and a node made only of zero-text units
 * resolves to its first unit with `intraOffset` encoding before/after.
 */
export function locateSelectionPoint(
  index: SelectionUnitIndex,
  point: SelectionPoint
): { unitIndex: number; intraOffset: number } {
  if (point.unitId !== undefined) {
    const unitIndex = index.byUnitId.get(point.unitId);
    if (unitIndex !== undefined) {
      const entry = index.entries[unitIndex];
      // Zero-text units (atom/boundary) carry no interior; encode before/after
      // exactly like the nodeId walk so a `{unitId, offset: 1}` focus keeps the
      // unit in range instead of collapsing to `before`.
      const intraOffset =
        entry.textLength === 0
          ? point.offset > 0
            ? 1
            : 0
          : clamp(point.offset, 0, entry.textLength);
      return { unitIndex, intraOffset };
    }
  }

  const candidates = index.byNodeId.get(point.nodeId);
  if (candidates && candidates.length > 0) {
    let remaining = point.offset;
    let lastContentIndex = -1;
    for (const unitIndex of candidates) {
      const entry = index.entries[unitIndex];
      if (entry.textLength > 0) {
        lastContentIndex = unitIndex;
        if (remaining <= entry.textLength) {
          return { unitIndex, intraOffset: clamp(remaining, 0, entry.textLength) };
        }
        remaining -= entry.textLength;
      }
    }
    if (lastContentIndex >= 0) {
      // Offset overruns every content unit: clamp to the last one's end.
      return {
        unitIndex: lastContentIndex,
        intraOffset: index.entries[lastContentIndex].textLength,
      };
    }
    // Node only has zero-text units: pick the first, before/after by offset.
    return { unitIndex: candidates[0], intraOffset: point.offset > 0 ? 1 : 0 };
  }

  // Nothing matched — clamp to the document start.
  return { unitIndex: 0, intraOffset: 0 };
}

/**
 * Return a range's endpoints in document order without changing their values.
 *
 * `anchor` and `focus` preserve gesture direction, so after a handle crosses
 * the opposite edge the anchor may follow the focus. Geometry, however, labels
 * the two visible handles as `start` / `end`. Keeping this normalization in one
 * place prevents a visual start handle from accidentally pinning itself when
 * the stored range is reversed.
 */
export function orderSelectionRange(
  index: SelectionUnitIndex,
  range: SelectionRange
): { start: SelectionPoint; end: SelectionPoint } {
  const anchor = locateSelectionPoint(index, range.anchor);
  const focus = locateSelectionPoint(index, range.focus);
  const anchorAfterFocus =
    anchor.unitIndex > focus.unitIndex ||
    (anchor.unitIndex === focus.unitIndex && anchor.intraOffset > focus.intraOffset);
  return anchorAfterFocus
    ? { start: range.focus, end: range.anchor }
    : { start: range.anchor, end: range.focus };
}

/**
 * Slice a text unit to `[from, to)` while preserving identity.
 *
 * `from`/`to` are first widened outward to the nearest grapheme-cluster
 * boundary (`from` snapped backward, `to` snapped forward) so a partial slice
 * never lands inside a multi-code-unit cluster — an astral emoji, a ZWJ
 * sequence, or a base character plus combining marks. This only ever grows
 * the covered range, so it can turn a would-be-partial slice into full
 * coverage but never drops text the caller asked for. See `text.ts` for the
 * snapping rules (and its `Intl.Segmenter`-less fallback).
 *
 * `sourceRange` is shifted so both endpoints are measured from the original
 * `startUtf16` (`startUtf16 + from` / `startUtf16 + to`, post-widening); if
 * that anchor is missing both stay undefined. Byte offsets are copied
 * verbatim (they cannot be shifted by UTF-16 units) and are only approximate
 * on a partial slice.
 */
export function splitTextUnit(
  unit: SelectionTextUnit,
  from: number,
  to: number
): SelectionTextUnit {
  const snappedFrom = snapToGraphemeBoundary(unit.text, from, 'backward');
  const snappedTo = snapToGraphemeBoundary(unit.text, to, 'forward');

  // Full coverage (checked post-widening): return the unit verbatim so its
  // `payload` (whole-unit markdown/source, e.g. an inline-code span or fenced
  // block) survives. A partial slice below deliberately drops the payload — a
  // fence/backtick representation cannot be re-derived for half a unit, so we
  // fall back to the sliced plain text instead of leaking the full syntax
  // into a partial copy.
  if (snappedFrom <= 0 && snappedTo >= unit.text.length) return unit;

  const base = unit.sourceRange;
  const sourceRange: SelectionSourceRange | undefined =
    base === undefined
      ? undefined
      : {
          ...base,
          startUtf16: base.startUtf16 === undefined ? undefined : base.startUtf16 + snappedFrom,
          endUtf16: base.startUtf16 === undefined ? undefined : base.startUtf16 + snappedTo,
        };
  return {
    kind: 'text',
    unitId: unit.unitId,
    nodeId: unit.nodeId,
    text: unit.text.slice(snappedFrom, snappedTo),
    node: unit.node,
    sourceRange,
  };
}

/**
 * Strip the syntax `payload` from a structural unit (leaving its plain `text`,
 * already valid TSV) when the selection covers only part of its group. A
 * partial table slice would otherwise leak unbalanced pipes / tags into the
 * markdown / HTML flavor; this mirrors `splitTextUnit`'s no-leak rule for the
 * scaffolding units that `splitTextUnit` never reaches (they are pushed
 * verbatim as interior units).
 */
function degradePartialStructural(
  unit: SelectionUnit,
  partialGroups: ReadonlySet<string>
): SelectionUnit {
  if (
    unit.kind === 'text' &&
    unit.structuralGroup !== undefined &&
    partialGroups.has(unit.structuralGroup)
  ) {
    return { ...unit, payload: undefined, structuralGroup: undefined };
  }
  return unit;
}

/**
 * Find the structural groups the `[start, end]` selection does NOT fully
 * enclose. A group spans from the first to the last unit index carrying its id;
 * it is fully covered only when the selection begins at or before the group's
 * first unit and ends at or after its last. Any group not fully covered is
 * "partial" and its units degrade to plain text (see `degradePartialStructural`).
 */
function findPartialStructuralGroups(
  index: SelectionUnitIndex,
  startUnitIndex: number,
  endUnitIndex: number
): Set<string> {
  const partial = new Set<string>();
  for (const [group, { min, max }] of index.structuralGroupBounds) {
    if (startUnitIndex > min || endUnitIndex < max) partial.add(group);
  }
  return partial;
}

/**
 * Resolve a `SelectionRange` into the ordered units it covers, ready to hand to
 * `serializeSelectionUnits`. Endpoints are normalized (anchor may follow
 * focus), text units at the edges are sliced, interior units are kept verbatim,
 * and edge atoms/boundaries/breaks are included only when fully covered.
 */
export function resolveSelectionRange(
  units: readonly SelectionUnit[],
  range: SelectionRange,
  /**
   * Prebuilt index for `units`. Pass one when resolving the same stream
   * repeatedly — every frame of a drag, say — so the O(N) index build is not
   * repeated. It MUST have been built from exactly these `units`; when in
   * doubt, omit it and one is built here.
   */
  prebuiltIndex?: SelectionUnitIndex
): SelectionUnit[] {
  const index = prebuiltIndex ?? buildUnitIndex(units);
  const clamped = clampSelectionRange(index, range);
  const a = locateSelectionPoint(index, clamped.anchor);
  const f = locateSelectionPoint(index, clamped.focus);

  // Normalize so that `start` precedes `end` in (unitIndex, intraOffset) order.
  let start = a;
  let end = f;
  if (a.unitIndex > f.unitIndex || (a.unitIndex === f.unitIndex && a.intraOffset > f.intraOffset)) {
    start = f;
    end = a;
  }

  // Collapsed selection.
  if (start.unitIndex === end.unitIndex && start.intraOffset === end.intraOffset) {
    return [];
  }

  const partialGroups = findPartialStructuralGroups(index, start.unitIndex, end.unitIndex);

  const startEntry = index.entries[start.unitIndex];
  const endEntry = index.entries[end.unitIndex];

  // Single-unit selection.
  if (start.unitIndex === end.unitIndex) {
    const unit = startEntry.unit;
    if (unit.kind === 'text') {
      // A single structural unit alone never spans its whole group, so its
      // payload is dropped by `partialGroups` — `splitTextUnit` also drops it on
      // a partial slice, and returns it verbatim only on full coverage.
      return [
        degradePartialStructural(
          splitTextUnit(unit, start.intraOffset, end.intraOffset),
          partialGroups
        ),
      ];
    }
    // A fully covered non-text unit (0 -> 1).
    return [unit];
  }

  const result: SelectionUnit[] = [];

  // First unit.
  const startUnit = startEntry.unit;
  if (startUnit.kind === 'text') {
    result.push(
      degradePartialStructural(
        start.intraOffset === 0
          ? startUnit
          : splitTextUnit(startUnit, start.intraOffset, startUnit.text.length),
        partialGroups
      )
    );
  } else if (start.intraOffset === 0) {
    // Non-text head unit only enters when fully covered (selection begins before it).
    result.push(startUnit);
  }

  // Interior units, verbatim (structural scaffolding degrades when its group is
  // only partially selected).
  for (let i = start.unitIndex + 1; i < end.unitIndex; i++) {
    result.push(degradePartialStructural(index.entries[i].unit, partialGroups));
  }

  // Last unit.
  const endUnit = endEntry.unit;
  if (endUnit.kind === 'text') {
    if (end.intraOffset > 0) {
      result.push(
        degradePartialStructural(splitTextUnit(endUnit, 0, end.intraOffset), partialGroups)
      );
    }
  } else if (end.intraOffset >= 1) {
    // Non-text tail unit only enters when its trailing edge is covered.
    result.push(endUnit);
  }

  return result;
}
