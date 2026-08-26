/**
 * Video Feature examples
 *
 * @packageDocumentation
 */

import type { ExampleDefinition } from '@supramark/core';

export const videoExamples: ExampleDefinition[] = [
  {
    name: 'Video embed - basic',
    description: 'Embed a video with the minimal JSON config',
    markdown: `
:::video
{
  "src": "https://example.com/demo.mp4"
}
:::
`.trim(),
  },
  {
    name: 'Video embed - poster and title',
    description: 'Show a thumbnail before playback and a caption below the player',
    markdown: `
:::video
{
  "src": "https://example.com/demo.mp4",
  "poster": "https://example.com/cover.jpg",
  "title": "Product demo"
}
:::
`.trim(),
  },
  {
    name: 'Video embed - autoplay muted loop',
    description:
      'Configure autoplay with muted and loop for an ambient clip (browsers require muted for autoplay)',
    markdown: `
:::video
{
  "src": "https://example.com/ambient.mp4",
  "autoplay": true,
  "muted": true,
  "loop": true,
  "controls": false
}
:::
`.trim(),
  },
  {
    name: 'Video embed - narrow width',
    description: 'Constrain the player to a percentage of the container width',
    markdown: `
:::video
{
  "src": "https://example.com/demo.mp4",
  "width": 60
}
:::
`.trim(),
  },
  {
    name: 'Video embed - invalid JSON shows an error card',
    description: 'An unparsable body renders an inline error instead of failing the document',
    markdown: `
:::video
{invalid json}
:::
`.trim(),
  },
];
