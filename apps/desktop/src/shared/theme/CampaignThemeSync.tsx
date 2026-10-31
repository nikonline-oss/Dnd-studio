import { z } from 'zod';
import { useSnapshotSelect } from '@/shared/game/useGame';
import { useSettings } from '@/shared/settings/settings';
import { parseThemePalette, usePluginPalette } from './pluginTheme';
import type { PluginDto } from '@/shared/ipc/client';

const themePlugin = z.object({ kind: z.literal('theme'), theme: z.unknown() });
export function pluginPalette(plugin: PluginDto | undefined) {
  const parsed = themePlugin.safeParse(plugin);
  return parsed.success ? parseThemePalette(parsed.data.theme) : null;
}
/** Mount only for an open campaign, under QueryClientProvider. */
export function CampaignThemeSync() {
  const selected = useSettings((s) => s.themePluginId);
  const plugin = useSnapshotSelect((s) => s.plugins.find((p) => p.id === selected));
  // The snapshot reference is structurally shared; pass it through the effect unchanged.
  const parsed = themePlugin.safeParse(plugin);
  usePluginPalette(parsed.success ? parsed.data.theme : null);
  return null;
}
