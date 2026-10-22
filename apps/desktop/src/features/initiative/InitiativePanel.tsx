import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { turnOrder } from './turnOrder';
import { useCampaign, useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { useShownMap } from '@/shared/game/useShownMap';
import { tokenCssColor, tokenInitial } from '@/shared/game/tokenLook';
import type { CombatantDto, CombatDto, Snapshot } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { ConfirmButton, ConfirmDelete, IconButton } from '@/shared/ui/IconButton';

/**
 * Initiative tracker of the shown map (ADR-0020, bottom panel). GM roles run the combat; a player
 * rolls for their own character and ends their own turn. The core checks every action.
 */
export function InitiativePanel() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();
  const map = useShownMap();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';

  if (!map || !snapshot || !campaign) {
    return <p className="initiative-empty muted">{t('initiative.noMap')}</p>;
  }
  const combat = snapshot.combats.find((c) => c.map_id === map.id);

  // Tokens of the map that can fight: not props on the background layer, not already in.
  const newcomers = (present: CombatantDto[]) =>
    snapshot.tokens.filter(
      (tok) =>
        tok.map_id === map.id &&
        tok.layer !== 'background' &&
        !present.some((c) => c.token_id === tok.id),
    );

  /** Runs intents one after another; the first rejection stops and shows in ErrorText. */
  const sequence = (steps: (() => Promise<unknown>)[]) => {
    void steps
      .reduce((previous, step) => previous.then(step), Promise.resolve<unknown>(undefined))
      .catch(() => undefined);
  };

  const addAll = (combatId: string, present: CombatantDto[]) =>
    newcomers(present).map(
      (tok) => () =>
        submit.mutateAsync({
          intent: {
            kind: 'add_combatant',
            id: uuidv7(),
            combat_id: combatId,
            token_id: tok.id,
            character_id: tok.character_id,
            name: tok.name.trim() || t('tokens.newName'),
          },
        }),
    );

  if (!combat) {
    if (!isGm) return <p className="initiative-empty muted">{t('initiative.noCombat')}</p>;
    const count = newcomers([]).length;
    return (
      <div className="initiative-empty">
        <p className="muted">{t('initiative.startHint', { count })}</p>
        <button
          type="button"
          className="btn-primary with-text"
          disabled={submit.isPending}
          onClick={() => {
            const id = uuidv7();
            sequence([
              () => submit.mutateAsync({ intent: { kind: 'start_combat', id, map_id: map.id } }),
              ...addAll(id, []),
            ]);
          }}
        >
          <Icon name="dice" />
          {t('initiative.start')}
        </button>
        <ErrorText error={submit.error} />
      </div>
    );
  }

  const order = turnOrder(snapshot.combatants, combat.id);
  const owns = (c: CombatantDto) => ownsCombatant(snapshot, c, campaign.user_id);
  const current = order.find((c) => c.id === combat.turn);
  const canPassTurn = isGm || (current !== undefined && owns(current));
  const unrolled = order.filter((c) => c.initiative === null);
  const missing = newcomers(order);

  const roll = (c: CombatantDto) => () =>
    submit.mutateAsync({
      intent: { kind: 'roll_initiative', id: c.id },
      expectedVersion: c.version,
    });

  return (
    <section className="initiative-panel" aria-label={t('panels.initiative')}>
      <header className="initiative-header">
        <span className="round">{t('initiative.round', { round: combat.round })}</span>
        <span className="muted turn-of">
          {current ? t('initiative.turnOf', { name: current.name }) : t('initiative.notStarted')}
        </span>
        <span className="spacer" />
        {isGm && unrolled.length > 0 && (
          <IconButton
            icon="dice"
            label={t('initiative.rollAll')}
            variant="default"
            disabled={submit.isPending}
            onClick={() => {
              sequence(unrolled.map(roll));
            }}
          >
            {t('initiative.rollAll')}
          </IconButton>
        )}
        {isGm && missing.length > 0 && (
          <IconButton
            icon="plus"
            label={t('initiative.addTokens', { count: missing.length })}
            variant="default"
            disabled={submit.isPending}
            onClick={() => {
              sequence(addAll(combat.id, order));
            }}
          >
            {t('initiative.addTokens', { count: missing.length })}
          </IconButton>
        )}
        {isGm && (
          <ConfirmButton
            icon="x"
            label={t('initiative.end')}
            confirmLabel={t('initiative.confirmEnd')}
            disabled={submit.isPending}
            onConfirm={() => {
              submit.mutate({
                intent: { kind: 'end_combat', id: combat.id },
                expectedVersion: combat.version,
              });
            }}
          />
        )}
        {canPassTurn && (
          <NextTurnButton combat={combat} disabled={submit.isPending || order.length === 0} />
        )}
      </header>
      {order.length === 0 && <p className="muted">{t('initiative.empty')}</p>}
      <ol className="combatant-list">
        {order.map((c) => (
          <li
            key={c.id}
            className="combatant"
            data-current={c.id === combat.turn}
            aria-current={c.id === combat.turn ? 'step' : undefined}
          >
            <span
              className="avatar"
              style={{
                background: tokenCssColor(c.character_id ?? c.token_id ?? c.id),
                color: '#fff',
              }}
              aria-hidden="true"
            >
              {tokenInitial(c.name)}
            </span>
            <span className="row-name">{c.name}</span>
            {isGm ? (
              <InitiativeInput key={c.version} combatant={c} />
            ) : (
              <span className="init-value">{c.initiative ?? '—'}</span>
            )}
            {(isGm || owns(c)) && (
              <IconButton
                icon="dice"
                label={t('initiative.roll', { name: c.name })}
                disabled={submit.isPending}
                onClick={() => {
                  sequence([roll(c)]);
                }}
              />
            )}
            {isGm && (
              <ConfirmDelete
                label={t('initiative.remove', { name: c.name })}
                disabled={submit.isPending}
                onConfirm={() => {
                  submit.mutate({
                    intent: { kind: 'remove_combatant', id: c.id },
                    expectedVersion: c.version,
                  });
                }}
              />
            )}
          </li>
        ))}
      </ol>
      <ErrorText error={submit.error} />
    </section>
  );
}

function ownsCombatant(snapshot: Snapshot, c: CombatantDto, userId: string): boolean {
  return (
    c.character_id !== null &&
    snapshot.characters.find((ch) => ch.id === c.character_id)?.owner === userId
  );
}

function NextTurnButton({ combat, disabled }: { combat: CombatDto; disabled: boolean }) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const label = combat.turn === null ? t('initiative.firstTurn') : t('initiative.nextTurn');
  return (
    <>
      <button
        type="button"
        className="btn-primary with-text"
        disabled={disabled || submit.isPending}
        onClick={() => {
          // The combat version stops a double click from skipping a turn.
          submit.mutate({
            intent: { kind: 'next_turn', combat_id: combat.id },
            expectedVersion: combat.version,
          });
        }}
      >
        {label}
      </button>
      <ErrorText error={submit.error} />
    </>
  );
}

/** GM: the initiative as an editable number; empty clears it. Commits on Enter or blur. */
function InitiativeInput({ combatant }: { combatant: CombatantDto }) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const [draft, setDraft] = useState(combatant.initiative?.toString() ?? '');

  const commit = () => {
    const text = draft.trim();
    const value = text === '' ? null : Number(text);
    if (value !== null && !Number.isSafeInteger(value)) {
      setDraft(combatant.initiative?.toString() ?? '');
      return;
    }
    if (value === combatant.initiative) return;
    submit.mutate({
      intent: { kind: 'set_initiative', id: combatant.id, initiative: value },
      expectedVersion: combatant.version,
    });
  };

  return (
    <input
      className="init-input"
      inputMode="numeric"
      aria-label={t('initiative.value', { name: combatant.name })}
      placeholder="—"
      value={draft}
      onChange={(e) => {
        setDraft(e.target.value);
      }}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === 'Enter') e.currentTarget.blur();
        if (e.key === 'Escape') setDraft(combatant.initiative?.toString() ?? '');
      }}
    />
  );
}
