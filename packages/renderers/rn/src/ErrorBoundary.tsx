import React, { Component, ReactNode } from 'react';
import { View, Text, StyleSheet, ScrollView } from 'react-native';
import type { SupramarkStyles } from './styles';

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
   * 错误展示使用的主题样式。
   */
  styles?: SupramarkStyles;
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
      return <ErrorDisplay error={this.state.error} styles={this.props.styles} />;
    }

    return this.props.children;
  }
}

/**
 * 默认错误展示组件
 */
export function ErrorDisplay({
  error,
  styles: customStyles,
}: {
  error: ErrorInfo;
  styles?: SupramarkStyles;
}) {
  // 默认错误样式与传入主题样式合并，保证单独使用 ErrorDisplay 时仍然可渲染。
  const styles = {
    ...defaultErrorStyles,
    ...customStyles,
  };

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
    <View style={styles.errorContainer}>
      <View style={styles.errorBox}>
        <View style={[styles.errorHeader, { backgroundColor: errorTypeColor[error.type] }]}>
          <Text style={styles.errorTitle}>{errorTypeText[error.type]}</Text>
        </View>
        <View style={styles.errorBody}>
          <Text style={styles.errorMessage}>{error.message}</Text>
          {error.details && (
            <View style={styles.errorSection}>
              <Text style={styles.errorSectionTitle}>详细信息：</Text>
              <ScrollView style={styles.errorDetailsScroll} horizontal>
                <Text style={styles.errorDetailsText}>{error.details}</Text>
              </ScrollView>
            </View>
          )}
          {__DEV__ && error.stack && (
            <View style={styles.errorSection}>
              <Text style={styles.errorSectionTitle}>堆栈跟踪（开发模式）：</Text>
              <ScrollView style={styles.errorStackScroll}>
                <Text style={styles.errorStackText}>{error.stack}</Text>
              </ScrollView>
            </View>
          )}
        </View>
      </View>
    </View>
  );
}

const defaultErrorStyles = StyleSheet.create({
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
});
