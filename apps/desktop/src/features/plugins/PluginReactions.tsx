import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import { ErrorText } from '@/shared/ui/ErrorText';

export function PluginReactions() {
  const { t, i18n } = useTranslation();
  const { data: campaign } = useCampaign();
  const runtime = useSnapshotSelect((s) => s.plugin_runtime);
  const tokens = useSnapshotSelect((s) => s.tokens) ?? [];
  const characters = useSnapshotSelect((s) => s.characters) ?? [];
  const submit = useSubmitIntent();
  const gm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  if (!campaign || campaign.role === 'spectator') return null;
  const pending = (runtime?.movements ?? []).filter(
    (m) =>
      gm ||
      m.offers.some((o) => {
        const c = tokens.find((token) => token.id === o.source)?.character_id;
        return characters.find((entry) => entry.id === c)?.owner === campaign.user_id;
      }),
  );
  if (!pending.length) return null;
  return (
    <aside className="plugin-reactions" aria-label={t('plugins.reactions')} aria-live="polite">
      {pending.map((m) => {
        const offer = m.offers[0];
        if (!offer) return null;
        return (
          <div key={m.id}>
            <strong>{labelText(offer.label, i18n.language)}</strong>
            <p>
              {tokens.find((token) => token.id === offer.source)?.name} →{' '}
              {tokens.find((token) => token.id === m.token)?.name}
            </p>
            {[true, false].map((accept) => (
              <button
                key={String(accept)}
                type="button"
                disabled={submit.isPending || !offer.ready}
                onClick={() => {
                  submit.mutate({
                    intent: {
                      kind: 'resolve_plugin_reaction',
                      movement: m.id,
                      source: offer.source,
                      accept,
                    },
                  });
                }}
              >
                {t(accept ? 'plugins.acceptReaction' : 'plugins.declineReaction')}
              </button>
            ))}
            {gm && (
              <button
                type="button"
                disabled={submit.isPending}
                onClick={() => {
                  submit.mutate({ intent: { kind: 'cancel_plugin_movement', movement: m.id } });
                }}
              >
                {t('plugins.cancelMovement')}
              </button>
            )}
          </div>
        );
      })}
      <ErrorText error={submit.error} />
    </aside>
  );
}
