import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useState, type SyntheticEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { queryKeys } from '@/shared/game/useGame';
import { useCampaignActions } from '@/shared/game/useCampaignActions';
import { errorCode, ipc } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';

/** Start screen: create a new campaign or open an existing one. */
export function CampaignScreen() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { create, open } = useCampaignActions();
  const [name, setName] = useState('');

  // Offered when opening failed because the file is damaged (CLAUDE.md §10).
  const restore = useMutation({
    mutationFn: ipc.campaignRestoreBackup,
    onSuccess: (info) => {
      open.reset();
      queryClient.setQueryData(queryKeys.campaign, info);
      void queryClient.invalidateQueries({ queryKey: queryKeys.snapshot });
    },
  });
  const corrupt = open.error !== null && errorCode(open.error) === 'campaign_corrupt';

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    create.mutate(name);
  };

  return (
    <main className="campaign-screen">
      <h1>{t('app.title')}</h1>
      <p>{t('campaign.welcome')}</p>
      <form onSubmit={submit} className="campaign-form">
        <label>
          {t('campaign.nameLabel')}
          <input
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            placeholder={t('campaign.namePlaceholder')}
            maxLength={200}
          />
        </label>
        <button type="submit" disabled={create.isPending || name.trim() === ''}>
          {t('campaign.create')}
        </button>
        <small className="muted">{t('campaign.createHint')}</small>
      </form>
      <button
        type="button"
        disabled={open.isPending}
        onClick={() => {
          restore.reset();
          open.mutate();
        }}
      >
        {t('campaign.open')}
      </button>
      <ErrorText error={create.error ?? open.error ?? restore.error} />
      {corrupt && (
        <div className="notice">
          <p>{t('campaign.corruptHint')}</p>
          <button
            type="button"
            disabled={restore.isPending}
            onClick={() => {
              restore.mutate();
            }}
          >
            {t('campaign.restore')}
          </button>
        </div>
      )}
    </main>
  );
}
