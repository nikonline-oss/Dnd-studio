import { useQuery } from '@tanstack/react-query';
import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';
import { useRemote } from '@/shared/game/useRemote';
import { commands, type CampaignInfo, type RemoteStatus } from '@/shared/ipc/client';
import { Icon } from '@/shared/ui/Icon';

/** Campaign, server status, active plugins, app version (CLAUDE.md §11). */
export function StatusBar({ campaign }: { campaign: CampaignInfo }) {
  const { t } = useTranslation();
  const { data: info } = useQuery({ queryKey: ['appInfo'], queryFn: commands.appInfo });
  const remote = useRemote();

  return (
    <footer className="statusbar" aria-label={t('status.label')}>
      <span className="status-item" title={campaign.path}>
        <Icon name="book" size={13} />
        {campaign.name}
      </span>
      {remote ? (
        <span className="status-item" title={remote.server} role="status">
          <span className="status-dot" data-state={remote.status.state} aria-hidden="true" />
          {statusText(remote.status, t)}
        </span>
      ) : (
        <span className="status-item">
          <span className="status-dot" data-state="local" aria-hidden="true" />
          {t('status.serverLocal')}
        </span>
      )}
      <span className="status-item">{t('status.plugins', { count: 0 })}</span>
      <span className="spacer" />
      <span className="status-item role" data-role={campaign.role}>
        {t(`roles.${campaign.role}`)}
      </span>
      {info && <span className="status-item">v{info.version}</span>}
    </footer>
  );
}

function statusText(status: RemoteStatus, t: TFunction): string {
  switch (status.state) {
    case 'connecting':
      return t('status.connecting');
    case 'online':
      return t('status.online');
    case 'reconnecting':
      return t('status.reconnecting', { seconds: Math.ceil(status.retry_in_ms / 1000) });
    case 'closed':
      return t(`status.closed.${status.reason}`, { defaultValue: t('status.closed.other') });
  }
}
