import React, { createContext, useContext, useEffect, useState } from 'react';
import {
  DEFAULT_CODE_ACTION_LABELS,
  type SupramarkCodeActionLabels,
  type SupramarkCodeNode,
} from '@supramark/core';
import { CodeActionIcon } from './CodeActionIcon';
import type { SupramarkClassNames } from './classNames';

/**
 * Context carrying everything a CodeBlock needs from the host: the copy
 * handler, the switches and the labels.
 *
 * The fullscreen action is React Native only — a mobile code block is the one
 * that needs to escape a narrow column, and the Web renderer keeps no handler
 * for it. `onFullscreenCode` lives on the RN props; the shared payload type
 * still comes from `@supramark/core`.
 *
 * The Supramark component wraps its rendered tree in this Provider so every
 * CodeBlock can read the host configuration without threading it through the
 * top-level renderNode signature (which would require updating every recursive
 * call site).
 */
export interface CodeActionsContextValue {
  onCopyCode?: (code: string, node: SupramarkCodeNode) => void | Promise<void>;
  /**
   * Master switch for the code-block chrome. `false` renders the bare spec
   * `<pre><code>` tree, whatever the other switches say.
   */
  codeBlockChrome?: boolean;
  /** Copy button (default: shown when a copy target exists). */
  copyButton?: boolean;
  /** Overrides for the built-in labels. */
  labels?: SupramarkCodeActionLabels;
}

export const CodeActionsContext = createContext<CodeActionsContextValue>({});

interface CodeBlockProps {
  node: SupramarkCodeNode;
  classNames: SupramarkClassNames;
  /** Already-rendered code content (the inner <code> tree, with or without highlight tokens). */
  children: React.ReactNode;
}

/** Edge length of an action icon, in pixels. Matches the RN icon's fontSize. */
const ACTION_ICON_SIZE = 18;

// Inline fallback styles so the buttons work out of the box even when the host
// uses the empty defaultClassNames. When the host supplies a className for the
// container / header / lang / actions / button / body, the inline style is
// dropped so className owns it. The buttons live in a header row (lang left,
// actions right) instead of an absolute overlay, so they never cover a code
// line.
//
// Colors go through CSS variables with the old values as fallbacks, so a host
// can restyle the card (including a dark branch) from plain CSS without
// fighting inline-style specificity:
//   :root { --sm-code-bg: #2d2d2d; --sm-code-lang-color: rgba(255,255,255,0.6); }
// The icons paint with `currentColor`, so --sm-code-action-color tints them.
const INLINE_CONTAINER_STYLE: React.CSSProperties = {
  backgroundColor: 'var(--sm-code-bg, #f5f5f5)',
  borderRadius: 4,
  marginBottom: 16,
  overflow: 'hidden',
};
const INLINE_HEADER_STYLE: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  padding: '4px 8px',
  userSelect: 'none',
};
const INLINE_LANG_STYLE: React.CSSProperties = {
  fontSize: 12,
  color: 'var(--sm-code-lang-color, rgba(0, 0, 0, 0.55))',
  fontFamily: 'monospace',
  userSelect: 'none',
};
const INLINE_CODEBLOCK_BODY_STYLE: React.CSSProperties = {
  margin: 0,
  padding: 16,
  overflowX: 'auto',
};
const INLINE_ACTIONS_STYLE: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  // Matches the RN action row, so the two platforms space the buttons alike.
  gap: 6,
};
const INLINE_BUTTON_STYLE: React.CSSProperties = {
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  // An icon-only button has no text to give it width, so the touch target is
  // set explicitly rather than left to padding around a glyph.
  width: 28,
  height: 24,
  padding: 0,
  // A <button> paints `buttonface` by default; clear it so the resting state is
  // just the icon on the card, and let the hover state add the tint back.
  background: 'none',
  color: 'var(--sm-code-action-color, rgba(0, 0, 0, 0.55))',
  border: 'none',
  borderRadius: 4,
  cursor: 'pointer',
  userSelect: 'none',
};
/** Tint applied while a pointer is over the button (and only then). */
const INLINE_BUTTON_HOVER_BACKGROUND = 'var(--sm-code-action-hover-bg, rgba(0, 0, 0, 0.06))';

/**
 * Renders the code-block shell: a card around every code block, with a header
 * row (language label left, copy button right) on fenced blocks that declare a
 * language, so the button never overlays a code line and neighbouring blocks
 * look alike whether or not they carry a language.
 *
 * The shape follows the same decisions the React Native renderer makes, minus
 * the fullscreen action, which is RN only:
 *   - `codeBlockChrome !== false` — the card;
 *   - `node.lang` — the header row;
 *   - a handler (and its switch) — each button.
 *
 * Web defaults the copy action to `navigator.clipboard.writeText` (zero
 * dependency); the host can override it via `onCopyCode`.
 *
 * Mouse clicks do not focus the button (mousedown default is prevented) so
 * Safari/Firefox do not draw a focus outline after copying; keyboard Tab focus
 * still shows the browser focus ring.
 */
export function CodeBlock({ node, classNames, children }: CodeBlockProps): React.ReactElement {
  const { onCopyCode, codeBlockChrome, copyButton, labels } = useContext(CodeActionsContext);
  // Clipboard capability is detected post-mount so SSR and the first client
  // render produce the same tree (hydration-safe); environments without
  // navigator.clipboard (insecure context, older browser) and no onCopyCode
  // then hide the button instead of leaving an inert one.
  const [canCopy, setCanCopy] = useState(true);
  // Hover is tracked in state rather than with a `:hover` rule because the
  // button's look is inline: an inline style cannot express a pseudo-class, and
  // injecting a stylesheet would be a global side effect this library avoids.
  const [hovered, setHovered] = useState(false);

  useEffect(() => {
    // A partial Clipboard object without writeText is not actionable; checking
    // the function avoids rendering an inert control in older/custom hosts.
    setCanCopy(
      Boolean(onCopyCode) ||
        (typeof navigator !== 'undefined' && typeof navigator.clipboard?.writeText === 'function')
    );
  }, [onCopyCode]);

  // The header row still requires node.lang: the AST does not distinguish
  // fenced from indented code (both are `type: 'code'`), so lang is the only
  // signal that the author marked a real code block. Documented decision:
  // language-less fences and indented code get the card but no header/button;
  // giving them a button needs a `fenced` flag in the parser.
  const showCard = codeBlockChrome !== false;
  const showHeader = Boolean(node.lang);
  const showCopy = copyButton !== false && canCopy;

  const copyLabel = labels?.copy ?? DEFAULT_CODE_ACTION_LABELS.copy;

  // The button has no success state to render, so the handler only performs the
  // copy: a rejected writeText (non-secure context, denied permission,
  // locked-down iframe) or a rejected onCopyCode is swallowed here so it never
  // surfaces as an unhandledrejection. Hosts that want to observe failures do so
  // inside their own onCopyCode handler.
  const handleCopyClick = (): void => {
    void (async () => {
      try {
        if (onCopyCode) {
          await onCopyCode(node.value, node);
        } else if (
          typeof navigator !== 'undefined' &&
          typeof navigator.clipboard?.writeText === 'function'
        ) {
          await navigator.clipboard.writeText(node.value);
        }
      } catch {
        // Clipboard write or host handler rejected.
      }
    })();
  };

  const containerStyle = classNames.codeBlockContainer ? undefined : INLINE_CONTAINER_STYLE;
  const headerStyle = classNames.codeBlockHeader ? undefined : INLINE_HEADER_STYLE;
  const langStyle = classNames.codeBlockLang ? undefined : INLINE_LANG_STYLE;
  const actionsStyle = classNames.codeActions ? undefined : INLINE_ACTIONS_STYLE;
  // The tint belongs to the pointer being over the button; a host that supplies
  // its own className owns the hover styling instead (see classNames.codeAction).
  const buttonStyle = classNames.codeAction
    ? undefined
    : {
        ...INLINE_BUTTON_STYLE,
        ...(hovered ? { background: INLINE_BUTTON_HOVER_BACKGROUND } : null),
      };
  const codeBlockBodyStyle = classNames.codeBlockBody ? undefined : INLINE_CODEBLOCK_BODY_STYLE;

  // codeBlockChrome: false renders the pre as before (no wrapper div, no inline
  // style) so conformance measures the spec <pre><code> DOM.
  if (!showCard) {
    return <pre className={classNames.codeBlock}>{children}</pre>;
  }

  return (
    <div className={classNames.codeBlockContainer} style={containerStyle}>
      {showHeader && (
        <div className={classNames.codeBlockHeader} style={headerStyle}>
          <span className={classNames.codeBlockLang} style={langStyle}>
            {node.lang}
          </span>
          {showCopy && (
            <span className={classNames.codeActions} style={actionsStyle}>
              <button
                type="button"
                className={classNames.codeAction}
                style={buttonStyle}
                onMouseDown={event => event.preventDefault()}
                // Pointer events rather than mouse events: they cover mouse/trackpad/pen, and
                // `pointerType` lets a touch tap through without leaving a stuck tint behind.
                onPointerEnter={event => {
                  if (event.pointerType !== 'touch') setHovered(true);
                }}
                onPointerLeave={() => setHovered(false)}
                onClick={handleCopyClick}
                title={copyLabel}
                aria-label={copyLabel}
              >
                <CodeActionIcon id="copy" size={ACTION_ICON_SIZE} />
              </button>
            </span>
          )}
        </div>
      )}
      <pre className={classNames.codeBlockBody} style={codeBlockBodyStyle}>
        {children}
      </pre>
    </div>
  );
}
