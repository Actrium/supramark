/**
 * Code-block helpers shared by the Web and React Native renderers.
 *
 * These live in core (not in a renderer) because both renderers must hand a
 * host the *same* string for the same block, and because the rule below is a
 * property of the AST + source pair rather than of any one platform.
 */

import type { SupramarkCodeNode } from './ast.js';

/**
 * Wrap `body` in a three-backtick fence carrying `info`.
 *
 * The body's line endings are left alone except for the one that terminates it:
 * a parser value already ends in a newline, so appending another would invent a
 * blank line the author never wrote. An empty body gets no body line at all,
 * matching an empty fence in source.
 *
 * Exported because the selection layer builds the same Markdown for its
 * copy-as-Markdown action; one implementation keeps what a host copies and what
 * a host receives from the fullscreen action identical.
 */
export function fenceMarkdown(info: string, body: string): string {
  const terminated = body === '' ? '' : body.endsWith('\n') ? body : `${body}\n`;
  return `\`\`\`${info}\n${terminated}\`\`\``;
}

/**
 * Rebuild a code block's fenced Markdown from its node alone.
 *
 * Used as the fallback when the original source is unavailable (a host that
 * renders a pre-parsed `ast` without passing `markdown`).
 *
 * Fidelity caveat: this is a *canonical* fence, not the author's text. A block
 * whose content itself contains ``` (fenced in the source with four backticks
 * or a `~~~` fence) cannot be expressed with a three-backtick fence — the inner
 * fence closes it early and the result parses as a different document. Prefer
 * `fencedCodeFromSource` whenever the source is at hand.
 */
export function rebuildFencedCode(node: SupramarkCodeNode): string {
  const info = node.lang ? node.lang + (node.meta ? ` ${node.meta}` : '') : '';
  return fenceMarkdown(info, node.value);
}

/**
 * An opening or closing fence line: up to three spaces of indent (a block may be
 * indented inside a list or a container), a run of markers, then whatever
 * follows.
 */
const FENCE_LINE = /^ {0,3}(`{3,}|~{3,})(.*)$/;

/**
 * Does `slice` look like *this* node's fence?
 *
 * The substring guard alone is not enough. `expandOpaqueContainers` re-parses a
 * container's body on its own and grafts the result back onto the tree without
 * rebasing its `position`, so a code block inside `:::note` still reports
 * offsets relative to the container body. Slicing the whole document at those
 * offsets lands on **some earlier block**, and a substring check waves it
 * through whenever that block contains the same lines — the host would be
 * handed a plausible, entirely wrong fragment.
 *
 * Nor is a substring check plus a matching info string enough. A wrong window
 * can land on a fence of the same language and length: the opening marker's
 * length and the body's length trade off against each other, so a four-backtick
 * block whose body is `AAA\n` and a three-backtick block whose body is
 * `AAA\nX\n` occupy the same number of characters, and a slice of the latter
 * "contains" the former's value.
 *
 * So the body is compared for **equality**, not containment: everything between
 * the opening and closing lines must be the node's own value, either verbatim or
 * with the indent the parser stripped from an indented fence put back (CommonMark
 * removes up to N spaces, N being the opening fence's indent, so each line is
 * tried at 1–3). Equality is what makes the window's identity decidable; a
 * window onto a different block can only match by being that block's exact text.
 *
 * This *replaces* the substring check rather than tightening it. An indented
 * fence's body is the value with its indent restored, which does not contain the
 * value as a substring at all — so containment would reject some correct slices
 * while still accepting a wrong window whose length happens to match.
 */
function sliceIsNodeFence(node: SupramarkCodeNode, slice: string): boolean {
  const openingLineEnd = slice.indexOf('\n');
  // A fence is at least two lines: markers, then a closing line.
  if (openingLineEnd === -1) {
    return false;
  }
  const opening = FENCE_LINE.exec(slice.slice(0, openingLineEnd));
  if (!opening) {
    return false;
  }
  const marker = opening[1];
  // Compare info strings with whitespace normalised on both sides — the author
  // may space the fence info freely, and the parser collapses it into lang/meta.
  const info = opening[2].trim().split(/\s+/).filter(Boolean).join(' ');
  const expected = [node.lang, node.meta].filter(Boolean).join(' ');
  if (info !== expected) {
    return false;
  }
  const closingLineStart = slice.lastIndexOf('\n') + 1;
  const closing = FENCE_LINE.exec(slice.slice(closingLineStart));
  // A closing fence carries no info string, and cannot be shorter than the one
  // that opened the block.
  if (!closing || closing[1][0] !== marker[0] || closing[1].length < marker.length) {
    return false;
  }
  if (closing[2].trim() !== '') {
    return false;
  }
  const body = slice.slice(openingLineEnd + 1, closingLineStart);
  if (body === node.value) {
    return true;
  }
  for (let indent = 1; indent <= 3; indent += 1) {
    if (body === indentLines(node.value, indent)) {
      return true;
    }
  }
  return false;
}

/** `text` with `indent` spaces added to every non-empty line. */
function indentLines(text: string, indent: number): string {
  const pad = ' '.repeat(indent);
  return text
    .split('\n')
    .map(line => (line === '' ? line : pad + line))
    .join('\n');
}

/**
 * Slice a code block's fenced Markdown out of the original source.
 *
 * The AST's `position` spans the whole fence (opening markers through closing
 * markers), so the slice is the author's exact text — fence length, `~~~`
 * markers, info-string spacing and all.
 *
 * Offsets are read from `utf16_offset`, never `byte_offset`: JavaScript strings
 * are UTF-16, so a CJK-heavy document would slice at the wrong place otherwise.
 *
 * Returns `null` whenever the slice cannot be shown to be this node's fence:
 * a position that is absent or out of range; a slice whose body is not the
 * node's value (see `sliceIsNodeFence`, which is what makes a wrong window
 * detectable at all); or an unterminated fence (the position of a still-open
 * fence stops before the line ending its value carries, so streaming input
 * deliberately takes the rebuild path and gets a closed fence instead). Callers
 * then fall back to `rebuildFencedCode`, trading the author's exact text for a
 * canonical fence of the right content — which beats handing a host wrong
 * content.
 *
 * Known limit: a code block inside a `:::` container never slices, because its
 * `position` is relative to the container body rather than the document. Its
 * `fenced` is the canonical rebuild. Fixing that needs the parser to report a
 * container body's offset so the subtree can be rebased.
 */
export function fencedCodeFromSource(node: SupramarkCodeNode, source: string): string | null {
  const start = node.position?.start.utf16_offset;
  const end = node.position?.end.utf16_offset;
  if (typeof start !== 'number' || typeof end !== 'number' || end <= start) {
    return null;
  }
  // Guard against an out-of-range position (a stale AST against a re-rendered
  // markdown prop) before touching the string.
  if (end > source.length) {
    return null;
  }
  const sliced = source.slice(start, end);
  return sliceIsNodeFence(node, sliced) ? sliced : null;
}

/**
 * The fenced Markdown for a code block, preferring the author's own text.
 *
 * `source` is the Markdown the AST was parsed from; pass `undefined` when the
 * host rendered a pre-parsed AST instead (then the canonical fence is the only
 * option).
 *
 * A host that re-renders the result (`<Supramark markdown={fenced} />`) or
 * re-parses it gets the block back — which is the point of handing over the
 * fence rather than the bare `value`.
 */
export function fencedCode(node: SupramarkCodeNode, source?: string): string {
  if (source) {
    const sliced = fencedCodeFromSource(node, source);
    if (sliced !== null) {
      return sliced;
    }
  }
  return rebuildFencedCode(node);
}

/**
 * Host overrides for the code-block action labels.
 *
 * The labels are not visible text — the buttons are icon-only — they are what a
 * screen reader announces, and what Web shows as the native hover tooltip. That
 * makes them the one part of the button a host must be able to localize: the
 * library ships English defaults and never guesses a language.
 *
 * Shared by both renderers rather than defined in each, so the two platforms
 * cannot drift on wording any more than they can on the artwork.
 */
export interface SupramarkCodeActionLabels {
  /** Announced for the copy button. */
  copy?: string;
  /** Announced for the fullscreen button; also its hover tooltip on Web. */
  fullscreen?: string;
}

/** Labels used when the host supplies none. */
export const DEFAULT_CODE_ACTION_LABELS: Required<SupramarkCodeActionLabels> = {
  copy: 'Copy code',
  fullscreen: 'Open code fullscreen',
};

/**
 * Everything a host needs to take over a code block, handed to the fullscreen
 * action (and available to any future host-owned action).
 *
 * Both renderers build this through `codeBlockContent()` so a host receives the
 * same shape — and the same `fenced` string — whichever platform it runs on.
 */
export interface SupramarkCodeBlockContent {
  /** The code itself, without fences (the AST's `value`). */
  code: string;
  /** The whole block as Markdown, fences included. */
  fenced: string;
  /** Info-string language, when the author declared one. */
  lang?: string;
  /** Info-string remainder after the language (e.g. `title="a"`). */
  meta?: string;
  /**
   * The AST node this content came from, including its `position` when the
   * parser recorded one.
   *
   * **Read-only — do not mutate it.** This is the live node from the rendered
   * tree, not a copy: the same snapshot may be shared across renderer instances
   * (the RN renderer caches parsed documents and a virtualized list remounts
   * them), so a mutation leaks into other renders. Only development builds
   * freeze the tree; in production a mutation is silent.
   */
  node: SupramarkCodeNode;
}

/**
 * Build the payload a host receives when it takes over a code block.
 *
 * Conditional fields are omitted rather than set to `undefined`, so a host can
 * use `'lang' in content` and the object survives `JSON.stringify` intact.
 */
export function codeBlockContent(
  node: SupramarkCodeNode,
  source?: string
): SupramarkCodeBlockContent {
  return {
    code: node.value,
    fenced: fencedCode(node, source),
    ...(node.lang === undefined ? {} : { lang: node.lang }),
    ...(node.meta === undefined ? {} : { meta: node.meta }),
    node,
  };
}
