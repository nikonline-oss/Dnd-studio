import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshot, useSubmitIntent } from './useGame';
import { labelText } from '@/shared/i18n/label';
import { ErrorText } from '@/shared/ui/ErrorText';
export function ActionSaves() {
  const { t, i18n } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();
  const gm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const groups = snapshot?.plugin_runtime?.saves ?? [];
  if (!groups.length) return null;
  return (
    <aside className="action-saves" aria-label={t('saves.title')}>
      {groups.map((g) => (
        <section key={g.id}>
          <strong>{labelText(g.action.label, i18n.language)}</strong>
          <p>{t('saves.damage', { damage: g.damage, dc: g.difficulty })}</p>
          {g.targets.map((target) => {
            const character = snapshot?.characters.find((c) => c.id === target.character);
            const canRoll =
              gm || (campaign?.role === 'player' && character?.owner === campaign.user_id);
            return (
              <div key={target.token}>
                {character?.name}:{' '}
                {target.status === 'pending' ? (
                  <button
                    disabled={!canRoll || submit.isPending}
                    onClick={() => {
                      submit.mutate({
                        intent: { kind: 'resolve_action_save', group: g.id, target: target.token },
                      });
                    }}
                  >
                    {t('saves.roll')}
                  </button>
                ) : (
                  t(`saves.${target.status}`)
                )}
              </div>
            );
          })}
          {gm && (
            <button
              disabled={submit.isPending}
              onClick={() => {
                submit.mutate({ intent: { kind: 'cancel_action_saves', group: g.id } });
              }}
            >
              {t('saves.cancel')}
            </button>
          )}
        </section>
      ))}
      <ErrorText error={submit.error} />
    </aside>
  );
}
