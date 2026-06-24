import React, { useContext, useEffect, useMemo, useState, ReactNode } from 'react';
import type {
  SupramarkRootNode,
  SupramarkNode,
  SupramarkParagraphNode,
  SupramarkHeadingNode,
  SupramarkCodeNode,
  SupramarkMathBlockNode,
  SupramarkInlineCodeNode,
  SupramarkListNode,
  SupramarkListItemNode,
  SupramarkDiagramNode,
  SupramarkContainerNode,
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
  SupramarkDefinitionTermNode,
  SupramarkDefinitionDescriptionNode,
  SupramarkDiagramConfig,
  SupramarkConfig,
  SupramarkCodeHighlightResult,
  SupramarkCodeHighlighter,
} from '@supramark/core';
import { type DiagramRenderResult, type DiagramRenderService } from '@supramark/engines';
import { createWebDiagramEngine } from '@supramark/engines/web';
import {
  parse,
  isFeatureEnabled,
  isDiagramFeatureEnabled,
  getFeatureOptionsAs,
  SUPRAMARK_ADMONITION_KINDS,
} from '@supramark/core';
import {
  type SupramarkClassNames,
  mergeClassNames,
  tailwindClassNames,
  minimalClassNames,
} from './classNames.js';
import { DiagramBlock } from './DiagramBlock.js';
import { DiagramEngineContext } from './DiagramEngineProvider.js';
import { ErrorBoundary, ErrorInfo, ErrorDisplay } from './ErrorBoundary.js';
import { MathBlockWeb, MathInlineWeb } from './MathBlockWeb.js';

export interface ContainerRendererWeb {
  (args: {
    node: any;
    key: number;
    classNames: SupramarkClassNames;
    config?: SupramarkConfig;
    renderNode: (node: SupramarkNode, key: number) => React.ReactNode;
    renderChildren: (children: SupramarkNode[]) => React.ReactNode;
  }): React.ReactNode;
}

export interface SupramarkWebProps {
  markdown: string;
  ast?: SupramarkRootNode;
  classNames?: SupramarkClassNames;
  theme?: 'tailwind' | 'minimal' | SupramarkClassNames;
  config?: SupramarkConfig;
  onError?: (error: Error, errorInfo?: React.ErrorInfo) => void;
  errorFallback?: (error: ErrorInfo) => ReactNode;
  errorClassNamePrefix?: string;
  containerRenderers?: Record<string, ContainerRendererWeb>;
  codeHighlighter?: SupramarkCodeHighlighter;
  codeHighlightTheme?: string;
  onRenderStateChange?: (state: SupramarkRenderState) => void;
}

export interface SupramarkRenderState {
  pending: boolean;
  renderTasks: number;
  highlightTasks: number;
  engines: string[];
}

type RenderTask = {
  key: string;
  engine: string;
  code: string;
  options?: Record<string, unknown>;
};

type CodeHighlightTask = {
  key: string;
  code: string;
  lang?: string;
  meta?: string;
  theme?: string;
};

/**
 * 递归遍历 AST，对 `mode: 'opaque'` 且有 `value` 的 container，
 * 用 parse(value) 解析 value 成 AST 子树，填回 container.children。
 *
 * 新 AST v2 的 opaque container children 为空，正文在 value（原始 markdown 字符串）。
 * Rust parser 不认 feature 插件 JS 侧注册的 registerContainerHook，把所有 :::xxx
 * 当 opaque 处理。这里在主组件异步上下文里预解析，renderNode 就能同步渲染 children。
 *
 * 递归处理已填充的 children（可能嵌套 opaque container）。
 */
async function expandOpaqueContainers(node: SupramarkNode): Promise<void> {
  if (!node || typeof node !== 'object' || !('children' in node)) {
    return;
  }
  const parent = node as { children: SupramarkNode[] };
  for (let i = 0; i < parent.children.length; i++) {
    const child = parent.children[i] as SupramarkNode & {
      mode?: string;
      value?: string;
    };
    if (
      child &&
      typeof child === 'object' &&
      child.mode === 'opaque' &&
      typeof child.value === 'string' &&
      child.value.length > 0
    ) {
      try {
        const sub = await parse(child.value);
        (child as { children: SupramarkNode[] }).children = sub.children;
        // 清空 value，避免 renderNode 误判再次解析
        child.value = undefined;
      } catch {
        // 解析失败保留原 children（可能为空），renderNode 走 fallback
      }
    }
    await expandOpaqueContainers(child);
  }
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
    (child): child is SupramarkDefinitionDescriptionNode => child.type === 'definition_description'
  );
}

const defaultDiagramEngine = createWebDiagramEngine();

// Admonition 默认主题（仅在未给出自定义 className 时生效）。
// key 对应 SUPRAMARK_ADMONITION_KINDS：note / tip / info / warning / danger。
const ADMONITION_STYLES: Record<string, { border: string; background: string; icon: string }> = {
  note: { border: '#3b82f6', background: '#eff6ff', icon: 'ℹ️' },
  tip: { border: '#10b981', background: '#ecfdf5', icon: '💡' },
  info: { border: '#0ea5e9', background: '#f0f9ff', icon: 'ℹ️' },
  warning: { border: '#f59e0b', background: '#fffbeb', icon: '⚠️' },
  danger: { border: '#ef4444', background: '#fef2f2', icon: '⛔' },
};

export const Supramark: React.FC<SupramarkWebProps> = ({
  markdown,
  ast,
  classNames: customClassNames,
  theme,
  config,
  onError,
  errorFallback,
  errorClassNamePrefix = 'sm-error',
  containerRenderers,
  codeHighlighter,
  codeHighlightTheme,
  onRenderStateChange,
}) => {
  const diagramEngine = useContext(DiagramEngineContext) ?? defaultDiagramEngine;
  const [root, setRoot] = useState<SupramarkRootNode | null>(ast ?? null);
  const [rendered, setRendered] = useState<Map<string, DiagramRenderResult>>(new Map());
  const [highlighted, setHighlighted] = useState<Map<string, SupramarkCodeHighlightResult>>(
    new Map()
  );
  const [parseError, setParseError] = useState<ErrorInfo | null>(null);

  const mergedClassNames = useMemo(() => {
    let themeClassNames: SupramarkClassNames | undefined;

    if (typeof theme === 'string') {
      themeClassNames = theme === 'tailwind' ? tailwindClassNames : minimalClassNames;
    } else if (theme) {
      themeClassNames = theme;
    }

    return mergeClassNames({
      ...themeClassNames,
      ...customClassNames,
    });
  }, [customClassNames, theme]);

  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        onRenderStateChange?.({
          pending: true,
          renderTasks: 0,
          highlightTasks: 0,
          engines: [],
        });

        const parsed = ast ?? (await parse(markdown, { config }));
        // Post-process：递归解析 opaque container 的 value。
        // 新 AST v2 的 opaque container children 为空，正文在 value（原始 markdown）。
        // Rust parser 不认 feature 插件 JS 侧注册的 registerContainerHook，
        // 把所有 :::xxx 当 opaque 处理。这里在主组件异步上下文里把 value 解析成
        // AST 子树填回 children，renderNode 就能正常渲染。
        await expandOpaqueContainers(parsed);
        const renderTasks = collectRenderTasks(parsed.children, config);
        const highlightTasks = collectCodeHighlightTasks(
          parsed.children,
          config,
          codeHighlightTheme
        );
        const engines = [...new Set(renderTasks.map(task => task.engine))];

        if (!cancelled) {
          onRenderStateChange?.({
            pending: true,
            renderTasks: renderTasks.length,
            highlightTasks: highlightTasks.length,
            engines,
          });
        }

        const renderedMap = await preRenderAll(renderTasks, diagramEngine);
        const highlightedMap = await preHighlightAll(highlightTasks, codeHighlighter);

        if (!cancelled) {
          setRoot(parsed);
          setRendered(renderedMap);
          setHighlighted(highlightedMap);
          setParseError(null);
          onRenderStateChange?.({
            pending: false,
            renderTasks: renderTasks.length,
            highlightTasks: highlightTasks.length,
            engines,
          });
        }
      } catch (error) {
        if (!cancelled) {
          const err = error as Error;
          setParseError({
            type: 'parse',
            message: err.message || '解析 Markdown 失败',
            details: err.toString(),
            stack: err.stack,
          });
          setRendered(new Map());
          setHighlighted(new Map());
          setRoot(null);
          onRenderStateChange?.({
            pending: false,
            renderTasks: 0,
            highlightTasks: 0,
            engines: [],
          });
          if (onError) {
            onError(err);
          }
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [
    markdown,
    ast,
    config,
    diagramEngine,
    onError,
    codeHighlighter,
    codeHighlightTheme,
    onRenderStateChange,
  ]);

  const mergedContainerRenderers = useMemo(() => {
    return containerRenderers ?? {};
  }, [containerRenderers]);

  if (parseError) {
    if (errorFallback) {
      return <>{errorFallback(parseError)}</>;
    }
    return (
      <div>
        <ErrorDisplay error={parseError} classNamePrefix={errorClassNamePrefix} />
        <pre className={mergedClassNames.codeBlock}>
          <code>{markdown}</code>
        </pre>
      </div>
    );
  }

  if (!root) {
    return null;
  }

  return (
    <ErrorBoundary
      onError={onError}
      fallback={errorFallback}
      classNamePrefix={errorClassNamePrefix}
    >
      <div className={mergedClassNames.root}>
        {root.children.map((node, index) =>
          renderNode(
            node,
            index,
            mergedClassNames,
            rendered,
            highlighted,
            config,
            mergedContainerRenderers
          )
        )}
      </div>
    </ErrorBoundary>
  );
};

function renderNode(
  node: SupramarkNode,
  key: number,
  classNames: SupramarkClassNames,
  rendered: Map<string, DiagramRenderResult>,
  highlighted: Map<string, SupramarkCodeHighlightResult>,
  config?: SupramarkConfig,
  containerRenderers?: Record<string, ContainerRendererWeb>
): React.ReactNode {
  switch (node.type) {
    case 'paragraph':
      return (
        <p key={key} className={classNames.paragraph}>
          {renderInlineNodes(
            (node as SupramarkParagraphNode).children,
            classNames,
            rendered,
            highlighted,
            config
          )}
        </p>
      );
    case 'heading': {
      const heading = node as SupramarkHeadingNode;
      const content = renderInlineNodes(
        heading.children,
        classNames,
        rendered,
        highlighted,
        config
      );
      switch (heading.depth) {
        case 1:
          return (
            <h1 key={key} className={classNames.h1}>
              {content}
            </h1>
          );
        case 2:
          return (
            <h2 key={key} className={classNames.h2}>
              {content}
            </h2>
          );
        case 3:
          return (
            <h3 key={key} className={classNames.h3}>
              {content}
            </h3>
          );
        case 4:
          return (
            <h4 key={key} className={classNames.h4}>
              {content}
            </h4>
          );
        case 5:
          return (
            <h5 key={key} className={classNames.h5}>
              {content}
            </h5>
          );
        default:
          return (
            <h6 key={key} className={classNames.h6}>
              {content}
            </h6>
          );
      }
    }
    case 'code': {
      const codeBlock = node as SupramarkCodeNode;
      return renderCodeBlock(codeBlock, key, classNames, highlighted);
    }
    case 'math_block': {
      const mathBlock = node as SupramarkMathBlockNode;
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-math'])) {
        return (
          <pre key={key} className={classNames.codeBlock}>
            <code className={classNames.code}>{mathBlock.value}</code>
          </pre>
        );
      }
      return (
        <MathBlockWeb
          key={key}
          classNames={classNames}
          value={mathBlock.value}
          result={rendered.get(buildRenderKey('math', mathBlock.value, { displayMode: true }))}
        />
      );
    }
    case 'list': {
      const list = node as SupramarkListNode;
      const items = list.children.map((item, index) =>
        renderNode(item, index, classNames, rendered, highlighted, config, containerRenderers)
      );
      return list.ordered ? (
        <ol key={key} className={classNames.listOrdered}>
          {items}
        </ol>
      ) : (
        <ul key={key} className={classNames.listUnordered}>
          {items}
        </ul>
      );
    }
    case 'list_item': {
      const item = node as SupramarkListItemNode;
      const isTaskListFeatureEnabled = isFeatureGroupEnabled(config, ['@supramark/feature-gfm']);
      const isTaskList = isTaskListFeatureEnabled && item.checked !== undefined;

      if (isTaskList) {
        return (
          <li key={key} className={classNames.taskListItem}>
            <input
              type="checkbox"
              checked={item.checked === true}
              disabled
              className={classNames.taskCheckbox}
            />
            {item.children.map((child, index) =>
              renderNode(
                child,
                index,
                classNames,
                rendered,
                highlighted,
                config,
                containerRenderers
              )
            )}
          </li>
        );
      }

      return (
        <li key={key} className={classNames.listItem}>
          {item.children.map((child, index) =>
            renderNode(child, index, classNames, rendered, highlighted, config, containerRenderers)
          )}
        </li>
      );
    }
    case 'diagram': {
      const diagram = node as SupramarkDiagramNode;
      if (!isDiagramFeatureEnabled(config, diagram.engine, 'web:diagram-feature')) {
        return renderDisabledDiagram(diagram, key, classNames);
      }

      if (isPreRenderedDiagramEngine(diagram.engine)) {
        return (
          <DiagramBlock
            key={key}
            classNames={classNames}
            code={diagram.code}
            engine={diagram.engine}
            result={rendered.get(buildRenderKey(diagram.engine, diagram.code, diagram.meta))}
          />
        );
      }

      return (
        <div key={key} data-supramark-diagram={diagram.engine} className={classNames.diagram}>
          <pre className={classNames.diagramPre}>
            <code className={classNames.diagramCode}>{diagram.code}</code>
          </pre>
        </div>
      );
    }
    case 'container': {
      const container = node as SupramarkContainerNode;
      const containerName = container.name;

      if (containerRenderers && containerRenderers[containerName]) {
        return containerRenderers[containerName]({
          node: container,
          key,
          classNames,
          config,
          renderNode: (nextNode, nextKey) =>
            renderNode(
              nextNode,
              nextKey,
              classNames,
              rendered,
              highlighted,
              config,
              containerRenderers
            ),
          renderChildren: children =>
            children.map((child, index) =>
              renderNode(
                child,
                index,
                classNames,
                rendered,
                highlighted,
                config,
                containerRenderers
              )
            ),
        });
      }

      // Admonition 可能以两种形态到达这里：
      //   1. 直接用 kind 作为 name（container.ts 内置解析）→ containerName ∈ SUPRAMARK_ADMONITION_KINDS
      //   2. 来自 @supramark/feature-admonition（feature 注册的 hook）→ name='admonition', data.kind=实际种类
      const kindFromData = container.data?.kind as string | undefined;
      const isAdmonition =
        SUPRAMARK_ADMONITION_KINDS.includes(containerName as any) ||
        (containerName === 'admonition' &&
          kindFromData !== undefined &&
          SUPRAMARK_ADMONITION_KINDS.includes(kindFromData as any));
      if (isAdmonition) {
        const kind = (kindFromData as string) || containerName;
        // title 优先使用 data.title（已剥离 kind 名），否则退回 params（可能含 kind 前缀）
        const title =
          (container.data?.title as string | undefined) ||
          (containerName === 'admonition' ? undefined : container.params);

        if (!isFeatureGroupEnabled(config, ['@supramark/feature-admonition'])) {
          return (
            <p key={key} className={classNames.paragraph}>
              {title ? <strong>{title}</strong> : null}
              {title ? ' ' : null}
              {container.children.map((child, index) =>
                renderNode(
                  child,
                  index,
                  classNames,
                  rendered,
                  highlighted,
                  config,
                  containerRenderers
                )
              )}
            </p>
          );
        }

        const adOptions =
          getFeatureOptionsAs<{ kinds?: string[] }>(config, '@supramark/feature-admonition') ?? {};
        if (
          Array.isArray(adOptions.kinds) &&
          adOptions.kinds.length > 0 &&
          !adOptions.kinds.includes(kind)
        ) {
          return (
            <p key={key} className={classNames.paragraph}>
              {title ? <strong>{title}</strong> : null}
              {title ? ' ' : null}
              {container.children.map((child, index) =>
                renderNode(
                  child,
                  index,
                  classNames,
                  rendered,
                  highlighted,
                  config,
                  containerRenderers
                )
              )}
            </p>
          );
        }

        const admonitionStyle = ADMONITION_STYLES[kind] ?? ADMONITION_STYLES.note;

        return (
          <div
            key={key}
            className={`admonition admonition-${kind} ${classNames.paragraph ?? ''}`.trim()}
            style={{
              margin: '1em 0',
              padding: '0.75em 1em',
              borderLeft: `4px solid ${admonitionStyle.border}`,
              background: admonitionStyle.background,
              borderRadius: 4,
            }}
          >
            {title ? (
              <p style={{ margin: '0 0 0.25em', color: admonitionStyle.border, fontWeight: 600 }}>
                <span aria-hidden="true" style={{ marginRight: 6 }}>
                  {admonitionStyle.icon}
                </span>
                {title}
              </p>
            ) : null}
            <div>
              {container.children.map((child, index) =>
                renderNode(
                  child,
                  index,
                  classNames,
                  rendered,
                  highlighted,
                  config,
                  containerRenderers
                )
              )}
            </div>
          </div>
        );
      }

      if (containerName === 'map') {
        const data = container.data || {};
        const center = data.center as [number, number] | undefined;
        const zoom = data.zoom as number | undefined;
        const marker = data.marker as { lat: number; lng: number } | undefined;

        const centerText = center ? `${center[0]}, ${center[1]}` : '未指定';
        const zoomText =
          typeof zoom === 'number' && !Number.isNaN(zoom) ? `缩放级别：${zoom}` : null;
        const markerText =
          marker && typeof marker.lat === 'number' && typeof marker.lng === 'number'
            ? `标记：${marker.lat}, ${marker.lng}`
            : null;

        return (
          <div key={key} className={classNames.paragraph}>
            <p>
              <strong>地图卡片</strong>
            </p>
            <p>
              中心：{centerText}
              {zoomText ? `；${zoomText}` : ''}
              {markerText ? `；${markerText}` : ''}
            </p>
          </div>
        );
      }

      return (
        <div
          key={key}
          className={`container container-${containerName} ${classNames.paragraph ?? ''}`.trim()}
        >
          {container.params && <div className="container-params">{container.params}</div>}
          <div className="container-content">
            {container.children.map((child, index) =>
              renderNode(
                child,
                index,
                classNames,
                rendered,
                highlighted,
                config,
                containerRenderers
              )
            )}
          </div>
        </div>
      );
    }
    case 'definition_list': {
      const list = node as SupramarkDefinitionListNode;
      const defOptions =
        getFeatureOptionsAs<{ compact?: boolean }>(config, '@supramark/feature-definition-list') ??
        {};
      const isCompact = defOptions.compact !== false;
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-definition-list'])) {
        return (
          <div key={key} className={classNames.paragraph}>
            {list.children.map((item, index) => {
              const defItem = item as SupramarkDefinitionItemNode;
              const terms = getDefinitionTerms(defItem);
              const descriptions = getDefinitionDescriptions(defItem);
              return (
                <div key={index} className={classNames.paragraph}>
                  {terms.map((term, termIndex) => (
                    <p key={`term-${termIndex}`} className={classNames.paragraph}>
                      <strong>
                        {renderInlineNodes(
                          term.children,
                          classNames,
                          rendered,
                          highlighted,
                          config
                        )}
                      </strong>
                    </p>
                  ))}
                  {descriptions.map((description, descriptionIndex) => (
                    <div key={`description-${descriptionIndex}`}>
                      {description.children.map((child, childIndex) =>
                        renderNode(
                          child,
                          childIndex,
                          classNames,
                          rendered,
                          highlighted,
                          config,
                          containerRenderers
                        )
                      )}
                    </div>
                  ))}
                </div>
              );
            })}
          </div>
        );
      }
      return (
        <dl key={key} className={classNames.paragraph}>
          {list.children.map((item, index) => {
            const defItem = item as SupramarkDefinitionItemNode;
            const terms = getDefinitionTerms(defItem);
            const descriptions = getDefinitionDescriptions(defItem);
            return (
              <React.Fragment key={index}>
                {terms.map((term, termIndex) => (
                  <dt key={`term-${termIndex}`}>
                    <strong>
                      {renderInlineNodes(term.children, classNames, rendered, highlighted, config)}
                    </strong>
                  </dt>
                ))}
                {descriptions.map((description, idx) => (
                  <dd key={idx}>
                    {description.children.map((child, childIndex) =>
                      renderNode(
                        child,
                        childIndex,
                        classNames,
                        rendered,
                        highlighted,
                        config,
                        containerRenderers
                      )
                    )}
                    {isCompact ? null : <br />}
                  </dd>
                ))}
              </React.Fragment>
            );
          })}
        </dl>
      );
    }
    case 'table': {
      const table = node as SupramarkTableNode;
      return (
        <table key={key} className={classNames.table}>
          <tbody className={classNames.tableBody}>
            {table.children.map((row, index) =>
              renderNode(row, index, classNames, rendered, highlighted, config, containerRenderers)
            )}
          </tbody>
        </table>
      );
    }
    case 'table_row': {
      const row = node as SupramarkTableRowNode;
      return (
        <tr key={key} className={classNames.tableRow}>
          {row.children.map((cell, index) =>
            renderNode(cell, index, classNames, rendered, highlighted, config, containerRenderers)
          )}
        </tr>
      );
    }
    case 'table_cell': {
      const cell = node as SupramarkTableCellNode;
      const alignStyle = cell.align ? { textAlign: cell.align } : undefined;
      const content = renderInlineNodes(cell.children, classNames, rendered, highlighted, config);

      if (cell.header) {
        return (
          <th key={key} style={alignStyle} className={classNames.tableHeaderCell}>
            {content}
          </th>
        );
      }

      return (
        <td key={key} style={alignStyle} className={classNames.tableCell}>
          {content}
        </td>
      );
    }
    case 'footnote_definition': {
      const def = node as SupramarkFootnoteDefinitionNode;
      // def.children 是块级节点（通常是单个 paragraph），不能直接喂给 renderInlineNodes。
      // 常见形态 `[^1]: 内容。` → children = [{ type: 'paragraph', children: [text] }]
      // 做一次扁平化：若 children 就是单个 paragraph，把其 inline 内容直接铺出来；
      // 否则按块级节点渲染（允许多段脚注）。
      const soleParagraph =
        def.children.length === 1 && def.children[0]?.type === 'paragraph'
          ? (def.children[0] as SupramarkParagraphNode)
          : null;
      const body = soleParagraph
        ? renderInlineNodes(soleParagraph.children, classNames, rendered, highlighted, config)
        : def.children.map((child, index) =>
            renderNode(child, index, classNames, rendered, highlighted, config, containerRenderers)
          );
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-footnote'])) {
        return soleParagraph ? (
          <p key={key} className={classNames.paragraph}>
            {body}
          </p>
        ) : (
          <div key={key} className={classNames.paragraph}>
            {body}
          </div>
        );
      }
      return soleParagraph ? (
        <p key={key} id={`fn-${def.index}`} className={classNames.paragraph}>
          <sup>[{def.index}]</sup> {body}
        </p>
      ) : (
        <div key={key} id={`fn-${def.index}`} className={classNames.paragraph}>
          <sup>[{def.index}]</sup> {body}
        </div>
      );
    }
    case 'text':
      return <React.Fragment key={key}>{(node as SupramarkTextNode).value}</React.Fragment>;
    case 'strong':
    case 'emphasis':
    case 'delete':
    case 'inline_code':
    case 'math_inline':
    case 'link':
    case 'image':
    case 'break':
    case 'footnote_reference':
      // Rust parser 把 list_item.children 等场景的 inline 节点扁平铺开（非 paragraph 包裹），
      // renderNode 遍历到这些类型时委托给 renderInlineNode，避免走 default 返回 null 吞掉内容。
      return renderInlineNode(node, key, classNames, rendered, highlighted, config);
    default:
      return null;
  }
}

function renderCodeBlock(
  codeBlock: SupramarkCodeNode,
  key: number,
  classNames: SupramarkClassNames,
  highlighted: Map<string, SupramarkCodeHighlightResult>
): React.ReactNode {
  const highlight = highlighted.get(
    buildCodeHighlightKey(codeBlock.value, codeBlock.lang, codeBlock.meta)
  );

  if (!highlight) {
    return (
      <pre key={key} className={classNames.codeBlock}>
        <code className={classNames.code}>{codeBlock.value}</code>
      </pre>
    );
  }

  return (
    <pre key={key} className={classNames.codeBlock}>
      <code className={classNames.code} data-language={highlight.language ?? codeBlock.lang}>
        {highlight.lines.map((line, lineIndex) => (
          <React.Fragment key={lineIndex}>
            {line.tokens.map((token, tokenIndex) => (
              <span key={tokenIndex} style={codeTokenStyle(token)}>
                {token.text}
              </span>
            ))}
            {lineIndex < highlight.lines.length - 1 ? '\n' : null}
          </React.Fragment>
        ))}
      </code>
    </pre>
  );
}

function codeTokenStyle(token: {
  color?: string;
  backgroundColor?: string;
  fontStyle?: Array<'bold' | 'italic' | 'underline'>;
}): React.CSSProperties {
  const fontStyle = token.fontStyle ?? [];
  return {
    color: token.color,
    backgroundColor: token.backgroundColor,
    fontWeight: fontStyle.includes('bold') ? 'bold' : undefined,
    fontStyle: fontStyle.includes('italic') ? 'italic' : undefined,
    textDecoration: fontStyle.includes('underline') ? 'underline' : undefined,
  };
}

function renderInlineNodes(
  nodes: SupramarkNode[],
  classNames: SupramarkClassNames,
  rendered: Map<string, DiagramRenderResult>,
  highlighted: Map<string, SupramarkCodeHighlightResult>,
  config?: SupramarkConfig
): React.ReactNode {
  return nodes.map((node, index) =>
    renderInlineNode(node, index, classNames, rendered, highlighted, config)
  );
}

function renderInlineNode(
  node: SupramarkNode,
  key: number,
  classNames: SupramarkClassNames,
  rendered: Map<string, DiagramRenderResult>,
  highlighted: Map<string, SupramarkCodeHighlightResult>,
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
        <strong key={key} className={classNames.strong}>
          {renderInlineNodes(strongNode.children, classNames, rendered, highlighted, config)}
        </strong>
      );
    }
    case 'emphasis': {
      const emphasisNode = node as SupramarkEmphasisNode;
      return (
        <em key={key} className={classNames.emphasis}>
          {renderInlineNodes(emphasisNode.children, classNames, rendered, highlighted, config)}
        </em>
      );
    }
    case 'inline_code': {
      const codeNode = node as SupramarkInlineCodeNode;
      return (
        <code key={key} className={classNames.inlineCode}>
          {codeNode.value}
        </code>
      );
    }
    case 'math_inline': {
      const mathNode = node as SupramarkMathInlineNode;
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-math'])) {
        return mathNode.value;
      }
      return (
        <MathInlineWeb
          key={key}
          classNames={classNames}
          value={mathNode.value}
          result={rendered.get(buildRenderKey('math', mathNode.value, { displayMode: false }))}
        />
      );
    }
    case 'link': {
      const linkNode = node as SupramarkLinkNode;
      return (
        <a key={key} href={linkNode.url} title={linkNode.title} className={classNames.link}>
          {renderInlineNodes(linkNode.children, classNames, rendered, highlighted, config)}
        </a>
      );
    }
    case 'image': {
      const imageNode = node as SupramarkImageNode;
      return (
        <img
          key={key}
          src={imageNode.url}
          alt={imageNode.alt}
          title={imageNode.title}
          className={classNames.image}
        />
      );
    }
    case 'break':
      return <br key={key} />;
    case 'delete': {
      const deleteNode = node as SupramarkDeleteNode;
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-gfm'])) {
        return renderInlineNodes(deleteNode.children, classNames, rendered, highlighted, config);
      }
      return (
        <del key={key} className={classNames.delete}>
          {renderInlineNodes(deleteNode.children, classNames, rendered, highlighted, config)}
        </del>
      );
    }
    case 'footnote_reference': {
      const ref = node as SupramarkFootnoteReferenceNode;
      return (
        <sup key={key} className={classNames.inlineCode}>
          <a href={`#fn-${ref.index}`} className={classNames.link}>
            [{ref.index}]
          </a>
        </sup>
      );
    }
    default:
      return null;
  }
}

function collectRenderTasks(nodes: SupramarkNode[], config?: SupramarkConfig): RenderTask[] {
  const tasks: RenderTask[] = [];

  function walk(list: SupramarkNode[]) {
    for (const node of list) {
      if (node.type === 'diagram') {
        const diagram = node as SupramarkDiagramNode;
        if (
          isPreRenderedDiagramEngine(diagram.engine) &&
          isDiagramFeatureEnabled(config, diagram.engine, 'web:diagram-feature')
        ) {
          tasks.push({
            key: buildRenderKey(diagram.engine, diagram.code, diagram.meta),
            engine: normalizeRenderEngine(diagram.engine),
            code: diagram.code,
            options: buildDiagramRenderOptions(diagram.engine, diagram.meta, config?.diagram),
          });
        }
      } else if (node.type === 'math_block') {
        const mathBlock = node as SupramarkMathBlockNode;
        if (isFeatureGroupEnabled(config, ['@supramark/feature-math'])) {
          tasks.push({
            key: buildRenderKey('math', mathBlock.value, { displayMode: true }),
            engine: 'math',
            code: mathBlock.value,
            options: { displayMode: true },
          });
        }
      } else if (node.type === 'math_inline') {
        const mathInline = node as SupramarkMathInlineNode;
        if (isFeatureGroupEnabled(config, ['@supramark/feature-math'])) {
          tasks.push({
            key: buildRenderKey('math', mathInline.value, { displayMode: false }),
            engine: 'math',
            code: mathInline.value,
            options: { displayMode: false },
          });
        }
      }

      if ('children' in node && Array.isArray((node as { children?: SupramarkNode[] }).children)) {
        walk((node as { children: SupramarkNode[] }).children);
      }
    }
  }

  walk(nodes);
  return tasks;
}

function collectCodeHighlightTasks(
  nodes: SupramarkNode[],
  config?: SupramarkConfig,
  theme?: string
): CodeHighlightTask[] {
  if (!isFeatureGroupEnabled(config, ['@supramark/feature-code-highlight'])) {
    return [];
  }

  const tasks: CodeHighlightTask[] = [];

  function walk(list: SupramarkNode[]) {
    for (const node of list) {
      if (node.type === 'code') {
        const code = node as SupramarkCodeNode;
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

async function preRenderAll(
  tasks: RenderTask[],
  engine: DiagramRenderService
): Promise<Map<string, DiagramRenderResult>> {
  if (tasks.length === 0) {
    return new Map();
  }

  const unique = new Map<string, RenderTask>();
  for (const task of tasks) {
    if (!unique.has(task.key)) {
      unique.set(task.key, task);
    }
  }

  const taskList = [...unique.values()];
  const results = await Promise.all(
    taskList.map(task =>
      engine.render({
        engine: task.engine,
        code: task.code,
        options: task.options,
      })
    )
  );

  return new Map(taskList.map((task, index) => [task.key, results[index]]));
}

async function preHighlightAll(
  tasks: CodeHighlightTask[],
  highlighter?: SupramarkCodeHighlighter
): Promise<Map<string, SupramarkCodeHighlightResult>> {
  if (!highlighter || tasks.length === 0) {
    return new Map();
  }

  const unique = new Map<string, CodeHighlightTask>();
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

function buildRenderKey(engine: string, code: string, options?: Record<string, unknown>): string {
  return `${normalizeRenderEngine(engine)}:${code}:${stableSerialize(options)}`;
}

function buildCodeHighlightKey(code: string, lang?: string, meta?: string): string {
  return `code:${lang ?? ''}:${meta ?? ''}:${code}`;
}

const PRE_RENDERED_DIAGRAM_ENGINES = new Set([
  'mermaid',
  'math',
  'dot',
  'graphviz',
  'echarts',
  'vega-lite',
  'vegalite',
  'vega',
  'chart',
  'chartjs',
  'plantuml',
  'd2',
]);

function normalizeRenderEngine(engine: string): string {
  const normalized = String(engine || '').toLowerCase();
  return PRE_RENDERED_DIAGRAM_ENGINES.has(normalized) ? normalized : 'mermaid';
}

function isPreRenderedDiagramEngine(engine: string): boolean {
  return PRE_RENDERED_DIAGRAM_ENGINES.has(String(engine || '').toLowerCase());
}

function buildDiagramRenderOptions(
  engine: string,
  meta: SupramarkDiagramNode['meta'],
  diagramConfig?: SupramarkDiagramConfig
): Record<string, unknown> | undefined {
  const base: Record<string, unknown> = {};
  const engineConfig = diagramConfig?.engines?.[engine];

  if (engineConfig) {
    if (typeof engineConfig.server === 'string') {
      base.server = engineConfig.server;
      base.plantumlServer = engineConfig.server;
    }
    if (typeof engineConfig.timeoutMs === 'number') {
      base.timeout = engineConfig.timeoutMs;
    }
    if (engineConfig.cache) {
      base.cache = engineConfig.cache;
    }

    for (const [key, value] of Object.entries(engineConfig as Record<string, unknown>)) {
      if (value === undefined) {
        continue;
      }
      if (key === 'enabled' || key === 'timeoutMs' || key === 'server' || key === 'cache') {
        continue;
      }
      base[key] = value;
    }
  }

  if (!meta) {
    return Object.keys(base).length > 0 ? base : undefined;
  }

  return { ...base, ...meta };
}

function stableSerialize(value: unknown): string {
  if (value === null || value === undefined) {
    return '';
  }
  if (Array.isArray(value)) {
    return `[${value.map(stableSerialize).join(',')}]`;
  }
  if (typeof value === 'object') {
    return `{${Object.entries(value as Record<string, unknown>)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, entryValue]) => `${key}:${stableSerialize(entryValue)}`)
      .join(',')}}`;
  }
  return String(value);
}

function renderDisabledDiagram(
  diagram: SupramarkDiagramNode,
  key: number,
  classNames: SupramarkClassNames
): React.ReactNode {
  const header = `[diagram engine="${diagram.engine}" 已被禁用]\n\n`;
  return (
    <pre key={key} className={classNames.codeBlock}>
      <code className={classNames.code}>{header + diagram.code}</code>
    </pre>
  );
}

function isFeatureGroupEnabled(config: SupramarkConfig | undefined, ids: string[]): boolean {
  if (!config || !config.features || config.features.length === 0) {
    return true;
  }

  const hasAny = ids.some(id => config.features!.some(feature => feature.id === id));
  if (!hasAny) {
    return true;
  }

  return ids.some(id => isFeatureEnabled(config, id));
}
