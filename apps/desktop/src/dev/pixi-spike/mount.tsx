import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { PixiSpike } from './PixiSpike';

export function mountPixiSpike(root: HTMLElement): void {
  createRoot(root).render(
    <StrictMode>
      <PixiSpike />
    </StrictMode>,
  );
}
