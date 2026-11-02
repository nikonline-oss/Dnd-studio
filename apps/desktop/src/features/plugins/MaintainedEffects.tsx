import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { useRemote } from '@/shared/game/useRemote';
import { labelText } from '@/shared/i18n/label';
import { ErrorText } from '@/shared/ui/ErrorText';

/** The kernel owns lifecycle and permissions; these buttons only send an intent. */
export function MaintainedEffects({ floating = false }: { floating?: boolean }) {
  const { t, i18n } = useTranslation();
  const { data: campaign } = useCampaign();
  const entries = useSnapshotSelect((s) => s.plugin_runtime?.maintained ?? []);
  const characters = useSnapshotSelect((s) => s.characters) ?? [];
  const tokens = useSnapshotSelect((s) => s.tokens) ?? [];
  const submit = useSubmitIntent();
  const remote = useRemote();
  const gm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  if (!campaign || (campaign.role !== 'player' && !gm)) return null;
  const visible = (entries ?? []).filter(
    (entry) => gm || characters.find((c) => c.id === entry.owner)?.owner === campaign.user_id,
  );
  if (floating && !visible.length) return null;
  const body = (
    <>
      {!visible.length && <p className="muted">{t('plugins.noMaintainedEffects')}</p>}
      <ul className="maintained-list">
        {visible.map((entry) => (
          <li key={entry.id}>
            <div>
              <strong>{labelText(entry.label, i18n.language)}</strong>
              <span>
                {tokens.find((token) => token.id === entry.source)?.name ??
                  t('plugins.unknownSource')}
              </span>
            </div>
            <button
              type="button"
              disabled={submit.isPending || (remote !== null && remote.status.state !== 'online')}
              onClick={() => {
                submit.mutate({ intent: { kind: 'end_maintained_effect', id: entry.id } });
              }}
            >
              {t('plugins.endMaintenance')}
            </button>
          </li>
        ))}
      </ul>
      <ErrorText error={submit.error} />
    </>
  );
  return floating ? (
    <details className="plugin-maintained">
      <summary>
        {t('plugins.maintainedEffects')} ({visible.length})
      </summary>
      {body}
    </details>
  ) : (
    <section aria-label={t('plugins.maintainedEffects')}>{body}</section>
  );
}
