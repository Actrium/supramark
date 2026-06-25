import React, { Component, type ReactNode } from 'react';

/**
 * 错误信息接口
 */
export interface ErrorInfo {
  type: 'parse' | 'render' | 'diagram' | 'unknown';
  message: string;
  details?: string;
  stack?: string;
}

/**
 * ErrorBoundary 属性
 */
export interface ErrorBoundaryProps {
  children: ReactNode;
  /**
   * 错误回调（可选）
   */
  onError?: (error: Error, errorInfo: React.ErrorInfo) => void;
  /**
   * 自定义错误展示组件（可选）
   */
  fallback?: (error: ErrorInfo) => ReactNode;
  /**
   * CSS 类名前缀，默认 'sm-error'
   */
  classNamePrefix?: string;
}

/**
 * ErrorBoundary 状态
 */
interface ErrorBoundaryState {
  hasError: boolean;
  error: ErrorInfo | null;
}

/**
 * Web 错误边界组件
 *
 * 捕获子组件树中的渲染错误，展示友好的错误信息
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = {
      hasError: false,
      error: null,
    };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    // 分析错误类型
    const errorType = ErrorBoundary.categorizeError(error);

    return {
      hasError: true,
      error: {
        type: errorType,
        message: error.message,
        details: error.toString(),
        stack: error.stack,
      },
    };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    // 调用错误回调
    if (this.props.onError) {
      this.props.onError(error, errorInfo);
    }

    // 在开发环境打印错误信息
    if (process.env.NODE_ENV === 'development') {
      console.error('Supramark Error Boundary caught an error:', error, errorInfo);
    }
  }

  /**
   * 根据错误信息分类错误类型
   */
  private static categorizeError(error: Error): ErrorInfo['type'] {
    const message = error.message.toLowerCase();
    const stack = error.stack?.toLowerCase() || '';

    if (message.includes('parse') || message.includes('syntax')) {
      return 'parse';
    }
    if (message.includes('diagram') || stack.includes('diagram')) {
      return 'diagram';
    }
    if (message.includes('render') || stack.includes('render')) {
      return 'render';
    }
    return 'unknown';
  }

  render() {
    if (this.state.hasError && this.state.error) {
      // 使用自定义 fallback 或默认错误展示组件
      if (this.props.fallback) {
        return this.props.fallback(this.state.error);
      }
      return <ErrorDisplay error={this.state.error} classNamePrefix={this.props.classNamePrefix} />;
    }

    return this.props.children;
  }
}

/**
 * 默认错误展示组件
 */
export function ErrorDisplay({
  error,
  classNamePrefix = 'sm-error',
}: {
  error: ErrorInfo;
  classNamePrefix?: string;
}) {
  const errorTypeText = {
    parse: '解析错误',
    render: '渲染错误',
    diagram: '图表错误',
    unknown: '未知错误',
  };

  const errorTypeColor = {
    parse: '#d4380d',
    render: '#d46b08',
    diagram: '#ad8b00',
    unknown: '#8c8c8c',
  };

  const isDev = process.env.NODE_ENV === 'development';

  return (
    <div className={`${classNamePrefix}-container`} style={styles.container}>
      <div className={`${classNamePrefix}-box`} style={styles.errorBox}>
        <div
          className={`${classNamePrefix}-header`}
          style={{
            ...styles.errorHeader,
            backgroundColor: errorTypeColor[error.type],
          }}
        >
          <span className={`${classNamePrefix}-title`} style={styles.errorTitle}>
            {errorTypeText[error.type]}
          </span>
        </div>
        <div className={`${classNamePrefix}-body`} style={styles.errorBody}>
          <p className={`${classNamePrefix}-message`} style={styles.errorMessage}>
            {error.message}
          </p>
          {error.details && (
            <details className={`${classNamePrefix}-details`} style={styles.detailsContainer}>
              <summary style={styles.detailsSummary}>详细信息</summary>
              <pre className={`${classNamePrefix}-details-text`} style={styles.detailsText}>
                <code>{error.details}</code>
              </pre>
            </details>
          )}
          {isDev && error.stack && (
            <details className={`${classNamePrefix}-stack`} style={styles.stackContainer}>
              <summary style={styles.stackSummary}>堆栈跟踪（开发模式）</summary>
              <pre className={`${classNamePrefix}-stack-text`} style={styles.stackText}>
                <code>{error.stack}</code>
              </pre>
            </details>
          )}
        </div>
      </div>
    </div>
  );
}

// 内联样式（作为默认样式，可通过 className 覆盖）
const styles: Record<string, React.CSSProperties> = {
  container: {
    padding: '12px',
  },
  errorBox: {
    border: '1px solid #ffccc7',
    borderRadius: '4px',
    overflow: 'hidden',
    backgroundColor: '#fff',
  },
  errorHeader: {
    padding: '12px',
  },
  errorTitle: {
    color: '#fff',
    fontSize: '16px',
    fontWeight: 600,
  },
  errorBody: {
    padding: '12px',
    backgroundColor: '#fff2f0',
  },
  errorMessage: {
    fontSize: '14px',
    color: '#262626',
    margin: 0,
    marginBottom: '8px',
  },
  detailsContainer: {
    marginTop: '8px',
    paddingTop: '8px',
    borderTop: '1px solid #ffccc7',
  },
  detailsSummary: {
    cursor: 'pointer',
    fontSize: '12px',
    color: '#8c8c8c',
    marginBottom: '8px',
    userSelect: 'none',
  },
  detailsText: {
    fontSize: '12px',
    color: '#595959',
    fontFamily: 'monospace',
    backgroundColor: '#fafafa',
    padding: '8px',
    borderRadius: '4px',
    overflow: 'auto',
    maxHeight: '200px',
    margin: 0,
  },
  stackContainer: {
    marginTop: '8px',
    paddingTop: '8px',
    borderTop: '1px solid #ffccc7',
  },
  stackSummary: {
    cursor: 'pointer',
    fontSize: '12px',
    color: '#8c8c8c',
    marginBottom: '8px',
    userSelect: 'none',
  },
  stackText: {
    fontSize: '10px',
    color: '#8c8c8c',
    fontFamily: 'monospace',
    backgroundColor: '#fafafa',
    padding: '8px',
    borderRadius: '4px',
    overflow: 'auto',
    maxHeight: '300px',
    margin: 0,
  },
};
