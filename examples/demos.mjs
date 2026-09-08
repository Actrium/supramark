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

// Aggregate all examples and attach an id field
export const DEMOS = [
  ...coreMarkdownExamples.map((ex, idx) => ({ ...ex, id: `core-${idx}` })),
  ...mathExamples.map(ex => ({ ...ex, id: 'math' })),
  ...gfmExamples.map(ex => ({ ...ex, id: 'gfm' })),
  ...admonitionExamples.map(ex => ({ ...ex, id: 'admonition' })),
  ...definitionListExamples.map(ex => ({ ...ex, id: 'definition-list' })),
  ...emojiExamples.map(ex => ({ ...ex, id: 'emoji' })),
  ...footnoteExamples.map(ex => ({ ...ex, id: 'footnote' })),
  ...htmlPageExamples.map(ex => ({ ...ex, id: 'html-page' })),
  ...mapExamples.map(ex => ({ ...ex, id: 'map' })),
  ...videoExamples.map(ex => ({ ...ex, id: 'video' })),
];
