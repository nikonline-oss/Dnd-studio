import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { useAttention } from '@/shared/state/attention';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { IconButton } from '@/shared/ui/IconButton';
import { playAttentionTone } from './attentionSound';
import './attention.css';

/** P1 / PLR-3: personal prompts use only the recipient's authoritative projection. */
export function AttentionNotice() {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const characters = useSnapshotSelect((s) => s.characters) ?? [];
  const requests = useSnapshotSelect((s) => s.roll_requests) ?? [];
  const combats = useSnapshotSelect((s) => s.combats) ?? [];
  const combatants = useSnapshotSelect((s) => s.combatants) ?? [];
  const tokens = useSnapshotSelect((s) => s.tokens) ?? [];
  const submit = useSubmitIntent();
  const sound = useAttention((s) => s.soundEnabled);
  const setSound = useAttention((s) => s.setSoundEnabled);
  const setOpen = useAttention((s) => s.setRequestsOpen);
  const [dismissed, setDismissed] = useState<string | null>(null);
  const gm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const player = campaign?.role === 'player';
  const mine = (character: string | null) =>
    character !== null &&
    characters.some((c) => c.id === character && c.owner === campaign?.user_id);
  const pending = requests.filter(
    (r) => r.status === 'pending' && (gm || (player && mine(r.character))),
  );
  const turns = player
    ? combats.flatMap((combat) => {
        const turn = combatants.find((c) => c.id === combat.turn && c.combat_id === combat.id);
        const character =
          turn?.character_id ??
          tokens.find((tok) => tok.id === turn?.token_id)?.character_id ??
          null;
        return turn && mine(character) ? [{ combat, turn }] : [];
      })
    : [];
  const keys = JSON.stringify([
    ...pending.map((r) => `roll:${r.id}`),
    ...turns.map(({ combat, turn }) => `turn:${combat.id}:${combat.round}:${turn.id}`),
  ]);
  const previous = useRef(new Set<string>());
  useEffect(() => {
    const current = new Set<string>(JSON.parse(keys) as string[]);
    if (sound && [...current].some((key) => !previous.current.has(key))) playAttentionTone();
    previous.current = current;
  }, [keys, sound]);
  useEffect(
    () => () => {
      useAttention.getState().setRequestsOpen(false);
    },
    [],
  );
  if ((!pending.length && !turns.length) || dismissed === keys) return null;
  const first = pending[0];
  return (
    <aside className="attention-notice" aria-label={t('attention.title')}>
      <header>
        <strong role="status">{t('attention.title')}</strong>
        <IconButton
          icon={sound ? 'volume' : 'volumeOff'}
          label={t(sound ? 'attention.mute' : 'attention.unmute')}
          onClick={() => {
            setSound(!sound);
          }}
        />
        <button
          type="button"
          onClick={() => {
            setDismissed(keys);
          }}
        >
          {t('attention.dismiss')}
        </button>
      </header>
      {first && (
        <div className="attention-row">
          <span>
            <strong>{first.label}</strong>
            <small>{characters.find((c) => c.id === first.character)?.name}</small>
          </span>
          <button
            className="btn-primary"
            type="button"
            disabled={submit.isPending}
            onClick={() => {
              submit.mutate({
                intent: { kind: 'fulfill_roll_request', id: first.id },
                expectedVersion: first.version,
              });
            }}
          >
            {t('rolls.roll')}
          </button>
          <button
            type="button"
            onClick={() => {
              setOpen(true);
            }}
          >
            {t('attention.allRequests', { count: pending.length })}
          </button>
        </div>
      )}
      {turns.map(({ combat, turn }) => (
        <div className="attention-row" key={combat.id}>
          <span role="status">
            {t('attention.yourTurn', { name: turn.name, round: combat.round })}
          </span>
          <button
            type="button"
            onClick={() => {
              const selection = useSelection.getState();
              selection.openMap(combat.map_id);
              selection.selectToken(turn.token_id);
            }}
          >
            {t('attention.showTurn')}
          </button>
        </div>
      ))}
      <ErrorText error={submit.error} />
    </aside>
  );
}
