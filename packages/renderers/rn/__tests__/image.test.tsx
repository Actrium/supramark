import { describe, expect, it } from 'bun:test';
import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { SupramarkRootNode } from '@supramark/core';
import type { SupramarkImagePressEvent } from '../src/Supramark';

import './support/mock-react-native';
import './support/mock-renderer';

// react-test-renderer needs the act environment to flush effects synchronously.
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

const { Supramark } = await import('../src/Supramark');

/** Builds the canonical paragraph shape emitted by the Rust parser for an image. */
function imageAst(children: SupramarkRootNode['children'][number][]): SupramarkRootNode {
  return {
    type: 'root',
    ast_version: 2,
    diagnostics: [],
    children: [{ type: 'paragraph', children }],
  } as SupramarkRootNode;
}

/** Builds a document from explicit top-level nodes for consecutive-paragraph layout tests. */
function documentAst(children: SupramarkRootNode['children']): SupramarkRootNode {
  return { type: 'root', ast_version: 2, diagnostics: [], children };
}

/** Renders a pre-parsed AST and flushes the renderer's asynchronous document preparation. */
async function renderAst(
  ast: SupramarkRootNode,
  styles?: React.ComponentProps<typeof Supramark>['styles'],
  config?: React.ComponentProps<typeof Supramark>['config'],
  onImagePress?: React.ComponentProps<typeof Supramark>['onImagePress']
): Promise<ReactTestRenderer> {
  let renderer: ReactTestRenderer | null = null;
  await act(async () => {
    renderer = create(
      React.createElement(Supramark, { ast, markdown: '', styles, config, onImagePress })
    );
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
  return renderer as unknown as ReactTestRenderer;
}

/** Finds the native horizontal image-gallery scroll view. */
function findImageGallery(renderer: ReactTestRenderer): ReactTestRenderer['root'] {
  return renderer.root.findByType('ScrollView');
}

describe('image rendering', () => {
  it('renders a standalone image in a stable 200x200 container with cover sizing', async () => {
    const renderer = await renderAst(
      imageAst([{ type: 'image', url: 'https://example.com/photo.jpg', alt: 'photo' }])
    );

    const image = renderer.root.findByType('Image');
    expect(image.props.source).toEqual({ uri: 'https://example.com/photo.jpg' });
    expect(image.props.accessibilityLabel).toBe('photo');
    expect(image.props.style).toMatchObject({ width: '100%', height: '100%', resizeMode: 'cover' });
    expect(image.parent?.type).toBe('View');
    expect(image.parent?.props.style).toMatchObject({ width: 200, height: 200 });
    expect(image.parent?.props.style).toMatchObject({ borderRadius: 8, overflow: 'hidden' });
    expect(renderer.root.findAllByType('ScrollView')).toHaveLength(0);
    expect(renderer.root.findAllByType('Text')).toHaveLength(0);
  });

  it('allows the host to override the stable block-image container dimensions', async () => {
    const renderer = await renderAst(
      imageAst([{ type: 'image', url: 'https://example.com/photo.jpg', alt: 'photo' }]),
      { imageContainer: { width: 320, height: 180 } }
    );

    const image = renderer.root.findByType('Image');
    expect(image.parent?.props.style).toMatchObject({ width: 320, height: 180 });
  });

  it('allows the host to override block-image sizing from cover to contain', async () => {
    const renderer = await renderAst(
      imageAst([{ type: 'image', url: 'https://example.com/photo.jpg', alt: 'photo' }]),
      { image: { resizeMode: 'contain' } }
    );

    const image = renderer.root.findByType('Image');
    expect(image.props.style).toMatchObject({ resizeMode: 'contain' });
  });

  it('allows the host to override the gallery gap and image corner radius', async () => {
    const renderer = await renderAst(
      imageAst([
        { type: 'image', url: 'https://example.com/a.jpg', alt: 'a' },
        { type: 'image', url: 'https://example.com/b.jpg', alt: 'b' },
      ]),
      {
        imageGallery: { gap: 12 },
        imageContainer: { height: 180, borderRadius: 16 },
      }
    );

    const image = renderer.root.findAllByType('Image')[0];
    expect(findImageGallery(renderer).props.style).toMatchObject({ height: 180 });
    expect(findImageGallery(renderer).props.contentContainerStyle).toMatchObject({ gap: 12 });
    expect(image.parent?.props.style).toMatchObject({ borderRadius: 16 });
  });

  it('lays out multiple images from one image-only paragraph in a horizontally scrolling row', async () => {
    const renderer = await renderAst(
      imageAst([
        { type: 'image', url: 'https://example.com/a.jpg', alt: 'a' },
        { type: 'text', value: ' ' },
        { type: 'image', url: 'https://example.com/b.jpg', alt: 'b' },
      ])
    );

    const images = renderer.root.findAllByType('Image');
    const gallery = findImageGallery(renderer);
    expect(images).toHaveLength(2);
    expect(gallery.props.horizontal).toBe(true);
    expect(gallery.props.directionalLockEnabled).toBe(true);
    expect(gallery.props.nestedScrollEnabled).toBe(true);
    expect(gallery.props.style).toMatchObject({ height: 200, overflow: 'hidden' });
    expect(gallery.props.contentContainerStyle).toMatchObject({
      flexDirection: 'row',
      gap: 8,
      alignSelf: 'flex-start',
      flexShrink: 0,
    });
    expect(gallery.props.onMoveShouldSetPanResponder).toBeUndefined();
  });

  it('groups consecutive image-only paragraphs and stops before normal content', async () => {
    const renderer = await renderAst(
      documentAst([
        {
          type: 'paragraph',
          children: [{ type: 'image', url: 'https://example.com/a.jpg', alt: 'a' }],
        },
        {
          type: 'paragraph',
          children: [{ type: 'image', url: 'https://example.com/b.jpg', alt: 'b' }],
        },
        { type: 'paragraph', children: [{ type: 'text', value: 'after' }] },
      ])
    );

    const images = renderer.root.findAllByType('Image');
    const gallery = findImageGallery(renderer);
    expect(images).toHaveLength(2);
    expect(gallery.findAllByType('Image')).toHaveLength(2);
    expect(renderer.root.findAllByType('Text')).toHaveLength(1);
  });

  it('keeps an image mixed with text inline without changing the paragraph structure', async () => {
    const renderer = await renderAst(
      imageAst([
        { type: 'text', value: 'before ' },
        { type: 'image', url: 'https://example.com/icon.png', alt: 'icon' },
        { type: 'text', value: ' after' },
      ])
    );

    const image = renderer.root.findByType('Image');
    expect(image.parent?.type).toBe('Text');
    expect(image.props.style).toMatchObject({ width: 20, height: 20, resizeMode: 'cover' });
  });

  it('keeps a standalone linked image in the same stable block container', async () => {
    const renderer = await renderAst(
      imageAst([
        {
          type: 'link',
          url: 'https://example.com/article',
          children: [{ type: 'image', url: 'https://example.com/photo.jpg', alt: 'linked photo' }],
        },
      ])
    );

    const image = renderer.root.findByType('Image');
    expect(image.parent?.type).toBe('View');
    expect(image.parent?.props.style).toMatchObject({ width: 200, height: 200 });
    expect(image.parent?.parent?.type).toBe('TouchableOpacity');
    expect(typeof image.parent?.parent?.props.onPress).toBe('function');
  });

  // Fix 1: list images must reach the stable block container, not 20x20 inlines
  it('renders a tight list image as a stable block gallery, not a 20×20 inline', async () => {
    const renderer = await renderAst({
      type: 'root',
      ast_version: 2,
      diagnostics: [],
      children: [
        {
          type: 'list',
          ordered: false,
          tight: true,
          children: [
            {
              type: 'list_item',
              children: [{ type: 'image', url: 'https://example.com/a.jpg', alt: 'a' }],
            },
          ],
        },
      ],
    } as unknown as SupramarkRootNode);

    const image = renderer.root.findByType('Image');
    expect(image.parent?.type).toBe('View');
    expect(image.parent?.props.style).toMatchObject({ width: 200, height: 200 });
  });

  it('renders a loose list first-paragraph image as a stable block gallery', async () => {
    const renderer = await renderAst({
      type: 'root',
      ast_version: 2,
      diagnostics: [],
      children: [
        {
          type: 'list',
          ordered: false,
          children: [
            {
              type: 'list_item',
              children: [
                {
                  type: 'paragraph',
                  children: [{ type: 'image', url: 'https://example.com/a.jpg', alt: 'a' }],
                },
              ],
            },
          ],
        },
      ],
    } as unknown as SupramarkRootNode);

    const image = renderer.root.findByType('Image');
    expect(image.parent?.type).toBe('View');
    expect(image.parent?.props.style).toMatchObject({ width: 200, height: 200 });
  });

  // Fix 2: hard break must produce the same gallery as a soft break
  it('groups two images split by a hard break into one gallery (same as soft break)', async () => {
    const renderer = await renderAst(
      imageAst([
        { type: 'image', url: 'https://example.com/a.jpg', alt: 'a' },
        { type: 'break' },
        { type: 'image', url: 'https://example.com/b.jpg', alt: 'b' },
      ])
    );

    const gallery = findImageGallery(renderer);
    expect(gallery.findAllByType('Image')).toHaveLength(2);
    expect(gallery.props.horizontal).toBe(true);
  });

  // Fix 3: disabled footnote must not nest a View under <Text>
  it('does not nest a View under Text when footnote is disabled and content is image-only', async () => {
    const renderer = await renderAst(
      {
        type: 'root',
        ast_version: 2,
        diagnostics: [],
        children: [
          {
            type: 'footnote_definition',
            index: 1,
            children: [
              {
                type: 'paragraph',
                children: [{ type: 'image', url: 'https://example.com/a.jpg', alt: 'a' }],
              },
            ],
          },
        ],
      } as unknown as SupramarkRootNode,
      undefined,
      { features: [{ id: '@supramark/feature-footnote', enabled: false }] }
    );

    const image = renderer.root.findByType('Image');
    expect(image.parent?.props.style).toMatchObject({ width: 200, height: 200 });
    // No <Text> anywhere in the tree means no View-in-Text invariant break.
    expect(renderer.root.findAllByType('Text')).toHaveLength(0);
  });

  // Fix 4: load failure / unusable URL must show a placeholder, not a blank box
  it('shows a placeholder for an empty URL instead of a blank image source', async () => {
    const renderer = await renderAst(imageAst([{ type: 'image', url: '', alt: 'missing' }]));

    expect(renderer.root.findAllByType('Image')).toHaveLength(0);
    expect(renderer.root.findByType('Text').props.children).toBe('missing');
  });

  it('shows a placeholder for a relative URL that RN cannot load', async () => {
    const renderer = await renderAst(imageAst([{ type: 'image', url: './photo.jpg', alt: 'rel' }]));

    expect(renderer.root.findAllByType('Image')).toHaveLength(0);
    expect(renderer.root.findByType('Text').props.children).toBe('rel');
  });

  it('switches to the placeholder when the image fails to load', async () => {
    const renderer = await renderAst(
      imageAst([{ type: 'image', url: 'https://example.com/broken.jpg', alt: 'broken' }])
    );

    expect(renderer.root.findByType('Image')).toBeTruthy();
    await act(async () => {
      renderer.root.findByType('Image').props.onError();
    });
    expect(renderer.root.findAllByType('Image')).toHaveLength(0);
    expect(renderer.root.findByType('Text').props.children).toBe('broken');
  });

  // --- nit2/nit3: accessibility label fallback + decorative image hiding ---
  it('marks an image with empty alt and no title as decorative for screen readers', async () => {
    const renderer = await renderAst(
      imageAst([{ type: 'image', url: 'https://example.com/deco.jpg', alt: '' }])
    );
    const image = renderer.root.findByType('Image');
    expect(image.props.accessibilityElementsHidden).toBe(true);
    expect(image.props.importantForAccessibility).toBe('no-hide-descendants');
    expect(image.props.accessibilityLabel).toBeUndefined();
  });

  it('falls back to image title for the accessibility label when alt is empty', async () => {
    const renderer = await renderAst(
      imageAst([{ type: 'image', url: 'https://example.com/titled.jpg', alt: '', title: 'A title' }])
    );
    expect(renderer.root.findByType('Image').props.accessibilityLabel).toBe('A title');
  });

  // --- nit4: explicit viewport height must win over the imageContainer fallback ---
  it('respects an explicit imageGalleryViewport height over the imageContainer fallback', async () => {
    const renderer = await renderAst(
      imageAst([
        { type: 'image', url: 'https://example.com/a.jpg', alt: 'a' },
        { type: 'image', url: 'https://example.com/b.jpg', alt: 'b' },
      ]),
      { imageGalleryViewport: { height: 120 } }
    );

    expect(findImageGallery(renderer).props.style).toMatchObject({ height: 120 });
  });

  // --- onImagePress: host-supplied image tap callback ---
  it('delegates a standalone image tap to the host onImagePress handler', async () => {
    const calls: SupramarkImagePressEvent[] = [];
    const renderer = await renderAst(
      imageAst([{ type: 'image', url: 'https://example.com/a.jpg', alt: 'a' }]),
      undefined,
      undefined,
      event => calls.push(event)
    );

    const touchable = renderer.root.findByType('TouchableOpacity');
    expect(touchable).toBeTruthy();
    await act(async () => {
      touchable.props.onPress();
    });
    expect(calls).toEqual([
      {
        url: 'https://example.com/a.jpg',
        alt: 'a',
        galleryImages: [{ url: 'https://example.com/a.jpg', alt: 'a' }],
        galleryIndex: 0,
      },
    ]);
  });

  it('delegates an image-link tap to onImagePress with the linkUrl, instead of Linking', async () => {
    const calls: SupramarkImagePressEvent[] = [];
    const renderer = await renderAst(
      imageAst([
        {
          type: 'link',
          url: 'https://example.com/article',
          children: [{ type: 'image', url: 'https://example.com/photo.jpg', alt: 'linked' }],
        },
      ]),
      undefined,
      undefined,
      event => calls.push(event)
    );

    const touchable = renderer.root.findByType('TouchableOpacity');
    await act(async () => {
      touchable.props.onPress();
    });
    expect(calls).toEqual([
      {
        url: 'https://example.com/photo.jpg',
        alt: 'linked',
        linkUrl: 'https://example.com/article',
        galleryImages: [{ url: 'https://example.com/photo.jpg', alt: 'linked' }],
        galleryIndex: 0,
      },
    ]);
  });

  it('leaves a standalone image non-tappable when no onImagePress handler is supplied', async () => {
    const renderer = await renderAst(
      imageAst([{ type: 'image', url: 'https://example.com/a.jpg', alt: 'a' }])
    );

    expect(renderer.root.findAllByType('TouchableOpacity')).toHaveLength(0);
  });

  it('carries the adjacent gallery group and the tapped index in onImagePress', async () => {
    const calls: SupramarkImagePressEvent[] = [];
    const renderer = await renderAst(
      imageAst([
        { type: 'image', url: 'https://example.com/a.jpg', alt: 'a' },
        { type: 'image', url: 'https://example.com/b.jpg', alt: 'b' },
      ]),
      undefined,
      undefined,
      event => calls.push(event)
    );

    const touchables = renderer.root.findAllByType('TouchableOpacity');
    expect(touchables).toHaveLength(2);
    await act(async () => {
      touchables[1].props.onPress();
    });
    expect(calls).toHaveLength(1);
    expect(calls[0].url).toBe('https://example.com/b.jpg');
    expect(calls[0].galleryIndex).toBe(1);
    expect(calls[0].galleryImages).toEqual([
      { url: 'https://example.com/a.jpg', alt: 'a' },
      { url: 'https://example.com/b.jpg', alt: 'b' },
    ]);
  });

  // --- #217: mixed-content inline images reuse the block path's loadability rule ---
  it('shows alt text instead of a blank 20x20 inline image for a relative URL in mixed content', async () => {
    const renderer = await renderAst(
      imageAst([
        { type: 'text', value: 'icon: ' },
        { type: 'image', url: './icon.png', alt: 'local icon' },
      ])
    );

    expect(renderer.root.findAllByType('Image')).toHaveLength(0);
    const texts = renderer.root.findAllByType('Text');
    expect(texts.some(t => t.props.children === 'local icon')).toBe(true);
  });

  it('shows placeholder text for an empty URL in a heading image', async () => {
    const renderer = await renderAst(
      documentAst([
        {
          type: 'heading',
          depth: 2,
          children: [
            { type: 'text', value: 'H ' },
            { type: 'image', url: '', alt: 'missing icon' },
          ],
        },
      ]) as SupramarkRootNode
    );

    expect(renderer.root.findAllByType('Image')).toHaveLength(0);
    expect(renderer.root.findAllByType('Text').some(t => t.props.children === 'missing icon')).toBe(
      true
    );
  });

  // --- #217: the accessible element is the TouchableOpacity wrapper, not the inner image ---
  it('carries the accessibility label on the link wrapper and not on the inner image', async () => {
    const renderer = await renderAst(
      imageAst([
        {
          type: 'link',
          url: 'https://example.com/article',
          children: [{ type: 'image', url: 'https://example.com/photo.jpg', alt: 'linked photo' }],
        },
      ])
    );

    const touchable = renderer.root.findByType('TouchableOpacity');
    expect(touchable.props.accessibilityLabel).toBe('linked photo');
    expect(touchable.props.accessibilityElementsHidden).toBe(false);
    const image = renderer.root.findByType('Image');
    expect(image.props.accessible).toBe(false);
    expect(image.props.accessibilityLabel).toBeUndefined();
  });

  it('carries the accessibility label on the onImagePress wrapper for standalone images', async () => {
    const renderer = await renderAst(
      imageAst([{ type: 'image', url: 'https://example.com/a.jpg', alt: 'solo' }]),
      undefined,
      undefined,
      () => {}
    );

    const touchable = renderer.root.findByType('TouchableOpacity');
    expect(touchable.props.accessibilityLabel).toBe('solo');
    expect(renderer.root.findByType('Image').props.accessible).toBe(false);
  });

  // The link wrapper is the actionable control (opens the article), so it
  // must stay in the accessibility tree even for a decorative image —
  // hiding it would make the link unreachable. The link URL becomes the
  // fallback accessible name.
  it('keeps a decorative linked image wrapper reachable, labeled by the link URL', async () => {
    const renderer = await renderAst(
      imageAst([
        {
          type: 'link',
          url: 'https://example.com/article',
          children: [{ type: 'image', url: 'https://example.com/photo.jpg', alt: '' }],
        },
      ])
    );

    const touchable = renderer.root.findByType('TouchableOpacity');
    expect(touchable.props.accessibilityElementsHidden).toBe(false);
    expect(touchable.props.importantForAccessibility).toBe('yes');
    expect(touchable.props.accessibilityLabel).toBe('https://example.com/article');
    // The inner image is still not individually focusable.
    expect(renderer.root.findByType('Image').props.accessible).toBe(false);
  });

  it('hides an unwrapped decorative image from screen readers', async () => {
    const renderer = await renderAst(
      imageAst([{ type: 'image', url: 'https://example.com/photo.jpg', alt: '' }])
    );

    const image = renderer.root.findByType('Image');
    expect(image.props.accessibilityElementsHidden).toBe(true);
    expect(image.props.importantForAccessibility).toBe('no-hide-descendants');
  });

  // --- #217: gallery images are keyed by index, so a URL change at the same
  // index must not inherit the previous URL's failure state ---
  it('resets the failure state when the image URL changes at the same gallery index', async () => {
    const renderer = await renderAst(
      imageAst([
        { type: 'image', url: 'https://example.com/a.jpg', alt: 'a' },
        { type: 'image', url: 'https://example.com/broken.jpg', alt: 'broken' },
      ])
    );

    const secondImage = renderer.root.findAllByType('Image')[1];
    await act(async () => {
      secondImage.props.onError();
    });
    expect(renderer.root.findAllByType('Image')).toHaveLength(1);

    await act(async () => {
      renderer.update(
        React.createElement(Supramark, {
          ast: imageAst([
            { type: 'image', url: 'https://example.com/a.jpg', alt: 'a' },
            { type: 'image', url: 'https://example.com/fresh.jpg', alt: 'fresh' },
          ]),
          markdown: '',
        })
      );
      await Promise.resolve();
      await Promise.resolve();
    });

    const images = renderer.root.findAllByType('Image');
    expect(images).toHaveLength(2);
    expect(images[1].props.source).toEqual({ uri: 'https://example.com/fresh.jpg' });
  });
});
