import { useState, type SyntheticEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaignActions } from '@/shared/game/useCampaignActions';
import { ErrorText } from '@/shared/ui/ErrorText';

/** Start screen: create a new campaign or open an existing one. */
export function CampaignScreen() {
  const { t } = useTranslation();
  const { create, open } = useCampaignActions();
  const [name, setName] = useState('');

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
          open.mutate();
        }}
      >
        {t('campaign.open')}
      </button>
      <ErrorText error={create.error ?? open.error} />
    </main>
  );
}
