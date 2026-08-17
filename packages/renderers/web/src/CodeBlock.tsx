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
// button or container, the inline style is dropped so the className owns it.
const INLINE_CONTAINER_STYLE: React.CSSProperties = { position: 'relative' };
const INLINE_BUTTON_STYLE: React.CSSProperties = {
  position: 'absolute',
  top: 8,
  right: 8,
  backgroundColor: 'rgba(0, 0, 0, 0.5)',
  color: '#ffffff',
  border: 'none',
  borderRadius: 4,
  padding: '4px 8px',
  fontSize: 12,
  cursor: 'pointer',
};

/**
 * Renders the code-block shell with an optional top-right copy button.
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

  const showButton = copyButton !== false;

  const handleClick = (): void => {
    if (onCopyCode) {
      onCopyCode(node.value, node);
    } else if (typeof navigator !== 'undefined' && navigator.clipboard) {
      void navigator.clipboard.writeText(node.value);
    }
    setCopied(true);
    if (timerRef.current) {
      clearTimeout(timerRef.current);
    }
    timerRef.current = setTimeout(() => setCopied(false), 1500);
  };

  // No button: render the pre as before (no wrapper div).
  if (!showButton) {
    return <pre className={classNames.codeBlock}>{children}</pre>;
  }

  const containerStyle = classNames.codeBlockContainer ? undefined : INLINE_CONTAINER_STYLE;
  const buttonStyle = classNames.codeButton ? undefined : INLINE_BUTTON_STYLE;

  return (
    <div className={classNames.codeBlockContainer} style={containerStyle}>
      <pre className={classNames.codeBlock}>{children}</pre>
      <button
        type="button"
        className={classNames.codeButton}
        style={buttonStyle}
        onClick={handleClick}
        aria-label="Copy code"
      >
        <span className={classNames.codeButtonText}>{copied ? 'Copied' : 'Copy'}</span>
      </button>
    </div>
  );
}
