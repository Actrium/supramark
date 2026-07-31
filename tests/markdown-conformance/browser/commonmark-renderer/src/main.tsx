import React, { useCallback, useRef } from 'react';
import { createRoot } from 'react-dom/client';
import { Supramark } from '@supramark/web-production';

type RenderRequest = {
  id: string;
  markdown: string;
  ast: unknown;
  gfmTagfilter?: boolean;
  gfmFootnoteStyle?: boolean;
  flattenNestedStrong?: boolean;
};

type RenderResponse = {
  html: string;
  errors: string[];
};

declare global {
  interface Window {
    renderSupramarkCase: (request: RenderRequest) => Promise<RenderResponse>;
  }
}

const mount = document.querySelector('#actual');
if (!(mount instanceof HTMLElement)) {
  throw new Error('Missing #actual renderer mount');
}

const reactRoot = createRoot(mount);

function RenderCase({
  request,
  resolve,
}: {
  request: RenderRequest;
  resolve: (response: RenderResponse) => void;
}) {
  const errors = useRef<string[]>([]);
  const settled = useRef(false);

  const finish = useCallback(() => {
    if (settled.current) return;
    settled.current = true;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        const rendererRoot = mount.querySelector('.commonmark-production-root');
        resolve({
          html: rendererRoot?.innerHTML ?? '',
          errors: [...errors.current],
        });
      });
    });
  }, [resolve]);

  const onRenderStateChange = useCallback(
    (state: { pending: boolean }) => {
      if (!state.pending) finish();
    },
    [finish]
  );

  const onError = useCallback((error: Error) => {
    errors.current.push(error.stack ?? error.message);
  }, []);

  return (
    <Supramark
      markdown={request.markdown}
      ast={request.ast as never}
      classNames={{ root: 'commonmark-production-root' }}
      config={{
        options: {
          allowDangerousHtml: true,
          ...(request.gfmTagfilter ? { gfmTagfilter: true } : {}),
          ...(request.gfmFootnoteStyle ? { gfmFootnoteStyle: true } : {}),
          ...(request.flattenNestedStrong ? { flattenNestedStrong: true } : {}),
        },
      }}
      onError={onError}
      onRenderStateChange={onRenderStateChange}
    />
  );
}

window.renderSupramarkCase = request =>
  new Promise(resolve => {
    reactRoot.render(<RenderCase key={request.id} request={request} resolve={resolve} />);
  });
