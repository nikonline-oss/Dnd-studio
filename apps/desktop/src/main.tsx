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
if (root) {
  createRoot(root).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </StrictMode>,
  );
}
