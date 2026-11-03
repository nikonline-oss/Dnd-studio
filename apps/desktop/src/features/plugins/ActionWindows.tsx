import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { useRemote } from '@/shared/game/useRemote';
import { labelText } from '@/shared/i18n/label';
import { ErrorText } from '@/shared/ui/ErrorText';

/** A filtered view of authoritative action stacks; clients never resolve effects locally. */
export function ActionWindows() {
  const { t, i18n } = useTranslation();
  const { data: campaign } = useCampaign();
  const stacks = useSnapshotSelect((s) => s.plugin_runtime?.action_stacks ?? []);
  const rules = useSnapshotSelect((s) => s.plugins.find((p) => p.id === s.system_plugin)?.system);
  const tokens = useSnapshotSelect((s) => s.tokens) ?? [];
  const characters = useSnapshotSelect((s) => s.characters) ?? [];
  const remote = useRemote();
  const submit = useSubmitIntent();
  const gm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  if (!campaign || campaign.role === 'spectator' || !stacks?.length) return null;
  const canEdit = !submit.isPending && (remote === null || remote.status.state === 'online');
  return (
    <aside className="action-windows" aria-label={t('plugins.actionWindows')} aria-live="polite">
      {stacks.map((stack) => (
        <section key={stack.id}>
          <h3>{t('plugins.actionWindows')}</h3>
          {!stack.frames.length && <p>{t('plugins.waitingAction')}</p>}
          <ol>
            {stack.frames.map((frame) => {
              const own = characters.find((c) => c.id === frame.owner)?.owner === campaign.user_id;
              const offer = frame.ready ? frame.offers.find((o) => o.ready) : undefined;
              const offeredCharacter = tokens.find(
                (token) => token.id === offer?.source,
              )?.character_id;
              const canAnswer =
                gm || characters.find((c) => c.id === offeredCharacter)?.owner === campaign.user_id;
              return (
                <li key={frame.id}>
                  <strong>{labelText(frame.label, i18n.language)}</strong> ·{' '}
                  {tokens.find((token) => token.id === frame.source)?.name}
                  {!!frame.deferred.length && (own || gm) && (
                    <p className="muted">
                      {t('plugins.reservedCosts', {
                        costs: frame.deferred
                          .map(
                            (cost) =>
                              `${cost.amount} × ${labelText(rules?.resources.find((r) => r.id === cost.resource)?.label ?? cost.resource, i18n.language)}`,
                          )
                          .join(', '),
                      })}
                    </p>
                  )}
                  {offer && canAnswer ? (
                    <div>
                      <p>
                        {labelText(offer.label, i18n.language)} ·{' '}
                        {tokens.find((token) => token.id === offer.source)?.name}
                      </p>
                      {[true, false].map((accept) => (
                        <button
                          key={String(accept)}
                          disabled={!canEdit}
                          onClick={() => {
                            submit.mutate({
                              intent: {
                                kind: 'resolve_action_reaction',
                                stack: stack.id,
                                frame: frame.id,
                                source: offer.source,
                                accept,
                              },
                            });
                          }}
                        >
                          {t(accept ? 'plugins.acceptReaction' : 'plugins.declineReaction')}
                        </button>
                      ))}
                    </div>
                  ) : frame.ready && !frame.awaiting_reactions && (gm || own) ? (
                    <button
                      disabled={!canEdit}
                      onClick={() => {
                        submit.mutate({
                          intent: {
                            kind: 'continue_plugin_action',
                            stack: stack.id,
                            frame: frame.id,
                          },
                        });
                      }}
                    >
                      {t('plugins.continueAction')}
                    </button>
                  ) : (
                    <p className="muted">{t('plugins.waitingAction')}</p>
                  )}
                </li>
              );
            })}
          </ol>
          {gm && (
            <button
              disabled={!canEdit}
              onClick={() => {
                submit.mutate({ intent: { kind: 'cancel_action_stack', stack: stack.id } });
              }}
            >
              {t('plugins.cancelActionStack')}
            </button>
          )}
        </section>
      ))}
      <ErrorText error={submit.error} />
    </aside>
  );
}
