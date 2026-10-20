import '@testing-library/jest-dom/vitest';
import { clearMocks } from '@tauri-apps/api/mocks';
import { cleanup } from '@testing-library/react';
import { afterEach } from 'vitest';
import i18n from '@/shared/i18n';

// jsdom has no layout engine; react-resizable-panels only needs the observer to exist.
globalThis.ResizeObserver = class {
  observe() {}
  unobserve() {}
  disconnect() {}
};

// Tests assert on English strings regardless of the machine locale.
void i18n.changeLanguage('en');

afterEach(async () => {
  cleanup();
  // Let unmount-time `unlisten` promises settle before the IPC mocks are removed.
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
});
