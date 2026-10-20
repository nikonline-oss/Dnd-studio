import '@testing-library/jest-dom/vitest';
import { clearMocks } from '@tauri-apps/api/mocks';
import { cleanup } from '@testing-library/react';
import { afterEach } from 'vitest';
import i18n from '@/shared/i18n';

// Tests assert on English strings regardless of the machine locale.
void i18n.changeLanguage('en');

afterEach(() => {
  cleanup();
  clearMocks();
});
