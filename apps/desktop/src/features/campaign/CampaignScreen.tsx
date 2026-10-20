import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useState, type SyntheticEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { queryKeys } from '@/shared/game/useGame';
import { ipc, type CampaignInfo } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';

/** Start screen: create a new campaign or open an existing one. */
export function CampaignScreen() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [name, setName] = useState('');

  const onOpened = (info: CampaignInfo | null) => {
    if (!info) return; // dialog cancelled
    queryClient.setQueryData(queryKeys.campaign, info);
    void queryClient.invalidateQueries({ queryKey: queryKeys.snapshot });
  };
  const create = useMutation({ mutationFn: ipc.campaignCreate, onSuccess: onOpened });
  const open = useMutation({ mutationFn: ipc.campaignOpen, onSuccess: onOpened });

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
        <small>{t('campaign.createHint')}</small>
      </form>
      <button
        type="button"
        disabled={open.isPending}
        onClick={() => {
          open.mutate();
        }}
      >
        {t('campaign.open')}
      </button>
      <ErrorText error={create.error ?? open.error} />
    </main>
  );
}
