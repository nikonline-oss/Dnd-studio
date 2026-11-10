import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import type { ItemStateDef, Label } from '@/shared/ipc/client';
import { campaignItemStates } from './itemStates';
import { ErrorText } from '@/shared/ui/ErrorText';

/** Plain, bounded ids; the core rejects anything else (ADR-0062). */
const STATE_ID = /^[A-Za-z0-9_-]{1,40}$/;

/**
 * The campaign's item states (ADR-0062): what an item may be, and whether it works in that state.
 * Saving sends the whole list; "restore the built-in set" sends an empty one, which the core reads
 * as "use the built-ins" and stores as the absence of the key.
 */
export function ItemStatesEditor() {
  const { t, i18n } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const submit = useSubmitIntent();
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const campaign = snapshot?.campaign;
  const states = campaignItemStates(campaign?.item_states);
  const custom = (campaign?.item_states?.length ?? 0) > 0;
  const [newId, setNewId] = useState('');
  const [newLabel, setNewLabel] = useState('');

  if (!campaign) return null;

  const save = (next: ItemStateDef[]) => {
    submit.mutate({
      intent: {
        kind: 'update_campaign_meta',
        name: null,
        description: null,
        token_inspector: null,
        item_states: next,
      },
      expectedVersion: campaign.version,
    });
  };

  const rename = (index: number, name: string) => {
    const next = states.map((state, i) =>
      i === index ? { ...state, label: withLanguage(state.label, lang, name) } : state,
    );
    save(next);
  };

  const toggle = (index: number, active: boolean) => {
    const next = states.map((state, i) => (i === index ? { ...state, active } : state));
    save(next);
  };

  const remove = (index: number) => {
    save(states.filter((_, i) => i !== index));
  };

  const add = () => {
    const id = newId.trim();
    const label = newLabel.trim();
    if (!STATE_ID.test(id) || !label || states.some((state) => state.id === id)) return;
    save([...states, { id, label: withLanguage(null, lang, label), active: true }]);
    setNewId('');
    setNewLabel('');
  };

  return (
    <div className="item-states">
      <p className="muted">{t('items.statesHint')}</p>
      <ul className="item-states-list">
        {states.map((state, index) => (
          <li key={state.id}>
            <code>{state.id}</code>
            <input
              aria-label={t('items.stateLabel', { id: state.id })}
              defaultValue={labelText(state.label, lang)}
              maxLength={40}
              onBlur={(e) => {
                const name = e.target.value.trim();
                if (name && name !== labelText(state.label, lang)) rename(index, name);
              }}
            />
            <label className="control-checkbox">
              <input
                type="checkbox"
                checked={state.active !== false}
                onChange={(e) => {
                  toggle(index, e.currentTarget.checked);
                }}
              />
              {t('items.stateWorks')}
            </label>
            <button
              type="button"
              className="btn-ghost"
              aria-label={t('items.removeState', { id: state.id })}
              disabled={submit.isPending || states.length <= 1}
              onClick={() => {
                remove(index);
              }}
            >
              ×
            </button>
          </li>
        ))}
      </ul>
      <div className="item-states-add">
        <input
          aria-label={t('items.newStateId')}
          placeholder={t('items.newStateId')}
          value={newId}
          maxLength={40}
          onChange={(e) => {
            setNewId(e.target.value);
          }}
        />
        <input
          aria-label={t('items.newStateLabel')}
          placeholder={t('items.newStateLabel')}
          value={newLabel}
          maxLength={40}
          onChange={(e) => {
            setNewLabel(e.target.value);
          }}
        />
        <button
          type="button"
          disabled={
            submit.isPending ||
            !STATE_ID.test(newId.trim()) ||
            !newLabel.trim() ||
            states.some((state) => state.id === newId.trim())
          }
          onClick={add}
        >
          {t('items.addState')}
        </button>
      </div>
      {states.some((state) => state.active !== false) ? null : (
        <p role="alert" className="error-text">
          {t('items.noActiveState')}
        </p>
      )}
      <button
        type="button"
        className="btn-ghost"
        disabled={submit.isPending || !custom}
        onClick={() => {
          save([]);
        }}
      >
        {t('items.resetStates')}
      </button>
      <ErrorText error={submit.error} />
    </div>
  );
}

/**
 * A label edit keeps the other language: the table may switch languages later, and a translation
 * nobody wrote is worse than the previous one. A plain-string label becomes localized.
 */
function withLanguage(label: Label | null | undefined, lang: string, text: string): Label {
  const ru = typeof label === 'object' && label !== null ? (label.ru ?? null) : (label ?? null);
  const en = typeof label === 'object' && label !== null ? (label.en ?? null) : (label ?? null);
  return lang.startsWith('ru') ? { ru: text, en } : { ru, en: text };
}
