import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import { uuidv7 } from '@/shared/ipc/uuid';
import type {
  ItemDto,
  ItemEffect,
  ItemKind,
  ItemValue,
  Label,
  Snapshot,
} from '@/shared/ipc/client';
import { campaignItemStates } from './itemStates';
import { CharacterSheet } from '@/shared/sheet/CharacterSheet';
import type { JsonSchema, SheetData, SheetNode } from '@/shared/sheet/model';
import type { ItemTypeDef } from '@/shared/ipc/client';
import { Dialog } from '@/shared/ui/Dialog';
import { ErrorText } from '@/shared/ui/ErrorText';

/** A dialog writes one intent and closes; the card behind only shows the result (ADR-0062). */

/** Creates an item or a skill: name, an optional plugin type, a description. */
export function CreateItemDialog({
  kind,
  onClose,
  onCreated,
}: {
  kind: ItemKind;
  onClose: () => void;
  onCreated: (id: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const snapshot = useSnapshot().data;
  const submit = useSubmitIntent();
  const [name, setName] = useState('');
  const [typeId, setTypeId] = useState('');
  const [description, setDescription] = useState('');
  const types =
    snapshot?.plugins.find((p) => p.id === snapshot.system_plugin)?.system?.mechanics?.item_types ??
    [];

  const create = () => {
    const id = uuidv7();
    submit.mutate(
      {
        intent: {
          kind: 'create_item',
          id,
          item_kind: kind,
          name: name.trim(),
          type_id: typeId || null,
          description,
          sheet_json: null,
          visible_to_players: false,
        },
      },
      {
        onSuccess: () => {
          onCreated(id);
          onClose();
        },
      },
    );
  };

  return (
    <Dialog
      title={t(kind === 'skill' ? 'items.newSkill' : 'items.newItem')}
      closeLabel={t('common.cancel')}
      size="sm"
      dismissible={!submit.isPending}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className="btn-primary"
            disabled={submit.isPending || !name.trim()}
            onClick={create}
          >
            {t('common.create')}
          </button>
        </>
      }
    >
      <div className="form-stack">
        <label className="control-field">
          {t('items.name')}
          <input
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            maxLength={200}
            placeholder={t('items.name')}
          />
        </label>
        <label className="control-field">
          {t('items.type')}
          <select
            value={typeId}
            onChange={(e) => {
              setTypeId(e.target.value);
            }}
          >
            <option value="">{t('items.noType')}</option>
            {types.map((def) => (
              <option key={def.id} value={def.id}>
                {labelText(def.label, i18n.resolvedLanguage ?? i18n.language)}
              </option>
            ))}
          </select>
        </label>
        <label className="control-field">
          {t('items.description')}
          <textarea
            rows={3}
            value={description}
            onChange={(e) => {
              setDescription(e.target.value);
            }}
            maxLength={20_000}
          />
        </label>
        <ErrorText error={submit.error} />
      </div>
    </Dialog>
  );
}

/** Renames an entry and edits its text; the GM's edit, the owner's too. */
export function EditItemDialog({ item, onClose }: { item: ItemDto; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const snapshot = useSnapshot().data;
  const submit = useSubmitIntent();
  const [name, setName] = useState(item.name);
  const [description, setDescription] = useState(item.description ?? '');
  const [state, setState] = useState(item.state ?? '');
  const states = campaignItemStates(snapshot?.campaign.item_states);

  const save = () => {
    submit.mutate(
      {
        intent: {
          kind: 'update_item',
          id: item.id,
          name: name.trim(),
          description,
          sheet_json: null,
          visible_to_players: null,
          state,
          effects: null,
          grants: null,
          values: null,
        },
        expectedVersion: item.version,
      },
      { onSuccess: onClose },
    );
  };

  return (
    <Dialog
      title={t('items.editTitle', { name: item.name })}
      closeLabel={t('common.cancel')}
      dismissible={!submit.isPending}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className="btn-primary"
            disabled={submit.isPending || !name.trim()}
            onClick={save}
          >
            {t('common.save')}
          </button>
        </>
      }
    >
      <div className="form-stack">
        <label className="control-field">
          {t('items.name')}
          <input
            value={name}
            maxLength={200}
            onChange={(e) => {
              setName(e.target.value);
            }}
          />
        </label>
        <label className="control-field">
          {t('items.description')}
          <textarea
            rows={5}
            value={description}
            maxLength={20_000}
            onChange={(e) => {
              setDescription(e.target.value);
            }}
          />
        </label>
        <label className="control-field">
          {t('items.state')}
          <select
            value={state}
            onChange={(e) => {
              setState(e.target.value);
            }}
          >
            <option value="">{t('items.stateDefault')}</option>
            {states.map((entry) => (
              <option key={entry.id} value={entry.id}>
                {labelText(entry.label, i18n.resolvedLanguage ?? i18n.language)}
              </option>
            ))}
          </select>
        </label>
        <ErrorText error={submit.error} />
      </div>
    </Dialog>
  );
}

/**
 * What the entry does: numeric adjustments to sheet fields, the conditions it grants and the
 * custom values plugins read as `@item.<key>`. Only the GM edits this (ADR-0062).
 */
export function ItemEffectsDialog({ item, onClose }: { item: ItemDto; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const snapshot = useSnapshot().data;
  const submit = useSubmitIntent();
  const [effects, setEffects] = useState<ItemEffect[]>(item.effects ?? []);
  const [values, setValues] = useState<ItemValue[]>(item.values ?? []);
  const [grants, setGrants] = useState<string[]>(item.grants ?? []);
  const [path, setPath] = useState('');
  const [add, setAdd] = useState('1');
  const [key, setKey] = useState('');
  const [value, setValue] = useState('0');
  const conditions =
    snapshot?.plugins.find((p) => p.id === snapshot.system_plugin)?.system?.conditions ?? [];
  const paths = numericSheetPaths(snapshot);

  const addEffect = () => {
    const amount = Number(add);
    if (!path.trim() || !Number.isFinite(amount) || amount === 0) return;
    setEffects([...effects, { path: path.trim(), add: amount, minimum: null, maximum: null }]);
    setPath('');
    setAdd('1');
  };

  const addValue = () => {
    const trimmed = key.trim();
    const amount = Number(value);
    if (!/^[A-Za-z0-9_-]{1,40}$/.test(trimmed) || !Number.isFinite(amount)) return;
    if (values.some((entry) => entry.key === trimmed)) return;
    setValues([...values, { key: trimmed, value: amount }]);
    setKey('');
    setValue('0');
  };

  const save = () => {
    submit.mutate(
      {
        intent: {
          kind: 'update_item',
          id: item.id,
          name: null,
          description: null,
          sheet_json: null,
          visible_to_players: null,
          state: null,
          effects,
          grants,
          values,
        },
        expectedVersion: item.version,
      },
      { onSuccess: onClose },
    );
  };

  return (
    <Dialog
      title={t('items.effectsTitle', { name: item.name })}
      closeLabel={t('common.cancel')}
      size="lg"
      dismissible={!submit.isPending}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button type="button" className="btn-primary" disabled={submit.isPending} onClick={save}>
            {t('common.save')}
          </button>
        </>
      }
    >
      <div className="form-stack">
        <section>
          <h4>{t('items.effects')}</h4>
          <p className="muted">{t('items.effectsHint')}</p>
          {effects.length > 0 && (
            <ul className="item-effects-list">
              {effects.map((effect, index) => (
                <li key={`${effect.path}:${String(index)}`}>
                  <code>{effect.path}</code>
                  <span>{effect.add > 0 ? `+${String(effect.add)}` : effect.add}</span>
                  <button
                    type="button"
                    className="btn-ghost"
                    aria-label={t('items.removeEffect')}
                    onClick={() => {
                      setEffects(effects.filter((_, i) => i !== index));
                    }}
                  >
                    ×
                  </button>
                </li>
              ))}
            </ul>
          )}
          <div className="item-add-row">
            <input
              aria-label={t('items.effectPath')}
              list="item-effect-paths"
              placeholder={t('items.effectPath')}
              value={path}
              onChange={(e) => {
                setPath(e.target.value);
              }}
            />
            <datalist id="item-effect-paths">
              {paths.map((entry) => (
                <option key={entry} value={entry} />
              ))}
            </datalist>
            <input
              aria-label={t('items.effectAdd')}
              type="number"
              value={add}
              onChange={(e) => {
                setAdd(e.target.value);
              }}
            />
            <button type="button" onClick={addEffect}>
              {t('common.add')}
            </button>
          </div>
        </section>
        <section>
          <h4>{t('items.values')}</h4>
          <p className="muted">{t('items.valuesHint')}</p>
          {values.length > 0 && (
            <ul className="item-effects-list">
              {values.map((entry, index) => (
                <li key={entry.key}>
                  <code>{`@item.${entry.key}`}</code>
                  <span>{entry.value}</span>
                  <button
                    type="button"
                    className="btn-ghost"
                    aria-label={t('items.removeValue')}
                    onClick={() => {
                      setValues(values.filter((_, i) => i !== index));
                    }}
                  >
                    ×
                  </button>
                </li>
              ))}
            </ul>
          )}
          <div className="item-add-row">
            <input
              aria-label={t('items.valueKey')}
              placeholder={t('items.valueKey')}
              value={key}
              maxLength={40}
              onChange={(e) => {
                setKey(e.target.value);
              }}
            />
            <input
              aria-label={t('items.valueAmount')}
              type="number"
              value={value}
              onChange={(e) => {
                setValue(e.target.value);
              }}
            />
            <button type="button" onClick={addValue}>
              {t('common.add')}
            </button>
          </div>
        </section>
        <section>
          <h4>{t('items.grants')}</h4>
          <div className="items-grants">
            {conditions.map((condition: { id: string; label: Label }) => (
              <label key={condition.id} className="control-checkbox">
                <input
                  type="checkbox"
                  checked={grants.includes(condition.id)}
                  onChange={(e) => {
                    setGrants(
                      e.target.checked
                        ? [...grants, condition.id]
                        : grants.filter((id) => id !== condition.id),
                    );
                  }}
                />
                {labelText(condition.label, i18n.resolvedLanguage ?? i18n.language)}
              </label>
            ))}
          </div>
        </section>
        <ErrorText error={submit.error} />
      </div>
    </Dialog>
  );
}

/** Hands an entry to a character (or to nobody). */
export function ItemOwnerDialog({ item, onClose }: { item: ItemDto; onClose: () => void }) {
  const { t } = useTranslation();
  const snapshot = useSnapshot().data;
  const submit = useSubmitIntent();
  const [owner, setOwner] = useState(item.owner ?? '');

  const save = () => {
    submit.mutate(
      {
        intent: { kind: 'transfer_item', id: item.id, owner: owner || null },
        expectedVersion: item.version,
      },
      { onSuccess: onClose },
    );
  };

  return (
    <Dialog
      title={t('items.ownerTitle', { name: item.name })}
      closeLabel={t('common.cancel')}
      size="sm"
      dismissible={!submit.isPending}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button type="button" className="btn-primary" disabled={submit.isPending} onClick={save}>
            {t('common.save')}
          </button>
        </>
      }
    >
      <label className="control-field">
        {t('items.owner')}
        <select
          value={owner}
          onChange={(e) => {
            setOwner(e.target.value);
          }}
        >
          <option value="">{t('items.noOwner')}</option>
          {snapshot?.characters.map((character) => (
            <option key={character.id} value={character.id}>
              {character.name}
            </option>
          ))}
        </select>
      </label>
      <ErrorText error={submit.error} />
    </Dialog>
  );
}

/** Field paths an effect may point at: the sheet's top-level integer fields. */
function numericSheetPaths(snapshot: Snapshot | undefined): string[] {
  const schema = snapshot?.plugins.find((p) => p.id === snapshot.system_plugin)?.system
    ?.sheet_schema as { properties?: Record<string, { type?: string }> } | undefined;
  return Object.entries(schema?.properties ?? {})
    .filter(([, value]) => value.type === 'integer')
    .map(([name]) => name);
}

/**
 * The plugin data of a typed entry (charges, wear, whatever the item type declares) — in its own
 * dialog, so the card stays a card. The system validates the result like any sheet.
 */
export function ItemSheetDialog({
  item,
  def,
  onClose,
}: {
  item: ItemDto;
  def: ItemTypeDef;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const [data, setData] = useState<SheetData>(() => {
    try {
      return JSON.parse(item.sheet_json) as SheetData;
    } catch {
      return {};
    }
  });

  return (
    <Dialog
      title={t('items.dataTitle', { name: item.name })}
      closeLabel={t('common.cancel')}
      size="lg"
      dismissible={!submit.isPending}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className="btn-primary"
            disabled={submit.isPending}
            onClick={() => {
              submit.mutate(
                {
                  intent: {
                    kind: 'update_item',
                    id: item.id,
                    name: null,
                    description: null,
                    sheet_json: JSON.stringify(data),
                    visible_to_players: null,
                    state: null,
                    effects: null,
                    grants: null,
                    values: null,
                  },
                  expectedVersion: item.version,
                },
                { onSuccess: onClose },
              );
            }}
          >
            {t('common.save')}
          </button>
        </>
      }
    >
      <CharacterSheet
        layout={def.sheet_layout as SheetNode}
        schema={def.sheet_schema as JsonSchema}
        data={data}
        computed={{}}
        resources={[]}
        rolls={[]}
        canEdit={!submit.isPending}
        onRoll={() => undefined}
        onChange={setData}
      />
      <ErrorText error={submit.error} />
    </Dialog>
  );
}
