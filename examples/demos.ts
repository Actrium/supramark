/**
 * Aggregates example data from every feature.
 * Each feature now owns its examples; this file only aggregates and re-exports them.
 */

import { mathExamples } from '@supramark/feature-math';
import { gfmExamples } from '@supramark/feature-gfm';
import { admonitionExamples } from '@supramark/feature-admonition';
import { definitionListExamples } from '@supramark/feature-definition-list';
import { emojiExamples } from '@supramark/feature-emoji';
import { footnoteExamples } from '@supramark/feature-footnote';
import { coreMarkdownExamples } from '@supramark/feature-core-markdown';
import { htmlPageExamples } from '@supramark/feature-html-page';
import { mapExamples } from '@supramark/feature-map';
import { videoExamples } from '@supramark/feature-video';

// Aggregate all examples and attach a unique id field
export const DEMOS = [
  ...coreMarkdownExamples.map((ex, idx) => ({ ...ex, id: `core-${idx}` })),
  ...mathExamples.map((ex, idx) => ({ ...ex, id: `math-${idx}` })),
  ...gfmExamples.map((ex, idx) => ({ ...ex, id: `gfm-${idx}` })),
  ...admonitionExamples.map((ex, idx) => ({ ...ex, id: `admonition-${idx}` })),
  ...definitionListExamples.map((ex, idx) => ({ ...ex, id: `definition-list-${idx}` })),
  ...emojiExamples.map((ex, idx) => ({ ...ex, id: `emoji-${idx}` })),
  ...footnoteExamples.map((ex, idx) => ({ ...ex, id: `footnote-${idx}` })),
  ...htmlPageExamples.map((ex, idx) => ({ ...ex, id: `html-page-${idx}` })),
  ...mapExamples.map((ex, idx) => ({ ...ex, id: `map-${idx}` })),
  ...videoExamples.map((ex, idx) => ({ ...ex, id: `video-${idx}` })),
];
