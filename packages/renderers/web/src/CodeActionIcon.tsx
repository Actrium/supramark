import React from 'react';
import {
  CODE_ACTION_ICONS,
  CODE_ACTION_ICON_VIEWBOX,
  type SupramarkCodeActionIconId,
} from '@supramark/core';

interface CodeActionIconProps {
  /** Which icon to draw. */
  id: SupramarkCodeActionIconId;
  /** Rendered edge length in pixels. */
  size: number;
}

/**
 * A code-block action icon, inlined as SVG.
 *
 * The geometry comes from `CODE_ACTION_ICONS` in `@supramark/core` — the same
 * path strings React Native feeds to `react-native-svg` — so the two platforms
 * cannot drift on the artwork. Inlining costs one element and no request, and
 * the icon paints with `currentColor`, so it follows the button's CSS colour
 * (including a host's dark branch) with no second asset.
 *
 * The icon is decorative: the enclosing button owns the accessible name
 * (`aria-label`), so the SVG is hidden from assistive tech rather than
 * announced as an unlabelled graphic.
 */
export function CodeActionIcon({ id, size }: CodeActionIconProps): React.ReactElement {
  const icon = CODE_ACTION_ICONS[id];
  return (
    <svg
      width={size}
      height={size}
      viewBox={CODE_ACTION_ICON_VIEWBOX}
      data-supramark-code-action={id}
      aria-hidden="true"
      focusable="false"
    >
      <path
        d={icon.path}
        fill={icon.paint === 'solid' ? 'currentColor' : 'none'}
        {...(icon.paint === 'outline'
          ? {
              stroke: 'currentColor',
              strokeWidth: 2,
              strokeLinecap: 'round',
              strokeLinejoin: 'round',
            }
          : {})}
      />
    </svg>
  );
}
