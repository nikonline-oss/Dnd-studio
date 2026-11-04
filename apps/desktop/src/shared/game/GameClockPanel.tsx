import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSnapshot, useSubmitIntent } from './useGame';
import { ErrorText } from '@/shared/ui/ErrorText';
import { labelText } from '@/shared/i18n/label';
import type { CalendarUnit } from '@/shared/ipc/client';
const UNITS: CalendarUnit[] = ['round', 'minute', 'hour', 'day', 'week', 'month', 'year', 'second'];
export function GameClockPanel({
  steps = [],
  disabled = false,
}: {
  steps?: string[];
  disabled?: boolean;
}) {
  const { t, i18n } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const submit = useSubmitIntent();
  const [amount, setAmount] = useState(1);
  const [unit, setUnit] = useState<CalendarUnit>('hour');
  const [pending, setPending] = useState<{
    amount: number;
    unit: CalendarUnit;
    version: number;
  } | null>(null);
  const clock = snapshot?.campaign.game_clock ?? { seconds: 0, version: 1 };
  const date = snapshot?.rules_view?.calendar;
  const waiting = Boolean(
    snapshot?.plugin_runtime?.action_stacks?.length ||
    snapshot?.plugin_runtime?.saves?.length ||
    snapshot?.plugin_runtime?.movements.length ||
    snapshot?.roll_requests?.some((r) => r.status === 'pending'),
  );
  const lang = i18n.language;
  const presets = steps.length ? steps.map(Number) : [60, 600, 3600, 28800];
  const describe = (n: number, u: CalendarUnit) => `${n} ${t(`calendar.units.${u}`)}`;
  return (
    <div className="game-clock-panel">
      {date && (
        <p role="status" className="game-clock-date">
          {date.day} {labelText(date.month, lang)} {date.year} · {labelText(date.weekday, lang)}
          <br />
          {String(date.hour).padStart(2, '0')}:{String(date.minute).padStart(2, '0')} ·{' '}
          {labelText(date.period, lang)}
        </p>
      )}
      {snapshot?.combats.map((c) => (
        <p key={c.id}>
          {snapshot.maps.find((m) => m.id === c.map_id)?.name}:{' '}
          {t('calendar.combat', {
            round: c.round,
            turn: snapshot.combatants.find((a) => a.id === c.turn)?.name ?? '—',
          })}
        </p>
      ))}
      <p className="muted">{t('gameClock.manual')}</p>
      {waiting && <p>{t('gameClock.waiting')}</p>}
      <fieldset disabled={disabled || submit.isPending || waiting}>
        <div className="action-buttons">
          {presets.map((seconds) => {
            const u: CalendarUnit =
              seconds % 3600 === 0 ? 'hour' : seconds % 60 === 0 ? 'minute' : 'second';
            const n = seconds / (u === 'hour' ? 3600 : u === 'minute' ? 60 : 1);
            return (
              <button
                key={seconds}
                onClick={() => {
                  setPending({ amount: n, unit: u, version: clock.version });
                }}
              >
                + {describe(n, u)}
              </button>
            );
          })}
        </div>
        <div className="game-clock-advance">
          <label className="control-field">
            {t('calendar.advance')}
            <input
              type="number"
              min={1}
              max={10000}
              value={amount}
              onChange={(e) => {
                setAmount(Number(e.target.value));
              }}
            />
          </label>
          <label className="control-field">
            {t('calendar.unit')}
            <select
              aria-label={t('calendar.unit')}
              value={unit}
              onChange={(e) => {
                setUnit(e.target.value as CalendarUnit);
              }}
            >
              {UNITS.map((u) => (
                <option key={u} value={u}>
                  {t(`calendar.units.${u}`)}
                </option>
              ))}
            </select>
          </label>
          <button
            disabled={!Number.isInteger(amount) || amount < 1 || amount > 10000}
            onClick={() => {
              setPending({ amount, unit, version: clock.version });
            }}
          >
            {t('calendar.advance')}
          </button>
        </div>
        {pending && pending.version === clock.version && (
          <div className="game-clock-confirm">
            <p>+ {describe(pending.amount, pending.unit)}</p>
            <button
              onClick={() => {
                submit.mutate(
                  {
                    intent: {
                      kind: 'advance_calendar_time',
                      amount: pending.amount,
                      unit: pending.unit,
                    },
                    expectedVersion: pending.version,
                  },
                  {
                    onSuccess: () => {
                      setPending(null);
                    },
                  },
                );
              }}
            >
              {t('gameClock.confirm')}
            </button>
            <button
              onClick={() => {
                setPending(null);
              }}
            >
              {t('picker.cancel')}
            </button>
          </div>
        )}
      </fieldset>
      <ErrorText error={submit.error} />
    </div>
  );
}
