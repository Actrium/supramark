/**
 * Context pipe for the host's video-press handler.
 *
 * Keeping this callback in the RN renderer package avoids adding a
 * video-specific field to core's feature-neutral ContainerRNRenderArgs.
 */

import { createContext, useContext } from 'react';
import type { SupramarkVideoPressEvent } from '@supramark/core';

/** Host handler invoked when the user taps a video card. */
export type SupramarkVideoPressHandler = (event: SupramarkVideoPressEvent) => void;

/** Carries the optional handler from the Supramark root to video cards. */
export const VideoPressContext = createContext<SupramarkVideoPressHandler | undefined>(undefined);

/** Reads the host-supplied handler from a React component. */
export function useVideoPressHandler(): SupramarkVideoPressHandler | undefined {
  return useContext(VideoPressContext);
}
