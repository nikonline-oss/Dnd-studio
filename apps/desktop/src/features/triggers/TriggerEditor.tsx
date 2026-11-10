import './TriggerEditor.css';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { incompleteSteps } from './validation';
import { triggerRunBlock, triggerMaps } from '@/shared/game/triggerScope';
import { IntentRejected } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { assetUrl } from '@/shared/ipc/client';
import { newAction, StepEditor } from './StepEditor';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
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
  const { data: campaign } = useCampaign();
  const hasCard = useSnapshotSelect((s) =>
    s.board_nodes.some((n) => n.content.type === 'trigger' && n.content.id === triggerId),
  );
  const trigger = useSnapshotSelect((s) => s.triggers.find((tr) => tr.id === triggerId));
  const rules = useSnapshotSelect(
    (s) => s.plugins.find((p) => p.id === s.system_plugin)?.system ?? null,
  );
  if (campaign?.role !== 'gm' && campaign?.role !== 'co_gm') return null;
  if (!hasCard)
    return (
      <p className="muted">
        {t('triggers.attachToBoard', { defaultValue: 'Place this trigger on a board to edit it.' })}
      </p>
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
  const snapshot = useSnapshotSelect((s) => s);
  const initial = {
    ...trigger.definition,
    map_ids: triggerMaps(trigger.definition, snapshot?.zones ?? []),
  };
  const [draft, setDraft] = useState<TriggerDef>(initial);
  const [deleting, setDeleting] = useState(false);
  const [fireToken, setFireToken] = useState<string | null>(null);
  const dirty = JSON.stringify(draft) !== JSON.stringify(initial);
  const event = draft.event;
  const mapIds = draft.map_ids ?? [];
  const maps = snapshot?.maps ?? [];
  const zones = snapshot?.zones ?? [];
  // One bound map is a dropdown, several are checkboxes; the editor starts in the matching mode.
  const [manyMaps, setManyMaps] = useState(initial.map_ids.length > 1);
  const boundMaps = maps.filter((map) => mapIds.includes(map.id));
  const errors: string[] = [];
  if (!draft.name.trim())
    errors.push(t('triggers.nameRequired', { defaultValue: 'Enter a trigger name.' }));
  if (mapIds.length === 0)
    errors.push(t('triggers.mapsRequired', { defaultValue: 'Select at least one map.' }));
  if (mapIds.some((id) => !maps.some((m) => m.id === id)))
    errors.push(
      t('triggers.mapMissing', { defaultValue: 'Remove unavailable maps from this trigger.' }),
    );
  if (
    (event.type === 'roll_in_zone' || event.type === 'token_entered_zone') &&
    !zones.some((z) => z.id === event.zone)
  )
    errors.push(t('triggers.zoneRequired', { defaultValue: 'Select a zone name.' }));
  if (event.type === 'item_received' && !event.name.trim())
    errors.push(t('triggers.itemRequired', { defaultValue: 'Enter an item name.' }));
  if (draft.steps.length === 0)
    errors.push(t('triggers.stepsRequired', { defaultValue: 'Add at least one action.' }));
  for (const n of snapshot ? incompleteSteps(draft, snapshot) : [])
    errors.push(
      t('triggers.stepIncomplete', {
        n,
        defaultValue: 'Step {{n}}: complete the required action inputs.',
      }),
    );
  const busy = submit.isPending || fire.isPending;
  const fireMaps = triggerMaps(trigger.definition, zones).filter((id) =>
    maps.some((m) => m.id === id),
  );
  const selectedToken = snapshot?.tokens.find((tok) => tok.id === fireToken);
  const runBlock = triggerRunBlock({
    trigger,
    maps: fireMaps,
    tokenId: fireToken,
    token: selectedToken,
    dirty,
    busy,
  });
  const canRun = runBlock === null;
  const serverField =
    submit.error instanceof IntentRejected && submit.error.reason.code === 'invalid'
      ? submit.error.reason.field
      : null;
  const update = (patch: Partial<TriggerDef>) => {
    submit.reset();
    setDraft((d) => ({ ...d, ...patch }));
  };

  return (
    <form
      className="trigger-editor"
      aria-label={trigger.definition.name}
      onSubmit={(e) => {
        e.preventDefault();
        if (busy || !dirty || errors.length) return;
        submit.mutate({
          intent: { kind: 'update_trigger', id: trigger.id, definition: draft },
          expectedVersion: trigger.version,
        });
      }}
    >
      <fieldset disabled={busy} className="trigger-form-fields">
        <header className="trigger-header">
          <Icon name="bolt" />
          <input
            className="trigger-name"
            aria-label={t('triggers.name')}
            aria-invalid={!draft.name.trim()}
            aria-describedby="trigger-errors"
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
            {t('triggers.oncePerMap', { defaultValue: 'Once per map' })}
          </label>
          {runBlock === 'spent' && <span className="badge">{t('triggers.fired')}</span>}
        </header>

        <fieldset className="trigger-section">
          <legend>{t('triggers.mapsTitle', { defaultValue: 'Maps' })}</legend>
          <p className="muted trigger-hint">
            {t('triggers.mapsHint', {
              defaultValue:
                'Map cards and connections are created on each board containing this trigger when you save.',
            })}
          </p>
          {maps.length > 1 && (
            <div
              className="trigger-maps-mode"
              role="radiogroup"
              aria-label={t('triggers.mapsTitle', { defaultValue: 'Maps' })}
            >
              <label className="inline">
                <input
                  type="radio"
                  checked={!manyMaps}
                  onChange={() => {
                    setManyMaps(false);
                    update({ map_ids: mapIds.slice(0, 1) });
                  }}
                />
                {t('triggers.oneMap', { defaultValue: 'One map' })}
              </label>
              <label className="inline">
                <input
                  type="radio"
                  checked={manyMaps}
                  onChange={() => {
                    setManyMaps(true);
                  }}
                />
                {t('triggers.manyMaps', { defaultValue: 'Several maps' })}
              </label>
            </div>
          )}
          {manyMaps ? (
            <>
              {maps.map((map) => (
                <label className="inline" key={map.id}>
                  <input
                    type="checkbox"
                    checked={mapIds.includes(map.id)}
                    onChange={(e) => {
                      update({
                        map_ids: e.target.checked
                          ? [...mapIds, map.id]
                          : mapIds.filter((id) => id !== map.id),
                      });
                    }}
                  />
                  {map.name}
                </label>
              ))}
              {mapIds
                .filter((id) => !maps.some((m) => m.id === id))
                .map((id) => (
                  <label key={id} className="inline">
                    <input
                      type="checkbox"
                      checked
                      onChange={() => {
                        update({ map_ids: mapIds.filter((m) => m !== id) });
                      }}
                    />
                    {t('picker.gone')}
                  </label>
                ))}
            </>
          ) : (
            <label className="trigger-field">
              <span>{t('triggers.map', { defaultValue: 'Map' })}</span>
              <select
                value={mapIds[0] ?? ''}
                onChange={(e) => {
                  update({ map_ids: e.target.value ? [e.target.value] : [] });
                }}
              >
                <option value="">
                  {t('triggers.chooseMap', { defaultValue: 'Choose a map…' })}
                </option>
                {maps.map((map) => (
                  <option key={map.id} value={map.id}>
                    {map.name}
                  </option>
                ))}
                {mapIds
                  .filter((id) => !maps.some((m) => m.id === id))
                  .map((id) => (
                    <option key={id} value={id}>
                      {t('picker.gone')}
                    </option>
                  ))}
              </select>
            </label>
          )}
          {boundMaps.length > 0 && (
            <div className="trigger-map-previews">
              {boundMaps.map((map) => {
                const asset = snapshot?.assets.find((a) => a.id === map.background_asset_id);
                return asset?.thumbnail_sha256 ? (
                  <img
                    key={map.id}
                    className="trigger-map-preview"
                    src={assetUrl(asset.thumbnail_sha256)}
                    alt={map.name}
                    title={map.name}
                  />
                ) : (
                  <span key={map.id} className="trigger-map-preview placeholder" title={map.name}>
                    {map.name}
                  </span>
                );
              })}
            </div>
          )}
          <p className="muted trigger-hint">
            {t('triggers.rearmHint', {
              defaultValue: 'Saving any edit rearms this trigger on every selected map.',
            })}
          </p>
        </fieldset>

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
            <div className="trigger-field">
              <label>
                <span>{t('triggers.zoneName', { defaultValue: 'Zone name' })}</span>
                <select
                  value={event.zone}
                  aria-invalid={!zones.some((z) => z.id === event.zone)}
                  onChange={(e) => {
                    update({ event: { ...event, zone: e.target.value } });
                  }}
                >
                  <option value="">
                    {t('triggers.chooseZone', { defaultValue: 'Choose a zone name…' })}
                  </option>
                  {zones
                    .filter((z) => mapIds.includes(z.map_id) || z.id === event.zone)
                    .map((zone) => (
                      <option key={zone.id} value={zone.id}>
                        {zone.name} —{' '}
                        {maps.find((m) => m.id === zone.map_id)?.name ?? t('picker.gone')}
                      </option>
                    ))}
                </select>
              </label>
              <p className="muted trigger-hint">
                {t('triggers.zoneNameHint', {
                  defaultValue:
                    'Matches this exact zone name on each selected map (case-sensitive, ignoring surrounding spaces).',
                })}
              </p>
            </div>
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
                mapIds={mapIds}
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

        {errors.length > 0 && (
          <ul id="trigger-errors" role="status" className="error-text">
            {errors.map((error) => (
              <li key={error}>{error}</li>
            ))}
          </ul>
        )}
        {serverField && (
          <p role="alert">
            {t('triggers.fieldRejected', {
              field: t(
                serverField === 'condition'
                  ? 'triggers.condition'
                  : serverField === 'steps'
                    ? 'triggers.thenTitle'
                    : serverField === 'event'
                      ? 'triggers.whenTitle'
                      : serverField === 'map_ids'
                        ? 'triggers.mapsTitle'
                        : 'triggers.name',
              ),
              defaultValue:
                'Check {{field}}: the server rejected this value. Your draft has been kept.',
            })}
          </p>
        )}
        <footer className="trigger-footer">
          {runBlock && trigger.definition.event.type === 'manual' && (
            <p id={`trigger-run-${trigger.id}`} className="trigger-run-status muted" role="status">
              {t(`triggers.runBlocked.${runBlock}`)}
            </p>
          )}
          <button
            type="submit"
            className="btn-primary"
            disabled={!dirty || busy || errors.length > 0}
          >
            {t('triggers.save')}
          </button>
          <button
            type="button"
            disabled={!dirty}
            onClick={() => {
              setDraft(initial);
              submit.reset();
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
                disabled={dirty || !canRun || busy}
                aria-describedby={runBlock ? `trigger-run-${trigger.id}` : undefined}
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
          <button
            type="button"
            disabled={busy}
            onClick={() => {
              setDeleting(true);
            }}
          >
            {t('triggers.delete')}
          </button>
        </footer>
      </fieldset>
      {deleting && (
        <div
          role="alertdialog"
          aria-label={t('triggers.delete')}
          className="confirm-bar"
          onKeyDown={(e) => {
            if (e.key === 'Escape') setDeleting(false);
          }}
        >
          <p>
            {t('triggers.deleteEverywhere', {
              defaultValue: 'Delete this trigger and its cards from every board?',
            })}
          </p>
          <button
            type="button"
            autoFocus
            disabled={busy}
            onClick={() => {
              if (busy) return;
              submit.mutate(
                {
                  intent: { kind: 'delete_trigger', id: trigger.id },
                  expectedVersion: trigger.version,
                },
                {
                  onSuccess: () => {
                    useSelection.getState().closeTab({ kind: 'trigger', id: trigger.id });
                  },
                },
              );
            }}
          >
            {t('common.confirmDelete')}
          </button>
          <button
            type="button"
            onClick={() => {
              setDeleting(false);
            }}
          >
            {t('journal.cancel')}
          </button>
        </div>
      )}
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
