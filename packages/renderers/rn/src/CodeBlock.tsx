import React, { createContext, useContext } from 'react';
import { Text, TouchableOpacity, View } from 'react-native';
import {
  codeBlockContent,
  DEFAULT_CODE_ACTION_LABELS,
  type SupramarkCodeActionLabels,
  type SupramarkCodeBlockContent,
  type SupramarkCodeNode,
} from '@supramark/core';
import { CodeActionIcon } from './CodeActionIcon';
import type { MergedStyles } from './styles';

/**
 * Context carrying everything a CodeBlock needs from the host: the action
 * handlers, the switches, the labels and the source text.
 *
 * The Supramark component wraps its rendered tree in this Provider so every
 * CodeBlock can read the host configuration without threading it through the
 * top-level renderNode / renderRootNodes signatures (which would require
 * updating every recursive call site).
 */
export interface CodeActionsContextValue {
  onCopyCode?: (code: string, node: SupramarkCodeNode) => void | Promise<void>;
  /**
   * Hand the block to the host when the fullscreen button is pressed.
   *
   * The host owns the fullscreen UI entirely (a modal, a route, a panel) — the
   * renderer only reports intent plus content. Omit it and the button is not
   * rendered at all, matching how `onCopyCode` gates the copy button.
   */
  onFullscreenCode?: (content: SupramarkCodeBlockContent) => void;
  /**
   * Master switch for the code-block chrome. `false` renders the bare code view
   * with no card and no header, whatever the other switches say.
   */
  codeBlockChrome?: boolean;
  /** Copy button (default: shown when a copy handler exists). */
  copyButton?: boolean;
  /** Fullscreen button (default: shown when a fullscreen handler exists). */
  fullscreenButton?: boolean;
  /** Overrides for the built-in labels. */
  labels?: SupramarkCodeActionLabels;
  /**
   * The Markdown the AST was parsed from, used to hand a host the author's own
   * fenced text. Absent when the host renders a pre-parsed `ast` without a
   * `markdown` prop; the fence is then rebuilt canonically.
   */
  source?: string;
}

export const CodeActionsContext = createContext<CodeActionsContextValue>({});

// Icon-only buttons are narrower than a text button; the slop restores a
// comfortable touch target without growing the header row. The horizontal slop
// stays at half the gap between buttons, so two neighbouring targets never
// overlap and a tap in the gap cannot land on the wrong action.
const ACTION_HIT_SLOP = { top: 8, bottom: 8, left: 3, right: 3 };

interface CodeBlockProps {
  node: SupramarkCodeNode;
  styles: MergedStyles;
  /** Already-rendered code content (the inner <Text> tree, with or without highlight tokens). */
  children: React.ComponentProps<typeof Text>['children'];
}

/**
 * Renders the code-block shell: a card around the block, with a header row
 * carrying the language label (left) and the action buttons (right) whenever
 * the author declared a language, so a button never overlays a code line.
 *
 * The three decisions mirror the Web renderer so both platforms agree on when
 * a card, a header and each button appear:
 *   - `codeBlockChrome !== false` — the card;
 *   - `node.lang` — the header row;
 *   - a handler (and its switch) — each button.
 *
 * React Native stays clipboard-free: the copy button needs a host
 * `onCopyCode`, which owns the clipboard API (expo-clipboard /
 * @react-native-clipboard / mini-program clipboard).
 */
export function CodeBlock({ node, styles, children }: CodeBlockProps): React.ReactElement {
  const {
    onCopyCode,
    onFullscreenCode,
    codeBlockChrome,
    copyButton,
    fullscreenButton,
    labels,
    source,
  } = useContext(CodeActionsContext);

  // The AST does not distinguish fenced from indented code (both are
  // `type: 'code'`), so `node.lang` is the signal that the author marked a real
  // code block. Indented pre-formatted text and language-less fences get the
  // card but no header, so they cannot carry an action button.
  const showCard = codeBlockChrome !== false;
  const showHeader = Boolean(node.lang);
  const showCopy = copyButton !== false && typeof onCopyCode === 'function';
  const showFullscreen = fullscreenButton !== false && typeof onFullscreenCode === 'function';
  const showActions = showCopy || showFullscreen;

  const copyLabel = labels?.copy ?? DEFAULT_CODE_ACTION_LABELS.copy;
  const fullscreenLabel = labels?.fullscreen ?? DEFAULT_CODE_ACTION_LABELS.fullscreen;

  // The button has no success state to render, so the handler only performs the
  // copy: a rejected onCopyCode is swallowed here so it never surfaces as an
  // unhandledrejection. Hosts that want to observe failures do so inside their
  // own onCopyCode handler.
  const handleCopyPress = (): void => {
    if (!onCopyCode) {
      return;
    }
    void (async () => {
      try {
        await onCopyCode(node.value, node);
      } catch {
        // Host handler rejected.
      }
    })();
  };

  // No feedback state to guard: the host owns what happens next, and this
  // component may well be unmounted by the time it finishes.
  const handleFullscreenPress = (): void => {
    onFullscreenCode?.(codeBlockContent(node, source));
  };

  if (!showCard) {
    return <View style={styles.codeBlock}>{children}</View>;
  }

  return (
    <View style={styles.codeBlockContainer}>
      {showHeader && (
        <View style={styles.codeBlockHeader}>
          <Text style={styles.codeBlockLang}>{node.lang}</Text>
          {showActions && (
            <View style={styles.codeActions}>
              {showCopy && (
                <TouchableOpacity
                  style={styles.codeAction}
                  onPress={handleCopyPress}
                  hitSlop={ACTION_HIT_SLOP}
                  accessibilityRole="button"
                  accessibilityLabel={copyLabel}
                >
                  <CodeActionIcon
                    id="copy"
                    size={styles.codeActionIcon.fontSize}
                    color={styles.codeActionIcon.color}
                  />
                </TouchableOpacity>
              )}
              {showFullscreen && (
                <TouchableOpacity
                  style={styles.codeAction}
                  onPress={handleFullscreenPress}
                  hitSlop={ACTION_HIT_SLOP}
                  accessibilityRole="button"
                  accessibilityLabel={fullscreenLabel}
                >
                  <CodeActionIcon
                    id="fullscreen"
                    size={styles.codeActionIcon.fontSize}
                    color={styles.codeActionIcon.color}
                  />
                </TouchableOpacity>
              )}
            </View>
          )}
        </View>
      )}
      <View style={styles.codeBlockBody}>{children}</View>
    </View>
  );
}
