/**
 * Supramark RN 样式系统
 *
 * 此文件定义了 Supramark React Native 组件的样式类型和默认样式。
 * 用户可以通过传入 styles prop 来覆盖默认样式。
 */

import { StyleSheet, type TextStyle, type ViewStyle } from 'react-native';

/**
 * Supramark 可自定义的样式键
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

  // Lists
  list?: ViewStyle;
  listItem?: ViewStyle;
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
 * 默认样式
 */
export const defaultStyles = StyleSheet.create({
  paragraph: {
    marginBottom: 8,
    lineHeight: 20,
  },
  h1: {
    fontSize: 24,
    fontWeight: '700',
    marginBottom: 12,
  },
  h2: {
    fontSize: 20,
    fontWeight: '600',
    marginBottom: 10,
  },
  h3: {
    fontSize: 18,
    fontWeight: '600',
    marginBottom: 8,
  },
  h4: {
    fontSize: 16,
    fontWeight: '500',
    marginBottom: 6,
  },
  h5: {
    fontSize: 14,
    fontWeight: '500',
    marginBottom: 4,
  },
  h6: {
    fontSize: 12,
    fontWeight: '500',
    marginBottom: 4,
  },
  codeBlock: {
    backgroundColor: '#f5f5f5',
    padding: 8,
    borderRadius: 4,
    marginBottom: 8,
  },
  code: {
    fontFamily: 'Menlo',
    fontSize: 12,
  },
  list: {
    marginBottom: 8,
  },
  listItem: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    marginBottom: 4,
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
    marginBottom: 8,
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
    marginBottom: 12,
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
    borderWidth: 1,
    borderColor: '#ddd',
    marginBottom: 12,
  },
  tableRow: {
    flexDirection: 'row',
    borderBottomWidth: 1,
    borderBottomColor: '#ddd',
  },
  tableCell: {
    flex: 1,
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
    // 默认无样式，用户可自定义
  },
});

/**
 * 合并用户样式和默认样式
 * @param customStyles 用户自定义样式
 * @returns 合并后的样式
 */
export function mergeStyles(customStyles?: SupramarkStyles): typeof defaultStyles {
  if (!customStyles) {
    return defaultStyles;
  }

  // 创建一个新对象,避免修改defaultStyles
  const merged: Record<string, TextStyle | ViewStyle> = {};

  // 先复制所有默认样式
  Object.keys(defaultStyles).forEach(key => {
    merged[key] = defaultStyles[key as keyof typeof defaultStyles];
  });

  // 然后合并用户样式
  Object.keys(customStyles).forEach(key => {
    const customStyle = customStyles[key as keyof SupramarkStyles];
    if (customStyle) {
      const defaultStyle = merged[key] || {};
      merged[key] = { ...defaultStyle, ...customStyle };
    }
  });

  return merged as typeof defaultStyles;
}

/**
 * Dark 主题样式
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
  root: {
    backgroundColor: '#0d1117',
  },
};

/**
 * Light 主题样式（默认主题的别名）
 */
export const lightThemeStyles: SupramarkStyles = {
  // Light 主题使用默认样式，这里可以做一些微调
  root: {
    backgroundColor: '#ffffff',
  },
};
