import type { ExampleDefinition } from '@supramark/core';

export const visonExamples: ExampleDefinition[] = [
  {
    name: 'Hello card',
    description: 'Minimal Vison card with a single text block.',
    markdown: [
      ':::vison',
      JSON.stringify(
        {
          version: '1',
          type: 'container',
          style: { padding: 12, backgroundColor: '#F5F5F5', borderRadius: 8 },
          children: [
            {
              type: 'text',
              props: { text: 'Hello Vison' },
              style: { fontSize: 16, fontWeight: 'bold' },
            },
          ],
        },
        null,
        2
      ),
      ':::',
    ].join('\n'),
  },
  {
    name: 'AI assistant card',
    description:
      'Realistic AI chat assistant card with avatar, divider, markdown body, and image.',
    markdown: [
      ':::vison',
      JSON.stringify(
        {
          version: '1',
          type: 'container',
          style: {
            padding: 16,
            backgroundColor: '#FFFFFF',
            borderRadius: 12,
            width: 340,
            gap: 12,
            borderWidth: 1,
            borderColor: '#E5E5E5',
          },
          children: [
            {
              type: 'container',
              style: { flexDirection: 'row', alignItems: 'center', gap: 8 },
              children: [
                {
                  type: 'image',
                  props: {
                    src: 'https://api.dicebear.com/7.x/bottts/svg?seed=vison',
                    width: 40,
                    aspectRatio: 1,
                  },
                  style: { borderRadius: 20, width: 40, height: 40 },
                },
                {
                  type: 'text',
                  props: { text: 'Vison Assistant' },
                  style: { fontSize: 16, fontWeight: '600', color: '#1A1A1A' },
                },
              ],
            },
            { type: 'divider', style: { margin: 4, borderColor: '#F0F0F0' } },
            {
              type: 'markdown',
              props: {
                content:
                  '### Deployment report\nService is live. Highlights:\n- **Performance**: +20%\n- **Security**: XSS hotfix shipped',
              },
              style: { fontSize: 14, color: '#4A4A4A' },
            },
          ],
        },
        null,
        2
      ),
      ':::',
    ].join('\n'),
  },
];
