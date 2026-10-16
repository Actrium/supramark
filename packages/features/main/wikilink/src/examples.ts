import type { ExampleDefinition } from '@supramark/core';

/**
 * WikiLink Feature usage examples.
 *
 * Rendered through the parser with `{ wikilink: true }` (or the feature
 * enabled in config, which turns the flag on).
 */
export const wikilinkExamples: ExampleDefinition[] = [
  {
    name: 'WikiLink',
    description: 'Shows the [[target]], [[target|label]] and [[target#section]] syntax.',
    markdown: `
# WikiLink Example

A plain wikilink: [[Project Plan]].

With a display label: [[Project Plan|the plan]].

With a heading fragment: [[Project Plan#Roadmap]], and both together:
[[Project Plan#Roadmap|the roadmap]].

Same-page fragment: [[#Q2-goals]].

Malformed forms degrade to literal text: [[]] and [[unclosed stay as-is.
    `.trim(),
  },
];
