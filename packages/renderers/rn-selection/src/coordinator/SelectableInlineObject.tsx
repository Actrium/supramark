import React, { createContext, useContext } from 'react';
import { View, type LayoutChangeEvent } from 'react-native';
import type { LayoutRect } from './registry';

/** The containing Text receives its attachments' paragraph-local native layout. */
export const InlineObjectLayoutContext = createContext<
  ((id: string, rect: LayoutRect) => void) | null
>(null);

/** Keep loading text and rendered SVG in one native attachment with a stable display offset. */
export function SelectableInlineObject({
  unitId,
  children,
}: {
  unitId: string;
  children: React.ReactNode;
}) {
  // This callback belongs to the paragraph, including nested emphasis and links.
  const publish = useContext(InlineObjectLayoutContext);
  // Native inline view layout is relative to the enclosing Text content box.
  const onLayout = (event: LayoutChangeEvent) => {
    const { x, y, width, height } = event.nativeEvent.layout;
    publish?.(unitId, { x, y, w: width, h: height });
  };
  return (
    <View collapsable={false} onLayout={onLayout} style={{ flexDirection: 'row' }}>
      {/* Match the ReactNode version bundled with React Native. */}
      {children as React.ComponentProps<typeof View>['children']}
    </View>
  );
}
