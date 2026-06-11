/**
 * Supramark RN 样式系统
 *
 * 此文件定义了 Supramark React Native 组件的样式类型和默认样式。
 * 用户可以通过传入 styles prop 来覆盖默认样式。
 */

import { StyleSheet, TextStyle, ViewStyle } from 'react-native';

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
  blockquote?: ViewStyle;
  thematicBreak?: ViewStyle;

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
  tableScrollContainer?: ViewStyle;
  tableScrollContent?: ViewStyle;
  tableContainer?: ViewStyle;
  tableTitleContainer?: ViewStyle;
  tableTitleText?: TextStyle;
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

  // Input blocks
  inputBlock?: ViewStyle;
  inputTitle?: TextStyle;

  // Error display
  errorContainer?: ViewStyle;
  errorBox?: ViewStyle;
  errorHeader?: ViewStyle;
  errorTitle?: TextStyle;
  errorBody?: ViewStyle;
  errorMessage?: TextStyle;
  errorSection?: ViewStyle;
  errorSectionTitle?: TextStyle;
  errorDetailsScroll?: ViewStyle;
  errorDetailsText?: TextStyle;
  errorStackScroll?: ViewStyle;
  errorStackText?: TextStyle;

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
 * 宿主可传入的正文样式 token，只描述内容文字，不暴露表格、分割线等结构样式 key。
 */
export interface SupramarkContentStyle {
  color?: TextStyle['color'];
  headingColor?: TextStyle['color'];
  fontFamily?: TextStyle['fontFamily'];
  fontSize?: TextStyle['fontSize'];
  lineHeight?: TextStyle['lineHeight'];
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
  blockquote: {
    marginBottom: 8,
    paddingLeft: 12,
    borderLeftWidth: 3,
    borderLeftColor: '#d0d7de',
  },
  thematicBreak: {
    marginTop: 16,
    marginBottom: 16,
    borderBottomWidth: 1,
    borderBottomColor: '#d0d7de',
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
  inputBlock: {
    marginBottom: 8,
    padding: 12,
    borderWidth: 1,
    borderColor: '#d0d7de',
    borderRadius: 6,
    backgroundColor: '#f6f8fa',
  },
  inputTitle: {
    fontWeight: '600',
    marginBottom: 4,
  },
  errorContainer: {
    padding: 12,
  },
  errorBox: {
    borderWidth: 1,
    borderColor: '#ffccc7',
    borderRadius: 4,
    overflow: 'hidden',
    backgroundColor: '#fff',
  },
  errorHeader: {
    padding: 12,
  },
  errorTitle: {
    color: '#fff',
    fontSize: 16,
    fontWeight: '600',
  },
  errorBody: {
    padding: 12,
    backgroundColor: '#fff2f0',
  },
  errorMessage: {
    fontSize: 14,
    color: '#262626',
    marginBottom: 8,
  },
  errorSection: {
    marginTop: 8,
    paddingTop: 8,
    borderTopWidth: 1,
    borderTopColor: '#ffccc7',
  },
  errorSectionTitle: {
    fontSize: 12,
    color: '#8c8c8c',
    marginBottom: 4,
  },
  errorDetailsScroll: {
    maxHeight: 60,
  },
  errorDetailsText: {
    fontSize: 12,
    color: '#595959',
    fontFamily: 'monospace',
  },
  errorStackScroll: {
    maxHeight: 100,
  },
  errorStackText: {
    fontSize: 10,
    color: '#8c8c8c',
    fontFamily: 'monospace',
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
  tableScrollContainer: {
    width: '100%',
  },
  tableScrollContent: {
    flexGrow: 1,
  },
  tableContainer: {
    width: '100%',
    marginTop: 8,
    marginBottom: 12,
    borderWidth: 1,
    borderColor: '#ddd',
    borderRadius: 8,
    overflow: 'hidden',
  },
  tableTitleContainer: {
    paddingHorizontal: 10,
    paddingVertical: 8,
    backgroundColor: '#f3f4f6',
  },
  tableTitleText: {
    fontSize: 13,
    fontWeight: '500',
    color: '#6b7280',
  },
  table: {},
  tableRow: {
    flexDirection: 'row',
    borderBottomWidth: 1,
    borderBottomColor: '#ddd',
  },
  tableCell: {
    padding: 8,
    borderRightWidth: 1,
    borderRightColor: '#ddd',
  },
  tableHeaderCell: {},
  tableCellText: {
    fontSize: 14,
    lineHeight: 22,
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
  const merged: Record<string, any> = {};

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
 * 按样式 key 合并多个样式层，保证后一个层只覆盖具体字段，不会整块抹掉前一个层。
 */
export function mergeStyleLayers(
  ...styleLayers: Array<SupramarkStyles | undefined>
): SupramarkStyles {
  const merged: Record<string, any> = {};

  styleLayers.forEach(styleLayer => {
    // 空样式层不参与合并。
    if (!styleLayer) {
      return;
    }

    Object.keys(styleLayer).forEach(key => {
      const layerStyle = styleLayer[key as keyof SupramarkStyles];

      // 当前样式 key 没有值时跳过，避免把前面的样式层清空。
      if (!layerStyle) {
        return;
      }

      const previousStyle = merged[key] || {};
      merged[key] = { ...previousStyle, ...layerStyle };
    });
  });

  return merged as SupramarkStyles;
}

/**
 * 将宿主传入的正文 token 转换为 Supramark 内部样式覆盖。
 */
export function createContentStyles(contentStyle?: SupramarkContentStyle): SupramarkStyles {
  // 没有正文 token 时不生成额外覆盖，保持默认主题样式。
  if (!contentStyle) {
    return {};
  }

  // 正文文本 token 会应用到段落、列表、代码文本和表格内容。
  const bodyText: TextStyle = {
    color: contentStyle.color,
    fontFamily: contentStyle.fontFamily,
    fontSize: contentStyle.fontSize,
    lineHeight: contentStyle.lineHeight,
  };

  // 标题只继承颜色和字体族，保留 Supramark 自己的标题层级字号。
  const headingText: TextStyle = {
    color: contentStyle.headingColor ?? contentStyle.color,
    fontFamily: contentStyle.fontFamily,
  };

  // 内联强调节点需要显式颜色，避免嵌套 Text 在深色模式下回退到系统默认黑色。
  const inlineText: TextStyle = {
    color: contentStyle.color,
    fontFamily: contentStyle.fontFamily,
  };

  return {
    paragraph: bodyText,
    h1: headingText,
    h2: headingText,
    h3: headingText,
    h4: headingText,
    h5: headingText,
    h6: headingText,
    code: bodyText,
    listItemText: bodyText,
    bullet: bodyText,
    strong: inlineText,
    emphasis: inlineText,
    inlineCode: bodyText,
    delete: inlineText,
    tableCellText: bodyText,
    tableHeaderText: headingText,
    inputTitle: headingText,
  };
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
  blockquote: {
    borderLeftColor: '#30363d',
  },
  thematicBreak: {
    borderBottomColor: '#21262d',
  },
  strong: {
    color: '#e6edf3',
  },
  emphasis: {
    color: '#e6edf3',
  },
  inlineCode: {
    backgroundColor: '#2d2d2d',
    color: '#e0e0e0',
  },
  bullet: {
    color: '#e0e0e0',
  },
  listItemText: {
    color: '#e0e0e0',
  },
  link: {
    color: '#58a6ff',
  },
  imageText: {
    color: '#8b949e',
  },
  delete: {
    color: '#e0e0e0',
  },
  inputBlock: {
    borderColor: '#30363d',
    backgroundColor: '#161b22',
  },
  inputTitle: {
    color: '#f0f6fc',
  },
  errorBox: {
    borderColor: '#6e3b2f',
    backgroundColor: '#161b22',
  },
  errorBody: {
    backgroundColor: '#2a1414',
  },
  errorMessage: {
    color: '#ffd8d3',
  },
  errorSection: {
    borderTopColor: '#6e3b2f',
  },
  errorSectionTitle: {
    color: '#f2a8a1',
  },
  errorDetailsText: {
    color: '#ffd8d3',
  },
  errorStackText: {
    color: '#f2a8a1',
  },
  table: {},
  tableContainer: {
    borderColor: '#30363d',
  },
  tableTitleContainer: {
    backgroundColor: '#161b22',
  },
  tableTitleText: {
    color: '#c9d1d9',
  },
  tableRow: {
    borderBottomColor: '#30363d',
  },
  tableCell: {
    borderRightColor: '#30363d',
  },
  tableHeaderCell: {
    backgroundColor: '#161b22',
  },
  tableCellText: {
    color: '#c9d1d9',
  },
  tableHeaderText: {
    color: '#f0f6fc',
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
 * Light 主题样式（默认主题的别名）
 */
export const lightThemeStyles: SupramarkStyles = {
  // Light 主题使用默认样式，根容器背景由宿主控制。
};
