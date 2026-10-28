import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSubmitIntent } from '@/shared/game/useGame';
import type { CharacterDto, SystemRulesDto } from '@/shared/ipc/client';
import { labelText } from '@/shared/i18n/label';
import { ErrorText } from '@/shared/ui/ErrorText';

/**
 * The character's conditions under the campaign's system (ADR-0024): chips, removable and
 * addable by GM roles and the owner; the core checks that they are the system's.
 */
export function Conditions({
  character,
  rules,
  canEdit,
}: {
  character: CharacterDto;
  rules: SystemRulesDto;
  canEdit: boolean;
}) {
  const { t, i18n } = useTranslation();
  const submit = useSubmitIntent();
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const active = character.conditions ?? [];
  const known = rules.conditions ?? [];
  if (known.length === 0) return null;

  const set = (conditions: string[]) => {
    submit.mutate({
      intent: { kind: 'set_conditions', id: character.id, conditions },
      expectedVersion: character.version,
    });
  };
  const available = known.filter((c) => !active.includes(c.id));

  return (
    <section className="conditions" aria-label={t('conditions.title')}>
      <h3 className="sheet-section-title">{t('conditions.title')}</h3>
      <div className="condition-chips">
        {active.length === 0 && <span className="muted">{t('conditions.none')}</span>}
        {active.map((id) => {
          const label = labelText(known.find((c) => c.id === id)?.label ?? id, lang);
          return (
            <span key={id} className="condition-chip">
              {label}
              {canEdit && (
                <button
                  type="button"
                  aria-label={t('conditions.remove', { name: label })}
                  title={t('conditions.remove', { name: label })}
                  onClick={() => {
                    set(active.filter((c) => c !== id));
                  }}
                >
                  ×
                </button>
              )}
            </span>
          );
        })}
        {canEdit && available.length > 0 && (
          <select
            aria-label={t('conditions.add')}
            value=""
            onChange={(e) => {
              if (e.target.value) set([...active, e.target.value]);
            }}
          >
            <option value="">{t('conditions.add')}</option>
            {available.map((c) => (
              <option key={c.id} value={c.id}>
                {labelText(c.label, lang)}
              </option>
            ))}
          </select>
        )}
      </div>
      <ErrorText error={submit.error} />
    </section>
  );
}

/** GM roles: puts an item into the character's inventory list (the system's `inventory`). */
export function GiveItem({ character }: { character: CharacterDto }) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const [name, setName] = useState('');
  return (
    <form
      className="inline give-item"
      onSubmit={(e) => {
        e.preventDefault();
        if (!name.trim()) return;
        submit.mutate(
          {
            intent: { kind: 'give_item', id: character.id, name },
            expectedVersion: character.version,
          },
          {
            onSuccess: () => {
              setName('');
            },
          },
        );
      }}
    >
      <input
        aria-label={t('items.name')}
        placeholder={t('items.name')}
        value={name}
        maxLength={200}
        onChange={(e) => {
          setName(e.target.value);
        }}
      />
      <button type="submit" disabled={!name.trim() || submit.isPending}>
        {t('items.give')}
      </button>
      <ErrorText error={submit.error} />
    </form>
  );
}
