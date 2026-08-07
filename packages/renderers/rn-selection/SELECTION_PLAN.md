# Supramark RN Selection Plan

## Goal

Build a Supramark-owned document selection system for React Native.

The long-term target is not only selectable text. The system should let users
select across Markdown content and copy feature-aware payloads from both text and
non-text nodes:

- text blocks: plain text, Markdown, source ranges;
- code blocks: raw code, fenced Markdown, highlighted HTML;
- tables: TSV, Markdown table, HTML table;
- math: TeX, SVG, PNG;
- diagrams: source, fenced Markdown, SVG, PNG;
- containers: feature-defined text, links, metadata, or custom payloads.

## Design Principles

- Supramark AST v2 is the source of truth.
- Selection ranges belong to Supramark, not to a single native text view.
- Native text controls are primitives for local text selection only.
- Feature packages can define their own selection and copy behavior.
- Renderers must not directly import diagram/math engines; payload generation
  should reuse `@supramark/engines` and feature-level contracts.
- The default `@supramark/rn` renderer should remain usable without the native
  selection runtime.

## Architecture

### Selection Model

The model linearizes AST content into selection units:

- `text`: selectable text with node/source metadata;
- `break`: block, line, list item, or custom separators;
- `atom`: a non-text item that can be selected as a single unit;
- `boundary`: a node that splits selection regions or needs custom handling.

This lets Supramark represent both ordinary text and rich extension nodes in one
selection stream.

### Feature Selection Providers

Each feature may provide selection behavior:

- `text`: participate in text selection;
- `atom`: selectable as a single object;
- `boundary`: split selection regions;
- `custom`: feature owns the unit/payload mapping;
- `none`: intentionally not selectable.

Providers should expose payloads in formats such as `plainText`, `markdown`,
`html`, `source`, `svg`, and `png`.

### Native Text Primitive

The vendored `native/selectable-rich-text` implementation, based on
`boomsi/selectable-library`, is the first native text primitive. It should remain
below the coordinator.

It is responsible for:

- native text selection inside a single text segment;
- platform handles and selection menu integration;
- local selection snapshots and rectangles.

It is not responsible for:

- global Supramark AST ranges;
- cross-block selection;
- diagram/math/table/container payloads;
- renderer-level policy.

### RN Coordinator

The future RN runtime should provide:

- node/layout registry;
- hit testing from touch coordinates to selection points;
- document-level selection range state;
- overlay highlights and handles;
- auto-scroll while dragging;
- menu actions and clipboard payload dispatch.

## Milestones

### 1. Core Model

- Define `SelectionRange`, `SelectionPoint`, `SelectionUnit`, and
  `SelectionPayload`.
- Implement AST linearization for core Markdown nodes, including blockquote,
  image, definition list, and footnote nodes.
- Implement `resolveSelectionRange` to resolve a `SelectionRange` into the
  selection units it covers.
- Implement selection serialization for plain text, Markdown, and source
  payloads.

### 2. Native Primitive Integration

- Adapt `native/selectable-rich-text` behind Supramark interfaces.
- Add commands for local select, set selection, clear selection, and rect reads.
- Preserve offset/span metadata needed to map native selection back to AST units.

### 3. RN Selection Runtime

- Add `SelectionRoot` and a coordinator hook.
- Register rendered node layouts and text spans.
- Draw cross-block overlay highlights and handles.
- Support paragraph, heading, list, code, and table-cell selection.

### 4. Feature Payloads

- Add providers for code, math, diagrams, tables, and containers.
- Reuse `@supramark/engines` outputs for SVG/PNG-capable payloads.
- Extend selection serialization to SVG, PNG, and HTML payloads.
- Expose menu actions based on available payload formats.

### 5. Production Hardening

- Handle UTF-16 offsets, emoji, CJK, and mixed direction text.
- Support auto-scroll and streaming Markdown updates.
- Add RN interaction tests around hit testing, dragging, and copy payloads.
- Document feature provider authoring rules.

## Status

### Milestone 1 — Core Model (implemented)

Delivered as pure TypeScript, with no native/RN runtime dependency (`tsc --noEmit`
clean, 32 unit tests):

- `model.ts` — `SelectionRange` / `SelectionPoint` / `SelectionUnit` /
  `SelectionPayload`. Every unit carries a globally unique `unitId`; several units
  may share one `nodeId` (e.g. a heading's syntax prefix + its text). Offset
  semantics: offsets count UTF-16 code units inside a unit's plain text; a
  zero-text unit (atom/boundary) encodes *before* (offset 0) / *after* (offset > 0).
- `linearize.ts` — linearizes core Markdown while keeping `unit.text` **plain**.
  Markdown affixes (heading prefix, list markers, blockquote `>`, code fences,
  inline `**`/`_`/`[..](url)`) live in per-format payloads or empty-text syntax
  units, so plain-text serialization is lossless and Markdown serialization is
  best-effort (see the limitations list in `README.md`). Covers
  paragraph, heading, list, blockquote, code, inline code, image, math, diagram,
  definition list, footnote, raw, and thematic break.
- `resolve.ts` — `resolveSelectionRange(units, range)` maps a range to the units it
  covers, splitting partial text units at offsets and preserving a unit's
  whole-unit payload only on full coverage (a partial slice falls back to plain
  sliced text rather than leaking the surrounding syntax).
- `serialize.ts` — plain-text / Markdown / source serialization.

### Milestones 2–3 (logic layers) + table & grapheme support (implemented)

All still pure TypeScript except the typecheck-only React wiring; 100 unit tests:

- **Tables** — `table`/`table_row`/`table_cell` linearize compositionally:
  per-cell inline text units plus structural units (tab cell separators,
  pipe/HTML-tag payloads, a Markdown alignment row from `table.align`). Full-table
  selection reconstructs a GFM table / TSV / `<table>` HTML through the ordinary
  serializer; structural units share a `structuralGroup` id so a *partial*
  selection strips the scaffolding and degrades to clean tab/newline plain text.
- **Blockquote** — per-line `> ` prefixing (a prefix after every interior break),
  so multi-paragraph quotes serialize to valid Markdown.
- **Grapheme safety** (milestone-5 item pulled forward) — `text.ts`
  `snapToGraphemeBoundary` (Intl.Segmenter, surrogate-pair fallback for older
  Hermes); `splitTextUnit` widens partial slices to whole grapheme clusters so
  emoji / ZWJ sequences / combining marks are never split.
- **Milestone 2, TS side** — `nativePrimitive.ts` rewritten to the real vendored
  command+event contract (`TextSegmentHandle`); `native/segmentAdapter.ts` maps
  `SelectableRichTextRef` to it, with pure segment-local ⇄ document offset mapping.
- **Milestone 3, logic core** — `coordinator/`: `registry.ts` (document-ordered
  block registry), `hitTest.ts` (root-coord point → `SelectionPoint` geometry),
  `state.ts` (idle → selecting → selected external store deriving covered units
  via `resolveSelectionRange`), plus thin React wiring (`SelectionRoot`,
  `useDocumentSelection`, `SelectionContext`) — components typecheck-only, all
  logic in pure tested modules.

### Overlay + native event wiring (implemented, simulator-verified)

- **Per-block event sinks** — `createBlockSink(nodeId)` closes the "native
  events carry no nodeId" gap: each `SelectableBlock` wires the vendored
  `onTextLongPress`/`onMenuAction` into its own sink, which maps events through
  the pure helpers into store actions; menu actions serialize the selection and
  deliver `{ id, format, payload, text, range }` through a host `onCopy`
  callback (the package stays clipboard-free).
- **Block-level overlay** — `computeOverlayRects` (covered blocks, vertical
  merge) + `SelectionOverlay` translucent views, subscribed to both the store
  and the registry version so re-layout repaints.
- **`SelectableBlock`** — plumbs the vendored `SelectableRichText` (layout
  registration, handle, sink). Children are always wrapped in `<Text>`: the
  Fabric reconciler validates raw strings against the host component type, so
  bare strings under a custom native component would throw at runtime.
- **Simulator-verified on iOS** (iPhone 17 Pro sim, RN 0.81 New Arch, Debug):
  the vendored pod autolinks and builds; programmatic cross-block selection
  paints a merged block-level highlight (uncovered blocks excluded); Markdown
  copy reconstructs heading prefix / bold / emoji for the cases exercised on
  screen; clear removes the
  overlay and returns the store to idle. The example app's `SelectionDemo`
  screen drives all of this with on-screen status for screenshot verification.

### Known limitations (deferred)

- **Still needing human/manual verification on device**: real long-press
  gesture → native event pipeline (wired end-to-end but only exercised by unit
  tests and programmatic selection so far), native selection-menu copy flow,
  drag handles, and auto-scroll. Text-precision overlay rects await a native
  selection-rects command.
- SVG / PNG payloads and the `@supramark/engines` dependency are deferred to
  milestone 4; the package currently depends only on `@supramark/core`. HTML
  serialization is implemented for table scaffolding; inline HTML for emphasis
  etc. still falls back to plain text.
- `Intl.Segmenter` is constructed per snap call (documented test-observability
  trade-off); acceptable until drag-time hit testing lands, then worth caching.
- Offsets remain UTF-16 code units at the API surface; only *slicing* is
  grapheme-safe.

### Interaction direction (decided, then REVERSED — see below)

> **Superseded.** The command-bridge direction recorded in this section and
> implemented in the next one was reversed; see "Interaction direction
> (reversed): fully self-drawn". Both are kept because the reversal only makes
> sense against the argument it overturns.

The coordinator draws its own overlay today while the vendored component's
command surface (`selectRange` / `selectParagraphAt` / `copyRange` — native
handles, edit menu, selection events) goes uncalled, leaving two parallel
selection representations with no bridge. Of the two candidate directions —
complete the vendored command bridge vs. go fully self-drawn (which would
require a new native `getSelectionRects` command) — **the vendored command
bridge is the chosen direction**: the native side already implements the hard
parts, and native handles/menu bring platform-correct interaction (magnifier,
haptics, accessibility) that an overlay would have to re-implement.

Workstreams, in order:

1. **Downlink** — `commit()` pushes the covered range to the owning block's
   `TextSegmentHandle.selectRange` through `segmentAdapter`'s offset mapping;
   `clear()` propagates deselection to the native side.
2. **Uplink** — native selection-change events feed the store through the
   per-block sinks; the store remains the single source of truth.
3. **Cross-block selection** stays on the coordinator overlay (native handles
   are per-block by nature); within-block refinement uses the native
   handles/menu.
4. **Version reconciliation** — align the package's platform claims with the
   vendored component's floors (see README "Platform requirements") and keep
   them in sync as the bridge lands.

### Command bridge (implemented, then removed — see the reversal below)

All four workstreams above are in place (19 new unit tests; 145 total):

- **Downlink** — `coordinator/nativeBridge.ts`: `createNativeBridge(store,
  registry)` subscribes the store and answers a committed single-block range
  with the owning handle's `selectRange`, and `idle` with `clearSelection`.
  `planNativeSelection` is the pure planning half: covered units vote on the
  owning block (units no block renders — trailing breaks, syntax units —
  abstain), and a second owner, an atom/boundary owner, or a handle-less owner
  vetoes the push. Command discipline: `selecting` never touches native (so a
  menu-action reflect cannot re-pop the menu the user just used), an identical
  re-commit is deduped, and a cross-block commit clears a previous native push.
- **Offset projection** — `rangeToSegmentSelection` now resolves endpoints via
  `locateSelectionPoint` on the document index before projecting onto segment
  spans, so a focus on a block's trailing break clamps to the segment end
  (the old per-span nodeId fallback collapsed it to the block head), a point
  before the block clamps to 0, and interleaved zero-text units land on the
  next span's start.
- **Uplink closure** — `createBlockSink.onLongPress` now commits after
  begin/extend, so a native long-press round-trips: gesture → store → bridge →
  native `selectRange` (selection handles + system menu) — the host response
  the vendored contract expects. `extendTo` afterwards re-enters `selecting`
  and hands display back to the overlay.
- **Version reconciliation** — `peerDependencies.react-native` tightened to
  `>= 0.81.0`; the README's per-platform floors (Android 0.85 hard) remain
  authoritative.

**Simulator-verified on iOS** (iPhone 17 Pro sim, RN 0.81 New Arch, Debug,
programmatic commits): a committed single-block range renders the native
selection highlight with system grab handles stopping exactly at the mapped
UTF-16 offset (trailing emoji excluded) and pops the system edit menu
(Cut / Copy / Look Up); a committed cross-block range draws only the merged
coordinator overlay — no native selection, no menu (the veto path); `clear()`
removes the native highlight and handles and returns the store to idle.

Known vendored-side limitation found during verification: `clearSelection`
clears the text selection but does NOT dismiss an already-presented system
edit menu — the menu lingers until the user touches elsewhere. The JS command
surface has no dismiss entrypoint, so this needs a vendored native fix
(dismiss the `UIEditMenuInteraction` in `clearSelection`), tracked for the
next native round.

Still pending on-device: real long-press gesture → native menu round-trip,
handle dragging inside a native selection, and behavior with
`clearSelectionOnMenuAction` (native clears itself; the bridge's pushed record
goes stale until the next range change — benign, but worth observing).

### Interaction direction (reversed): fully self-drawn

The command bridge was the wrong trade. Four things decided it:

1. **It only ever served the case we least need.** Native text selection is
   per-text-view by construction, so `planNativeSelection` vetoes anything
   spanning two blocks. The requirement is cross-block continuous selection —
   heading + body + list — which our own overlay had to draw regardless. We
   were paying for a second selection implementation that covered only the
   degenerate case.
2. **One document, two interactions.** Long-press one paragraph and you got
   native handles and the system edit menu; drag across two and you got a flat
   block rectangle with no handles and no menu. Nothing in the UI explained why.
3. **Two owners for one selection is a bug generator.** The duplicate highlight
   and the yield mechanism that fixed it; the store staying `selected` after the
   native side silently cleared itself, which needed a whole new codegen event
   to observe; `clearSelection` not dismissing an already-presented
   `UIEditMenuInteraction`. All the same root cause.
4. **The menu was not ours.** Item order, grouping, icons, styling and above all
   the dismissal lifecycle belonged to UIKit and to Android's `ActionMode`, and
   the two disagreed on all of it. Every product action meant touching
   Objective-C++, Kotlin and the codegen spec.

What we give up, deliberately: the iOS magnifier while dragging a handle (the
best thing about native selection, and not reimplemented here), haptics, the
platform text-selection accessibility affordances, and system menu items such
as Look Up and Translate. Accessibility is the highest residual risk and is
tracked as its own milestone rather than treated as closed.

**Target: the text view renders text and reports metrics; everything else is
ours.**

- `metrics.ts` — the whole contract between a rendered block and the selection
  UI: a line table, plus pure `offsetAtLocalPoint` (point to offset) and
  `rectsForRange` (range to rectangles). Three interchangeable providers:
  React Native `onTextLayout` (the default, no native code); an exact provider
  filling `charXs` from `NSLayoutManager` / `android.text.Layout` (follow-up);
  and `Range.getClientRects()` on web (later).
- `coordinator/overlay.ts` — per-line highlight rectangles, projecting the
  document range onto each covered block through the existing
  `rangeToSegmentSelection`. Falls back to the whole block rect when a block has
  no metrics yet, so nothing regresses while a provider is missing.
- `coordinator/handles.ts` + `SelectionHandles.tsx` — drag handles from the
  first and last rectangle, with a touch radius far larger than the drawn knob.
- `coordinator/toolbar.ts` + `SelectionToolbar.tsx` — the action bar: item
  model, placement arithmetic (prefer above, flip below, clamp into the
  viewport, arrow tracks the selection centre), host-supplied `toolbarItems`,
  and a `renderToolbar` escape hatch.
- `coordinator/gesture.ts` — a pure state machine (`idle -> pending ->
  extending | handle -> idle`) fed touch events plus an injected clock, so the
  long-press threshold is tested by advancing a number. Tap-to-dismiss lives
  here, which is what makes the store/native desync structurally impossible.
- `words.ts` — word granularity for the long press, `Intl.Segmenter` with a
  script-class fallback for Hermes.

Retired with the bridge: `nativeBridge.ts`, `blockSink.ts`, the overlay's
`yieldNodeId`, `SelectionContext.nativePushed`, `nativePrimitive.ts`'s
`TextSegmentHandle` command surface, the vendored-type shim, and the vendored
peer dependency. `native/selectable-rich-text` stays in the tree, unwired, as
the natural home for the exact-metrics work.

Consequences worth stating plainly: the package is now plain TypeScript against
public React Native APIs, so the Fabric-only and Android >= 0.85 floors are
gone (peer range back to `>= 0.72`), and the same pure geometry will serve
`@supramark/web`.

**Still pending on device**: the touch dispatch and responder negotiation with
an enclosing `ScrollView`; long-press threshold and move-tolerance tuning;
handle knob geometry and hit slop; and whether `onTextLayout` line coordinates
need any adjustment for padding or `lineHeight` on either platform. The
decision-making those feed is unit-tested; the wiring is not, and cannot be
without a device.

## Initial Scope

The seed package provides, ahead of the RN coordinator:

- the `@supramark/rn-selection` workspace package;
- the vendored native text primitive with upstream credit preserved;
- the selection model, AST linearizer, range resolver, provider contract, and
  serializer (milestone 1 above).

The default RN renderer behavior is intentionally unchanged until the coordinator
is ready.
