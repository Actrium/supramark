import React from 'react';
import { Path, Svg } from 'react-native-svg';
import {
  CODE_ACTION_ICONS,
  CODE_ACTION_ICON_VIEWBOX,
  type SupramarkCodeActionIconId,
} from '@supramark/core';

interface CodeActionIconProps {
  /** Which icon to draw. */
  id: SupramarkCodeActionIconId;
  /** Rendered edge length in points. */
  size: number;
  /** Theme colour; the artwork itself carries no colour. */
  color: string;
}

/**
 * A code-block action icon, drawn through `react-native-svg`.
 *
 * The geometry comes from `CODE_ACTION_ICONS` in `@supramark/core` — the same
 * path strings Web inlines — so the two platforms cannot drift on the artwork.
 *
 * The icon is decorative: the enclosing `TouchableOpacity` owns the
 * accessibility label, so the SVG is hidden from screen readers rather than
 * announced as an unlabelled image.
 *
 * `react-native-svg` is already a hard requirement of this package (DiagramNode,
 * MathBlock and MathInline import it at module scope), so using it here adds no
 * new dependency for a host.
 */
export function CodeActionIcon({ id, size, color }: CodeActionIconProps): React.ReactElement {
  const icon = CODE_ACTION_ICONS[id];
  return (
    <Svg
      width={size}
      height={size}
      viewBox={CODE_ACTION_ICON_VIEWBOX}
      testID={`supramark-code-action-${id}`}
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
    >
      <Path
        d={icon.path}
        fill={icon.paint === 'solid' ? color : 'none'}
        {...(icon.paint === 'outline'
          ? { stroke: color, strokeWidth: 2, strokeLinecap: 'round', strokeLinejoin: 'round' }
          : {})}
      />
    </Svg>
  );
}
