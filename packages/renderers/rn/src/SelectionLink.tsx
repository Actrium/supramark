import React, { useContext } from 'react';
import { Linking, Text, type TextProps } from 'react-native';
import { SelectionUIContext } from '@supramark/rn-selection';

export function SelectionLink({
  href,
  children,
  style,
}: Pick<TextProps, 'children' | 'style'> & { href: string }) {
  const selection = useContext(SelectionUIContext);
  return (
    <Text
      style={style}
      onPress={event => {
        if (selection?.suppressLinkPress?.(event)) return;
        Linking.openURL(href).catch(error => console.error('Failed to open URL:', error));
      }}
    >
      {children}
    </Text>
  );
}
