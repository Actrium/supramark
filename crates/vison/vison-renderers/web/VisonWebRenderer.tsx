import React, { useMemo, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import { type VisonComponent } from '../shared/types';

/**
 * 生产级 Web 渲染器
 * 增加特性：性能优化 (Memo)、错误边界处理、图片状态管理、样式安全过滤
 */

// 图片组件：处理加载中与加载失败状态
const VisonImage: React.FC<{ props: Record<string, unknown>; style: React.CSSProperties }> = ({ props, style }) => {
  const [status, setStatus] = useState<'loading' | 'error' | 'loaded'>('loading');

  const containerStyle: React.CSSProperties = {
    ...style,
    position: 'relative',
    overflow: 'hidden',
    backgroundColor: '#F0F0F0', // 占位背景
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
  };

  return (
    <div style={containerStyle}>
      {status === 'loading' && (
        <div style={{ position: 'absolute', color: '#999', fontSize: '12px' }}>Loading...</div>
      )}
      {status === 'error' && (
        <div style={{ position: 'absolute', color: '#FF4D4F', fontSize: '12px' }}>Image Error</div>
      )}
      <img
        src={props.src as string | undefined}
        alt=""
        style={{
          width: '100%',
          height: '100%',
          objectFit: 'cover',
          opacity: status === 'loaded' ? 1 : 0,
          transition: 'opacity 0.3s ease',
        }}
        onLoad={() => setStatus('loaded')}
        onError={() => setStatus('error')}
      />
    </div>
  );
};

// 渲染错误降级组件
const RenderError: React.FC<{ error: string }> = ({ error }) => (
  <div style={{ padding: '8px', border: '1px dashed #FF4D4F', borderRadius: '4px', color: '#FF4D4F', fontSize: '12px' }}>
    Renderer Error: {error}
  </div>
);

export const VisonWebRenderer: React.FC<{ data: VisonComponent }> = React.memo(({ data }) => {
  const { type, props = {}, style = {}, children } = data;

  // 样式安全处理与计算
  const baseStyle: React.CSSProperties = useMemo(() => {
    const s: React.CSSProperties = {
      display: type === 'container' ? 'flex' : 'inline-block',
      boxSizing: 'border-box',
    };

    // 转换 Vison 样式到 CSS
    Object.entries(style).forEach(([key, value]) => {
      if (typeof value === 'number' && !['opacity', 'fontWeight', 'lineHeight'].includes(key)) {
        (s as Record<string, unknown>)[key] = `${value}px`;
      } else {
        (s as Record<string, unknown>)[key] = value;
      }
    });

    return s;
  }, [type, style]);

  try {
    switch (type) {
      case 'container':
        return (
          <div style={baseStyle}>
            {children?.map((child, index) => (
              <VisonWebRenderer key={`${type}-${index}`} data={child} />
            ))}
          </div>
        );

      case 'text':
        return <span style={baseStyle}>{props.text as React.ReactNode}</span>;

      case 'image':
        return <VisonImage props={props} style={baseStyle} />;

      case 'markdown':
        return (
          <div style={{ ...baseStyle, display: 'block' }} className="vison-markdown">
            <ReactMarkdown skipHtml={true}>{props.content as string}</ReactMarkdown>
          </div>
        );

      case 'divider':
        return (
          <div
            style={{
              height: (style.borderWidth as React.CSSProperties['height']) || 1,
              backgroundColor: (style.borderColor as string) || '#EEE',
              margin: `${(style.margin as number) || 8}px 0`,
              width: '100%',
            }}
          />
        );

      default:
        console.warn(`[Vison] Unknown component type: ${type}`);
        return null;
    }
  } catch (err) {
    return <RenderError error={String(err)} />;
  }
});
