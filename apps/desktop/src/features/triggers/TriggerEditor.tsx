import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { newAction, StepEditor } from './StepEditor';
import { useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import type { SystemRulesDto, TriggerDef, TriggerDto, TriggerEvent } from '@/shared/ipc/client';
import { ElementField } from '@/shared/ui/ElementPicker';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';

const EVENT_TYPES: TriggerEvent['type'][] = [
  'roll_in_zone',
  'token_entered_zone',
  'manual',
  'item_received',
];

function newEvent(type: TriggerEvent['type']): TriggerEvent {
  switch (type) {
    case 'roll_in_zone':
      return { type, zone: '', rolls: [] };
    case 'token_entered_zone':
      return { type, zone: '' };
    case 'manual':
      return { type };
    case 'item_received':
      return { type, name: '' };
  }
}

/**
 * A trigger in a center tab (ADR-0024): when → who → if → then. The form edits a draft; Save
 * sends it whole, and the core checks every reference and expression.
 */
export function TriggerEditor({ triggerId }: { triggerId: string }) {
  const { t } = useTranslation();
  const trigger = useSnapshotSelect((s) => s.triggers.find((tr) => tr.id === triggerId));
  const rules = useSnapshotSelect(
    (s) => s.plugins.find((p) => p.id === s.system_plugin)?.system ?? null,
  );
  if (!trigger) return <p className="muted">{t('triggers.missing')}</p>;
  // Keyed by version: a saved edit (here or by another GM) starts a fresh draft.
  return <Form key={trigger.version} trigger={trigger} rules={rules ?? null} />;
}

function Form({ trigger, rules }: { trigger: TriggerDto; rules: SystemRulesDto | null }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const submit = useSubmitIntent();
  const fire = useSubmitIntent();
  const [draft, setDraft] = useState<TriggerDef>(trigger.definition);
  const [fireToken, setFireToken] = useState<string | null>(null);
  const dirty = JSON.stringify(draft) !== JSON.stringify(trigger.definition);
  const event = draft.event;
  const update = (patch: Partial<TriggerDef>) => {
    setDraft((d) => ({ ...d, ...patch }));
  };

  return (
    <form
      className="trigger-editor"
      aria-label={trigger.definition.name}
      onSubmit={(e) => {
        e.preventDefault();
        submit.mutate({
          intent: { kind: 'update_trigger', id: trigger.id, definition: draft },
          expectedVersion: trigger.version,
        });
      }}
    >
      <header className="trigger-header">
        <Icon name="bolt" />
        <input
          className="trigger-name"
          aria-label={t('triggers.name')}
          value={draft.name}
          maxLength={200}
          onChange={(e) => {
            update({ name: e.target.value });
          }}
        />
        <label className="inline">
          <input
            type="checkbox"
            checked={draft.enabled}
            onChange={(e) => {
              update({ enabled: e.target.checked });
            }}
          />
          {t('triggers.enabled')}
        </label>
        <label className="inline">
          <input
            type="checkbox"
            checked={draft.once}
            onChange={(e) => {
              update({ once: e.target.checked });
            }}
          />
          {t('triggers.once')}
        </label>
        {trigger.fired && <span className="badge">{t('triggers.fired')}</span>}
      </header>

      <fieldset className="trigger-section">
        <legend>{t('triggers.whenTitle')}</legend>
        <label className="trigger-field">
          <span>{t('triggers.event')}</span>
          <select
            value={event.type}
            onChange={(e) => {
              update({ event: newEvent(e.target.value as TriggerEvent['type']) });
            }}
          >
            {EVENT_TYPES.map((type) => (
              <option key={type} value={type}>
                {t(`triggers.events.${type}`)}
              </option>
            ))}
          </select>
        </label>
        {(event.type === 'roll_in_zone' || event.type === 'token_entered_zone') && (
          <ElementField
            label={t('zones.title')}
            kinds={['zone']}
            value={event.zone ? { kind: 'zone', id: event.zone } : null}
            onChange={({ id }) => {
              update({ event: { ...event, zone: id } });
            }}
          />
        )}
        {event.type === 'roll_in_zone' && (
          <div className="trigger-field">
            <span>{t('triggers.rolls')}</span>
            <div className="trigger-rolls">
              {(rules?.rolls ?? []).map((roll) => (
                <label key={roll.id} className="inline">
                  <input
                    type="checkbox"
                    checked={event.rolls.includes(roll.id)}
                    onChange={(e) => {
                      const rolls = e.target.checked
                        ? [...event.rolls, roll.id]
                        : event.rolls.filter((r) => r !== roll.id);
                      update({ event: { ...event, rolls } });
                    }}
                  />
                  {labelText(roll.label, lang)}
                </label>
              ))}
              {event.rolls.length === 0 && <span className="muted">{t('triggers.anyRoll')}</span>}
            </div>
          </div>
        )}
        {event.type === 'item_received' && (
          <label className="trigger-field">
            <span>{t('items.name')}</span>
            <input
              value={event.name}
              maxLength={200}
              onChange={(e) => {
                update({ event: { type: 'item_received', name: e.target.value } });
              }}
            />
          </label>
        )}
      </fieldset>

      <fieldset className="trigger-section">
        <legend>{t('triggers.whoTitle')}</legend>
        <ul className="trigger-tokens">
          {draft.tokens.map((token) => (
            <li key={token}>
              <TokenChip
                id={token}
                onRemove={() => {
                  update({ tokens: draft.tokens.filter((x) => x !== token) });
                }}
              />
            </li>
          ))}
        </ul>
        {draft.tokens.length === 0 && <p className="muted">{t('triggers.anyToken')}</p>}
        <ElementField
          label={t('triggers.addToken')}
          kinds={['token']}
          value={null}
          onChange={({ id }) => {
            if (!draft.tokens.includes(id)) update({ tokens: [...draft.tokens, id] });
          }}
        />
      </fieldset>

      <fieldset className="trigger-section">
        <legend>{t('triggers.ifTitle')}</legend>
        <input
          className="mono"
          aria-label={t('triggers.condition')}
          placeholder={t('triggers.alwaysPlaceholder')}
          value={draft.condition ?? ''}
          onChange={(e) => {
            update({ condition: e.target.value.trim() ? e.target.value : null });
          }}
        />
        <p className="muted trigger-hint">{t('triggers.conditionHint')}</p>
      </fieldset>

      <fieldset className="trigger-section">
        <legend>{t('triggers.thenTitle')}</legend>
        <ol className="trigger-steps">
          {draft.steps.map((step, index) => (
            <StepEditor
              key={index}
              step={step}
              index={index}
              count={draft.steps.length}
              rules={rules}
              onChange={(next) => {
                update({ steps: draft.steps.map((s, i) => (i === index ? next : s)) });
              }}
              onMove={(delta) => {
                const steps = [...draft.steps];
                const [moved] = steps.splice(index, 1);
                if (moved) steps.splice(index + delta, 0, moved);
                update({ steps });
              }}
              onRemove={() => {
                update({ steps: draft.steps.filter((_, i) => i !== index) });
              }}
            />
          ))}
        </ol>
        <button
          type="button"
          onClick={() => {
            update({ steps: [...draft.steps, { when: null, action: newAction('chat', rules) }] });
          }}
        >
          {t('triggers.addStep')}
        </button>
      </fieldset>

      <footer className="trigger-footer">
        <button type="submit" className="btn-primary" disabled={!dirty || submit.isPending}>
          {t('triggers.save')}
        </button>
        <button
          type="button"
          disabled={!dirty}
          onClick={() => {
            setDraft(trigger.definition);
          }}
        >
          {t('triggers.revert')}
        </button>
        {trigger.definition.event.type === 'manual' && (
          <span className="trigger-run">
            <ElementField
              label={t('triggers.runFor')}
              kinds={['token']}
              value={fireToken ? { kind: 'token', id: fireToken } : null}
              onChange={({ id }) => {
                setFireToken(id);
              }}
            />
            <button
              type="button"
              disabled={dirty || trigger.fired || fire.isPending}
              onClick={() => {
                fire.mutate({
                  intent: { kind: 'fire_trigger', id: trigger.id, token: fireToken },
                });
              }}
            >
              <Icon name="bolt" />
              {t('triggers.run')}
            </button>
          </span>
        )}
      </footer>
      <ErrorText error={submit.error ?? fire.error} />
    </form>
  );
}

function TokenChip({ id, onRemove }: { id: string; onRemove: () => void }) {
  const { t } = useTranslation();
  const name = useSnapshotSelect((s) => s.tokens.find((tok) => tok.id === id)?.name);
  return (
    <span className="condition-chip">
      {name ?? t('picker.gone')}
      <button type="button" aria-label={t('triggers.removeToken', { name })} onClick={onRemove}>
        ×
      </button>
    </span>
  );
}
