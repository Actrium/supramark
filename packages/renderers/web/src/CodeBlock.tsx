import React, { createContext, useContext, useEffect, useRef, useState } from 'react';
import type { SupramarkCodeNode } from '@supramark/core';
import type { SupramarkClassNames } from './classNames';

/**
 * Context carrying the host-provided copy handler and the copyButton toggle.
 *
 * The Supramark component wraps its rendered tree in this Provider so every
 * CodeBlock can read the host callback without threading it through the
 * top-level renderNode signature (which would require updating every
 * recursive call site).
 */
export interface CodeCopyContextValue {
  onCopyCode?: (code: string, node: SupramarkCodeNode) => void | Promise<void>;
  copyButton?: boolean;
}

export const CodeCopyContext = createContext<CodeCopyContextValue>({});

interface CodeBlockProps {
  node: SupramarkCodeNode;
  classNames: SupramarkClassNames;
  /** Already-rendered code content (the inner <code> tree, with or without highlight tokens). */
  children: React.ReactNode;
}

// Inline fallback styles so the button works out of the box even when the host
// uses the empty defaultClassNames. When the host supplies a className for the
// container / header / lang / button / body, the inline style is dropped so
// className owns it. The button lives in a header row (lang left, button
// right) instead of an absolute overlay, so it never covers a code line.
//
// Colors go through CSS variables with the old values as fallbacks, so a host
// can restyle the card (including a dark branch) from plain CSS without
// fighting inline-style specificity:
//   :root { --sm-code-bg: #2d2d2d; --sm-code-lang-color: rgba(255,255,255,0.6); }
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
const INLINE_BUTTON_STYLE: React.CSSProperties = {
  backgroundColor: 'var(--sm-code-btn-bg, rgba(0, 0, 0, 0.5))',
  color: 'var(--sm-code-btn-color, #ffffff)',
  border: 'none',
  borderRadius: 4,
  padding: '4px 8px',
  fontSize: 12,
  cursor: 'pointer',
  userSelect: 'none',
};

/**
 * Renders the code-block shell: a card around every code block, with a header
 * row (language label left, copy button right) on fenced blocks that declare a
 * language, so the button never overlays a code line and neighbouring blocks
 * look alike whether or not they carry a language.
 *
 * Web defaults to `navigator.clipboard.writeText` (zero dependency); the host
 * can override the action via `onCopyCode`. `copyButton: false` opts out of
 * the card entirely and renders the bare spec `<pre><code>` tree (the shape
 * the conformance harness measures).
 *
 * Mouse clicks do not focus the button (mousedown default is prevented) so
 * Safari/Firefox do not draw a focus outline after copying; keyboard Tab
 * focus still shows the browser focus ring.
 */
export function CodeBlock({ node, classNames, children }: CodeBlockProps): React.ReactElement {
  const { onCopyCode, copyButton } = useContext(CodeCopyContext);
  const [copied, setCopied] = useState(false);
  // Clipboard capability is detected post-mount so SSR and the first client
  // render produce the same tree (hydration-safe); environments without
  // navigator.clipboard (insecure context, older browser) and no onCopyCode
  // then hide the button instead of leaving an inert one.
  const [canCopy, setCanCopy] = useState(true);
  // Prevent a copy Promise that settles after unmount from updating state or
  // creating a feedback timer that no mounted block can consume.
  const mountedRef = useRef(true);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    // A partial Clipboard object without writeText is not actionable; checking
    // the function avoids rendering an inert control in older/custom hosts.
    setCanCopy(
      Boolean(onCopyCode) ||
        (typeof navigator !== 'undefined' && typeof navigator.clipboard?.writeText === 'function')
    );
  }, [onCopyCode]);

  // Timer cleanup is mount-scoped: tying it to onCopyCode would cancel the
  // pending label reset whenever a host replaces its callback reference.
  useEffect(() => {
    // StrictMode replays effect setup, so mark the live setup as mounted too.
    mountedRef.current = true;
    // Clear the "Copied" reset timer if the block unmounts mid-feedback.
    return () => {
      mountedRef.current = false;
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }
    };
  }, []);

  // copyButton !== false wraps every code block in the card, so a fence with a
  // language and one without do not look unrelated in the same document. The
  // header row (language label + copy button) still requires node.lang: the
  // AST does not distinguish fenced from indented code (both are
  // `type: 'code'`), so lang is the only signal that the author marked a real
  // code block. Documented decision: language-less fences and indented code
  // get the card but no header/button; giving them a button needs a `fenced`
  // flag in the parser.
  const showCard = copyButton !== false;
  const showHeader = Boolean(node.lang);
  const showButton = showHeader && canCopy;

  // Wait for the clipboard write to resolve before flipping the label: a
  // rejected writeText (non-secure context, denied permission, locked-down
  // iframe) or a rejected onCopyCode must leave "Copy" in place so the user
  // does not see a fake success. The handler stays void (not async) to satisfy
  // the onClick contract; the async IIFE carries its own catch so rejections
  // never surface as unhandledrejection. Hosts that want to observe failures
  // do so inside their own onCopyCode handler.
  const handleClick = (): void => {
    void (async () => {
      try {
        if (onCopyCode) {
          await onCopyCode(node.value, node);
        } else if (
          typeof navigator !== 'undefined' &&
          typeof navigator.clipboard?.writeText === 'function'
        ) {
          await navigator.clipboard.writeText(node.value);
        } else {
          return;
        }
        // The clipboard may resolve after navigation removed this block.
        if (!mountedRef.current) {
          return;
        }
        setCopied(true);
        if (timerRef.current) {
          clearTimeout(timerRef.current);
        }
        timerRef.current = setTimeout(() => setCopied(false), 1500);
      } catch {
        // Clipboard write or host handler rejected: keep "Copy".
      }
    })();
  };

  const containerStyle = classNames.codeBlockContainer ? undefined : INLINE_CONTAINER_STYLE;
  const headerStyle = classNames.codeBlockHeader ? undefined : INLINE_HEADER_STYLE;
  const langStyle = classNames.codeBlockLang ? undefined : INLINE_LANG_STYLE;
  const buttonStyle = classNames.codeButton ? undefined : INLINE_BUTTON_STYLE;
  const codeBlockBodyStyle = classNames.codeBlockBody ? undefined : INLINE_CODEBLOCK_BODY_STYLE;

  // copyButton: false renders the pre as before (no wrapper div, no inline
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
          {showButton && (
            <button
              type="button"
              className={classNames.codeButton}
              style={buttonStyle}
              onMouseDown={event => event.preventDefault()}
              onClick={handleClick}
              aria-label={copied ? 'Copied code' : 'Copy code'}
            >
              <span className={classNames.codeButtonText}>{copied ? 'Copied' : 'Copy'}</span>
            </button>
          )}
        </div>
      )}
      <pre className={classNames.codeBlockBody} style={codeBlockBodyStyle}>
        {children}
      </pre>
    </div>
  );
}
