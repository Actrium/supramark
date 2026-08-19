/**
 * Supramark RN style system
 *
 * This file defines the style types and default styles for the Supramark
 * React Native components. Users can override the default styles by passing
 * the styles prop.
 */

import { StyleSheet, type TextStyle, type ViewStyle } from 'react-native';

/**
 * Supramark's customizable style keys
 */
export interface SupramarkStyles {
  // Block elements
  paragraph?: TextStyle;
  h1?: TextStyle;
  h2?: TextStyle;
  h3?: TextStyle;
  h4?: TextStyle;
  h5?: TextStyle;
  h6?: TextStyle;

  // Code blocks
  codeBlock?: ViewStyle;
  code?: TextStyle;
  /** Wrapper around a code block header (lang + button) and the code body. */
  codeBlockContainer?: ViewStyle;
  /** Header row: language label on the left, copy button on the right. */
  codeBlockHeader?: ViewStyle;
  /** Language label in the header (the fenced info string). */
  codeBlockLang?: TextStyle;
  /** Copy button in the header (shown only when onCopyCode is provided). */
  codeButton?: ViewStyle;
  /** Label text inside the copy button. */
  codeButtonText?: TextStyle;

  // Lists
  list?: ViewStyle;
  listItem?: ViewStyle;
  listItemBlock?: ViewStyle;
  listItemIndent?: ViewStyle;
  bullet?: TextStyle;
  listItemText?: TextStyle;

  // Inline elements
  strong?: TextStyle;
  emphasis?: TextStyle;
  inlineCode?: TextStyle;
  link?: TextStyle;
  imageText?: TextStyle;
  delete?: TextStyle;

  // Tables
  table?: ViewStyle;
  tableRow?: ViewStyle;
  tableCell?: ViewStyle;
  tableHeaderCell?: ViewStyle;
  tableCellText?: TextStyle;
  tableHeaderText?: TextStyle;
  textCenter?: TextStyle;
  textRight?: TextStyle;

  // Blockquote & thematic break
  blockquote?: ViewStyle;
  thematicBreak?: ViewStyle;

  // Diagram
  diagramPlaceholder?: ViewStyle;
  diagramPlaceholderText?: TextStyle;

  // Map
  mapCard?: ViewStyle;
  mapCardHeader?: ViewStyle;
  mapCardTitle?: TextStyle;
  mapCardSubtitle?: TextStyle;
  mapCardContent?: ViewStyle;
  mapCardInfo?: TextStyle;
  mapContainer?: ViewStyle;
  map?: ViewStyle;
  mapGridOverlay?: ViewStyle;
  mapGridLine?: ViewStyle;
  mapGridLineVertical?: ViewStyle;
  mapCenterMarker?: ViewStyle;
  mapCenterMarkerText?: TextStyle;
  mapMarker?: ViewStyle;
  mapMarkerText?: TextStyle;
  mapOverlay?: ViewStyle;
  mapOverlayText?: TextStyle;

  // Container
  root?: ViewStyle;
}

/**
 * Default styles
 */
export const defaultStyles = StyleSheet.create({
  paragraph: {
    lineHeight: 20,
  },
  h1: {
    fontSize: 24,
    fontWeight: '700',
    marginTop: 8,
  },
  h2: {
    fontSize: 20,
    fontWeight: '600',
    marginTop: 6,
  },
  h3: {
    fontSize: 18,
    fontWeight: '600',
    marginTop: 4,
  },
  h4: {
    fontSize: 16,
    fontWeight: '500',
    marginTop: 2,
  },
  h5: {
    fontSize: 14,
    fontWeight: '500',
  },
  h6: {
    fontSize: 12,
    fontWeight: '500',
  },
  codeBlock: {
    backgroundColor: '#f5f5f5',
    padding: 8,
    borderRadius: 4,
  },
  code: {
    fontFamily: 'Menlo',
    fontSize: 12,
  },
  codeBlockContainer: {
    borderRadius: 4,
    overflow: 'hidden',
    marginBottom: 4,
  },
  codeBlockHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: 8,
    paddingVertical: 4,
    backgroundColor: 'rgba(0, 0, 0, 0.08)',
  },
  codeBlockLang: {
    fontSize: 12,
    color: 'rgba(0, 0, 0, 0.55)',
    fontFamily: 'Menlo',
  },
  codeButton: {
    backgroundColor: 'rgba(0, 0, 0, 0.5)',
    borderRadius: 4,
    paddingHorizontal: 8,
    paddingVertical: 4,
  },
  codeButtonText: {
    color: '#ffffff',
    fontSize: 12,
  },
  list: {
    gap: 4,
  },
  listItem: {
    flexDirection: 'row',
    alignItems: 'flex-start',
  },
  listItemBlock: {
    flexDirection: 'column',
  },
  listItemIndent: {
    paddingLeft: 16,
  },
  bullet: {
    marginRight: 6,
    lineHeight: 20,
  },
  listItemText: {
    flex: 1,
    lineHeight: 20,
  },
  diagramPlaceholder: {
    padding: 8,
    borderRadius: 4,
    borderWidth: 1,
    borderColor: '#ccc',
  },
  diagramPlaceholderText: {
    fontSize: 12,
    color: '#666',
  },
  mapCard: {
    backgroundColor: '#f8f9fa',
    borderWidth: 1,
    borderColor: '#dee2e6',
    borderRadius: 8,
    padding: 16,
  },
  mapCardHeader: {
    marginBottom: 12,
  },
  mapCardTitle: {
    fontSize: 16,
    fontWeight: '600',
    color: '#212529',
    marginBottom: 4,
  },
  mapCardSubtitle: {
    fontSize: 12,
    color: '#6c757d',
  },
  mapCardContent: {
    gap: 6,
  },
  mapCardInfo: {
    fontSize: 14,
    color: '#495057',
    lineHeight: 20,
  },
  mapContainer: {
    height: 200,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: '#e9ecef',
  },
  map: {
    height: 200,
    position: 'relative',
    backgroundColor: '#e8f4fd',
    borderRadius: 8,
    overflow: 'hidden',
  },
  mapGridOverlay: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
  },
  mapGridLine: {
    position: 'absolute',
    backgroundColor: '#d1e7dd',
    opacity: 0.3,
    height: 1,
    left: 0,
    right: 0,
  },
  mapGridLineVertical: {
    height: '100%',
    width: 1,
    top: 0,
    bottom: 0,
  },
  mapCenterMarker: {
    position: 'absolute',
    top: '50%',
    left: '50%',
    marginTop: -12,
    marginLeft: -12,
  },
  mapCenterMarkerText: {
    fontSize: 24,
  },
  mapMarker: {
    position: 'absolute',
    marginTop: -12,
    marginLeft: -12,
  },
  mapMarkerText: {
    fontSize: 20,
  },
  mapOverlay: {
    position: 'absolute',
    top: 8,
    right: 8,
    backgroundColor: 'rgba(0, 0, 0, 0.7)',
    borderRadius: 4,
    paddingHorizontal: 8,
    paddingVertical: 4,
  },
  mapOverlayText: {
    color: '#fff',
    fontSize: 12,
  },
  // Inline styles
  strong: {
    fontWeight: '700',
  },
  emphasis: {
    fontStyle: 'italic',
  },
  inlineCode: {
    fontFamily: 'Menlo',
    fontSize: 12,
    backgroundColor: '#f5f5f5',
    paddingHorizontal: 4,
    paddingVertical: 2,
    borderRadius: 2,
  },
  link: {
    color: '#0366d6',
    textDecorationLine: 'underline',
  },
  imageText: {
    color: '#666',
    fontStyle: 'italic',
  },
  delete: {
    textDecorationLine: 'line-through',
    textDecorationStyle: 'solid',
  },
  // Table styles
  table: {
    maxWidth: '100%',
    borderWidth: 1,
    borderColor: '#ddd',
  },
  blockquote: {
    borderLeftWidth: 3,
    borderLeftColor: '#d0d7de',
    paddingLeft: 12,
    paddingVertical: 2,
    marginVertical: 4,
  },
  thematicBreak: {
    height: 1,
    backgroundColor: '#d0d7de',
    marginVertical: 8,
  },
  tableRow: {
    flexDirection: 'row',
    borderBottomWidth: 1,
    borderBottomColor: '#ddd',
  },
  tableCell: {
    flex: 1,
    flexShrink: 1,
    padding: 8,
    borderRightWidth: 1,
    borderRightColor: '#ddd',
  },
  tableHeaderCell: {
    backgroundColor: '#f5f5f5',
  },
  tableCellText: {
    fontSize: 14,
  },
  tableHeaderText: {
    fontWeight: '600',
  },
  textCenter: {
    textAlign: 'center',
  },
  textRight: {
    textAlign: 'right',
  },
  root: {
    flexDirection: 'column',
    gap: 8,
  },
});

/**
 * Merges user styles with the default styles
 * @param customStyles user-supplied custom styles
 * @returns the merged styles
 */
export function mergeStyles(customStyles?: SupramarkStyles): typeof defaultStyles {
  if (!customStyles) {
    return defaultStyles;
  }

  // Create a new object to avoid mutating defaultStyles
  const merged: Record<string, TextStyle | ViewStyle> = {};

  // First copy all default styles
  Object.keys(defaultStyles).forEach(key => {
    merged[key] = defaultStyles[key as keyof typeof defaultStyles];
  });

  // Then merge in the user styles
  Object.keys(customStyles).forEach(key => {
    const customStyle = customStyles[key as keyof SupramarkStyles];
    if (customStyle) {
      const defaultStyle = merged[key] || {};
      merged[key] = { ...defaultStyle, ...customStyle };
    }
  });

  return merged as typeof defaultStyles;
}

/** The fully-merged style shape consumed by renderer components. */
export type MergedStyles = ReturnType<typeof mergeStyles>;

/**
 * Dark theme styles
 */
export const darkThemeStyles: SupramarkStyles = {
  paragraph: {
    color: '#e0e0e0',
  },
  h1: {
    color: '#ffffff',
  },
  h2: {
    color: '#ffffff',
  },
  h3: {
    color: '#ffffff',
  },
  h4: {
    color: '#ffffff',
  },
  h5: {
    color: '#ffffff',
  },
  h6: {
    color: '#ffffff',
  },
  code: {
    color: '#e0e0e0',
  },
  codeBlock: {
    backgroundColor: '#2d2d2d',
  },
  codeBlockHeader: {
    backgroundColor: 'rgba(255, 255, 255, 0.08)',
  },
  codeBlockLang: {
    color: 'rgba(255, 255, 255, 0.6)',
  },
  codeButton: {
    backgroundColor: 'rgba(255, 255, 255, 0.25)',
  },
  inlineCode: {
    backgroundColor: '#2d2d2d',
    color: '#e0e0e0',
  },
  link: {
    color: '#58a6ff',
  },
  imageText: {
    color: '#8b949e',
  },
  table: {
    borderColor: '#444',
  },
  tableRow: {
    borderBottomColor: '#444',
  },
  tableCell: {
    borderRightColor: '#444',
  },
  tableHeaderCell: {
    backgroundColor: '#2d2d2d',
  },
  tableCellText: {
    color: '#e0e0e0',
  },
  tableHeaderText: {
    color: '#ffffff',
  },
  diagramPlaceholder: {
    borderColor: '#444',
    backgroundColor: '#1a1a1a',
  },
  diagramPlaceholderText: {
    color: '#8b949e',
  },
  mapCard: {
    backgroundColor: '#21262d',
    borderColor: '#30363d',
  },
  mapCardTitle: {
    color: '#f0f6fc',
  },
  mapCardSubtitle: {
    color: '#8b949e',
  },
  mapCardInfo: {
    color: '#e6edf3',
  },
};

/**
 * Recommended canvas background colors that pair with the built-in theme
 * foreground colors.
 *
 * The component itself does not paint a background on root — the canvas is
 * provided by the host. To keep foreground text readable, the host's
 * rendering container should use a background color matching the chosen
 * theme's brightness (this constant may be used for that):
 *
 * - theme="dark"  → themeBackground.dark  (#0d1117)
 * - theme="light" → themeBackground.light (#ffffff)
 *
 * The host may also use its own canvas color, as long as its brightness matches the theme.
 */
export const themeBackground = {
  light: '#ffffff',
  dark: '#0d1117',
} as const;
