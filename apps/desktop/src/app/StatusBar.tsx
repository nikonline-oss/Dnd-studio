import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { commands, type CampaignInfo } from '@/shared/ipc/client';
import { Icon } from '@/shared/ui/Icon';

/** Campaign, server status, active plugins, app version (CLAUDE.md §11). */
export function StatusBar({ campaign }: { campaign: CampaignInfo }) {
  const { t } = useTranslation();
  const { data: info } = useQuery({ queryKey: ['appInfo'], queryFn: commands.appInfo });

  return (
    <footer className="statusbar" aria-label={t('status.label')}>
      <span className="status-item" title={campaign.path}>
        <Icon name="book" size={13} />
        {campaign.name}
      </span>
      <span className="status-item">
        <span className="status-dot" data-state="local" aria-hidden="true" />
        {t('status.serverLocal')}
      </span>
      <span className="status-item">{t('status.plugins', { count: 0 })}</span>
      <span className="spacer" />
      <span className="status-item role" data-role={campaign.role}>
        {t(`roles.${campaign.role}`)}
      </span>
      {info && <span className="status-item">v{info.version}</span>}
    </footer>
  );
}
