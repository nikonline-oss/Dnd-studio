import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from '@/app/App';
import type { Role } from '@/shared/ipc/client';
import '@/shared/i18n';
import '@/app/styles.css';

const queryClient = new QueryClient({
  defaultOptions: {
    // Data comes from the local process and is pushed via events; no polling or refetch storms.
    queries: { staleTime: Infinity, refetchOnWindowFocus: false, retry: false },
    mutations: { retry: false },
  },
});

function renderApp(root: HTMLElement) {
  createRoot(root).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </StrictMode>,
  );
}

const root = document.getElementById('root');
const params = new URLSearchParams(location.search);
// Dev-only pages; `import.meta.env.DEV` is false in builds, so these branches are dropped.
if (root && import.meta.env.DEV && params.get('spike') === 'pixi') {
  // Render spike (ADR-0017).
  void import('./dev/pixi-spike/mount').then(({ mountPixiSpike }) => {
    mountPixiSpike(root);
  });
} else if (root && import.meta.env.DEV && params.has('mock')) {
  // The app in a plain browser against a mocked Rust side: `?mock=gm`, `?mock=player`, or
  // `?mock=start` for the start screen; add `&stress=1` for a big campaign.
  const mock = params.get('mock');
  const role: Role = mock === 'player' ? 'player' : 'gm';
  void import('./dev/mock-ipc/install').then(async ({ installMockIpc }) => {
    await installMockIpc(role, mock !== 'start', params.has('stress'));
    renderApp(root);
  });
} else if (root) {
  renderApp(root);
}
