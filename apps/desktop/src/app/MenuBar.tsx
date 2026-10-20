import { useTranslation } from 'react-i18next';
import { useCampaignActions } from '@/shared/game/useCampaignActions';
import { useSettings, type Settings } from '@/shared/settings/settings';
import { Menu } from '@/shared/ui/Menu';

export function MenuBar({ breadcrumbs }: { breadcrumbs: string[] }) {
  const { t } = useTranslation();
  const { open, close } = useCampaignActions();
  const settings = useSettings();

  const option = <K extends keyof Settings>(key: K, value: Settings[K], label: string) => ({
    label,
    checked: settings[key] === value,
    onSelect: () => {
      settings.update({ [key]: value });
    },
  });

  return (
    <header className="menubar">
      <nav aria-label={t('menu.label')} className="menus">
        <Menu
          label={t('menu.file')}
          items={[
            {
              label: t('campaign.open'),
              onSelect: () => {
                open.mutate();
              },
            },
            'separator',
            {
              label: t('campaign.close'),
              onSelect: () => {
                close.mutate();
              },
            },
          ]}
        />
        <Menu
          label={t('menu.view')}
          items={[
            option('layoutPreset', 'gm', t('layout.gm')),
            option('layoutPreset', 'player', t('layout.player')),
            'separator',
            option('theme', 'system', t('theme.system')),
            option('theme', 'light', t('theme.light')),
            option('theme', 'dark', t('theme.dark')),
            'separator',
            option('language', 'system', t('language.system')),
            option('language', 'ru', 'Русский'),
            option('language', 'en', 'English'),
          ]}
        />
      </nav>
      <ol className="breadcrumbs" aria-label={t('menu.breadcrumbs')}>
        {breadcrumbs.map((crumb, n) => (
          <li key={`${n}-${crumb}`}>{crumb}</li>
        ))}
      </ol>
    </header>
  );
}
