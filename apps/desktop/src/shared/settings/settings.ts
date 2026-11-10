import { z } from 'zod';
import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';

export const settingsSchema = z.object({
  pluginEffects: z.boolean(),
  pluginEffectOpacity: z.number().min(0).max(1),
  theme: z.enum(['system', 'light', 'dark']),
  themePluginId: z.string().min(1).max(256).nullable(),
  language: z.enum(['system', 'ru', 'en']),
  /** Panel preset (CLAUDE.md §5): `player` = map + chat + character sheet, other panels hidden. */
  layoutPreset: z.enum(['gm', 'player']),
  /** Plugin catalog: the directory URL that holds `index.json` and its signature (ADR-0060). */
  catalogUrl: z.string().max(500),
  /** The catalog's Ed25519 public key, 64 hex characters. Empty means "catalog not set up". */
  catalogKey: z.string().max(64),
  /** The view open in the side panel (Activity Bar). */
  sideView: z.enum([
    'characters',
    'library',
    'plugins',
    'boards',
    'triggers',
    'compendia',
    'items',
    'skills',
    'catalog',
  ]),
});

export type SideView = Settings['sideView'];

export type Settings = z.infer<typeof settingsSchema>;

export const defaultSettings: Settings = {
  pluginEffects: true,
  pluginEffectOpacity: 0.8,
  theme: 'system',
  themePluginId: null,
  language: 'system',
  layoutPreset: 'gm',
  sideView: 'boards',
  catalogUrl: '',
  catalogKey: '',
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
      partialize: ({
        pluginEffects,
        pluginEffectOpacity,
        theme,
        themePluginId,
        language,
        layoutPreset,
        sideView,
      }) => ({
        pluginEffects,
        pluginEffectOpacity,
        theme,
        themePluginId,
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
