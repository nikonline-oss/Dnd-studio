import { useTranslation } from 'react-i18next';
import { useSnapshotSelect } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import { useSettings } from '@/shared/settings/settings';
import { Menu } from '@/shared/ui/Menu';
import { pluginPalette } from './CampaignThemeSync';

/** A device preference accessible to every role; never submits an intent. */
export function ThemePluginMenu() {
  const { t, i18n } = useTranslation();
  const selected = useSettings((s) => s.themePluginId);
  const update = useSettings((s) => s.update);
  const plugins = useSnapshotSelect((s) => s.plugins) ?? [];
  const themes = plugins.filter((plugin) => pluginPalette(plugin));
  return (
    <Menu
      label={t('theme.palette', { defaultValue: 'Palette' })}
      items={[
        {
          label: t('theme.defaultPalette', { defaultValue: 'Default palette' }),
          checked: !themes.some((theme) => theme.id === selected),
          onSelect: () => {
            update({ themePluginId: null });
          },
        },
        ...themes.map((plugin) => ({
          label: labelText(plugin.name, i18n.resolvedLanguage ?? i18n.language),
          checked: selected === plugin.id,
          onSelect: () => {
            update({ themePluginId: plugin.id });
          },
        })),
      ]}
    />
  );
}
