import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useSaveStatus } from '@/shared/game/useSaveStatus';
import { commands, type CampaignInfo } from '@/shared/ipc/client';

/** Campaign, save state, server status, active plugins, app version (CLAUDE.md §11). */
export function StatusBar({ campaign }: { campaign: CampaignInfo }) {
  const { t } = useTranslation();
  const { data: info } = useQuery({ queryKey: ['appInfo'], queryFn: commands.appInfo });
  const save = useSaveStatus();

  const saveLabel = save.error
    ? t('status.saveFailed')
    : save.dirty
      ? t('status.unsaved')
      : t('status.saved');

  return (
    <footer className="statusbar" aria-label={t('status.label')}>
      <span title={campaign.path}>{campaign.name}</span>
      <span
        data-save={save.error ? 'error' : save.dirty ? 'dirty' : 'saved'}
        title={save.error ?? ''}
      >
        {saveLabel}
      </span>
      <span>{t('status.serverLocal')}</span>
      <span>{t('status.plugins', { count: 0 })}</span>
      <span className="spacer" />
      <span>{t(`roles.${campaign.role}`)}</span>
      {info && <span>v{info.version}</span>}
    </footer>
  );
}
