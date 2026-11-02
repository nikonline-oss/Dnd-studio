import { useState, type SyntheticEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useSubmitIntent } from '@/shared/game/useGame';
import { IntentRejected, type CharacterDto, type UserDto } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { Dialog } from '@/shared/ui/Dialog';
import { ErrorText } from '@/shared/ui/ErrorText';
import './requestRolls.css';

export function RequestRollsDialog({
  characters,
  users,
  onClose,
}: {
  characters: readonly CharacterDto[];
  users: readonly UserDto[];
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const [id] = useState(uuidv7);
  const [label, setLabel] = useState('');
  const [dice, setDice] = useState('1d20');
  const [selected, setSelected] = useState<string[]>([]);
  const available = characters.filter((c) => c.status === 'active');
  const targets = selected.filter((id) => available.some((c) => c.id === id));
  const problem = !label.trim()
    ? 'nameRequired'
    : !dice.trim()
      ? 'diceRequired'
      : targets.length === 0
        ? 'targetsRequired'
        : targets.length > 20
          ? 'tooMany'
          : null;
  const rejected =
    submit.error instanceof IntentRejected && submit.error.reason.code === 'invalid'
      ? submit.error.reason.field
      : null;
  const send = (event: SyntheticEvent) => {
    event.preventDefault();
    if (problem || submit.isPending) return;
    submit.mutate(
      {
        intent: {
          kind: 'request_rolls',
          id,
          label: label.trim(),
          dice: dice.trim(),
          characters: targets,
        },
      },
      {
        onSuccess: onClose,
      },
    );
  };
  return (
    <Dialog title={t('chat.request.title')} closeLabel={t('session.close')} onClose={onClose}>
      <form className="request-rolls-form" onSubmit={send}>
        <label>
          {t('chat.request.name')}
          <input
            value={label}
            maxLength={200}
            onChange={(e) => {
              setLabel(e.target.value);
            }}
          />
        </label>
        <label>
          {t('chat.request.dice')}
          <input
            value={dice}
            maxLength={256}
            spellCheck={false}
            onChange={(e) => {
              setDice(e.target.value);
            }}
          />
        </label>
        <p className="muted">{t('chat.request.hint')}</p>
        <div className="request-rolls-controls">
          <button
            type="button"
            onClick={() => {
              setSelected(
                available
                  .filter(
                    (c) =>
                      c.kind === 'pc' && users.some((u) => u.id === c.owner && u.role === 'player'),
                  )
                  .map((c) => c.id),
              );
            }}
          >
            {t('chat.request.allPlayers')}
          </button>
          <button
            type="button"
            onClick={() => {
              setSelected([]);
            }}
          >
            {t('chat.request.clear')}
          </button>
          <span>{t('chat.request.selected', { count: targets.length })}</span>
        </div>
        <fieldset className="request-rolls-targets">
          <legend>{t('chat.request.targets')}</legend>
          {available.length === 0 && <p>{t('chat.request.noCharacters')}</p>}
          {available.map((c) => (
            <label key={c.id}>
              <input
                type="checkbox"
                checked={targets.includes(c.id)}
                onChange={(e) => {
                  setSelected((prev) =>
                    e.target.checked
                      ? [...prev.filter((id) => id !== c.id), c.id]
                      : prev.filter((id) => id !== c.id),
                  );
                }}
              />
              <span>
                {c.name}
                <small>
                  {users.find((u) => u.id === c.owner)?.name ?? t('chat.request.gmControls')}
                </small>
              </span>
            </label>
          ))}
        </fieldset>
        {problem && (
          <p role="status" className="muted">
            {t(`chat.request.${problem}`)}
          </p>
        )}
        {rejected ? (
          <p role="alert" className="error-text">
            {t(rejected === 'dice' ? 'chat.request.invalidDice' : 'chat.request.invalidTargets')}
          </p>
        ) : (
          <ErrorText error={submit.error} />
        )}
        <footer>
          <button type="submit" className="btn-primary" disabled={!!problem || submit.isPending}>
            {t('chat.request.send')}
          </button>
        </footer>
      </form>
    </Dialog>
  );
}
