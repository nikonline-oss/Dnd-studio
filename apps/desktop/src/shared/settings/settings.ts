import { z } from 'zod';
import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';

export const settingsSchema = z.object({
  theme: z.enum(['system', 'light', 'dark']),
  language: z.enum(['system', 'ru', 'en']),
  /** Panel preset (CLAUDE.md §5): `player` = map + chat + character sheet, other panels hidden. */
  layoutPreset: z.enum(['gm', 'player']),
  /** The view open in the side panel (Activity Bar). */
  sideView: z.enum([
    'maps',
    'characters',
    'journal',
    'plugins',
    'boards',
    'triggers',
    'images',
    'audio',
    'compendia',
  ]),
});

export type SideView = Settings['sideView'];

export type Settings = z.infer<typeof settingsSchema>;

export const defaultSettings: Settings = {
  theme: 'system',
  language: 'system',
  layoutPreset: 'gm',
  sideView: 'maps',
};

interface SettingsState extends Settings {
  update: (patch: Partial<Settings>) => void;
}

/**
 * Per-device UI preferences. Persisted in webview storage; anything that fails validation
 * (manual edits, older versions) falls back to defaults field by field.
 */
export const useSettings = create<SettingsState>()(
  persist(
    (set) => ({
      ...defaultSettings,
      update: (patch) => {
        set(patch);
      },
    }),
    {
      name: 'dndstudio.settings',
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: ({ theme, language, layoutPreset, sideView }) => ({
        theme,
        language,
        layoutPreset,
        sideView,
      }),
      merge: (persisted, current) => ({ ...current, ...parseSettings(persisted) }),
    },
  ),
);

/** Keeps valid fields of untrusted stored data. */
export function parseSettings(value: unknown): Partial<Settings> {
  if (typeof value !== 'object' || value === null) return {};
  const result: Partial<Settings> = {};
  const shape = settingsSchema.shape;
  for (const key of Object.keys(shape) as (keyof Settings)[]) {
    const parsed = shape[key].safeParse((value as Record<string, unknown>)[key]);
    if (parsed.success) Object.assign(result, { [key]: parsed.data });
  }
  return result;
}
