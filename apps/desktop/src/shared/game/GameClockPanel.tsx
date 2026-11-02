import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSnapshot, useSubmitIntent } from './useGame';
import { ErrorText } from '@/shared/ui/ErrorText';

export function GameClockPanel({
  steps = [],
  disabled = false,
}: {
  steps?: string[];
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const submit = useSubmitIntent();
  const [pending, setPending] = useState<{ seconds: number; version: number } | null>(null);
  const clock = snapshot?.campaign.game_clock ?? { seconds: 0, version: 1 };
  const presets = steps.length ? steps.map(Number) : [6, 60, 600, 3600];
  const waiting = Boolean(
    snapshot?.plugin_runtime?.action_stacks?.length ||
    snapshot?.plugin_runtime?.movements.length ||
    snapshot?.roll_requests?.some((r) => r.status === 'pending'),
  );
  return (
    <div>
      <p role="status">{t('gameClock.elapsed', { seconds: clock.seconds })}</p>
      <p className="muted">{t('gameClock.manual')}</p>
      {waiting && <p>{t('gameClock.waiting')}</p>}
      <fieldset disabled={disabled || submit.isPending || waiting}>
        {presets.map((seconds) => (
          <button
            key={seconds}
            onClick={() => {
              setPending({ seconds, version: clock.version });
            }}
          >
            {t('gameClock.advance', { seconds })}
          </button>
        ))}
        {pending && pending.version === clock.version && (
          <div>
            <p>
              {t('gameClock.review', {
                seconds: pending.seconds,
                until: clock.seconds + pending.seconds,
              })}
            </p>
            <button
              onClick={() => {
                submit.mutate(
                  {
                    intent: { kind: 'advance_game_time', seconds: pending.seconds },
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
