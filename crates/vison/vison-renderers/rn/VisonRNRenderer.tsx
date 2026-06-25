import React, { useMemo, useState } from 'react';
import { View, Text, Image, ActivityIndicator } from 'react-native';
import type { StyleProp, ViewStyle, TextStyle, ImageStyle } from 'react-native';
import Markdown from 'react-native-markdown-display';
import { type VisonComponent } from '../shared/types';

// React Native's bundled @types/react differs from the workspace React types
// (e.g. on bigint in ReactNode); derive the node type from Text's own children.
type RNNode = React.ComponentProps<typeof Text>['children'];

/**
 * 生产级 React Native 渲染器
 * 增加特性：图片占位符、错误捕获、性能优化 (Memo)
 */

const VisonImage: React.FC<{ props: Record<string, unknown>; style: StyleProp<ViewStyle> }> = ({ props, style }) => {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);

  return (
    <View style={[style as StyleProp<ViewStyle>, { backgroundColor: '#F0F0F0', justifyContent: 'center', alignItems: 'center', overflow: 'hidden' }]}>
      {loading && <ActivityIndicator size="small" color="#999" style={{ position: 'absolute' }} />}
      {error && <Text style={{ color: '#FF4D4F', fontSize: 10 }}>Error</Text>}
      <Image
        source={{ uri: props.src as string | undefined }}
        style={[style as StyleProp<ImageStyle>, { position: 'absolute', width: '100%', height: '100%' }]}
        onLoadEnd={() => setLoading(false)}
        onError={() => {
          setLoading(false);
          setError(true);
        }}
      />
    </View>
  );
};

export const VisonRNRenderer: React.FC<{ data: VisonComponent }> = React.memo(({ data }) => {
  const { type, props = {}, style = {}, children } = data;

  // 使用 useMemo 避免多余重绘
  const containerStyle = useMemo(() => style as StyleProp<ViewStyle & TextStyle>, [style]);

  try {
    switch (type) {
      case 'container':
        return (
          <View style={containerStyle}>
            {children?.map((child, index) => (
              <VisonRNRenderer key={`${type}-${index}`} data={child} />
            ))}
          </View>
        );

      case 'text':
        return <Text style={containerStyle}>{props.text as RNNode}</Text>;

      case 'image':
        return <VisonImage props={props} style={containerStyle} />;

      case 'markdown':
        return (
          <View style={containerStyle}>
            <Markdown
              style={{
                body: { color: style.color as string | undefined, fontSize: style.fontSize as number | undefined, lineHeight: style.lineHeight as number | undefined },
                heading1: { marginTop: 0 },
                paragraph: { marginVertical: 4 }
              }}
            >
              {props.content as string}
            </Markdown>
          </View>
        );

      case 'divider':
        return (
          <View
            style={{
              height: (style.borderWidth as number) || 1,
              backgroundColor: (style.borderColor as string) || '#EEE',
              marginVertical: (style.margin as number) || 8,
              width: '100%',
            }}
          />
        );

      default:
        return null;
    }
  } catch (err) {
    return (
      <View style={{ padding: 10, backgroundColor: '#FFF2F0', borderWidth: 1, borderColor: '#FFCCC7' }}>
        <Text style={{ color: '#FF4D4F' }}>Render Error</Text>
      </View>
    );
  }
});
