import React, { useEffect, useState, useMemo, ReactNode } from 'react';
import { Text, View, Linking, TouchableOpacity, Dimensions, ScrollView } from 'react-native';
import type {
  SupramarkRootNode,
  SupramarkNode,
  SupramarkParagraphNode,
  SupramarkHeadingNode,
  SupramarkBlockquoteNode,
  SupramarkCodeNode,
  SupramarkMathBlockNode,
  SupramarkInlineCodeNode,
  SupramarkListNode,
  SupramarkListItemNode,
  SupramarkDiagramNode,
  SupramarkContainerNode,
  SupramarkInputNode,
  SupramarkTextNode,
  SupramarkStrongNode,
  SupramarkEmphasisNode,
  SupramarkLinkNode,
  SupramarkImageNode,
  SupramarkBreakNode,
  SupramarkDeleteNode,
  SupramarkTableNode,
  SupramarkTableRowNode,
  SupramarkTableCellNode,
  SupramarkMathInlineNode,
  SupramarkFootnoteReferenceNode,
  SupramarkFootnoteDefinitionNode,
  SupramarkDefinitionListNode,
  SupramarkDefinitionItemNode,
  SupramarkConfig,
} from '@supramark/core';
import {
  parseMarkdown,
  isFeatureEnabled,
  warnIfUnknownDiagramEngine,
  getFeatureOptionsAs,
  SUPRAMARK_ADMONITION_KINDS,
} from '@supramark/core';
import { DiagramNode } from './DiagramNode';
import { MathBlock, MathInline } from './math';
import {
  type SupramarkContentStyle,
  type SupramarkStyles,
  createContentStyles,
  defaultStyles,
  mergeStyleLayers,
  mergeStyles,
  darkThemeStyles,
  lightThemeStyles,
} from './styles';
import { ErrorBoundary, ErrorInfo, ErrorDisplay } from './ErrorBoundary';

// RN 渲染层当前只需要区分浅色和深色两种主题。
type SupramarkThemeName = 'light' | 'dark';

export interface ContainerRendererRN {
  (args: {
    node: any;
    key: number;
    styles: ReturnType<typeof mergeStyles>;
    theme: SupramarkThemeName;
    config?: SupramarkConfig;
    onOpenHtmlPage?: (node: SupramarkContainerNode) => void;
    renderNode: (node: SupramarkNode, key: number) => React.ReactNode;
    renderChildren: (children: SupramarkNode[]) => React.ReactNode;
  }): React.ReactNode;
}

export interface SupramarkProps {
  /** Markdown 源文本 */
  markdown: string;
  /** 预解析的 AST（优先级高于 markdown） */
  ast?: SupramarkRootNode;
  /** 自定义样式（覆盖默认样式） */
  styles?: SupramarkStyles;
  /** 宿主正文样式 token，用于统一正文文字颜色、字号和行高 */
  contentStyle?: SupramarkContentStyle;
  /** 主题：'light' | 'dark' | 自定义样式对象 */
  theme?: 'light' | 'dark' | SupramarkStyles;
  /** Feature 配置（用于按需启用/禁用图表等扩展能力） */
  config?: SupramarkConfig;
  /** 错误回调（可选） */
  onError?: (error: Error, errorInfo?: React.ErrorInfo) => void;
  /** 自定义错误展示组件（可选） */
  errorFallback?: (error: ErrorInfo) => ReactNode;

  /**
   * Container 扩展渲染器注册表：node.type === 'container' 时按 node.name 委派。
   */
  containerRenderers?: Record<string, ContainerRendererRN>;

  /**
   * 当用户点击 HTML Page 卡片时的回调。
   *
   * - node.data.html 为完整 HTML 内容；
   * - 宿主可以在回调中打开新的页面 / Modal / WebView。
   */
  onOpenHtmlPage?: (node: SupramarkContainerNode) => void;
}

// 表格列宽策略：每列至少 120，避免内容过早折行。
const TABLE_COLUMN_MIN_WIDTH = 120;

// 表格列宽策略：每列最多 360，避免单列吞掉过多横向空间。
const TABLE_COLUMN_MAX_WIDTH = 360;

// 估算字符宽度，用于在不做原生测量的前提下近似计算列宽。
const TABLE_COLUMN_CHAR_WIDTH = 8;

// 单元格左右内边距总和，用于把文本宽度换算成最终列宽。
const TABLE_COLUMN_HORIZONTAL_PADDING = 16;

// 最后一个块节点不保留段后距，避免宿主容器底部多出额外空白。
const LAST_BLOCK_STYLE = { marginBottom: 0 } as const;

// 定义列表条目默认仍保留条目间距，最后一项再由 LAST_BLOCK_STYLE 清掉。
const DEFINITION_ITEM_STYLE = { marginBottom: 8 } as const;

export const Supramark: React.FC<SupramarkProps> = ({
  markdown,
  ast,
  styles: customStyles,
  contentStyle,
  theme,
  config,
  onError,
  errorFallback,
  onOpenHtmlPage,
  containerRenderers,
}) => {
  const [root, setRoot] = useState<SupramarkRootNode | null>(ast ?? null);
  const [parseError, setParseError] = useState<ErrorInfo | null>(null);

  // 合并样式：theme -> contentStyle -> customStyles -> defaultStyles
  const mergedStyles = useMemo(() => {
    let themeStyles: SupramarkStyles | undefined;

    if (typeof theme === 'string') {
      themeStyles = theme === 'dark' ? darkThemeStyles : lightThemeStyles;
    } else if (theme) {
      themeStyles = theme;
    }

    // contentStyle 是常规宿主入口，customStyles 作为逃生口保留最高优先级。
    const contentStyles = createContentStyles(contentStyle);
    const finalCustomStyles = mergeStyleLayers(themeStyles, contentStyles, customStyles);

    return mergeStyles(finalCustomStyles);
  }, [contentStyle, customStyles, theme]);

  // 传给 RN container runtime 的主题名只表达明暗模式，不影响自定义 styles 的合并优先级。
  const resolvedThemeName: SupramarkThemeName = theme === 'dark' ? 'dark' : 'light';

  useEffect(() => {
    if (ast) {
      setRoot(ast);
      setParseError(null);
      return;
    }

    let cancelled = false;
    (async () => {
      try {
        const parsed = await parseMarkdown(markdown, { config });
        if (!cancelled) {
          setRoot(parsed);
          setParseError(null);
        }
      } catch (error) {
        if (!cancelled) {
          const err = error as Error;
          const errorInfo: ErrorInfo = {
            type: 'parse',
            message: err.message || '解析 Markdown 失败',
            details: err.toString(),
            stack: err.stack,
          };
          setParseError(errorInfo);
          setRoot(null);

          // 调用错误回调
          if (onError) {
            onError(err);
          }
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [markdown, ast, onError]);

  const mergedContainerRenderers = useMemo(
    () => ({ ...(containerRenderers ?? {}) }),
    [containerRenderers]
  );

  // 解析错误降级：显示错误信息或原始 markdown
  if (parseError) {
    if (errorFallback) {
      return <>{errorFallback(parseError)}</>;
    }
    return (
      <View>
        <ErrorDisplay error={parseError} styles={mergedStyles} />
        <View style={mergedStyles.codeBlock}>
          <Text style={mergedStyles.code}>{markdown}</Text>
        </View>
      </View>
    );
  }

  if (!root) {
    // 解析中时的简单回退：直接显示原始 markdown 文本。
    return <Text>{markdown}</Text>;
  }

  return (
    <ErrorBoundary onError={onError} fallback={errorFallback} styles={mergedStyles}>
      <View style={mergedStyles.root}>
        {/* 根节点最后一个块不保留段后距，避免宿主容器底部多出空白。 */}
        {root.children.map((node, index) =>
          renderNode(
            node,
            index,
            mergedStyles,
            config,
            onOpenHtmlPage,
            mergedContainerRenderers,
            resolvedThemeName,
            index === root.children.length - 1
          )
        )}
      </View>
    </ErrorBoundary>
  );
};

/**
 * 渲染块级节点，isLastBlock 用来清掉当前容器内最后一个块节点的尾部间距。
 */
function renderNode(
  node: SupramarkNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  config?: SupramarkConfig,
  onOpenHtmlPage?: (node: SupramarkContainerNode) => void,
  containerRenderers?: Record<string, ContainerRendererRN>,
  themeName: SupramarkThemeName = 'light',
  isLastBlock = false
): React.ReactNode {
  switch (node.type) {
    case 'paragraph':
      return (
        <Text key={key} style={[styles.paragraph, isLastBlock && LAST_BLOCK_STYLE]}>
          {renderInlineNodes(node.children, styles, config)}
        </Text>
      );
    case 'heading': {
      const heading = node as SupramarkHeadingNode;
      return (
        <Text
          key={key}
          style={[headingStyle(heading.depth, styles), isLastBlock && LAST_BLOCK_STYLE]}
        >
          {renderInlineNodes(heading.children, styles, config)}
        </Text>
      );
    }
    case 'code': {
      const codeBlock = node as SupramarkCodeNode;
      return (
        <View key={key} style={[styles.codeBlock, isLastBlock && LAST_BLOCK_STYLE]}>
          <Text style={styles.code}>{codeBlock.value}</Text>
        </View>
      );
    }
    case 'blockquote': {
      const blockquote = node as SupramarkBlockquoteNode;
      // 引用块需要显式渲染为带左边框的容器，避免 RN 端把已解析的内容静默丢弃。
      return (
        <View key={key} style={[styles.blockquote, isLastBlock && LAST_BLOCK_STYLE]}>
          {blockquote.children.map((child, index) =>
            renderNode(
              child,
              index,
              styles,
              config,
              onOpenHtmlPage,
              containerRenderers,
              themeName,
              index === blockquote.children.length - 1
            )
          )}
        </View>
      );
    }
    case 'thematic_break': {
      // 分隔线节点需要落地成一条横线，避免 `---` / `***` 在 RN 中无输出。
      return <View key={key} style={[styles.thematicBreak, isLastBlock && LAST_BLOCK_STYLE]} />;
    }
    case 'math_block': {
      const mathBlock = node as SupramarkMathBlockNode;
      // 如果禁用了 Math Feature，则降级为普通代码块展示原始 TeX
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-math'])) {
        return renderDisabledMathBlock(mathBlock, key, styles, isLastBlock);
      }
      return <MathBlock key={key} node={mathBlock} />;
    }
    case 'list': {
      const list = node as SupramarkListNode;
      const start = typeof list.start === 'number' ? list.start : 1;
      const isOrdered = list.ordered === true;

      return (
        <View key={key} style={[styles.list, isLastBlock && LAST_BLOCK_STYLE]}>
          {list.children.map((item, index) => {
            if (item.type !== 'list_item') {
              return renderNode(
                item,
                index,
                styles,
                config,
                onOpenHtmlPage,
                containerRenderers,
                themeName,
                index === list.children.length - 1
              );
            }

            const listItem = item as SupramarkListItemNode;
            const isTaskList = listItem.checked !== undefined;
            const marker = isTaskList
              ? listItem.checked === true
                ? '☑'
                : '☐'
              : isOrdered
                ? `${start + index}.`
                : '•';

            return (
              <View
                key={index}
                style={[styles.listItem, index === list.children.length - 1 && LAST_BLOCK_STYLE]}
              >
                <Text style={styles.bullet}>{marker}</Text>
                <View style={{ flex: 1, minWidth: 0 }}>
                  {listItem.children.map((child, childIndex) =>
                    renderNode(
                      child,
                      childIndex,
                      styles,
                      config,
                      onOpenHtmlPage,
                      containerRenderers,
                      themeName,
                      childIndex === listItem.children.length - 1
                    )
                  )}
                </View>
              </View>
            );
          })}
        </View>
      );
    }
    case 'list_item': {
      const item = node as SupramarkListItemNode;
      const isTaskList = item.checked !== undefined;
      const marker = isTaskList ? (item.checked === true ? '☑' : '☐') : '•';

      return (
        <View key={key} style={[styles.listItem, isLastBlock && LAST_BLOCK_STYLE]}>
          <Text style={styles.bullet}>{marker}</Text>
          <View style={{ flex: 1, minWidth: 0 }}>
            {item.children.map((child, index) =>
              renderNode(
                child,
                index,
                styles,
                config,
                onOpenHtmlPage,
                containerRenderers,
                themeName,
                index === item.children.length - 1
              )
            )}
          </View>
        </View>
      );
    }
    case 'diagram': {
      const diagram = node as SupramarkDiagramNode;
      // 如果配置中显式禁用了对应图表 Feature，则降级为代码块渲染
      if (!isDiagramFeatureEnabled(config, diagram.engine)) {
        return renderDisabledDiagram(diagram, key, styles, isLastBlock);
      }
      return <DiagramNode key={key} node={diagram} diagramConfig={config?.diagram} />;
    }
    case 'container': {
      const container = node as SupramarkContainerNode;
      const containerName = container.name;

      // 检查是否有注册的自定义渲染器
      if (containerRenderers && containerRenderers[containerName]) {
        return containerRenderers[containerName]({
          node: container,
          key,
          styles,
          theme: themeName,
          config,
          onOpenHtmlPage,
          renderNode: (n, k) =>
            renderNode(n, k, styles, config, onOpenHtmlPage, containerRenderers, themeName),
          renderChildren: children =>
            children.map((child, index) =>
              renderNode(
                child,
                index,
                styles,
                config,
                onOpenHtmlPage,
                containerRenderers,
                themeName,
                index === children.length - 1
              )
            ),
        });
      }

      // 内置处理：map 类型
      if (containerName === 'map') {
        return renderMapNodeFromContainer(container, key, styles, config);
      }

      // 内置处理：html 类型
      if (containerName === 'html') {
        const data = container.data || {};
        const title = (data.title as string) || container.params || '[HTML 页面]';
        const content = (
          <View style={[styles.listItem, isLastBlock && LAST_BLOCK_STYLE]}>
            <Text style={[styles.listItemText, { fontWeight: '600' }]}>{title}</Text>
            <Text style={styles.listItemText}>
              点击卡片以在独立容器中打开 HTML 页面（需要宿主实现 onOpenHtmlPage 回调）。
            </Text>
          </View>
        );

        if (!onOpenHtmlPage) {
          return <View key={key}>{content}</View>;
        }

        return (
          <TouchableOpacity key={key} activeOpacity={0.8} onPress={() => onOpenHtmlPage(container)}>
            {content}
          </TouchableOpacity>
        );
      }

      // 内置处理：admonition 类型 (note, tip, warning, etc.)
      if (SUPRAMARK_ADMONITION_KINDS.includes(containerName as any)) {
        const title = container.params || (container.data?.title as string | undefined);
        const kind = containerName;

        if (!isFeatureGroupEnabled(config, ['@supramark/feature-admonition'])) {
          return (
            <View key={key} style={[styles.listItem, isLastBlock && LAST_BLOCK_STYLE]}>
              {title ? <Text style={styles.listItemText}>{title}</Text> : null}
              <Text style={styles.listItemText}>
                {renderInlineNodes(container.children, styles, config)}
              </Text>
            </View>
          );
        }

        const adOptions =
          getFeatureOptionsAs<{ kinds?: string[] }>(config, '@supramark/feature-admonition') ?? {};
        if (Array.isArray(adOptions.kinds) && adOptions.kinds.length > 0) {
          if (!adOptions.kinds.includes(kind)) {
            return (
              <View key={key} style={[styles.listItem, isLastBlock && LAST_BLOCK_STYLE]}>
                {title ? <Text style={styles.listItemText}>{title}</Text> : null}
                <Text style={styles.listItemText}>
                  {renderInlineNodes(container.children, styles, config)}
                </Text>
              </View>
            );
          }
        }

        return (
          <View key={key} style={[styles.listItem, isLastBlock && LAST_BLOCK_STYLE]}>
            {title ? (
              <Text style={[styles.listItemText, { fontWeight: '600' }]}>{title}</Text>
            ) : null}
            <Text style={styles.listItemText}>
              {renderInlineNodes(container.children, styles, config)}
            </Text>
          </View>
        );
      }

      // 默认：渲染为通用容器块
      return (
        <View key={key} style={[styles.listItem, isLastBlock && LAST_BLOCK_STYLE]}>
          {container.params && (
            <Text style={[styles.listItemText, { fontWeight: '600' }]}>
              {container.name}: {container.params}
            </Text>
          )}
          {container.children.map((child, index) =>
            renderNode(
              child,
              index,
              styles,
              config,
              onOpenHtmlPage,
              containerRenderers,
              themeName,
              index === container.children.length - 1
            )
          )}
        </View>
      );
    }
    case 'input': {
      const input = node as SupramarkInputNode;
      // 通用 input 块先渲染为信息卡片，保证 `%%%...%%%` 在 RN 端至少可见。
      const dataText =
        input.data && Object.keys(input.data).length > 0 ? JSON.stringify(input.data, null, 2) : '';

      return (
        <View key={key} style={[styles.inputBlock, isLastBlock && LAST_BLOCK_STYLE]}>
          <Text style={[styles.listItemText, styles.inputTitle]}>
            %%%{input.name}
            {input.params ? ` ${input.params}` : ''}
          </Text>
          {dataText ? <Text style={styles.code}>{dataText}</Text> : null}
          {input.children.map((child, index) =>
            renderNode(
              child,
              index,
              styles,
              config,
              onOpenHtmlPage,
              containerRenderers,
              themeName,
              index === input.children.length - 1
            )
          )}
        </View>
      );
    }
    case 'definition_list': {
      const list = node as SupramarkDefinitionListNode;
      const defOptions =
        getFeatureOptionsAs<{ compact?: boolean }>(config, '@supramark/feature-definition-list') ??
        {};
      const isCompact = defOptions.compact !== false; // 默认紧凑
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-definition-list'])) {
        // 禁用时，将定义列表退化为普通列表样式
        return (
          <View key={key} style={[styles.list, isLastBlock && LAST_BLOCK_STYLE]}>
            {list.children.map((item, index) => {
              const defItem = item as SupramarkDefinitionItemNode;
              return (
                <View
                  key={index}
                  style={[
                    DEFINITION_ITEM_STYLE,
                    index === list.children.length - 1 && LAST_BLOCK_STYLE,
                  ]}
                >
                  <Text style={[styles.listItemText, { fontWeight: '600' }]}>
                    {renderInlineNodes(defItem.term, styles, config)}
                  </Text>
                  {defItem.descriptions.map((descNodes, idx) => (
                    <Text key={idx} style={styles.listItemText}>
                      {renderInlineNodes(descNodes, styles, config)}
                    </Text>
                  ))}
                </View>
              );
            })}
          </View>
        );
      }
      return (
        <View key={key} style={[styles.list, isLastBlock && LAST_BLOCK_STYLE]}>
          {list.children.map((item, index) => {
            const defItem = item as SupramarkDefinitionItemNode;
            return (
              <View
                key={index}
                style={[
                  DEFINITION_ITEM_STYLE,
                  index === list.children.length - 1 && LAST_BLOCK_STYLE,
                ]}
              >
                <Text style={[styles.listItemText, { fontWeight: '600' }]}>
                  {renderInlineNodes(defItem.term, styles, config)}
                </Text>
                {defItem.descriptions.map((descNodes, idx) => (
                  <Text key={idx} style={styles.listItemText}>
                    {renderInlineNodes(descNodes, styles, config)}
                    {isCompact ? '' : '\n'}
                  </Text>
                ))}
              </View>
            );
          })}
        </View>
      );
    }
    case 'footnote_definition': {
      const def = node as SupramarkFootnoteDefinitionNode;
      // 第一阶段：简单以「[n] 内容」形式追加在文末
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-footnote'])) {
        // 禁用脚注 Feature 时，直接渲染为普通段落
        return (
          <View key={key} style={[styles.listItem, isLastBlock && LAST_BLOCK_STYLE]}>
            <View style={{ flex: 1, minWidth: 0 }}>
              {def.children.map((child, index) =>
                renderNode(
                  child,
                  index,
                  styles,
                  config,
                  onOpenHtmlPage,
                  containerRenderers,
                  themeName,
                  index === def.children.length - 1
                )
              )}
            </View>
          </View>
        );
      }
      return (
        <View key={key} style={[styles.listItem, isLastBlock && LAST_BLOCK_STYLE]}>
          <Text style={styles.bullet}>[{def.index}]</Text>
          <View style={{ flex: 1, minWidth: 0 }}>
            {def.children.map((child, index) =>
              renderNode(
                child,
                index,
                styles,
                config,
                onOpenHtmlPage,
                containerRenderers,
                themeName,
                index === def.children.length - 1
              )
            )}
          </View>
        </View>
      );
    }
    case 'table': {
      const table = node as SupramarkTableNode;
      return renderTableNode(table, key, styles, config, isLastBlock);
    }
    case 'table_row': {
      const row = node as SupramarkTableRowNode;
      return (
        <View key={key} style={styles.tableRow}>
          {row.children.map((cell, index) =>
            renderNode(cell, index, styles, config, onOpenHtmlPage, containerRenderers, themeName)
          )}
        </View>
      );
    }
    case 'table_cell': {
      const cell = node as SupramarkTableCellNode;
      const cellStyle = [styles.tableCell, cell.header && styles.tableHeaderCell];
      const textStyle = [
        styles.tableCellText,
        cell.header && styles.tableHeaderText,
        cell.align === 'center' && styles.textCenter,
        cell.align === 'right' && styles.textRight,
      ];

      return (
        <View key={key} style={cellStyle}>
          <Text style={textStyle}>{renderInlineNodes(cell.children, styles)}</Text>
        </View>
      );
    }
    case 'text':
      return (
        <Text key={key} style={[styles.paragraph, isLastBlock && LAST_BLOCK_STYLE]}>
          {(node as SupramarkTextNode).value}
        </Text>
      );
    default:
      return null;
  }
}

function renderInlineNodes(
  nodes: SupramarkNode[],
  styles: ReturnType<typeof mergeStyles>,
  config?: SupramarkConfig
): React.ReactNode {
  return nodes.map((node, index) => renderInlineNode(node, index, styles, config));
}

function renderInlineNode(
  node: SupramarkNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  config?: SupramarkConfig
): React.ReactNode {
  switch (node.type) {
    case 'text': {
      const textNode = node as SupramarkTextNode;
      return textNode.value;
    }
    case 'strong': {
      const strongNode = node as SupramarkStrongNode;
      return (
        <Text key={key} style={styles.strong}>
          {renderInlineNodes(strongNode.children, styles)}
        </Text>
      );
    }
    case 'emphasis': {
      const emphasisNode = node as SupramarkEmphasisNode;
      return (
        <Text key={key} style={styles.emphasis}>
          {renderInlineNodes(emphasisNode.children, styles)}
        </Text>
      );
    }
    case 'inline_code': {
      const codeNode = node as SupramarkInlineCodeNode;
      return (
        <Text key={key} style={styles.inlineCode}>
          {codeNode.value}
        </Text>
      );
    }
    case 'math_inline': {
      const mathNode = node as SupramarkMathInlineNode;
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-math'])) {
        return mathNode.value;
      }
      return <MathInline key={key} value={mathNode.value} textStyle={styles.paragraph} />;
    }
    case 'link': {
      const linkNode = node as SupramarkLinkNode;
      return (
        <Text
          key={key}
          style={styles.link}
          onPress={() => {
            Linking.openURL(linkNode.url).catch(err => console.error('Failed to open URL:', err));
          }}
        >
          {renderInlineNodes(linkNode.children, styles)}
        </Text>
      );
    }
    case 'image': {
      const imageNode = node as SupramarkImageNode;
      // RN 中暂时用文本展示图片（未来可以用 Image 组件）
      return (
        <Text key={key} style={styles.imageText}>
          [Image: {imageNode.alt || imageNode.url}]
        </Text>
      );
    }
    case 'break': {
      return '\n';
    }
    case 'delete': {
      const deleteNode = node as SupramarkDeleteNode;
      return (
        <Text key={key} style={styles.delete}>
          {renderInlineNodes(deleteNode.children, styles, config)}
        </Text>
      );
    }
    case 'footnote_reference': {
      const ref = node as SupramarkFootnoteReferenceNode;
      const label = ref.index;
      if (!isFeatureGroupEnabled(undefined, ['@supramark/feature-footnote'])) {
        return `[${label}]`;
      }
      return (
        <Text key={key} style={styles.inlineCode}>
          [{label}]
        </Text>
      );
    }
    default:
      return null;
  }
}

function headingStyle(
  depth: SupramarkHeadingNode['depth'],
  styles: ReturnType<typeof mergeStyles>
) {
  switch (depth) {
    case 1:
      return styles.h1;
    case 2:
      return styles.h2;
    case 3:
      return styles.h3;
    case 4:
      return styles.h4;
    case 5:
      return styles.h5;
    case 6:
      return styles.h6;
    default:
      return styles.h4;
  }
}

/**
 * RN 端表格使用专用渲染，按列估算宽度并在超出时启用横向滚动。
 */
function renderTableNode(
  table: SupramarkTableNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  config?: SupramarkConfig,
  isLastBlock = false
): React.ReactNode {
  // 先根据整张表的文本内容估算每一列宽度，避免继续走均分列宽策略。
  const columnWidths = estimateTableColumnWidths(table);
  const totalWidth = columnWidths.reduce((sum, width) => sum + width, 0);

  return (
    <View key={key} style={[styles.tableContainer, isLastBlock && LAST_BLOCK_STYLE]}>
      <View style={styles.tableTitleContainer}>
        <Text style={styles.tableTitleText}>表格</Text>
      </View>
      <ScrollView
        horizontal
        nestedScrollEnabled
        directionalLockEnabled
        showsHorizontalScrollIndicator
        style={styles.tableScrollContainer}
        contentContainerStyle={styles.tableScrollContent}
      >
        <View style={[styles.table, { width: totalWidth }]}>
          {(table.children as SupramarkTableRowNode[]).map((row, rowIndex) =>
            renderTableRowNode(
              row,
              rowIndex,
              columnWidths,
              styles,
              config,
              rowIndex === table.children.length - 1
            )
          )}
        </View>
      </ScrollView>
    </View>
  );
}

/**
 * 表格行使用统一的列宽数组，保证同一列在所有行里宽度一致。
 */
function renderTableRowNode(
  row: SupramarkTableRowNode,
  key: number,
  columnWidths: number[],
  styles: ReturnType<typeof mergeStyles>,
  config?: SupramarkConfig,
  isLastRow?: boolean
): React.ReactNode {
  // 最后一行不再绘制底部分隔线，避免形成表格主体的外边框。
  const rowStyle = [styles.tableRow, isLastRow && { borderBottomWidth: 0 }];

  return (
    <View key={key} style={rowStyle}>
      {columnWidths.map((columnWidth, columnIndex) => {
        const cell = row.children[columnIndex] as SupramarkTableCellNode | undefined;

        // 缺失单元格时补空白列，保证整行列数和表格宽度保持一致。
        if (!cell) {
          return (
            <View
              key={columnIndex}
              style={[
                styles.tableCell,
                createTableCellWidthStyle(columnWidth),
                // 最后一列不再绘制右侧分隔线，避免形成表格主体的外边框。
                columnIndex === columnWidths.length - 1 && { borderRightWidth: 0 },
              ]}
            />
          );
        }

        return renderTableCellNode(
          cell,
          columnIndex,
          columnWidth,
          styles,
          config,
          columnIndex === columnWidths.length - 1
        );
      })}
    </View>
  );
}

/**
 * 单元格宽度固定为列宽，文本在列宽内换行而不是继续压缩整列。
 */
function renderTableCellNode(
  cell: SupramarkTableCellNode,
  key: number,
  columnWidth: number,
  styles: ReturnType<typeof mergeStyles>,
  config?: SupramarkConfig,
  isLastColumn?: boolean
): React.ReactNode {
  const cellStyle = [
    styles.tableCell,
    createTableCellWidthStyle(columnWidth),
    isLastColumn && { borderRightWidth: 0 },
    cell.header && styles.tableHeaderCell,
  ];
  const textStyle = [
    styles.tableCellText,
    cell.header && styles.tableHeaderText,
    cell.align === 'center' && styles.textCenter,
    cell.align === 'right' && styles.textRight,
  ];

  return (
    <View key={key} style={cellStyle}>
      <Text style={textStyle}>{renderInlineNodes(cell.children, styles, config)}</Text>
    </View>
  );
}

/**
 * 列宽固定后显式关闭 flex 均分，避免默认 `flex: 1` 再次把列压回平均分配。
 */
function createTableCellWidthStyle(columnWidth: number) {
  return {
    width: columnWidth,
    minWidth: columnWidth,
    maxWidth: columnWidth,
    flexBasis: columnWidth,
    flexGrow: 0,
    flexShrink: 0,
  } as const;
}

/**
 * 通过扫描所有行的文本内容估算每列宽度，并限制在固定最小/最大范围内。
 */
function estimateTableColumnWidths(table: SupramarkTableNode): number[] {
  const rows = table.children as SupramarkTableRowNode[];
  const columnCount = Math.max(0, ...rows.map(row => row.children.length));

  return Array.from({ length: columnCount }, (_, columnIndex) => {
    const longestCellTextLength = rows.reduce((maxLength, row) => {
      const cell = row.children[columnIndex] as SupramarkTableCellNode | undefined;

      // 当前行没有这一列时跳过，避免把空单元格也算进宽度估算。
      if (!cell) {
        return maxLength;
      }

      const cellTextLength = extractPlainTextFromNodes(cell.children).trim().length;
      return Math.max(maxLength, cellTextLength);
    }, 0);

    // 空列仍然保留最小宽度，避免边框和布局塌掉。
    if (longestCellTextLength === 0) {
      return TABLE_COLUMN_MIN_WIDTH;
    }

    const estimatedWidth =
      longestCellTextLength * TABLE_COLUMN_CHAR_WIDTH + TABLE_COLUMN_HORIZONTAL_PADDING;
    return clamp(estimatedWidth, TABLE_COLUMN_MIN_WIDTH, TABLE_COLUMN_MAX_WIDTH);
  });
}

/**
 * 把 inline 节点树拍平成纯文本，用于列宽估算。
 */
function extractPlainTextFromNodes(nodes: SupramarkNode[]): string {
  return nodes.map(node => extractPlainTextFromNode(node)).join('');
}

/**
 * 针对常见 inline 节点提取文本内容，保证粗体、链接、删除线等也能参与列宽估算。
 */
function extractPlainTextFromNode(node: SupramarkNode): string {
  switch (node.type) {
    case 'text':
      return (node as SupramarkTextNode).value;
    case 'strong':
    case 'emphasis':
    case 'link':
    case 'delete':
      return extractPlainTextFromNodes(
        (
          node as
            | SupramarkStrongNode
            | SupramarkEmphasisNode
            | SupramarkLinkNode
            | SupramarkDeleteNode
        ).children
      );
    case 'inline_code':
    case 'math_inline':
      return (node as SupramarkInlineCodeNode | SupramarkMathInlineNode).value;
    case 'image': {
      const imageNode = node as SupramarkImageNode;
      return imageNode.alt || imageNode.url;
    }
    case 'break':
      return ' ';
    default:
      return '';
  }
}

/**
 * 统一限制列宽上下界，避免极端内容把布局拉爆或压得过窄。
 */
function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

/**
 * 根据配置判断某个 diagram engine 是否被启用。
 *
 * 设计约定：
 * - 如果未提供 config，或 config.features 为空，则视为所有内置图表功能启用；
 * - 仅当 config 中**显式包含**相关 Feature 且 enabled 为 false 时，才视为禁用；
 * - 这样既兼容「自动生成的完整配置」，也兼容「只配置少数 Feature」的场景。
 */
function isDiagramFeatureEnabled(config: SupramarkConfig | undefined, engine: string): boolean {
  const ids = getFeatureIdsForEngine(engine);
  if (!ids.length) {
    // 当前 engine 尚未与具体 Feature 绑定，默认启用，但给出一次性告警
    warnIfUnknownDiagramEngine(engine as any, 'rn:diagram-feature');
    return true;
  }
  return isFeatureGroupEnabled(config, ids);
}

/**
 * 将 diagram.engine 映射到对应的 Feature ID。
 *
 * 后续如果增加独立的 Mermaid / PlantUML Feature，只需在此补充映射即可。
 */
function getFeatureIdsForEngine(engine: string): string[] {
  const normalized = engine.toLowerCase();

  if (
    normalized === 'vega' ||
    normalized === 'vega-lite' ||
    normalized === 'chart' ||
    normalized === 'chartjs'
  ) {
    return ['@supramark/feature-diagram-vega-lite'];
  }

  if (normalized === 'echarts') {
    return ['@supramark/feature-diagram-echarts'];
  }

  return [];
}

/**
 * 判断一组 Feature ID 是否被启用。
 *
 * 约定：
 * - 未提供 config 或 config.features 为空 → 视为全部启用；
 * - 如果 config 中根本没有提到这些 ID → 视为使用默认行为（启用）；
 * - 一旦显式配置了其中任意一个 ID，则以配置为准，只要有一个 enabled:true 就认为启用。
 */
function isFeatureGroupEnabled(config: SupramarkConfig | undefined, ids: string[]): boolean {
  if (!config || !config.features || config.features.length === 0) {
    return true;
  }

  const hasAny = ids.some(id => config.features!.some(f => f.id === id));
  if (!hasAny) {
    return true;
  }

  return ids.some(id => isFeatureEnabled(config, id));
}

function renderDisabledDiagram(
  diagram: SupramarkDiagramNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  isLastBlock = false
): React.ReactNode {
  const header = `[diagram engine="${diagram.engine}" 已被禁用]\n\n`;
  return (
    <View key={key} style={[styles.codeBlock, isLastBlock && LAST_BLOCK_STYLE]}>
      <Text style={styles.code}>{header + diagram.code}</Text>
    </View>
  );
}

function renderDisabledMathBlock(
  math: SupramarkMathBlockNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  isLastBlock = false
): React.ReactNode {
  const header = '[math 已被禁用]\n\n';
  return (
    <View key={key} style={[styles.codeBlock, isLastBlock && LAST_BLOCK_STYLE]}>
      <Text style={styles.code}>{header + math.value}</Text>
    </View>
  );
}

function renderMapNodeFromContainer(
  container: SupramarkContainerNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  config?: SupramarkConfig
): React.ReactNode {
  // 从 container.data 中提取 map 数据
  const data = container.data || {};
  const center = (data.center as [number, number]) || [0, 0];
  const zoom = (data.zoom as number) || 12;
  const marker = data.marker as { lat: number; lng: number } | undefined;

  // 尝试使用真实的 react-native-maps
  try {
    // react-native-maps is an optional dependency; keep it lazy-loaded.
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const MapView = require('react-native-maps').default;
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const { Marker } = require('react-native-maps');

    const { width } = Dimensions.get('window');

    // 解析坐标
    const latitude = center[0] || 0;
    const longitude = center[1] || 0;

    // 计算地图区域 - 根据zoom调整视野范围
    const latitudeDelta = Math.max(0.001, 0.1 * Math.pow(0.5, zoom - 8));
    const longitudeDelta = Math.max(0.001, 0.1 * Math.pow(0.5, zoom - 8));

    const region = {
      latitude,
      longitude,
      latitudeDelta,
      longitudeDelta,
    };

    const hasMarker = marker && typeof marker.lat === 'number' && typeof marker.lng === 'number';

    return (
      <View key={key} style={styles.mapCard}>
        <View style={styles.mapCardHeader}>
          <Text style={styles.mapCardTitle}>🗺️ 真实地图</Text>
          <Text style={styles.mapCardSubtitle}>React Native Maps 实现</Text>
        </View>

        <View style={styles.mapContainer}>
          <MapView
            style={[styles.map, { width: width - 32 }]}
            region={region}
            mapType="standard"
            showsUserLocation={false}
            showsMyLocationButton={false}
            zoomEnabled={true}
            scrollEnabled={true}
            rotateEnabled={true}
            pitchEnabled={false}
          >
            {/* 中心标记 */}
            <Marker
              coordinate={{ latitude, longitude }}
              title="中心点"
              description={`坐标: ${latitude}, ${longitude}`}
              pinColor="red"
            />

            {/* 额外标记 */}
            {hasMarker && (
              <Marker
                coordinate={{
                  latitude: marker!.lat,
                  longitude: marker!.lng,
                }}
                title="标记点"
                description={`位置: ${marker!.lat}, ${marker!.lng}`}
                pinColor="blue"
              />
            )}
          </MapView>
        </View>

        <View style={styles.mapCardContent}>
          <Text style={styles.mapCardInfo}>
            📍 中心：{latitude.toFixed(4)}, {longitude.toFixed(4)}
          </Text>
          <Text style={styles.mapCardInfo}>🔍 缩放级别：{zoom}</Text>
          {hasMarker && (
            <Text style={styles.mapCardInfo}>
              📌 标记：{marker!.lat}, {marker!.lng}
            </Text>
          )}
          <Text style={[styles.mapCardInfo, { color: '#28a745', fontWeight: '500' }]}>
            ✅ 真实地图已启用
          </Text>
        </View>
      </View>
    );
  } catch (error) {
    // 如果 react-native-maps 不可用，显示智能占位卡片
    const { width } = Dimensions.get('window');
    const centerText = center ? `${center[0]}, ${center[1]}` : '未指定';
    const hasMarkerFallback =
      marker && typeof marker.lat === 'number' && typeof marker.lng === 'number';

    return (
      <View key={key} style={styles.mapCard}>
        <View style={styles.mapCardHeader}>
          <Text style={styles.mapCardTitle}>🗺️ 智能地图卡片</Text>
          <Text style={styles.mapCardSubtitle}>可视化占位符 (react-native-maps 未就绪)</Text>
        </View>

        {/* 智能地图占位区域 */}
        <View style={styles.mapContainer}>
          <View style={[styles.map, { width: width - 32 }]}>
            {/* 模拟地图网格 */}
            <View style={styles.mapGridOverlay}>
              {Array.from({ length: 4 }, (_, i) => (
                <View key={`h-${i}`} style={[styles.mapGridLine, { top: `${(i + 1) * 20}%` }]} />
              ))}
              {Array.from({ length: 4 }, (_, i) => (
                <View
                  key={`v-${i}`}
                  style={[
                    styles.mapGridLine,
                    styles.mapGridLineVertical,
                    { left: `${(i + 1) * 20}%` },
                  ]}
                />
              ))}
            </View>

            {/* 中心标记 */}
            <View style={styles.mapCenterMarker}>
              <Text style={styles.mapCenterMarkerText}>📍</Text>
            </View>

            {/* 额外标记 */}
            {hasMarkerFallback && (
              <View
                style={[
                  styles.mapMarker,
                  {
                    top: '30%',
                    left: '60%',
                  },
                ]}
              >
                <Text style={styles.mapMarkerText}>📌</Text>
              </View>
            )}

            {/* 地图信息覆盖层 */}
            <View style={styles.mapOverlay}>
              <Text style={styles.mapOverlayText}>模拟 {zoom}x</Text>
            </View>
          </View>
        </View>

        <View style={styles.mapCardContent}>
          <Text style={styles.mapCardInfo}>📍 中心：{centerText}</Text>
          <Text style={styles.mapCardInfo}>🔍 缩放级别：{zoom}</Text>
          {hasMarkerFallback && (
            <Text style={styles.mapCardInfo}>
              📌 标记：{marker!.lat}, {marker!.lng}
            </Text>
          )}
          <Text style={[styles.mapCardInfo, { color: '#ffc107', fontStyle: 'italic' }]}>
            ⚠️ 安装 react-native-maps 以启用真实地图
          </Text>
        </View>
      </View>
    );
  }
}
