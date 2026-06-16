import {
  useState,
  useRef,
  useCallback,
  useEffect,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
} from 'react';
import { Supramark, type SupramarkRenderState } from '@supramark/web/client';
import {
  featureRegistry,
  findFeature,
  containerRenderers,
  type FeatureEntry,
} from './feature-registry';
import './FeaturePreview.css';

type MobilePane = 'preview' | 'editor';

const MOBILE_QUERY = '(max-width: 767px)';
const LINE_HEIGHT = 19.5; // 13px * 1.5
const IDLE_RENDER_STATE: SupramarkRenderState = {
  pending: false,
  renderTasks: 0,
  highlightTasks: 0,
  engines: [],
};

function previewEnginesForFeature(shortName: string): string[] {
  switch (shortName) {
    case 'mermaid':
      return ['mermaid'];
    case 'd2':
      return ['d2'];
    case 'diagram-dot':
      return ['dot'];
    case 'plantuml':
      return ['plantuml'];
    case 'diagram-echarts':
      return ['echarts'];
    case 'diagram-vega-lite':
      return ['vega-lite'];
    case 'math':
      return ['math'];
    default:
      return [];
  }
}

export function FeaturePreview({ initialFeature }: { initialFeature: string }) {
  const initial = findFeature(initialFeature) ?? featureRegistry[0];
  const [feature, setFeature] = useState<FeatureEntry>(initial);
  const [selectedExample, setSelectedExample] = useState(0);
  const [markdown, setMarkdown] = useState(initial.examples[0]?.markdown ?? '');
  const [dirty, setDirty] = useState(false); // track if editor content modified from example
  const [previewWidth, setPreviewWidth] = useState(480); // px
  const [maxPreviewWidth, setMaxPreviewWidth] = useState(1200); // actual available width
  const [bgMode, setBgMode] = useState<'light' | 'dark'>('light');
  const [copied, setCopied] = useState(false);
  const [isMobile, setIsMobile] = useState(false);
  const [mobilePane, setMobilePane] = useState<MobilePane>('preview');
  const [renderState, setRenderState] = useState<SupramarkRenderState>(IDLE_RENDER_STATE);

  // Splitter drag
  const [leftPct, setLeftPct] = useState(50);
  const dragging = useRef(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const previewAreaRef = useRef<HTMLDivElement>(null);

  // Editor + gutter scroll sync
  const editorRef = useRef<HTMLTextAreaElement>(null);
  const gutterRef = useRef<HTMLDivElement>(null);

  const onDragStart = useCallback(
    (e: ReactMouseEvent) => {
      if (isMobile) return;
      e.preventDefault();
      dragging.current = true;
    },
    [isMobile]
  );

  useEffect(() => {
    const media = window.matchMedia(MOBILE_QUERY);
    let observer: ResizeObserver | undefined;
    const update = () => {
      const viewportWidth = window.visualViewport?.width ?? window.innerWidth;
      const containerWidth = containerRef.current?.getBoundingClientRect().width ?? 0;
      setIsMobile(
        media.matches || viewportWidth <= 767 || (containerWidth > 0 && containerWidth <= 767)
      );
    };

    update();
    if (containerRef.current) {
      observer = new ResizeObserver(update);
      observer.observe(containerRef.current);
    }

    media.addEventListener('change', update);
    window.addEventListener('resize', update);
    window.visualViewport?.addEventListener('resize', update);

    return () => {
      observer?.disconnect();
      media.removeEventListener('change', update);
      window.removeEventListener('resize', update);
      window.visualViewport?.removeEventListener('resize', update);
    };
  }, []);

  useEffect(() => {
    if (isMobile) {
      setMobilePane('preview');
    }
  }, [feature.shortName, isMobile, selectedExample]);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (isMobile || !dragging.current || !containerRef.current) return;
      const rect = containerRef.current.getBoundingClientRect();
      const pct = ((e.clientX - rect.left) / rect.width) * 100;
      setLeftPct(Math.min(80, Math.max(20, pct)));
    };
    const onUp = () => {
      dragging.current = false;
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
  }, [isMobile]);

  // Sync gutter scroll with textarea
  const onEditorScroll = useCallback(() => {
    if (editorRef.current && gutterRef.current) {
      gutterRef.current.scrollTop = editorRef.current.scrollTop;
    }
  }, []);

  // Compute visible line count from editor height
  const [visibleLines, setVisibleLines] = useState(50);
  useEffect(() => {
    const el = editorRef.current;
    if (!el) return;
    const observer = new ResizeObserver(() => {
      const h = el.clientHeight;
      setVisibleLines(Math.ceil(h / LINE_HEIGHT));
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [isMobile, mobilePane]);

  // Track available width in preview area.
  useEffect(() => {
    const el = previewAreaRef.current;
    if (!el) return;
    const minWidth = isMobile ? 280 : 120;
    const horizontalPadding = isMobile ? 24 : 32;
    const update = () => {
      const available = Math.max(minWidth, el.clientWidth - horizontalPadding);
      setMaxPreviewWidth(available);
      setPreviewWidth(prev => (isMobile ? available : Math.min(prev, available)));
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, [isMobile, mobilePane]);

  const lineCount = markdown.split('\n').length;
  const gutterLines = Math.max(lineCount, visibleLines);

  const switchFeature = (shortName: string) => {
    const f = findFeature(shortName);
    if (!f) return;
    setRenderState({
      pending: true,
      renderTasks: 0,
      highlightTasks: 0,
      engines: previewEnginesForFeature(f.shortName),
    });
    setFeature(f);
    setSelectedExample(0);
    setMarkdown(f.examples[0]?.markdown ?? '');
    setDirty(false);
    const url = new URL(window.location.href);
    url.searchParams.set('feature', shortName);
    window.history.replaceState(null, '', url.toString());
  };

  const switchExample = (idx: number) => {
    setRenderState({
      pending: true,
      renderTasks: 0,
      highlightTasks: 0,
      engines: previewEnginesForFeature(feature.shortName),
    });
    setSelectedExample(idx);
    setMarkdown(feature.examples[idx]?.markdown ?? '');
    setDirty(false);
  };

  const onEditorChange = (value: string) => {
    setMarkdown(value);
    setDirty(value !== (feature.examples[selectedExample]?.markdown ?? ''));
  };

  const onRenderStateChange = useCallback((state: SupramarkRenderState) => {
    setRenderState(state);
  }, []);

  const copyVersion = () => {
    navigator.clipboard.writeText(feature.version);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const monoFont = 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace';

  const shellStyle: CSSProperties = {
    position: 'fixed',
    inset: 0,
    display: 'flex',
    overflow: 'hidden',
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
    fontSize: 13,
    userSelect: dragging.current ? 'none' : 'auto',
  };

  const selectBaseStyle: CSSProperties = {
    border: '1px solid #d0d0d0',
    borderRadius: 4,
    background: '#fff',
    cursor: 'pointer',
    outline: 'none',
    minWidth: 0,
  };

  const renderFeatureControls = (compact = false) => (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: compact ? 8 : 12,
        flex: compact ? 1 : 'initial',
        minWidth: 0,
        flexWrap: compact ? 'wrap' : 'nowrap',
      }}
    >
      <select
        value={feature.shortName}
        onChange={e => switchFeature(e.target.value)}
        style={{
          ...selectBaseStyle,
          fontSize: 14,
          fontWeight: 600,
          padding: compact ? '6px 8px' : '2px 6px',
          color: '#1a1a1a',
          flex: compact ? '1 1 160px' : 'initial',
          width: compact ? 'auto' : 'auto',
        }}
      >
        {featureRegistry.map(f => (
          <option key={f.shortName} value={f.shortName}>
            {f.displayName}
          </option>
        ))}
      </select>
      {feature.examples.length > 0 && (
        <select
          value={selectedExample}
          onChange={e => switchExample(Number(e.target.value))}
          style={{
            ...selectBaseStyle,
            fontSize: 13,
            fontWeight: 500,
            padding: compact ? '6px 8px' : '2px 6px',
            color: '#333',
            flex: compact ? '1 1 130px' : 'initial',
            width: compact ? 'auto' : 'auto',
          }}
        >
          {feature.examples.map((ex, i) => (
            <option key={i} value={i}>
              {ex.name}
              {i === selectedExample && dirty ? '*' : ''}
            </option>
          ))}
        </select>
      )}
    </div>
  );

  const renderVersion = () => (
    <span
      onClick={copyVersion}
      title="Click to copy"
      style={{ fontSize: 12, color: '#999', cursor: 'pointer', whiteSpace: 'nowrap' }}
    >
      {copied ? 'Copied!' : `Ver: ${feature.version}`}
    </span>
  );

  const renderBackgroundToggle = () => (
    <>
      <span
        style={{
          fontSize: 11,
          color: bgMode === 'light' ? '#333' : '#aaa',
          fontWeight: bgMode === 'light' ? 600 : 400,
          userSelect: 'none',
        }}
      >
        Light
      </span>
      <button
        type="button"
        aria-label="Toggle preview background"
        onClick={() => setBgMode(bgMode === 'light' ? 'dark' : 'light')}
        style={{
          width: 36,
          height: 20,
          borderRadius: 10,
          cursor: 'pointer',
          position: 'relative',
          background: bgMode === 'dark' ? '#333' : '#ccc',
          transition: 'background 0.2s',
          flexShrink: 0,
          padding: 0,
          border: 0,
        }}
      >
        <span
          style={{
            width: 16,
            height: 16,
            borderRadius: 8,
            background: '#fff',
            position: 'absolute',
            top: 2,
            left: bgMode === 'dark' ? 18 : 2,
            transition: 'left 0.2s',
            boxShadow: '0 1px 2px rgba(0,0,0,0.3)',
          }}
        />
      </button>
      <span
        style={{
          fontSize: 11,
          color: bgMode === 'dark' ? '#333' : '#aaa',
          fontWeight: bgMode === 'dark' ? 600 : 400,
          userSelect: 'none',
        }}
      >
        Dark
      </span>
    </>
  );

  const renderPreviewLoading = () => {
    if (!renderState.pending) return null;

    const engines =
      renderState.engines.length > 0 ? renderState.engines.join(' / ') : feature.displayName;
    const detail =
      renderState.renderTasks > 0 ? `Rendering ${engines}` : `Preparing ${feature.displayName}`;

    return (
      <div className="feature-preview-loading" role="status" aria-live="polite">
        <div className="feature-preview-loading__spinner" />
        <div className="feature-preview-loading__body">
          <strong>{detail}</strong>
          <span>Loading parser and renderer modules</span>
        </div>
      </div>
    );
  };

  const renderEditor = (compact = false) => (
    <div
      style={{ flex: 1, display: 'flex', overflow: 'hidden', position: 'relative', minHeight: 0 }}
    >
      <div
        ref={gutterRef}
        style={{
          width: compact ? 36 : 40,
          flexShrink: 0,
          overflow: 'hidden',
          background: '#f5f5f5',
          borderRight: '1px solid #e0e0e0',
          padding: '8px 0',
          textAlign: 'right',
          fontFamily: monoFont,
          fontSize: 13,
          lineHeight: '1.5',
          color: '#b0b0b0',
          userSelect: 'none',
        }}
      >
        {Array.from({ length: gutterLines }, (_, i) => (
          <div key={i} style={{ paddingRight: 8, height: LINE_HEIGHT }}>
            {i + 1}
          </div>
        ))}
      </div>
      <textarea
        ref={editorRef}
        value={markdown}
        onChange={e => onEditorChange(e.target.value)}
        onScroll={onEditorScroll}
        spellCheck={false}
        style={{
          flex: 1,
          padding: compact ? '10px 12px' : '8px 12px',
          border: 'none',
          resize: 'none',
          background: '#fff',
          color: '#1a1a1a',
          fontFamily: monoFont,
          fontSize: 13,
          lineHeight: '1.5',
          outline: 'none',
          tabSize: 2,
          minWidth: 0,
        }}
        placeholder="Enter Markdown here..."
      />
    </div>
  );

  const renderPreview = (compact = false) => (
    <div
      ref={previewAreaRef}
      style={{
        flex: 1,
        overflow: 'auto',
        minHeight: 0,
        background: bgMode === 'dark' ? '#1a1a1a' : '#f5f5f5',
        display: 'flex',
        justifyContent: 'center',
        alignItems: 'flex-start',
        padding: compact ? '12px' : '24px 16px',
      }}
    >
      <div
        className={`feature-preview-render-shell${renderState.pending ? ' is-loading' : ''}`}
        data-preview-theme={bgMode}
        style={{
          width: compact ? maxPreviewWidth : previewWidth,
          maxWidth: '100%',
        }}
      >
        {renderPreviewLoading()}
        <div
          className="feature-preview-render"
          data-preview-theme={bgMode}
          style={{
            width: '100%',
            boxSizing: 'border-box',
            background: bgMode === 'dark' ? '#2d2d2d' : '#fff',
            color: bgMode === 'dark' ? '#e0e0e0' : '#1a1a1a',
            borderRadius: 6,
            boxShadow: '0 1px 4px rgba(0,0,0,0.1)',
            padding: compact ? 16 : 20,
            alignSelf: 'flex-start',
          }}
        >
          <Supramark
            markdown={markdown}
            containerRenderers={containerRenderers}
            onRenderStateChange={onRenderStateChange}
          />
        </div>
      </div>
    </div>
  );

  if (isMobile) {
    const tabBaseStyle: CSSProperties = {
      flex: 1,
      minWidth: 0,
      border: 0,
      borderRadius: 6,
      padding: '8px 10px',
      fontSize: 13,
      fontWeight: 700,
      background: 'transparent',
      color: '#555',
      cursor: 'pointer',
    };

    return (
      <div ref={containerRef} style={{ ...shellStyle, flexDirection: 'column' }}>
        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: 10,
            padding: '10px 12px',
            borderBottom: '1px solid #e0e0e0',
            flexShrink: 0,
            background: '#fafafa',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8, minWidth: 0 }}>
            {renderFeatureControls(true)}
            {renderVersion()}
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, minWidth: 0 }}>
            <div
              role="tablist"
              aria-label="Preview mode"
              style={{
                display: 'flex',
                flex: 1,
                minWidth: 0,
                padding: 3,
                borderRadius: 8,
                border: '1px solid #ddd',
                background: '#efeff4',
              }}
            >
              <button
                type="button"
                role="tab"
                aria-selected={mobilePane === 'preview'}
                onClick={() => setMobilePane('preview')}
                style={{
                  ...tabBaseStyle,
                  background: mobilePane === 'preview' ? '#fff' : 'transparent',
                  color: mobilePane === 'preview' ? '#1a1a1a' : '#666',
                  boxShadow: mobilePane === 'preview' ? '0 1px 2px rgba(0,0,0,0.12)' : 'none',
                }}
              >
                Preview
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={mobilePane === 'editor'}
                onClick={() => setMobilePane('editor')}
                style={{
                  ...tabBaseStyle,
                  background: mobilePane === 'editor' ? '#fff' : 'transparent',
                  color: mobilePane === 'editor' ? '#1a1a1a' : '#666',
                  boxShadow: mobilePane === 'editor' ? '0 1px 2px rgba(0,0,0,0.12)' : 'none',
                }}
              >
                Markdown
              </button>
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexShrink: 0 }}>
              {renderBackgroundToggle()}
            </div>
          </div>
        </div>
        <div style={{ flex: 1, minHeight: 0, display: 'flex', overflow: 'hidden' }}>
          {mobilePane === 'editor' ? renderEditor(true) : renderPreview(true)}
        </div>
      </div>
    );
  }

  return (
    <div ref={containerRef} style={shellStyle}>
      <div
        style={{
          width: `${leftPct}%`,
          display: 'flex',
          flexDirection: 'column',
          borderRight: '1px solid #d0d0d0',
          minWidth: 0,
        }}
      >
        <div
          style={{
            height: 36,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            padding: '0 12px',
            borderBottom: '1px solid #e0e0e0',
            flexShrink: 0,
            background: '#fafafa',
          }}
        >
          {renderFeatureControls()}
          {renderVersion()}
        </div>
        {renderEditor()}
      </div>

      <div
        onMouseDown={onDragStart}
        style={{ width: 5, cursor: 'col-resize', background: '#e0e0e0', flexShrink: 0 }}
        onMouseEnter={e => (e.currentTarget.style.background = '#4a90d9')}
        onMouseLeave={e => {
          if (!dragging.current) e.currentTarget.style.background = '#e0e0e0';
        }}
      />

      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
        <div
          style={{
            height: 36,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'flex-end',
            gap: 12,
            padding: '0 12px',
            borderBottom: '1px solid #e0e0e0',
            flexShrink: 0,
            background: '#fafafa',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <span style={{ fontSize: 12, color: '#666' }}>Width:</span>
            <input
              type="range"
              min={120}
              max={maxPreviewWidth}
              step={10}
              value={previewWidth}
              onChange={e => setPreviewWidth(Math.min(Number(e.target.value), maxPreviewWidth))}
              style={{ width: 120, accentColor: '#333' }}
            />
            <span style={{ fontSize: 11, color: '#999', minWidth: 40, textAlign: 'right' }}>
              {previewWidth}px
            </span>
          </div>
          <div style={{ width: 20, flexShrink: 0 }} />
          {renderBackgroundToggle()}
        </div>
        {renderPreview()}
      </div>
    </div>
  );
}
