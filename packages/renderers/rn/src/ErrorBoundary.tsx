import React, { Component, type ReactNode } from 'react';
import { View, Text, StyleSheet, ScrollView } from 'react-native';

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
}

/**
 * ErrorBoundary 状态
 */
interface ErrorBoundaryState {
  hasError: boolean;
  error: ErrorInfo | null;
}

/**
 * React Native 错误边界组件
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
    if (__DEV__) {
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
      return <ErrorDisplay error={this.state.error} />;
    }

    return this.props.children;
  }
}

/**
 * 默认错误展示组件
 */
export function ErrorDisplay({ error }: { error: ErrorInfo }) {
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

  return (
    <View style={styles.container}>
      <View style={styles.errorBox}>
        <View style={[styles.errorHeader, { backgroundColor: errorTypeColor[error.type] }]}>
          <Text style={styles.errorTitle}>{errorTypeText[error.type]}</Text>
        </View>
        <View style={styles.errorBody}>
          <Text style={styles.errorMessage}>{error.message}</Text>
          {error.details && (
            <View style={styles.detailsContainer}>
              <Text style={styles.detailsTitle}>详细信息：</Text>
              <ScrollView style={styles.detailsScroll} horizontal>
                <Text style={styles.detailsText}>{error.details}</Text>
              </ScrollView>
            </View>
          )}
          {__DEV__ && error.stack && (
            <View style={styles.stackContainer}>
              <Text style={styles.stackTitle}>堆栈跟踪（开发模式）：</Text>
              <ScrollView style={styles.stackScroll}>
                <Text style={styles.stackText}>{error.stack}</Text>
              </ScrollView>
            </View>
          )}
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
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
  detailsContainer: {
    marginTop: 8,
    paddingTop: 8,
    borderTopWidth: 1,
    borderTopColor: '#ffccc7',
  },
  detailsTitle: {
    fontSize: 12,
    color: '#8c8c8c',
    marginBottom: 4,
  },
  detailsScroll: {
    maxHeight: 60,
  },
  detailsText: {
    fontSize: 12,
    color: '#595959',
    fontFamily: 'monospace',
  },
  stackContainer: {
    marginTop: 8,
    paddingTop: 8,
    borderTopWidth: 1,
    borderTopColor: '#ffccc7',
  },
  stackTitle: {
    fontSize: 12,
    color: '#8c8c8c',
    marginBottom: 4,
  },
  stackScroll: {
    maxHeight: 100,
  },
  stackText: {
    fontSize: 10,
    color: '#8c8c8c',
    fontFamily: 'monospace',
  },
});
