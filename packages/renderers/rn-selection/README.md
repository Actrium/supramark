# @supramark/rn-selection

Document-level selection system for Supramark on React Native.

This package owns the Supramark selection model **and the selection UI**. The
highlight, the drag handles and the action bar are all drawn by this package, in
JavaScript, identically on every platform. No native selection component is
involved: blocks render a plain React Native `<Text>` and report their line
metrics through `onTextLayout`.

## Direction

- Keep Supramark AST v2 as the source of truth.
- Linearize selectable AST content into text, breaks, atoms, and boundaries.
- Let features provide custom payloads for nodes such as diagrams, math, tables,
  code blocks, and containers.
- Treat a rendered text block as a _metrics source_, not as a selection owner.
- Own the whole interaction — gestures, highlight, handles, action bar and
  payload serialization — so behavior is identical across platforms and the
  product controls the bar.

See [SELECTION_PLAN.md](./SELECTION_PLAN.md) for the full target architecture and
execution plan, including why the earlier native-command-bridge direction was
reversed.

## Status

The core model, table and grapheme-safe selection, the coordinator, and the
self-drawn selection UI are implemented and unit-tested. The pipeline from AST
to copyable selection:

```ts
import {
  linearizeForSelection,
  resolveSelectionRange,
  serializeSelectionUnits,
} from '@supramark/rn-selection';

const units = linearizeForSelection(ast); // AST v2 -> flat selection-unit stream
const selected = resolveSelectionRange(units, range); // range -> covered units
const markdown = serializeSelectionUnits(selected, 'markdown'); // 'plainText' | 'markdown' | 'source' | 'html'
```

`unit.text` holds plain text only; Markdown syntax is reconstructed on
serialization. Plain-text copy is lossless. **Markdown copy is best-effort**, not
lossless — see "Markdown copy: known limitations" below. Full-table selections
copy as GFM table / TSV / HTML; partial table selections degrade to clean
tab-separated plain text. Partial slices never split emoji, flags, ZWJ sequences
or combining marks, on engines with `Intl.Segmenter` and on Hermes alike.

### Markdown copy: known limitations

Reproduced through `linearize -> resolve -> serialize`. None of these lose text;
they lose or invent _markup_, so a copied fragment may not re-parse to the same
AST it came from.

- **Text is never re-escaped.** AST text is decoded on the way in and emitted
  verbatim, so `a * b _c_ [d] \ #` round-trips into emphasis and a heading.
- **Fixed code fences.** Inline code always uses a single backtick, so
  ``a ` b`` breaks; fenced code always uses three, so an embedded ``` breaks.
- **Hard breaks flatten.** A `break` unit emits a bare `\n`, which re-parses as
  a space rather than a hard break.
- **Link titles and URLs are not escaped**, so a URL containing spaces breaks.
- **Emphasis is ambiguous.** Nested and intra-word emphasis both emit `_`.
- **`footnote_reference` leaks its marker** into plain text: `linearize.ts`
  classifies it as a text unit rather than a syntax unit, so `see[^1]` copies
  with the `[^1]` visible. Heading marks and list markers are correctly syntax
  units and do not have this problem.

The coordinator layer (`SelectionRoot`, `useDocumentSelection`, registry /
hit-testing / selection state) and the self-drawn UI (highlight, handles,
toolbar, gesture machine) are in place as pure tested modules. On-device gesture
flows cover iOS and Android long press, handle drag, nested-list scrolling,
viewport clipping, and ordinary page scrolling.

## Selection UI

```tsx
<SelectionRoot
  units={units}
  onCopy={req => Clipboard.setString(String(req.payload ?? req.text))}
  toolbarItems={[
    { id: 'copy', title: 'Copy', format: 'plainText' },
    { id: 'copy-md', title: 'Copy MD', format: 'markdown' },
    { id: 'quote', title: 'Quote' },
  ]}
>
  <SelectableBlock nodeId="p1" unitIds={['p1#0']}>
    Hello world
  </SelectableBlock>
</SelectionRoot>
```

Long press selects the word under the finger and shows the bar; dragging from
there, or from either handle, extends the selection across blocks; a tap
dismisses it. The package never touches a clipboard library — `onCopy` receives
the serialized payload and the host decides what to do with it.

`SelectionRoot` also takes `renderToolbar` for hosts that want their own bar
component, and `overlay` / `handles` / `toolbar` / `gestures` booleans for hosts
that want to drive parts of the layer themselves.

### Nested scrollers

Wrap a nested `ScrollView` or `FlatList` in `SelectionViewport`. It composes the
child's `onScroll`, moves mounted block geometry from the native content-offset
delta in the same frame, clips selection UI to the visible box, and pauses the
nested scroller during a selection drag:

```tsx
<SelectionViewport style={{ height: 240 }}>
  <FlatList
    data={rows}
    renderItem={({ item }) => (
      <SelectableBlock nodeId={item.id} unitIds={item.unitIds}>
        {item.text}
      </SelectableBlock>
    )}
  />
</SelectionViewport>
```

When `SelectionRoot` itself sits inside a host `ScrollView`, use
`onGestureActiveChange` to pause that enclosing scroller while selection owns a
drag or a nested `SelectionViewport` owns a touch/scroll. The nested scroller
stays enabled for its own interaction; ordinary touches outside it are never
claimed by the root:

```tsx
const [selectionScrollLocked, setSelectionScrollLocked] = useState(false);

<ScrollView scrollEnabled={!selectionScrollLocked}>
  <SelectionRoot units={units} onGestureActiveChange={setSelectionScrollLocked}>
    {/* selectable document */}
  </SelectionRoot>
</ScrollView>;
```

### Text precision depends on metrics

`SelectableBlock` publishes the line table React Native reports through
`onTextLayout`: one entry per laid-out line, with its own text and box. Position
_within_ a line is then interpolated across the line's advance width — exact for
monospaced text, and off by up to about half a character in proportional text.
That is accurate enough to place a highlight edge and a drag handle, and the
metrics contract carries a `charXs` slot for a provider that reports exact
per-character positions; filling it from the platform text engines is tracked as
follow-up work.

A block that has not been measured yet (or is not text at all) highlights as a
whole rectangle and long-presses as a whole block, rather than not at all.

## Platform Requirements

- **React Native >= 0.72**, Paper or Fabric, iOS and Android alike. There is no
  native module, no pod, no codegen spec and no autolinking step: the package is
  plain TypeScript against public React Native APIs.
- `Intl.Segmenter` is used when present, and falls back to the in-package UAX #29
  tables and a script-class scan when it is not — which is the normal case on
  Hermes.

## Native Primitive Boundary

`native/selectable-rich-text` stays vendored here for reference and for the
exact-metrics follow-up, but **nothing imports it any more**. It is not on the
render path, not a peer dependency, and not required to build or run the
selection layer.

## Milestones

1. Selection model and AST linearization.
2. Registry, hit testing, and the self-drawn selection UI.
3. Exact per-character metrics from the platform text engines.
4. Feature selection providers for code, math, diagrams, tables, and containers.
5. Copy actions for text, Markdown, source, SVG, PNG, and HTML.
