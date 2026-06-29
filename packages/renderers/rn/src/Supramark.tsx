import React, { useEffect, useState, useMemo } from 'react';
import { Text, View, Linking, TouchableOpacity, Dimensions } from 'react-native';
import type {
  SupramarkRootNode,
  SupramarkNode,
  SupramarkHeadingNode,
  SupramarkCodeNode,
  SupramarkMathBlockNode,
  SupramarkDiagramNode,
  SupramarkContainerNode,
  SupramarkDefinitionItemNode,
  SupramarkDefinitionTermNode,
  SupramarkDefinitionDescriptionNode,
  SupramarkConfig,
  SupramarkCodeHighlightResult,
  SupramarkCodeHighlighter,
} from '@supramark/core';
import {
  parse,
  expandOpaqueContainers,
  isFeatureEnabled,
  isDiagramFeatureEnabled,
  getFeatureOptionsAs,
  SUPRAMARK_ADMONITION_KINDS,
} from '@supramark/core';
import { DiagramNode } from './DiagramNode';
import { MathBlock } from './MathBlock';
import { MathInline } from './MathInline';
import {
  type SupramarkStyles,
  mergeStyles,
  darkThemeStyles,
  lightThemeStyles,
} from './styles';
import { ErrorBoundary, type ErrorInfo, ErrorDisplay } from './ErrorBoundary';

type RenderedNode = React.ComponentProps<typeof Text>['children'];

// Minimal shape of the optional `react-native-maps` module. Only the members
// used here are declared; the package itself is an optional peer dependency.
interface ReactNativeMapsModule {
  default: React.ComponentType<Record<string, unknown>>;
  Marker: React.ComponentType<Record<string, unknown>>;
}

function getDefinitionTerms(item: SupramarkDefinitionItemNode): SupramarkDefinitionTermNode[] {
  return item.children.filter(
    (child): child is SupramarkDefinitionTermNode => child.type === 'definition_term'
  );
}

function getDefinitionDescriptions(
  item: SupramarkDefinitionItemNode
): SupramarkDefinitionDescriptionNode[] {
  return item.children.filter(
    (child): child is SupramarkDefinitionDescriptionNode =>
      child.type === 'definition_description'
  );
}

export interface ContainerRendererRN {
  (args: {
    node: SupramarkContainerNode;
    key: number;
    styles: ReturnType<typeof mergeStyles>;
    config?: SupramarkConfig;
    onOpenHtmlPage?: (node: SupramarkContainerNode) => void;
    renderNode: (node: SupramarkNode, key: number) => RenderedNode;
    renderChildren: (children: SupramarkNode[]) => RenderedNode;
  }): RenderedNode;
}

export interface SupramarkProps {
  /** Markdown 源文本 */
  markdown: string;
  /** 预解析的 AST（优先级高于 markdown） */
  ast?: SupramarkRootNode;
  /** 自定义样式（覆盖默认样式） */
  styles?: SupramarkStyles;
  /** 主题：'light' | 'dark' | 自定义样式对象 */
  theme?: 'light' | 'dark' | SupramarkStyles;
  /** Feature 配置（用于按需启用/禁用图表等扩展能力） */
  config?: SupramarkConfig;
  /** 错误回调（可选） */
  onError?: (error: Error, errorInfo?: React.ErrorInfo) => void;
  /** 自定义错误展示组件（可选） */
  errorFallback?: (error: ErrorInfo) => RenderedNode;

  /**
   * Container 扩展渲染器注册表：node.type === 'container' 时按 node.name 委派。
   */
  containerRenderers?: Record<string, ContainerRendererRN>;
  codeHighlighter?: SupramarkCodeHighlighter;
  codeHighlightTheme?: string;

  /**
   * 当用户点击 HTML Page 卡片时的回调。
   *
   * - node.data.html 为完整 HTML 内容；
   * - 宿主可以在回调中打开新的页面 / Modal / 外部浏览器。
   */
  onOpenHtmlPage?: (node: SupramarkContainerNode) => void;
}

export const Supramark: React.FC<SupramarkProps> = ({
  markdown,
  ast,
  styles: customStyles,
  theme,
  config,
  onError,
  errorFallback,
  onOpenHtmlPage,
  containerRenderers,
  codeHighlighter,
  codeHighlightTheme,
}) => {
  const [root, setRoot] = useState<SupramarkRootNode | null>(ast ?? null);
  const [highlighted, setHighlighted] = useState<Map<string, SupramarkCodeHighlightResult>>(
    new Map()
  );
  const [parseError, setParseError] = useState<ErrorInfo | null>(null);

  // 合并样式：theme -> customStyles -> defaultStyles
  const mergedStyles = useMemo(() => {
    let themeStyles: SupramarkStyles | undefined;

    if (typeof theme === 'string') {
      themeStyles = theme === 'dark' ? darkThemeStyles : lightThemeStyles;
    } else if (theme) {
      themeStyles = theme;
    }

    // 如果同时提供了 theme 和 customStyles，customStyles 优先级更高
    const finalCustomStyles = {
      ...themeStyles,
      ...customStyles,
    };

    return mergeStyles(finalCustomStyles);
  }, [customStyles, theme]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const parsed = ast ?? (await parse(markdown, { config }));
        // Post-process：递归解析 opaque container 的 value。
        // 新 AST v2 的 opaque container children 为空，正文在 value（原始 markdown）。
        // Rust parser 不认 feature 插件 JS 侧注册的 registerContainerHook，
        // 把所有 :::xxx 当 opaque 处理。这里在主组件异步上下文里把 value 解析成
        // AST 子树填回 children，renderNode 就能正常渲染。
        await expandOpaqueContainers(parsed);
        const highlightedMap = await preHighlightAll(
          collectCodeHighlightTasks(parsed.children, config, codeHighlightTheme),
          codeHighlighter
        );
        if (!cancelled) {
          setRoot(parsed);
          setHighlighted(highlightedMap);
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
          setHighlighted(new Map());
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
  }, [markdown, ast, config, onError, codeHighlighter, codeHighlightTheme]);

  const mergedContainerRenderers = useMemo(() => {
    // FeatureConfig 只描述启用状态与 options，不再携带 renderer 定义。
    // container 渲染器需要由宿主显式注入，避免运行时隐式耦合到 feature 包实现。
    return containerRenderers ?? {};
  }, [containerRenderers]);

  // 解析错误降级：显示错误信息或原始 markdown
  if (parseError) {
    if (errorFallback) {
      return <>{errorFallback(parseError)}</>;
    }
    return (
      <View>
        <ErrorDisplay error={parseError} />
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
    <ErrorBoundary onError={onError} fallback={errorFallback}>
      <View style={mergedStyles.root}>
        {root.children.map((node, index) =>
          renderNode(
            node,
            index,
            mergedStyles,
            highlighted,
            config,
            onOpenHtmlPage,
            mergedContainerRenderers
          )
        )}
      </View>
    </ErrorBoundary>
  );
};

function renderNode(
  node: SupramarkNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  highlighted: Map<string, SupramarkCodeHighlightResult>,
  config?: SupramarkConfig,
  onOpenHtmlPage?: (node: SupramarkContainerNode) => void,
  containerRenderers?: Record<string, ContainerRendererRN>
): RenderedNode {
  switch (node.type) {
    case 'paragraph':
      return (
        <Text key={key} style={styles.paragraph}>
          {renderInlineNodes(node.children, styles, highlighted, config)}
        </Text>
      );
    case 'heading': {
      const heading = node;
      return (
        <Text key={key} style={headingStyle(heading.depth, styles)}>
          {renderInlineNodes(heading.children, styles, highlighted, config)}
        </Text>
      );
    }
    case 'code': {
      const codeBlock = node;
      return renderCodeBlock(codeBlock, key, styles, highlighted);
    }
    case 'math_block': {
      const mathBlock = node;
      // 如果禁用了 Math Feature，则降级为普通代码块展示原始 TeX
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-math'])) {
        return renderDisabledMathBlock(mathBlock, key, styles);
      }
      return <MathBlock key={key} node={mathBlock} />;
    }
    case 'list': {
      const list = node;
      return (
        <View key={key} style={styles.list}>
          {list.children.map((item, index) =>
            renderNode(item, index, styles, highlighted, config, onOpenHtmlPage, containerRenderers)
          )}
        </View>
      );
    }
    case 'list_item': {
      const item = node;
      const isTaskList = item.checked !== undefined;
      const checkSymbol = item.checked === true ? '☑' : '☐';

      return (
        <View key={key} style={styles.listItem}>
          <Text style={styles.bullet}>{isTaskList ? checkSymbol : '•'}</Text>
          <Text style={styles.listItemText}>
            {renderInlineNodes(item.children, styles, highlighted, config)}
          </Text>
        </View>
      );
    }
    case 'diagram': {
      const diagram = node;
      // 如果配置中显式禁用了对应图表 Feature，则降级为代码块渲染
      if (!isDiagramFeatureEnabled(config, diagram.engine, 'rn:diagram-feature')) {
        return renderDisabledDiagram(diagram, key, styles);
      }
      return <DiagramNode key={key} node={diagram} diagramConfig={config?.diagram} />;
    }
    case 'container': {
      const container = node;
      const containerName = container.name;

      // 检查是否有注册的自定义渲染器
      if (containerRenderers && containerRenderers[containerName]) {
        return containerRenderers[containerName]({
          node: container,
          key,
          styles,
          config,
          onOpenHtmlPage,
          renderNode: (n, k) =>
            renderNode(n, k, styles, highlighted, config, onOpenHtmlPage, containerRenderers),
          renderChildren: children =>
            children.map((child, index) =>
              renderNode(
                child,
                index,
                styles,
                highlighted,
                config,
                onOpenHtmlPage,
                containerRenderers
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
          <View style={styles.listItem}>
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
      // opaque container 的 children 已在主组件 useEffect 里通过 expandOpaqueContainers
      // 预解析填充（parse(value) → children）。这里直接渲染 children。
      // Column 布局：title 一行，正文一行。
      if (
        SUPRAMARK_ADMONITION_KINDS.includes(
          containerName as (typeof SUPRAMARK_ADMONITION_KINDS)[number]
        )
      ) {
        const title = container.params || (container.data?.title as string | undefined);
        const kind = containerName;
        const admonitionContainerStyle = { flexDirection: 'column' as const, marginBottom: 4 };

        const renderAdmonitionContent = () =>
          container.children.map((child, index) =>
            renderNode(child, index, styles, highlighted, config, onOpenHtmlPage, containerRenderers)
          );

        if (!isFeatureGroupEnabled(config, ['@supramark/feature-admonition'])) {
          return (
            <View key={key} style={admonitionContainerStyle}>
              {title ? <Text style={styles.listItemText}>{title}</Text> : null}
              {renderAdmonitionContent()}
            </View>
          );
        }

        const adOptions =
          getFeatureOptionsAs<{ kinds?: string[] }>(config, '@supramark/feature-admonition') ?? {};
        if (Array.isArray(adOptions.kinds) && adOptions.kinds.length > 0) {
          if (!adOptions.kinds.includes(kind)) {
            return (
              <View key={key} style={admonitionContainerStyle}>
                {title ? <Text style={styles.listItemText}>{title}</Text> : null}
                {renderAdmonitionContent()}
              </View>
            );
          }
        }

        return (
          <View key={key} style={admonitionContainerStyle}>
            {title ? (
              <Text style={[styles.listItemText, { fontWeight: '600' }]}>{title}</Text>
            ) : null}
            {renderAdmonitionContent()}
          </View>
        );
      }

      // 默认：渲染为通用容器块
      return (
        <View key={key} style={styles.listItem}>
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
              highlighted,
              config,
              onOpenHtmlPage,
              containerRenderers
            )
          )}
        </View>
      );
    }
    case 'definition_list': {
      const list = node;
      const defOptions =
        getFeatureOptionsAs<{ compact?: boolean }>(config, '@supramark/feature-definition-list') ??
        {};
      const isCompact = defOptions.compact !== false; // 默认紧凑
      // Column 布局：term 一行，description 缩进一行。
      // 避免 row 布局下 description 被 term 挤压导致 Text 不换行。
      const defItemStyle = { flexDirection: 'column' as const, marginBottom: 4 };
      const defDescriptionStyle = { paddingLeft: 16 };
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-definition-list'])) {
        // 禁用时，将定义列表退化为普通列表样式
        return (
          <View key={key} style={styles.list}>
            {list.children.map((item, index) => {
              const defItem = item;
              const terms = getDefinitionTerms(defItem);
              const descriptions = getDefinitionDescriptions(defItem);
              return (
                <View key={index} style={defItemStyle}>
                  {terms.map((term, termIndex) => (
                    <Text key={`term-${termIndex}`} style={[styles.listItemText, { fontWeight: '600' }]}>
                      {renderInlineNodes(term.children, styles, highlighted, config)}
                    </Text>
                  ))}
                  {descriptions.map((description, descriptionIndex) => (
                    <View key={`description-${descriptionIndex}`} style={defDescriptionStyle}>
                      {description.children.map((child, childIndex) =>
                        renderNode(
                          child,
                          childIndex,
                          styles,
                          highlighted,
                          config,
                          onOpenHtmlPage,
                          containerRenderers
                        )
                      )}
                    </View>
                  ))}
                </View>
              );
            })}
          </View>
        );
      }
      return (
        <View key={key} style={styles.list}>
          {list.children.map((item, index) => {
            const defItem = item;
            const terms = getDefinitionTerms(defItem);
            const descriptions = getDefinitionDescriptions(defItem);
            return (
              <View key={index} style={defItemStyle}>
                {terms.map((term, termIndex) => (
                  <Text key={`term-${termIndex}`} style={[styles.listItemText, { fontWeight: '600' }]}>
                    {renderInlineNodes(term.children, styles, highlighted, config)}
                  </Text>
                ))}
                {descriptions.map((description, idx) => (
                  <View key={idx} style={defDescriptionStyle}>
                    {description.children.map((child, childIndex) =>
                      renderNode(
                        child,
                        childIndex,
                        styles,
                        highlighted,
                        config,
                        onOpenHtmlPage,
                        containerRenderers
                      )
                    )}
                    {isCompact ? null : <Text style={styles.listItemText}>{'\n'}</Text>}
                  </View>
                ))}
              </View>
            );
          })}
        </View>
      );
    }
    case 'footnote_definition': {
      const def = node;
      // 新 AST v2 的 footnote_definition.children 是 block 节点（paragraph 等），
      // 不是 inline 节点。用 renderNode 渲染 children，避免 renderInlineNodes 跳过 block 节点。
      const renderFootnoteContent = () =>
        def.children.map((child, childIndex) =>
          renderNode(child, childIndex, styles, highlighted, config, onOpenHtmlPage, containerRenderers)
        );
      // 第一阶段：简单以「[n] 内容」形式追加在文末
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-footnote'])) {
        // 禁用脚注 Feature 时，直接渲染为普通段落
        return (
          <View key={key} style={styles.listItem}>
            <Text style={styles.listItemText}>{renderFootnoteContent()}</Text>
          </View>
        );
      }
      return (
        <View key={key} style={styles.listItem}>
          <Text style={styles.bullet}>[{def.index}]</Text>
          <View style={styles.listItemText}>{renderFootnoteContent()}</View>
        </View>
      );
    }
    case 'table': {
      const table = node;
      const screenWidth = Dimensions.get('window').width;
      return (
        <View key={key} style={[styles.table, { width: screenWidth }]}>
          {table.children.map((row, index) =>
            renderNode(row, index, styles, highlighted, config, onOpenHtmlPage, containerRenderers)
          )}
        </View>
      );
    }
    case 'table_row': {
      const row = node;
      return (
        <View key={key} style={styles.tableRow}>
          {row.children.map((cell, index) =>
            renderNode(cell, index, styles, highlighted, config, onOpenHtmlPage, containerRenderers)
          )}
        </View>
      );
    }
    case 'table_cell': {
      const cell = node;
      const cellStyle = [styles.tableCell, cell.header && styles.tableHeaderCell];
      const textStyle = [
        styles.tableCellText,
        cell.header && styles.tableHeaderText,
        cell.align === 'center' && styles.textCenter,
        cell.align === 'right' && styles.textRight,
      ];

      return (
        <View key={key} style={cellStyle}>
          <Text style={textStyle}>
            {renderInlineNodes(cell.children, styles, highlighted, config)}
          </Text>
        </View>
      );
    }
    case 'text':
      return (
        <Text key={key} style={styles.paragraph}>
          {node.value}
        </Text>
      );
    default:
      return null;
  }
}

function renderCodeBlock(
  codeBlock: SupramarkCodeNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  highlighted: Map<string, SupramarkCodeHighlightResult>
): RenderedNode {
  const highlight = highlighted.get(
    buildCodeHighlightKey(codeBlock.value, codeBlock.lang, codeBlock.meta)
  );

  if (!highlight) {
    return (
      <View key={key} style={styles.codeBlock}>
        <Text style={styles.code}>{codeBlock.value}</Text>
      </View>
    );
  }

  return (
    <View key={key} style={styles.codeBlock}>
      <Text style={styles.code}>
        {highlight.lines.map((line, lineIndex) => (
          <Text key={lineIndex}>
            {line.tokens.map((token, tokenIndex) => (
              <Text key={tokenIndex} style={codeTokenTextStyle(token)}>
                {token.text}
              </Text>
            ))}
            {lineIndex < highlight.lines.length - 1 ? '\n' : null}
          </Text>
        ))}
      </Text>
    </View>
  );
}

function codeTokenTextStyle(token: {
  color?: string;
  backgroundColor?: string;
  fontStyle?: Array<'bold' | 'italic' | 'underline'>;
}) {
  const fontStyle = token.fontStyle ?? [];
  return {
    color: token.color,
    backgroundColor: token.backgroundColor,
    fontWeight: fontStyle.includes('bold') ? ('700' as const) : undefined,
    fontStyle: fontStyle.includes('italic') ? ('italic' as const) : undefined,
    textDecorationLine: fontStyle.includes('underline') ? ('underline' as const) : undefined,
  };
}

function renderInlineNodes(
  nodes: SupramarkNode[],
  styles: ReturnType<typeof mergeStyles>,
  highlighted: Map<string, SupramarkCodeHighlightResult>,
  config?: SupramarkConfig
): RenderedNode {
  return nodes.map((node, index) => renderInlineNode(node, index, styles, highlighted, config));
}

function renderInlineNode(
  node: SupramarkNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  highlighted: Map<string, SupramarkCodeHighlightResult>,
  config?: SupramarkConfig
): RenderedNode {
  switch (node.type) {
    case 'text': {
      const textNode = node;
      return textNode.value;
    }
    case 'strong': {
      const strongNode = node;
      return (
        <Text key={key} style={styles.strong}>
          {renderInlineNodes(strongNode.children, styles, highlighted, config)}
        </Text>
      );
    }
    case 'emphasis': {
      const emphasisNode = node;
      return (
        <Text key={key} style={styles.emphasis}>
          {renderInlineNodes(emphasisNode.children, styles, highlighted, config)}
        </Text>
      );
    }
    case 'inline_code': {
      const codeNode = node;
      return (
        <Text key={key} style={styles.inlineCode}>
          {codeNode.value}
        </Text>
      );
    }
    case 'math_inline': {
      const mathNode = node;
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-math'])) {
        return mathNode.value;
      }
      return <MathInline key={key} value={mathNode.value} textStyle={styles.paragraph} />;
    }
    case 'link': {
      const linkNode = node;
      return (
        <Text
          key={key}
          style={styles.link}
          onPress={() => {
            Linking.openURL(linkNode.url).catch(err => console.error('Failed to open URL:', err));
          }}
        >
          {renderInlineNodes(linkNode.children, styles, highlighted, config)}
        </Text>
      );
    }
    case 'image': {
      const imageNode = node;
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
      const deleteNode = node;
      return (
        <Text key={key} style={styles.delete}>
          {renderInlineNodes(deleteNode.children, styles, highlighted, config)}
        </Text>
      );
    }
    case 'footnote_reference': {
      const ref = node;
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

function collectCodeHighlightTasks(
  nodes: SupramarkNode[],
  config?: SupramarkConfig,
  theme?: string
): Array<{ key: string; code: string; lang?: string; meta?: string; theme?: string }> {
  if (!isFeatureGroupEnabled(config, ['@supramark/feature-code-highlight'])) {
    return [];
  }

  const tasks: Array<{ key: string; code: string; lang?: string; meta?: string; theme?: string }> =
    [];

  function walk(list: SupramarkNode[]) {
    for (const node of list) {
      if (node.type === 'code') {
        const code = node;
        tasks.push({
          key: buildCodeHighlightKey(code.value, code.lang, code.meta),
          code: code.value,
          lang: code.lang,
          meta: code.meta,
          theme,
        });
      }

      if ('children' in node && Array.isArray((node as { children?: SupramarkNode[] }).children)) {
        walk((node as { children: SupramarkNode[] }).children);
      }
    }
  }

  walk(nodes);
  return tasks;
}

async function preHighlightAll(
  tasks: Array<{ key: string; code: string; lang?: string; meta?: string; theme?: string }>,
  highlighter?: SupramarkCodeHighlighter
): Promise<Map<string, SupramarkCodeHighlightResult>> {
  if (!highlighter || tasks.length === 0) {
    return new Map();
  }

  const unique = new Map<
    string,
    { key: string; code: string; lang?: string; meta?: string; theme?: string }
  >();
  for (const task of tasks) {
    if (!unique.has(task.key)) {
      unique.set(task.key, task);
    }
  }

  const entries = await Promise.all(
    [...unique.values()].map(async task => {
      try {
        const result = await highlighter({
          code: task.code,
          lang: task.lang,
          meta: task.meta,
          theme: task.theme,
        });
        return result ? ([task.key, result] as const) : null;
      } catch {
        return null;
      }
    })
  );

  return new Map(
    entries.filter(
      (entry): entry is readonly [string, SupramarkCodeHighlightResult] => entry !== null
    )
  );
}

function buildCodeHighlightKey(code: string, lang?: string, meta?: string): string {
  return `code:${lang ?? ''}:${meta ?? ''}:${code}`;
}

function renderDisabledDiagram(
  diagram: SupramarkDiagramNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>
): RenderedNode {
  const header = `[diagram engine="${diagram.engine}" 已被禁用]\n\n`;
  return (
    <View key={key} style={styles.codeBlock}>
      <Text style={styles.code}>{header + diagram.code}</Text>
    </View>
  );
}

function renderDisabledMathBlock(
  math: SupramarkMathBlockNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>
): RenderedNode {
  const header = '[math 已被禁用]\n\n';
  return (
    <View key={key} style={styles.codeBlock}>
      <Text style={styles.code}>{header + math.value}</Text>
    </View>
  );
}

function renderMapNodeFromContainer(
  container: SupramarkContainerNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  _config?: SupramarkConfig
): RenderedNode {
  // 从 container.data 中提取 map 数据
  const data = container.data || {};
  const center = (data.center as [number, number]) || [0, 0];
  const zoom = (data.zoom as number) || 12;
  const marker = data.marker as { lat: number; lng: number } | undefined;

  // 尝试使用真实的 react-native-maps
  try {
    // react-native-maps is an optional dependency; keep it lazy-loaded.
    // Cast the untyped require() result to a minimal local module shape so the
    // downstream JSX usage stays type-safe without depending on the package types.
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const maps = require('react-native-maps') as ReactNativeMapsModule;
    const MapView = maps.default;
    const { Marker } = maps;

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
                  latitude: marker.lat,
                  longitude: marker.lng,
                }}
                title="Marker"
                description={`Location: ${marker.lat}, ${marker.lng}`}
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
              Marker: {marker.lat}, {marker.lng}
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
              Marker: {marker.lat}, {marker.lng}
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
