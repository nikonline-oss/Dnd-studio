import { renderHook } from '@testing-library/react';
import { act } from 'react';
import { afterEach, describe, expect, it } from 'vitest';
import { defaultSettings, parseSettings, useSettings } from './settings';
import { useThemeSync } from '@/shared/theme/useThemeSync';

afterEach(() => {
  useSettings.setState(defaultSettings);
  delete document.documentElement.dataset.theme;
});

describe('settings', () => {
  it('keeps only valid fields from storage', () => {
    expect(parseSettings({ theme: 'dark', language: 'klingon', layoutPreset: 7 })).toEqual({
      theme: 'dark',
    });
    expect(parseSettings('garbage')).toEqual({});
    expect(parseSettings(null)).toEqual({});
  });

  it('theme preference drives data-theme', () => {
    renderHook(() => {
      useThemeSync();
    });
    expect(document.documentElement.dataset.theme).toBeUndefined();

    act(() => {
      useSettings.getState().update({ theme: 'dark' });
    });
    expect(document.documentElement.dataset.theme).toBe('dark');

    act(() => {
      useSettings.getState().update({ theme: 'system' });
    });
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });
});
