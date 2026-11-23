/**
 * Vector data for the code-block action buttons (copy / copied / fullscreen).
 *
 * Each entry is geometry only — no colour, no size — drawn inside the
 * `CODE_ACTION_ICON_VIEWBOX` coordinate system. Web inlines it as
 * `<svg viewBox=...><path d=... /></svg>` and React Native feeds the same
 * string to `react-native-svg`'s `<Path d=... />`, so both platforms share one
 * source of truth and a host theme needs no second asset.
 *
 * This is deliberately data, not files: an image resource would need
 * per-platform bundler handling (Metro asset resolution vs. a web bundler),
 * `@2x`/`@3x` variants, and a dark-mode twin, and could not follow a theme
 * colour. A path string has none of those problems and scales with the font.
 *
 * The paths below follow the Lucide icon set (ISC licence), redrawn on a 24×24
 * grid with a 2px stroke so the three read as one family. Keep them monochrome
 * and stroked; the renderer applies the theme colour.
 */

/**
 * The coordinate system every path in `CODE_ACTION_ICONS` is drawn in.
 *
 * Shared so Web and React Native cannot drift on it.
 */
export const CODE_ACTION_ICON_VIEWBOX = '0 0 24 24';

export interface SupramarkCodeActionIcon {
  /**
   * Path data. A single string may hold several subpaths (`M ... M ...`), which
   * both platforms render as one icon.
   */
  path: string;
  /**
   * How to paint the path. `outline` strokes it (Lucide/Feather lineage),
   * `solid` fills it (Material lineage). Explicit rather than defaulted so
   * swapping the artwork cannot silently change an icon's weight.
   */
  paint: 'outline' | 'solid';
}

/**
 * Icons for the built-in code-block actions.
 *
 * `copy` is the copy button's only state — the button does not swap its icon
 * after a successful copy; `fullscreen` is the host-owned fullscreen action
 * (React Native only).
 */
export const CODE_ACTION_ICONS: Record<'copy' | 'fullscreen', SupramarkCodeActionIcon> = {
  // Two offset rounded sheets: the back sheet's top-right corner is omitted
  // where the front sheet overlaps it, so the outline stays legible when the
  // icon is drawn small.
  copy: {
    path: 'M8 8V6a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-2M6 8h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2z',
    paint: 'outline',
  },
  // Four corner brackets opening outwards — the conventional "go fullscreen"
  // gesture, as opposed to "expand" arrows which read as "zoom in".
  fullscreen: {
    path: 'M8 3H5a2 2 0 0 0-2 2v3M21 8V5a2 2 0 0 0-2-2h-3M3 16v3a2 2 0 0 0 2 2h3M16 21h3a2 2 0 0 0 2-2v-3',
    paint: 'outline',
  },
};

/** Identifiers of the icons the built-in code-block actions use. */
export type SupramarkCodeActionIconId = keyof typeof CODE_ACTION_ICONS;
