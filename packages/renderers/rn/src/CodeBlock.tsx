import React, { createContext, useContext, useEffect, useRef, useState } from 'react';
import { Text, TouchableOpacity, View } from 'react-native';
import type { SupramarkCodeNode } from '@supramark/core';
import type { MergedStyles } from './styles';

/**
 * Context carrying the host-provided copy handler and the copyButton toggle.
 *
 * The Supramark component wraps its rendered tree in this Provider so every
 * CodeBlock can read the host callback without threading it through the
 * top-level renderNode / renderRootNodes signatures (which would require
 * updating every recursive call site).
 */
export interface CodeCopyContextValue {
  onCopyCode?: (code: string, node: SupramarkCodeNode) => void | Promise<void>;
  copyButton?: boolean;
}

export const CodeCopyContext = createContext<CodeCopyContextValue>({});

interface CodeBlockProps {
  node: SupramarkCodeNode;
  styles: MergedStyles;
  /** Already-rendered code content (the inner <Text> tree, with or without highlight tokens). */
  children: React.ComponentProps<typeof Text>['children'];
}

/**
 * Renders the code-block shell with an optional top-right copy button.
 *
 * React Native stays clipboard-free: the button is rendered only when the host
 * provides `onCopyCode` (and has not disabled it via `copyButton: false`).
 * The host owns the clipboard API (expo-clipboard / @react-native-clipboard /
 * mini-program clipboard) inside that callback.
 */
export function CodeBlock({ node, styles, children }: CodeBlockProps): React.ReactElement {
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
  // stay a plain <View> without a "Copy" button.
  const showButton =
    copyButton !== false && typeof onCopyCode === 'function' && Boolean(node.lang);

  const handlePress = (): void => {
    if (!onCopyCode) {
      return;
    }
    void onCopyCode(node.value, node);
    setCopied(true);
    if (timerRef.current) {
      clearTimeout(timerRef.current);
    }
    timerRef.current = setTimeout(() => setCopied(false), 1500);
  };

  if (!showButton) {
    return <View style={styles.codeBlock}>{children}</View>;
  }

  return (
    <View style={[styles.codeBlock, { position: 'relative' }]}>
      {children}
      <TouchableOpacity
        style={styles.codeButton}
        onPress={handlePress}
        accessibilityLabel="Copy code"
      >
        <Text style={styles.codeButtonText}>{copied ? 'Copied' : 'Copy'}</Text>
      </TouchableOpacity>
    </View>
  );
}
