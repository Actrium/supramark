/**
 * Selection demo screen.
 *
 * Uses a literal `SelectionUnit[]` fixture instead of `parse()`: `parse` is
 * async and routes through the linked native Rust markdown lib, which is
 * fragile to await inside an automated screenshot harness, and its exact
 * `nodeId` / unit granularity would then have to be reverse-engineered to
 * build matching `SelectableBlock`s. A literal fixture keeps the programmatic
 * "Select A..B" button deterministic (known `unitId`s) and needs no native
 * binary. `linearizeForSelection` remains available for real documents; this
 * fixture mirrors its output shape (text units + trailing block `break`s,
 * whole-unit `payload.markdown` on bold/heading units).
 *
 * The selection UI here is entirely self-drawn: long-press to select a word,
 * drag to extend, drag either handle to refine, tap to dismiss, and act from
 * the bar that appears. No native selection component is involved.
 */

import React, { useCallback, useState, useSyncExternalStore } from 'react';
import {
  FlatList,
  SafeAreaView,
  ScrollView,
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
} from 'react-native';

import {
  SelectionRoot,
  SelectionViewport,
  SelectableBlock,
  useSelectionRects,
  useSelectionContext,
  useSelectionStore,
  serializeSelectionUnits,
  type SelectionUnit,
  type SelectionCopyRequest,
  type SelectionToolbarItem,
} from '@supramark/rn-selection';
import type { SupramarkNode } from '@supramark/core';

const NODE = { type: 'text', value: '' } as unknown as SupramarkNode;
const CJK_TEXT = String.fromCodePoint(0x6f22, 0x5b57, 0x6e2c, 0x8a66);
const FLAT_ROWS = Array.from({ length: 16 }, (_, index) => {
  const n = String(index + 1).padStart(2, '0');
  return {
    nodeId: `flat-${n}`,
    unitId: `flat-${n}#0`,
    breakId: `flat-${n}#1`,
    text: `Flat row ${n} selection target.`,
  };
});

function t(unitId: string, nodeId: string, text: string, markdown?: string): SelectionUnit {
  return {
    kind: 'text',
    unitId,
    nodeId,
    text,
    node: NODE,
    ...(markdown ? { payload: { markdown } } : {}),
  } as SelectionUnit;
}

function brk(unitId: string, nodeId: string): SelectionUnit {
  return {
    kind: 'break',
    unitId,
    nodeId,
    text: '\n',
    reason: 'block',
    node: NODE,
  } as SelectionUnit;
}

// Blocks register VISIBLE text unit ids only (no trailing break).
const UNITS: SelectionUnit[] = [
  t('h#0', 'h', 'Selection Demo', '# Selection Demo'),
  brk('h#1', 'h'),
  t('p1#0', 'p1', 'Hello '),
  t('p1#1', 'p1', 'world', '**world**'),
  t('p1#2', 'p1', ' \u{1F31F}'),
  brk('p1#3', 'p1'),
  t('p2#0', 'p2', 'Second paragraph for range selection.'),
  brk('p2#1', 'p2'),
  t('cjk#0', 'cjk', CJK_TEXT),
  brk('cjk#1', 'cjk'),
  ...FLAT_ROWS.flatMap(row => [t(row.unitId, row.nodeId, row.text), brk(row.breakId, row.nodeId)]),
];

// The bar is ours, so its items are just data. A product would add its own
// actions here ("Quote", "Ask AI", ...) and handle them by id in `onCopy`.
const TOOLBAR_ITEMS: SelectionToolbarItem[] = [
  { id: 'copy', title: 'Copy', format: 'plainText' },
  { id: 'copy-md', title: 'Copy MD', format: 'markdown' },
  { id: 'quote', title: 'Quote' },
];

interface SelectionDemoProps {
  onBack: () => void;
  e2e?: boolean;
  scrollSentinel?: boolean;
  flatList?: boolean;
}

export default function SelectionDemo({
  onBack,
  e2e = false,
  scrollSentinel = false,
  flatList = false,
}: SelectionDemoProps) {
  const [status, setStatus] = useState('idle');
  const [selectionGestureActive, setSelectionGestureActive] = useState(false);
  const [pageOffset, setPageOffset] = useState(0);

  const onCopy = (req: SelectionCopyRequest) => {
    setStatus(`${req.id} (${req.format}): ${req.text}`);
  };

  return (
    <SafeAreaView style={s.root}>
      <TouchableOpacity onPress={onBack}>
        <Text style={s.back}>back</Text>
      </TouchableOpacity>
      <ScrollView
        nestedScrollEnabled
        scrollEnabled={!selectionGestureActive}
        scrollEventThrottle={16}
        onScroll={event => {
          const next = Math.round(event.nativeEvent.contentOffset.y);
          setPageOffset(prev => (prev === next ? prev : next));
        }}
        contentContainerStyle={s.body}
      >
        <SelectionRoot
          units={UNITS}
          onCopy={onCopy}
          toolbarItems={TOOLBAR_ITEMS}
          onGestureActiveChange={setSelectionGestureActive}
        >
          <SelectableBlock nodeId="h" unitIds={['h#0']} style={s.h1}>
            Selection Demo
          </SelectableBlock>
          <SelectableBlock nodeId="p1" unitIds={['p1#0', 'p1#1', 'p1#2']} style={s.p}>
            <Text>
              Hello <Text style={s.bold}>world</Text> {'\u{1F31F}'}
            </Text>
          </SelectableBlock>
          <SelectableBlock nodeId="p2" unitIds={['p2#0']} style={s.p}>
            Second paragraph for range selection.
          </SelectableBlock>
          <SelectableBlock nodeId="cjk" unitIds={['cjk#0']} style={s.cjk}>
            {CJK_TEXT}
          </SelectableBlock>
          <SelectionControls e2e={e2e} onStatus={setStatus} />
          {e2e && (
            <Text testID="selection-page-offset" style={s.e2eStatus}>
              page offset {pageOffset}
            </Text>
          )}
          {flatList && <FlatListSelectionFixture />}
        </SelectionRoot>
        <View style={s.statusPanel}>
          <Text testID="selection-status">{status}</Text>
        </View>
        {scrollSentinel && (
          <View style={s.scrollSentinel}>
            <Text testID="selection-scroll-sentinel">Scroll sentinel</Text>
          </View>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}

function FlatListSelectionFixture() {
  const [offset, setOffset] = useState(0);
  const ctx = useSelectionContext();
  const subscribeRegistry = useCallback(
    (onChange: () => void) => ctx.registry.subscribe(() => onChange()),
    [ctx.registry]
  );
  useSyncExternalStore(subscribeRegistry, ctx.registry.getVersion, ctx.registry.getVersion);
  const row03Measured = ctx.registry.getBlock('flat-03')?.rect !== undefined;

  return (
    <View style={s.flatPanel}>
      <Text style={s.flatTitle}>Nested selection list</Text>
      <Text testID="selection-flat-offset" style={s.e2eStatus}>
        flat offset {offset}
      </Text>
      <Text testID="selection-flat-row-03-measure" style={s.e2eStatus}>
        flat row 03 {row03Measured ? 'measured' : 'pending'}
      </Text>
      <SelectionViewport style={s.flatList}>
        <FlatList
          testID="selection-flat-list"
          data={FLAT_ROWS}
          keyExtractor={item => item.nodeId}
          nestedScrollEnabled
          style={s.flatScroller}
          contentContainerStyle={s.flatContent}
          scrollEventThrottle={16}
          onScroll={e => {
            const next = Math.round(e.nativeEvent.contentOffset.y);
            setOffset(prev => (prev === next ? prev : next));
          }}
          renderItem={({ item }) => (
            <SelectableBlock
              nodeId={item.nodeId}
              unitIds={[item.unitId]}
              style={s.flatRowText}
              containerStyle={s.flatRow}
            >
              {item.text}
            </SelectableBlock>
          )}
          ItemSeparatorComponent={() => <View style={s.flatSeparator} />}
        />
      </SelectionViewport>
    </View>
  );
}

function SelectionControls({
  e2e,
  onStatus,
}: {
  e2e: boolean;
  onStatus: (status: string) => void;
}) {
  const store = useSelectionStore();
  const snap = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
  const visibleRects = useSelectionRects();
  const afterControlTap = (select: () => void) => {
    setTimeout(select, 0);
  };

  // Programmatic cross-block selection: the highlight spans both blocks and
  // carries handles and the action bar, exactly like a gesture-driven one.
  const selectAB = () => {
    afterControlTap(() => {
      store.beginAt({ nodeId: 'h', unitId: 'h#0', offset: 0 });
      store.extendTo({ nodeId: 'p1', unitId: 'p1#2', offset: 3 });
      store.commit();
    });
  };

  // Within a single block, for comparing against the cross-block case.
  const selectInBlock = () => {
    afterControlTap(() => {
      store.beginAt({ nodeId: 'p1', unitId: 'p1#0', offset: 0 });
      store.extendTo({ nodeId: 'p1', unitId: 'p1#1', offset: 5 });
      store.commit();
    });
  };

  const copyMarkdown = () => {
    const md = serializeSelectionUnits(snap.units, 'markdown');
    onStatus(typeof md === 'string' ? md : '');
  };

  const selectCjkHalf = () => {
    afterControlTap(() => {
      store.beginAt({ nodeId: 'cjk', unitId: 'cjk#0', offset: 0 });
      store.extendTo({ nodeId: 'cjk', unitId: 'cjk#0', offset: CJK_TEXT.length / 2 });
      store.commit();
      onStatus('cjk half');
    });
  };

  const clear = () => {
    store.clear();
    onStatus('idle');
  };

  return (
    <View style={s.controls}>
      <TouchableOpacity style={s.button} onPress={selectAB}>
        <Text style={s.buttonText}>Select A..B</Text>
      </TouchableOpacity>
      <TouchableOpacity style={s.button} onPress={selectInBlock}>
        <Text style={s.buttonText}>Select in block</Text>
      </TouchableOpacity>
      <TouchableOpacity style={s.button} onPress={copyMarkdown}>
        <Text style={s.buttonText}>Copy markdown</Text>
      </TouchableOpacity>
      {e2e && (
        <TouchableOpacity style={s.button} onPress={selectCjkHalf}>
          <Text style={s.buttonText}>Select CJK half</Text>
        </TouchableOpacity>
      )}
      <TouchableOpacity style={s.button} onPress={clear}>
        <Text style={s.buttonText}>Clear</Text>
      </TouchableOpacity>
      <Text testID="selection-phase">
        {snap.phase} · {snap.units.length} units
      </Text>
      {e2e && (
        <>
          <Text testID="selection-phase-ascii" style={s.e2eStatus}>
            {snap.phase} {snap.units.length} units
          </Text>
          <Text testID="selection-visible-rects" style={s.e2eStatus}>
            visible selection rects {visibleRects.length}
          </Text>
        </>
      )}
    </View>
  );
}

const s = StyleSheet.create({
  root: { flex: 1 },
  body: { padding: 16, gap: 12 },
  h1: { fontSize: 22, fontWeight: '600' },
  p: { fontSize: 15, lineHeight: 22 },
  cjk: { fontSize: 26, lineHeight: 34 },
  bold: { fontWeight: '700' },
  statusPanel: {
    marginTop: 16,
    padding: 8,
    borderWidth: 1,
    borderColor: '#ccc',
    borderRadius: 4,
  },
  scrollSentinel: {
    height: 1200,
    justifyContent: 'flex-end',
  },
  e2eStatus: { color: '#595959', fontSize: 12 },
  flatPanel: { marginTop: 8 },
  flatTitle: { fontSize: 13, fontWeight: '600', marginBottom: 4 },
  flatList: {
    height: 220,
    borderWidth: 1,
    borderColor: '#d9d9d9',
    borderRadius: 4,
  },
  flatScroller: { flex: 1 },
  flatContent: { padding: 8 },
  flatRow: { alignSelf: 'flex-start', paddingHorizontal: 10, paddingVertical: 8 },
  flatRowText: { fontSize: 16, lineHeight: 22 },
  flatSeparator: { height: 4 },
  back: { color: '#2f54eb', padding: 8 },
  controls: { flexDirection: 'row', flexWrap: 'wrap', gap: 8, marginTop: 8 },
  button: {
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 6,
    backgroundColor: '#f5f5f5',
  },
  buttonText: { fontSize: 12, fontWeight: '600', color: '#2f54eb' },
});
