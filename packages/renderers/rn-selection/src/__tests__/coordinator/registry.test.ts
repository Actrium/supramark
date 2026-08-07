import { describe, expect, test } from 'bun:test';
import type { SupramarkTextNode } from '@supramark/core';
import type {
  SelectionBoundaryUnit,
  SelectionBreakUnit,
  SelectionTextUnit,
  SelectionUnit,
} from '../../model';
import { buildTextMetrics } from '../../metrics';
import {
  SelectionRegistry,
  type LayoutRect,
  type RegisteredBlock,
} from '../../coordinator/registry';

// resolve/registry copy `node` but never inspect it.
const NODE = { type: 'text', value: '' } as SupramarkTextNode;
const tUnit = (unitId: string, nodeId: string, text: string): SelectionTextUnit => ({
  kind: 'text',
  unitId,
  nodeId,
  text,
  node: NODE,
});
const brk = (unitId: string, nodeId: string): SelectionBreakUnit => ({
  kind: 'break',
  unitId,
  nodeId,
  text: '\n',
  reason: 'block',
  node: NODE,
});
const bound = (unitId: string, nodeId: string): SelectionBoundaryUnit => ({
  kind: 'boundary',
  unitId,
  nodeId,
  node: NODE,
  reason: 'custom',
});

// Document order: text a#0, break a#1 (node a); text b#0 (node b); boundary c#0 (node c).
const baseUnits = (): SelectionUnit[] => [
  tUnit('a#0', 'a', 'a'),
  brk('a#1', 'a'),
  tUnit('b#0', 'b', 'b'),
  bound('c#0', 'c'),
];

const blockA: RegisteredBlock = { nodeId: 'a', unitIds: ['a#0', 'a#1'], kind: 'text' };
const blockB: RegisteredBlock = { nodeId: 'b', unitIds: ['b#0'], kind: 'text' };
const blockC: RegisteredBlock = { nodeId: 'c', unitIds: ['c#0'], kind: 'boundary' };

describe('SelectionRegistry', () => {
  test('iterates in document order regardless of registration order', () => {
    const reg = new SelectionRegistry(baseUnits());
    reg.register({ ...blockC });
    reg.register({ ...blockA });
    reg.register({ ...blockB });
    expect(reg.getBlocks().map(b => b.nodeId)).toEqual(['a', 'b', 'c']);
  });

  test('unregister removes a block and keeps order', () => {
    const reg = new SelectionRegistry(baseUnits());
    reg.register({ ...blockA });
    reg.register({ ...blockB });
    reg.register({ ...blockC });
    reg.unregister('b');
    expect(reg.getBlocks().map(b => b.nodeId)).toEqual(['a', 'c']);
  });

  test('updateLayout mutates rect and notifies', () => {
    const reg = new SelectionRegistry(baseUnits());
    reg.register({ ...blockA });
    const events: Array<[string, string]> = [];
    reg.subscribe((change, nodeId) => events.push([change, nodeId]));
    const rect: LayoutRect = { x: 1, y: 2, w: 3, h: 4 };
    reg.updateLayout('a', rect);
    expect(reg.getBlock('a')?.rect).toEqual(rect);
    expect(events).toContainEqual(['layout', 'a']);
  });

  test('updateLayout on an unknown block is a no-op', () => {
    const reg = new SelectionRegistry(baseUnits());
    reg.updateLayout('missing', { x: 0, y: 0, w: 1, h: 1 });
    expect(reg.getBlock('missing')).toBeUndefined();
  });

  test('register replaces an existing block by nodeId', () => {
    const reg = new SelectionRegistry(baseUnits());
    reg.register({ ...blockA });
    reg.register({ ...blockB });
    reg.register({ ...blockC });
    reg.register({ nodeId: 'a', unitIds: ['a#0'], kind: 'text', rect: { x: 5, y: 5, w: 5, h: 5 } });
    expect(reg.getBlocks()).toHaveLength(3);
    expect(reg.getBlock('a')?.unitIds).toEqual(['a#0']);
    expect(reg.getBlock('a')?.rect).toEqual({ x: 5, y: 5, w: 5, h: 5 });
  });

  test('register without a rect preserves the previously measured rect', () => {
    // Mirrors a React re-registration: a block re-registers (effect cleanup +
    // re-run) carrying no rect while onLayout does not re-fire. The measured box
    // must survive so the overlay/hit-test keep working.
    const reg = new SelectionRegistry(baseUnits());
    reg.register({ ...blockA });
    reg.updateLayout('a', { x: 1, y: 2, w: 3, h: 4 });
    reg.register({ nodeId: 'a', unitIds: ['a#0', 'a#1'], kind: 'text' });
    expect(reg.getBlock('a')?.rect).toEqual({ x: 1, y: 2, w: 3, h: 4 });
  });

  test('register with a rect overrides the previous rect', () => {
    const reg = new SelectionRegistry(baseUnits());
    reg.register({ ...blockA });
    reg.updateLayout('a', { x: 1, y: 2, w: 3, h: 4 });
    reg.register({ nodeId: 'a', unitIds: ['a#0'], kind: 'text', rect: { x: 9, y: 9, w: 9, h: 9 } });
    expect(reg.getBlock('a')?.rect).toEqual({ x: 9, y: 9, w: 9, h: 9 });
  });

  test('updateUnits replaces unitIds in place, preserving rect and reordering', () => {
    const reg = new SelectionRegistry(baseUnits());
    const handle = { nodeId: 'a', selectRange() {}, clearSelection() {}, copyRange() {} };
    reg.register({ nodeId: 'a', unitIds: ['a#0', 'a#1'], kind: 'text', handle });
    reg.register({ ...blockB });
    reg.updateLayout('a', { x: 1, y: 2, w: 3, h: 4 });
    const events: Array<[string, string]> = [];
    reg.subscribe((change, nodeId) => events.push([change, nodeId]));
    reg.updateUnits('a', ['a#0']);
    expect(reg.getBlock('a')?.unitIds).toEqual(['a#0']);
    expect(reg.getBlock('a')?.rect).toEqual({ x: 1, y: 2, w: 3, h: 4 });
    expect(reg.getBlock('a')?.handle).toBe(handle);
    // The unitId -> block map is invalidated so a dropped unit no longer resolves.
    expect(reg.getBlockForUnit('a#1')).toBeUndefined();
    expect(events).toContainEqual(['units', 'a']);
  });

  test('updateUnits on an unknown block is a no-op', () => {
    const reg = new SelectionRegistry(baseUnits());
    reg.updateUnits('missing', ['x#0']);
    expect(reg.getBlock('missing')).toBeUndefined();
  });

  test('getVersion bumps on every mutation notification', () => {
    const reg = new SelectionRegistry(baseUnits());
    const v0 = reg.getVersion();
    reg.register({ ...blockA });
    const v1 = reg.getVersion();
    reg.updateLayout('a', { x: 0, y: 0, w: 1, h: 1 });
    const v2 = reg.getVersion();
    reg.updateUnits('a', ['a#0']);
    const v3 = reg.getVersion();
    reg.unregister('a');
    const v4 = reg.getVersion();
    // Each notifying mutation strictly increments; a no-op does not.
    expect(v1).toBeGreaterThan(v0);
    expect(v2).toBeGreaterThan(v1);
    expect(v3).toBeGreaterThan(v2);
    expect(v4).toBeGreaterThan(v3);
    reg.updateLayout('missing', { x: 0, y: 0, w: 1, h: 1 });
    expect(reg.getVersion()).toBe(v4);
  });

  test('getBlockForUnit maps unitId to its block', () => {
    const reg = new SelectionRegistry(baseUnits());
    reg.register({ ...blockA });
    reg.register({ ...blockB });
    expect(reg.getBlockForUnit('a#1')?.nodeId).toBe('a');
    expect(reg.getBlockForUnit('b#0')?.nodeId).toBe('b');
    expect(reg.getBlockForUnit('nope')).toBeUndefined();
  });

  test('blocks whose units are absent from the index sort last', () => {
    const reg = new SelectionRegistry(baseUnits());
    reg.register({ ...blockA });
    reg.register({ nodeId: 'x', unitIds: ['x#0'], kind: 'atom' });
    reg.register({ ...blockB });
    expect(reg.getBlocks().map(b => b.nodeId)).toEqual(['a', 'b', 'x']);
  });

  test('setUnits re-indexes and reorders existing blocks', () => {
    const reg = new SelectionRegistry(baseUnits());
    reg.register({ ...blockA });
    reg.register({ ...blockB });
    expect(reg.getBlocks().map(b => b.nodeId)).toEqual(['a', 'b']);
    // Swap node positions: b now precedes a in the linearized stream.
    reg.setUnits([tUnit('b#0', 'b', 'b'), tUnit('a#0', 'a', 'a'), brk('a#1', 'a')]);
    expect(reg.getBlocks().map(b => b.nodeId)).toEqual(['b', 'a']);
  });
});

describe('registration identity guard', () => {
  test('a stale disposer cannot unregister the live block that reused its nodeId', () => {
    // React mounts a replacement BEFORE unmounting what it replaces, so two
    // instances sharing a nodeId produce register(new) -> unregister(old).
    // `linearize.ts` derives block ids as `pos:<start>-<end>`, which a re-parse
    // readily reassigns, so this ordering is reachable in normal use.
    const reg = new SelectionRegistry([tUnit('a#0', 'a', 'hello')]);

    const oldBlock: RegisteredBlock = { nodeId: 'a', unitIds: ['a#0'], kind: 'text' };
    const oldRegistered = reg.register(oldBlock);

    const newBlock: RegisteredBlock = {
      nodeId: 'a',
      unitIds: ['a#0'],
      kind: 'text',
      rect: { x: 0, y: 0, w: 10, h: 10 } as LayoutRect,
    };
    const newRegistered = reg.register(newBlock);

    // Old instance's cleanup runs last and must be a no-op.
    reg.unregister('a', oldRegistered);

    expect(reg.getBlock('a')).toBe(newRegistered);
    expect(reg.getBlock('a')?.rect).toEqual({ x: 0, y: 0, w: 10, h: 10 });
  });

  test('the live disposer still unregisters', () => {
    const reg = new SelectionRegistry([tUnit('a#0', 'a', 'hello')]);
    const registered = reg.register({ nodeId: 'a', unitIds: ['a#0'], kind: 'text' });
    reg.unregister('a', registered);
    expect(reg.getBlock('a')).toBeUndefined();
  });

  test('an unguarded unregister still removes, for callers that do not track identity', () => {
    const reg = new SelectionRegistry([tUnit('a#0', 'a', 'hello')]);
    reg.register({ nodeId: 'a', unitIds: ['a#0'], kind: 'text' });
    reg.unregister('a');
    expect(reg.getBlock('a')).toBeUndefined();
  });
});

describe('setUnits drops blocks whose units no longer exist', () => {
  test('a block left behind by a removed paragraph is dropped', () => {
    const reg = new SelectionRegistry([tUnit('a#0', 'a', 'a'), tUnit('b#0', 'b', 'b')]);
    reg.register({ nodeId: 'a', unitIds: ['a#0'], kind: 'text' });
    reg.register({ nodeId: 'b', unitIds: ['b#0'], kind: 'text' });

    // Re-parse: 'b' is gone and 'a' got a fresh unit id, which its own block
    // has already pushed in (children effects run before the parent's).
    reg.updateUnits('a', ['a#1']);
    reg.setUnits([tUnit('a#1', 'a', 'a')]);

    expect(reg.getBlock('b')).toBeUndefined();
    expect(reg.getBlock('a')).toBeDefined();
    // The stale block no longer bunches at the end of document order...
    expect(reg.getBlocks().map(x => x.nodeId)).toEqual(['a']);
    // ...nor answers ownership questions with a dead unit id.
    expect(reg.getBlockForUnit('b#0')).toBeUndefined();
  });

  test('a block that has not received its units yet is kept', () => {
    const reg = new SelectionRegistry([tUnit('a#0', 'a', 'a')]);
    reg.register({ nodeId: 'pending', unitIds: [], kind: 'text' });
    reg.setUnits([tUnit('a#0', 'a', 'a')]);
    expect(reg.getBlock('pending')).toBeDefined();
  });

  test('a block keeping one live unit out of several survives', () => {
    const reg = new SelectionRegistry([tUnit('a#0', 'a', 'a'), tUnit('a#1', 'a', 'b')]);
    reg.register({ nodeId: 'a', unitIds: ['a#0', 'a#1'], kind: 'text' });
    reg.setUnits([tUnit('a#0', 'a', 'a')]);
    expect(reg.getBlock('a')).toBeDefined();
  });
});

describe('SelectionRegistry metrics', () => {
  const metrics = buildTextMetrics([{ text: 'hello', x: 0, y: 0, width: 50, height: 20 }]);

  test('setMetrics stores the line table and bumps the version', () => {
    const reg = new SelectionRegistry([tUnit('a#0', 'a', 'hello')]);
    reg.register({ nodeId: 'a', unitIds: ['a#0'], kind: 'text' });
    const before = reg.getVersion();
    reg.setMetrics('a', metrics);
    expect(reg.getBlock('a')?.metrics?.textLength).toBe(5);
    // The overlay repaints off the version, so a re-measure has to move it.
    expect(reg.getVersion()).toBeGreaterThan(before);
  });

  test('setMetrics on an unknown block is a no-op', () => {
    const reg = new SelectionRegistry([tUnit('a#0', 'a', 'hello')]);
    expect(() => reg.setMetrics('missing', metrics)).not.toThrow();
  });

  test('re-registering preserves measured geometry', () => {
    // A React block re-registers on effect cleanup + re-run while neither
    // onLayout nor onTextLayout re-fires for an unchanged layout. Losing the
    // line table here would blank the highlight until the next reflow.
    const reg = new SelectionRegistry([tUnit('a#0', 'a', 'hello')]);
    reg.register({ nodeId: 'a', unitIds: ['a#0'], kind: 'text' });
    reg.updateLayout('a', { x: 0, y: 0, w: 50, h: 20 });
    reg.setMetrics('a', metrics);
    reg.setContentOffset('a', { x: 4, y: 2 });

    reg.register({ nodeId: 'a', unitIds: ['a#0'], kind: 'text' });
    expect(reg.getBlock('a')?.rect).toEqual({ x: 0, y: 0, w: 50, h: 20 });
    expect(reg.getBlock('a')?.metrics?.textLength).toBe(5);
    expect(reg.getBlock('a')?.contentOffset).toEqual({ x: 4, y: 2 });
  });

  test('a re-registration carrying its own metrics wins', () => {
    const reg = new SelectionRegistry([tUnit('a#0', 'a', 'hello')]);
    reg.register({ nodeId: 'a', unitIds: ['a#0'], kind: 'text' });
    reg.setMetrics('a', metrics);
    reg.register({
      nodeId: 'a',
      unitIds: ['a#0'],
      kind: 'text',
      metrics: buildTextMetrics([{ text: 'hi', x: 0, y: 0, width: 20, height: 20 }]),
    });
    expect(reg.getBlock('a')?.metrics?.textLength).toBe(2);
  });
});

describe('SelectionRegistry nested viewports', () => {
  test('moves mounted blocks by the exact scroll delta and publishes once', () => {
    const reg = new SelectionRegistry(baseUnits());
    reg.registerViewport('list');
    reg.updateViewportLayout('list', { x: 0, y: 100, w: 200, h: 80 });
    reg.register({ ...blockA, viewportId: 'list' });
    reg.updateLayout('a', { x: 10, y: 120, w: 60, h: 20 });
    const before = reg.getVersion();

    reg.updateViewportScroll('list', { x: 0, y: 25 });

    expect(reg.getBlock('a')?.rect).toEqual({ x: 10, y: 95, w: 60, h: 20 });
    expect(reg.getBlock('a')?.clipRect).toEqual({ x: 0, y: 100, w: 200, h: 80 });
    expect(reg.getVersion()).toBe(before + 1);
  });

  test('a fresh native measurement becomes the next scroll baseline', () => {
    const reg = new SelectionRegistry(baseUnits());
    reg.registerViewport('list');
    reg.register({ ...blockA, viewportId: 'list' });
    reg.updateViewportScroll('list', { x: 0, y: 25 });
    reg.updateLayout('a', { x: 10, y: 110, w: 60, h: 20 });

    reg.updateViewportScroll('list', { x: 0, y: 35 });

    expect(reg.getBlock('a')?.rect?.y).toBe(100);
  });

  test('an initial non-zero offset does not translate already-positioned blocks', () => {
    const reg = new SelectionRegistry(baseUnits());
    reg.register({
      ...blockA,
      viewportId: 'list',
      rect: { x: 10, y: 110, w: 60, h: 20 },
    });
    reg.registerViewport('list', { x: 0, y: 40 });

    reg.updateViewportScroll('list', { x: 0, y: 40 });

    expect(reg.getBlock('a')?.rect?.y).toBe(110);
  });

  test('disposing a viewport removes its clip from surviving blocks', () => {
    const reg = new SelectionRegistry(baseUnits());
    const dispose = reg.registerViewport('list');
    reg.updateViewportLayout('list', { x: 0, y: 100, w: 200, h: 80 });
    reg.register({ ...blockA, viewportId: 'list' });
    expect(reg.getBlock('a')?.clipRect).toBeDefined();

    dispose();

    expect(reg.getBlock('a')?.clipRect).toBeUndefined();
  });
});
