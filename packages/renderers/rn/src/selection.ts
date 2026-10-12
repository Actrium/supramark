import { SUPRAMARK_ADMONITION_KINDS } from '@supramark/core';
import type { SupramarkContainerNode, SupramarkNode, SupramarkRootNode } from '@supramark/core';
import {
  linearizeDocument,
  type SelectableBlockBinding,
  type SelectionAtomUnit,
  type SelectionCopyRequest,
  type SelectionRange,
  type SelectionRootProps,
  type SelectionToolbarItem,
  type SelectionUnit,
} from '@supramark/rn-selection';

/**
 * Host-facing selection configuration for `Supramark`.
 *
 * Passing `selection` mounts the self-drawn selection layer
 * (`@supramark/rn-selection`) over the rendered document: long press selects a
 * word, dragging extends across blocks, the bar shows `toolbarItems`. The
 * package never touches a clipboard — `onAction` receives the serialized
 * selection and the host decides what to do with it.
 *
 * Omitting the prop entirely keeps the existing render path, layout and events,
 * and builds no selection store, unit index, measurement listeners or overlay.
 */
export interface SupramarkSelectionOptions {
  /**
   * Whether selection may currently be entered (default: true). Hosts derive
   * this from their own message state — a still-streaming answer, a failed one,
   * or a message whose final snapshot has not resolved yet passes `false`,
   * which clears any current selection and refuses new ones without touching
   * the content subtree. Distinct from omitting `selection` altogether.
   */
  enabled?: boolean;
  /**
   * Identity of the host's document instance (session / message / question or
   * answer side / fragment). Two bubbles rendering the same cached AST must
   * pass different ids: it namespaces their selections, callbacks and scroll
   * locks so nothing leaks between instances.
   */
  documentId?: string;
  /**
   * The built-in long-press word selection (default: true). A host that starts
   * selection from its own long-press menu passes `false`: the hold then
   * belongs to that menu, and selections start through the handle below — the
   * two gestures never compete.
   */
  longPress?: boolean;
  /**
   * Receives the imperative selection handle once mounted (null on unmount):
   * the entry for host-driven selections — a menu item calling
   * `trySelectParagraphAt` with the long-press coordinates, or a background tap
   * calling `clearSelection`.
   */
  onHandleReady?(this: void, handle: SupramarkSelectionHandle | null): void;
  /**
   * Container names whose host renderer draws readable text and should
   * therefore be selectable. Custom container renderers are opaque cards by
   * default (their content is unknown to this layer); declaring a name here
   * opts it into recursion, so children it renders through `renderNode` become
   * selectable. The host's own chrome is not covered — only rendered children.
   */
  containerText?: readonly string[];
  /**
   * Actions shown in the selection bar; defaults to Copy / Copy as Markdown.
   * An item without a `format` still receives the selected plain text, so host
   * actions such as "Quote" or "Ask AI" need no format of their own.
   */
  toolbarItems?: readonly SelectionToolbarItem[];
  /** Receives the serialized selection when a toolbar action fires. */
  onAction?(request: SupramarkSelectionActionRequest): void;
  /** How long a press must be held before it selects. Defaults to 400ms. */
  longPressMs?: number;
  /** Finger drift allowed during the hold before the press counts as a scroll. */
  moveTolerance?: number;
  /**
   * Notifies an enclosing scroll container while a selection drag owns the
   * touch, so the host can pause its scrolling for the duration.
   */
  onGestureActiveChange?(this: void, active: boolean): void;
  /**
   * Reports the selection range on every change, and null when it is cleared —
   * the host's signal for outside entries such as dismissing a selection with a
   * background tap.
   */
  onSelectionChange?(this: void, range: SelectionRange | null): void;
  /** Replace the built-in bar with a host component. */
  renderToolbar?: SelectionRootProps['renderToolbar'];
  /** Toggle the built-in layers; all default to on. */
  overlay?: boolean;
  handles?: boolean;
  toolbar?: boolean;
}

/**
 * What `onAction` receives: the serialized selection plus the plan identity it
 * belongs to, so the host can verify a late callback against the content
 * version and document instance it was raised for.
 */
export interface SupramarkSelectionActionRequest extends SelectionCopyRequest {
  /** The plan's document instance id, when the host supplied one. */
  documentId?: string;
  /** Monotonic revision of the parsed snapshot the selection belongs to. */
  revision: number;
}

/**
 * Imperative commands for host-driven selection entries (see
 * `SupramarkSelectionOptions.onHandleReady`).
 */
export interface SupramarkSelectionHandle {
  /**
   * Select the block under a press, resolved within this root's own block
   * extent. Pass RN page coordinates (`nativeEvent.pageX/pageY`, or the last
   * two values of `View.measure`) relative to the same React root. Do not pass
   * `measureInWindow` coordinates or raw RNGH `absoluteX/absoluteY`.
   * Resolves with the REAL outcome: false when selection is disabled, the root
   * is out of reach, or nothing selectable is under the press.
   */
  trySelectParagraphAt(pageX: number, pageY: number): Promise<boolean>;
  /** Clear the current selection (an outside entry, e.g. a background tap). */
  clearSelection(): void;
  /**
   * This root's live RN page-space box, measured on demand for a host that must
   * pick ONE root among several before asking it to select. Null when it
   * cannot be measured.
   */
  measurePageRect(): Promise<{ x: number; y: number; w: number; h: number } | null>;
}

/**
 * The selection plan for one parsed document snapshot.
 *
 * Built from the same AST the renderer walks, in one `linearizeDocument` pass,
 * so node identities, the unit stream and the per-block bindings can never
 * disagree. The renderer only consumes the plan: blocks bind to it by AST node
 * identity, and the revision names the content version the plan describes.
 */
export interface SupramarkSelectionPlan {
  /** Host identity of the document instance, when the host supplied one. */
  documentId?: string;
  /**
   * Monotonic revision of the parsed snapshot this plan was built from. Every
   * document change gets a fresh revision, so late callbacks and gestures can
   * be matched against the content they actually belong to.
   */
  revision: number;
  units: readonly SelectionUnit[];
  /** Undefined when this node is not rendered as a selectable text block. */
  bindingFor(node: SupramarkNode): SelectableBlockBinding | undefined;
  /**
   * The atom unit belonging to a rendered inline object, matched by AST node
   * identity. O(1): every inline object would otherwise rescan the unit stream
   * on each render.
   */
  atomFor(node: SupramarkNode): SelectionAtomUnit | undefined;
}

/**
 * Opaque container names under the given host renderers and text declarations:
 * the built-in html / map cards plus every host container renderer except the
 * names declared as text. Exported so the renderer and its tests derive the
 * same set from one place.
 */
export function resolveOpaqueContainers(
  containerRenderers: Record<string, unknown> | undefined,
  declaredText: readonly string[] | undefined
): ReadonlySet<string> {
  const names = new Set<string>(['html', 'map']);
  const textDeclared = new Set(declaredText ?? []);
  for (const name of Object.keys(containerRenderers ?? {})) {
    if (!textDeclared.has(name)) names.add(name);
  }
  return names;
}

/**
 * A container's title exactly as the render branches draw it — the admonition
 * branches use `params || data.title`, a generic container draws
 * `name: params`. html / map cards are opaque and never reach this path; a host
 * renderer declared as text draws its own chrome, which this layer cannot know.
 */
export function rendererContainerTitle(node: SupramarkContainerNode): string | undefined {
  const name = node.name;
  if (SUPRAMARK_ADMONITION_KINDS.includes(name as (typeof SUPRAMARK_ADMONITION_KINDS)[number])) {
    return node.params || (node.data?.title as string | undefined);
  }
  return node.params ? `${name}: ${node.params}` : undefined;
}

export function buildSelectionPlan(
  root: SupramarkRootNode | SupramarkNode[],
  identity: {
    documentId?: string;
    revision: number;
    /**
     * Container names the renderer draws as opaque cards — their content is not
     * readable text, so it stays a hard boundary (see `linearizeDocument`).
     */
    opaqueContainers?: ReadonlySet<string>;
    /**
     * Mirrors the renderer's feature switches: a node that currently draws its
     * raw source as readable text (a disabled math / diagram feature) must
     * linearize as text, not as an object (see `linearizeDocument`).
     */
    rendersAsText?: (node: SupramarkNode) => boolean;
    /**
     * Container titles as the renderer draws them; see `linearizeDocument`.
     */
    containerTitle?: (node: SupramarkContainerNode) => string | undefined;
  }
): SupramarkSelectionPlan {
  const { units, blocks } = linearizeDocument(root, {
    opaqueContainers: identity.opaqueContainers,
    rendersAsText: identity.rendersAsText,
    containerTitle: identity.containerTitle,
  });
  const byNode = new Map<SupramarkNode, SelectableBlockBinding>(
    blocks.map(binding => [binding.node, binding])
  );
  const atomsByNode = new Map<SupramarkNode, SelectionAtomUnit>();
  for (const unit of units) {
    if (unit.kind === 'atom') atomsByNode.set(unit.node, unit);
  }
  return {
    documentId: identity.documentId,
    revision: identity.revision,
    units,
    bindingFor: node => byNode.get(node),
    atomFor: node => atomsByNode.get(node),
  };
}
