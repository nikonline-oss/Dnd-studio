import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState, type SyntheticEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { queryKeys } from '@/shared/game/useGame';
import { useCampaignActions } from '@/shared/game/useCampaignActions';
import { useDialogs } from '@/shared/state/dialogs';
import { errorCode, ipc } from '@/shared/ipc/client';
import { labelText } from '@/shared/i18n/label';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';

/** Start screen: create a new campaign or open an existing one. */
export function CampaignScreen() {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const { create, open, importArchive } = useCampaignActions();
  const showDialog = useDialogs((s) => s.show);
  const [name, setName] = useState('');
  const [system, setSystem] = useState('srd5');
  const bundled = useQuery({ queryKey: ['pluginBundled'], queryFn: ipc.pluginBundled });
  const systems = (bundled.data ?? []).filter((p) => p.kind === 'system');
  const lang = i18n.resolvedLanguage ?? i18n.language;

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
    create.mutate({ name, system: system === '' ? null : system });
  };

  return (
    <main className="campaign-screen">
      <div className="campaign-card">
        <div className="campaign-hero">
          <span className="hero-mark" aria-hidden="true">
            <Icon name="dice" size={32} />
          </span>
          <h1>{t('app.title')}</h1>
          <p className="muted">{t('campaign.welcome')}</p>
        </div>
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
          <label>
            {t('campaign.system')}
            <select
              value={system}
              onChange={(e) => {
                setSystem(e.target.value);
              }}
            >
              {systems.map((p) => (
                <option key={p.id} value={p.id}>
                  {labelText(p.name, lang)}
                </option>
              ))}
              <option value="">{t('campaign.noSystem')}</option>
            </select>
          </label>
          <button
            type="submit"
            className="btn-primary"
            disabled={create.isPending || name.trim() === ''}
          >
            {t('campaign.create')}
          </button>
          <small className="muted">{t('campaign.createHint')}</small>
        </form>
        <div className="divider" role="presentation">
          <span>{t('campaign.or')}</span>
        </div>
        <button
          type="button"
          className="with-text"
          disabled={open.isPending}
          onClick={() => {
            restore.reset();
            open.mutate();
          }}
        >
          <Icon name="folderOpen" />
          {t('campaign.open')}
        </button>
        <button
          type="button"
          className="with-text"
          disabled={importArchive.isPending}
          onClick={() => {
            importArchive.mutate();
          }}
        >
          <Icon name="filePlus" />
          {t('campaign.import')}
        </button>
        <button
          type="button"
          className="with-text"
          onClick={() => {
            showDialog('connect');
          }}
        >
          <Icon name="cloud" />
          {t('session.connect')}
        </button>
      </div>
      <ErrorText error={create.error ?? open.error ?? importArchive.error ?? restore.error} />
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
