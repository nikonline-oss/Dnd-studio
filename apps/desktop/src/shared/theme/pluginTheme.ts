import { useEffect } from 'react';
import { z } from 'zod';
import { useSettings } from '@/shared/settings/settings';

/** Exact token whitelist shared with the theme package contract; never arbitrary CSS. */
export const THEME_COLORS = [
  '--color-bg',
  '--color-surface',
  '--color-surface-alt',
  '--color-surface-raised',
  '--color-hover',
  '--color-border',
  '--color-border-strong',
  '--color-text',
  '--color-text-muted',
  '--color-accent',
  '--color-accent-soft',
  '--color-accent-contrast',
  '--color-danger',
  '--color-danger-soft',
  '--color-success',
  '--color-focus',
] as const;
const colors = new Set<string>(THEME_COLORS);
const hex = /^#(?:[\da-f]{3}|[\da-f]{6}|[\da-f]{8})$/i;
const palette = z.record(
  z.string().refine((key) => colors.has(key)),
  z.string().refine((value) => [4, 7, 9].includes(value.length) && hex.test(value)),
);
const themeSchema = z
  .object({ light: palette.default({}), dark: palette.default({}) })
  .strict()
  .refine((value) => Object.keys(value.light).length + Object.keys(value.dark).length > 0);
export type ThemePalette = z.infer<typeof themeSchema>;
export function parseThemePalette(value: unknown): ThemePalette | null {
  const result = themeSchema.safeParse(value);
  return result.success ? result.data : null;
}

/** All applied properties are scoped to this effect and restored on switch/unmount. */
export function usePluginPalette(value: unknown) {
  const mode = useSettings((s) => s.theme);
  useEffect(() => {
    const theme = parseThemePalette(value);
    if (!theme) return;
    const root = document.documentElement;
    const media =
      typeof window.matchMedia === 'function'
        ? window.matchMedia('(prefers-color-scheme: dark)')
        : null;
    let restore: (() => void) | null = null;
    const apply = () => {
      restore?.();
      const variant = mode === 'system' ? (media?.matches ? 'dark' : 'light') : mode;
      const before = Object.keys(theme[variant]).map((key) => ({
        key,
        value: root.style.getPropertyValue(key),
        priority: root.style.getPropertyPriority(key),
      }));
      for (const [key, color] of Object.entries(theme[variant])) root.style.setProperty(key, color);
      restore = () => {
        for (const old of before) {
          if (old.value) root.style.setProperty(old.key, old.value, old.priority);
          else root.style.removeProperty(old.key);
        }
      };
    };
    apply();
    if (mode === 'system') media?.addEventListener('change', apply);
    return () => {
      media?.removeEventListener('change', apply);
      restore?.();
    };
  }, [value, mode]);
}
