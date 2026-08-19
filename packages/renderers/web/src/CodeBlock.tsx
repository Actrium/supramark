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
// header / lang / button / container, the inline style is dropped so the
// className owns it. The button lives in a header row (lang left, button
// right) instead of an absolute overlay, so it never covers a code line.
const INLINE_CONTAINER_STYLE: React.CSSProperties = {
  backgroundColor: '#f5f5f5',
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
  color: 'rgba(0, 0, 0, 0.55)',
  fontFamily: 'monospace',
  userSelect: 'none',
};
const INLINE_CODEBLOCK_STYLE: React.CSSProperties = {
  margin: 0,
  padding: 16,
  overflowX: 'auto',
};
const INLINE_BUTTON_STYLE: React.CSSProperties = {
  backgroundColor: 'rgba(0, 0, 0, 0.5)',
  color: '#ffffff',
  border: 'none',
  borderRadius: 4,
  padding: '4px 8px',
  fontSize: 12,
  cursor: 'pointer',
  userSelect: 'none',
};

/**
 * Renders the code-block shell with an optional header row carrying the
 * language label (left) and a copy button (right), so the button never
 * overlays a code line.
 *
 * Web defaults to `navigator.clipboard.writeText` (zero dependency); the host
 * can override the action via `onCopyCode`. Disable the button with
 * `copyButton: false`.
 */
export function CodeBlock({ node, classNames, children }: CodeBlockProps): React.ReactElement {
  const { onCopyCode, copyButton } = useContext(CodeCopyContext);
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Clear the "Copied" reset timer if the block unmounts mid-feedback.
  useEffect(() => {
    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }
    };
  }, []);

  // Only fenced code blocks that declare a language (info string) get the
  // button. The AST does not distinguish fenced from indented code (both are
  // `type: 'code'`), so `node.lang` is the signal that the author marked a
  // real code block; indented pre-formatted text and language-less fences
  // stay a plain <pre> without a "Copy" button.
  const showButton = copyButton !== false && Boolean(node.lang);

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
        } else if (typeof navigator !== 'undefined' && navigator.clipboard) {
          await navigator.clipboard.writeText(node.value);
        } else {
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
  const codeBlockStyle = classNames.codeBlock ? undefined : INLINE_CODEBLOCK_STYLE;

  // No button: render the pre as before (no wrapper div).
  if (!showButton) {
    return <pre className={classNames.codeBlock} style={codeBlockStyle}>{children}</pre>;
  }

  return (
    <div className={classNames.codeBlockContainer} style={containerStyle}>
      <div className={classNames.codeBlockHeader} style={headerStyle}>
        <span className={classNames.codeBlockLang} style={langStyle}>
          {node.lang}
        </span>
        <button
          type="button"
          className={classNames.codeButton}
          style={buttonStyle}
          onClick={handleClick}
          aria-label={copied ? 'Copied code' : 'Copy code'}
        >
          <span className={classNames.codeButtonText}>{copied ? 'Copied' : 'Copy'}</span>
        </button>
      </div>
      <pre className={classNames.codeBlock} style={codeBlockStyle}>{children}</pre>
    </div>
  );
}
