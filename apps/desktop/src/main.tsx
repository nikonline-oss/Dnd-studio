import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from '@/app/App';
import '@/shared/i18n';
import '@/app/styles.css';

const queryClient = new QueryClient({
  defaultOptions: {
    // Data comes from the local process and is pushed via events; no polling or refetch storms.
    queries: { staleTime: Infinity, refetchOnWindowFocus: false, retry: false },
    mutations: { retry: false },
  },
});

const root = document.getElementById('root');
// Dev-only render spike (ADR-0017); `import.meta.env.DEV` is false in builds, so it is dropped.
if (root && import.meta.env.DEV && new URLSearchParams(location.search).get('spike') === 'pixi') {
  void import('./dev/pixi-spike/mount').then(({ mountPixiSpike }) => {
    mountPixiSpike(root);
  });
} else if (root) {
  createRoot(root).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </StrictMode>,
  );
}
