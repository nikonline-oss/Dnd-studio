import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaignActions } from '@/shared/game/useCampaignActions';
import { commands, unwrap, type CampaignInfo } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';
import './safeMode.css';

/**
 * Safe Mode (ADR-0057): the campaign is open without its rules, so a broken plugin cannot stop the
 * user from exporting, fixing data or removing the package. Nothing is deleted — a normal reopen
 * restores the rules. The second banner offers the mode after repeated unclean shutdowns.
 */
export function SafeMode({ campaign }: { campaign: CampaignInfo }) {
  const { t } = useTranslation();
  const { onOpened } = useCampaignActions();
  const [dismissed, setDismissed] = useState(false);
  const safe = useQuery({
    queryKey: ['safe-mode', campaign.path],
    queryFn: () => unwrap(commands.campaignSafeMode()),
  });
  const info = useQuery({ queryKey: ['appInfo'], queryFn: commands.appInfo });
  const reopen = useMutation({
    mutationFn: () => unwrap(commands.campaignReopenSafe()),
    onSuccess: onOpened,
  });
  if (safe.data === true) {
    return (
      <section className="safe-mode" role="status" aria-label={t('safeMode.activeTitle')}>
        <div>
          <strong>{t('safeMode.activeTitle')}</strong>
          <p>{t('safeMode.activeBody')}</p>
          {reopen.error && <ErrorText error={reopen.error} />}
        </div>
        <button
          type="button"
          disabled={reopen.isPending}
          onClick={() => {
            reopen.mutate();
          }}
        >
          {t('safeMode.leave')}
        </button>
      </section>
    );
  }
  if (dismissed || info.data?.safe_mode_suggested !== true) return null;
  return (
    <section className="safe-mode" role="alert" aria-label={t('safeMode.suggestTitle')}>
      <div>
        <strong>{t('safeMode.suggestTitle')}</strong>
        <p>{t('safeMode.suggestBody')}</p>
        {reopen.error && <ErrorText error={reopen.error} />}
      </div>
      <div className="safe-mode-actions">
        <button
          type="button"
          disabled={reopen.isPending}
          onClick={() => {
            reopen.mutate();
          }}
        >
          {t('safeMode.open')}
        </button>
        <button
          type="button"
          onClick={() => {
            setDismissed(true);
          }}
        >
          {t('safeMode.dismiss')}
        </button>
      </div>
    </section>
  );
}
