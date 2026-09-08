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
 * Renders the code-block shell with an optional header row carrying the
 * language label (left) and a copy button (right), so the button never
 * overlays a code line.
 *
 * React Native stays clipboard-free: the button is rendered only when the host
 * provides `onCopyCode` (and has not disabled it via `copyButton: false`).
 * The host owns the clipboard API (expo-clipboard / @react-native-clipboard /
 * mini-program clipboard) inside that callback.
 */
export function CodeBlock({ node, styles, children }: CodeBlockProps): React.ReactElement {
  const { onCopyCode, copyButton } = useContext(CodeCopyContext);
  const [copied, setCopied] = useState(false);
  // Prevent a host Promise that settles after unmount from updating state or
  // creating a feedback timer that no mounted block can consume.
  const mountedRef = useRef(true);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Clear the "Copied" reset timer if the block unmounts mid-feedback.
  useEffect(() => {
    // StrictMode replays effect setup, so mark the live setup as mounted too.
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
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
  const showButton = copyButton !== false && typeof onCopyCode === 'function' && Boolean(node.lang);

  // Wait for the host handler to resolve before flipping the label: a
  // rejected onCopyCode must leave "Copy" in place so the user does not see a
  // fake success. The handler stays void (not async) to satisfy the onPress
  // contract; the async IIFE carries its own catch.
  const handlePress = (): void => {
    if (!onCopyCode) {
      return;
    }
    void (async () => {
      try {
        await onCopyCode(node.value, node);
        // The host callback may resolve after navigation removed this block.
        if (!mountedRef.current) {
          return;
        }
        setCopied(true);
        if (timerRef.current) {
          clearTimeout(timerRef.current);
        }
        timerRef.current = setTimeout(() => setCopied(false), 1500);
      } catch {
        // Host handler rejected: keep "Copy".
      }
    })();
  };

  if (!showButton) {
    return <View style={styles.codeBlock}>{children}</View>;
  }

  return (
    <View style={styles.codeBlockContainer}>
      <View style={styles.codeBlockHeader}>
        <Text style={styles.codeBlockLang}>{node.lang}</Text>
        <TouchableOpacity
          style={styles.codeButton}
          onPress={handlePress}
          accessibilityRole="button"
          accessibilityLabel={copied ? 'Copied code' : 'Copy code'}
        >
          <Text style={styles.codeButtonText}>{copied ? 'Copied' : 'Copy'}</Text>
        </TouchableOpacity>
      </View>
      <View style={styles.codeBlockBody}>{children}</View>
    </View>
  );
}
