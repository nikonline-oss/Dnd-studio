import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { defaultSettings, parseSettings, useSettings } from '@/shared/settings/settings';
import { parseThemePalette, usePluginPalette } from './pluginTheme';

const palette = {
  light: { '--color-bg': '#abcdef', '--color-accent': '#123' },
  dark: { '--color-bg': '#123456' },
};
afterEach(() => {
  useSettings.setState(defaultSettings);
  document.documentElement.removeAttribute('style');
  vi.restoreAllMocks();
});
describe('safe plugin palettes', () => {
  it('rejects unknown tokens, URLs, CSS expressions and invalid hex', () => {
    expect(parseThemePalette(palette)).toEqual(palette);
    for (const light of [
      { '--font-ui': '#123' },
      { '--color-madeup': '#123' },
      { '--color-bg': 'url(https://bad)' },
      { '--color-bg': 'var(--other)' },
      { '--color-bg': '#12' },
      { '--color-bg': '#abcd' },
      { '--color-bg': '#abc\n' },
    ]) {
      expect(parseThemePalette({ ...palette, light })).toBeNull();
    }
  });
  it('keeps previous settings valid and persists a nullable local selection', () => {
    expect(parseSettings({ theme: 'dark' })).toEqual({ theme: 'dark' });
    expect(parseSettings({ themePluginId: null })).toEqual({ themePluginId: null });
    expect(parseSettings({ themePluginId: 'my-theme' })).toEqual({ themePluginId: 'my-theme' });
    act(() => {
      useSettings.getState().update({ themePluginId: 'my-theme' });
    });
    const persisted: unknown = JSON.parse(localStorage.getItem('dndstudio.settings') ?? '{}');
    expect(persisted).toMatchObject({ state: { themePluginId: 'my-theme' } });
  });
  it('switches variants, removes absent tokens and restores defaults on unmount', () => {
    useSettings.setState({ theme: 'light' });
    const initialProps: { value: unknown } = { value: palette };
    const { rerender, unmount } = renderHook(
      ({ value }: { value: unknown }) => {
        usePluginPalette(value);
      },
      { initialProps },
    );
    expect(document.documentElement.style.getPropertyValue('--color-bg')).toBe('#abcdef');
    act(() => {
      useSettings.getState().update({ theme: 'dark' });
    });
    expect(document.documentElement.style.getPropertyValue('--color-bg')).toBe('#123456');
    expect(document.documentElement.style.getPropertyValue('--color-accent')).toBe('');
    rerender({ value: null });
    expect(document.documentElement.style.getPropertyValue('--color-bg')).toBe('');
    rerender({ value: palette });
    unmount();
    expect(document.documentElement.style.getPropertyValue('--color-bg')).toBe('');
  });
  it('follows OS changes in system mode and removes its listener', () => {
    let dark = false;
    const listeners = new Set<() => void>();
    vi.stubGlobal('matchMedia', () => ({
      get matches() {
        return dark;
      },
      addEventListener: (_event: string, fn: () => void) => {
        listeners.add(fn);
      },
      removeEventListener: (_event: string, fn: () => void) => {
        listeners.delete(fn);
      },
    }));
    const { unmount } = renderHook(() => {
      usePluginPalette(palette);
    });
    expect(document.documentElement.style.getPropertyValue('--color-bg')).toBe('#abcdef');
    act(() => {
      dark = true;
      for (const fn of listeners) fn();
    });
    expect(document.documentElement.style.getPropertyValue('--color-bg')).toBe('#123456');
    unmount();
    expect(listeners.size).toBe(0);
    vi.unstubAllGlobals();
  });
});
