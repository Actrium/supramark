import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './index.css';
import { FeaturePreview } from './FeaturePreview.tsx';
import { DEFAULT_PLAYGROUND_FEATURE, featureFromPlaygroundPath } from './playground-routes';

const initialFeature =
  featureFromPlaygroundPath(window.location.pathname, import.meta.env.BASE_URL) ??
  DEFAULT_PLAYGROUND_FEATURE;

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <FeaturePreview initialFeature={initialFeature} />
  </StrictMode>
);
