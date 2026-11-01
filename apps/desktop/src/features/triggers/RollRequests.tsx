import { useAttention } from '@/shared/state/attention';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import type { RollRequestDto } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';
import { IconButton } from '@/shared/ui/IconButton';

/**
 * Manual rolls a trigger asked for (a `change_resource` step with `manual: true`): a die by the
 * status bar. A player sees the requests of their own characters, GM roles see and manage every
 * request — including cancelling damage that a roll already applied.
 */
export function RollRequests() {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const userId = campaign?.user_id;
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const characters = useSnapshotSelect((s) => s.characters) ?? [];
  const requests = useSnapshotSelect((s) => s.roll_requests) ?? [];
  const submit = useSubmitIntent();
  const open = useAttention((s) => s.requestsOpen);
  const setOpen = useAttention((s) => s.setRequestsOpen);
  if (!campaign || campaign.role === 'spectator') return null;
  const mine = requests.filter((request) => {
    if (isGm) return true;
    return characters.find((c) => c.id === request.character)?.owner === userId;
  });
  const pending = mine.filter((request) => request.status === 'pending').length;
  if (mine.length === 0) return null;
  const nameOf = (request: RollRequestDto) =>
    characters.find((c) => c.id === request.character)?.name ?? t('rolls.unknown');
  return (
    <div className="roll-requests">
      <IconButton
        icon="dice"
        className="roll-requests-toggle"
        label={t('rolls.title')}
        onClick={() => {
          setOpen(!open);
        }}
      >
        {pending > 0 && <span className="roll-requests-badge">{pending}</span>}
      </IconButton>
      {open && (
        <ul className="roll-requests-pop">
          {mine.map((request) => (
            <li key={request.id}>
              <span className="roll-request-head">
                <strong>{request.label}</strong>
                <span className="muted">
                  {nameOf(request)} · {request.dice}
                </span>
              </span>
              <span className="roll-request-actions">
                {request.status === 'pending' ? (
                  <>
                    <span className="muted">{t('rolls.pending')}</span>
                    <button
                      type="button"
                      disabled={submit.isPending}
                      onClick={() => {
                        submit.mutate({
                          intent: { kind: 'fulfill_roll_request', id: request.id },
                          expectedVersion: request.version,
                        });
                      }}
                    >
                      {t('rolls.roll')}
                    </button>
                  </>
                ) : (
                  <span className="muted">
                    {request.status === 'rolled'
                      ? request.total != null
                        ? t('rolls.rolled', { total: request.total })
                        : t('rolls.rolled')
                      : t('rolls.cancelled')}
                  </span>
                )}
                {isGm &&
                  request.status !== 'cancelled' &&
                  (request.status === 'pending' ||
                    characters.some((c) => c.id === request.character)) && (
                    <button
                      type="button"
                      disabled={submit.isPending}
                      onClick={() => {
                        submit.mutate({
                          intent: { kind: 'cancel_roll_request', id: request.id },
                          expectedVersion: request.version,
                        });
                      }}
                    >
                      {request.status === 'rolled' ? t('rolls.undo') : t('rolls.cancel')}
                    </button>
                  )}
              </span>
            </li>
          ))}
          <ErrorText error={submit.error} />
        </ul>
      )}
    </div>
  );
}
