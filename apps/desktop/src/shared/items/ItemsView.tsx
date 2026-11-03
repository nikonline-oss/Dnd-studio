import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import { uuidv7 } from '@/shared/ipc/uuid';
import { CharacterSheet } from '@/shared/sheet/CharacterSheet';
import type { JsonSchema, SheetData, SheetNode } from '@/shared/sheet/model';
import { ErrorText } from '@/shared/ui/ErrorText';

export function ItemsView({ owner }: { owner?: string }) {
  const { t, i18n } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();
  const [selected, setSelected] = useState('');
  const [typeId, setTypeId] = useState('');
  const [name, setName] = useState('');
  const [recipient, setRecipient] = useState(owner ?? '');
  const gm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const defs =
    snapshot?.plugins.find((p) => p.id === snapshot.system_plugin)?.system?.mechanics?.item_types ??
    [];
  const items = (snapshot?.items ?? []).filter((i) => !owner || i.owner === owner);
  const item = items.find((i) => i.id === selected) ?? items[0];
  const def = defs.find((d) => d.id === item?.type_id);
  const character = snapshot?.characters.find((c) => c.id === item?.owner);
  const canEdit =
    gm ||
    (campaign?.role === 'player' && !item?.is_template && character?.owner === campaign.user_id);
  let data: SheetData = {};
  try {
    data = JSON.parse(item?.sheet_json ?? '{}') as SheetData;
  } catch {
    /* Stored data is validated by the authority. */
  }
  return (
    <section className="items-view">
      <h3>{t('items.title')}</h3>
      {gm && !owner && (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            submit.mutate(
              {
                intent: {
                  kind: 'create_item',
                  id: uuidv7(),
                  name,
                  type_id: typeId || defs[0]?.id || '',
                  sheet_json: null,
                  visible_to_players: false,
                },
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
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            placeholder={t('items.name')}
            maxLength={200}
          />
          <select
            aria-label={t('items.type')}
            value={typeId || defs[0]?.id || ''}
            onChange={(e) => {
              setTypeId(e.target.value);
            }}
          >
            {defs.map((d) => (
              <option key={d.id} value={d.id}>
                {labelText(d.label, i18n.language)}
              </option>
            ))}
          </select>
          <button disabled={!name.trim() || !defs.length || submit.isPending}>
            {t('items.createTemplate')}
          </button>
        </form>
      )}
      <div className="items-list">
        {items.map((i) => (
          <button
            key={i.id}
            aria-pressed={item?.id === i.id}
            onClick={() => {
              setSelected(i.id);
            }}
          >
            {i.name} · {t(i.is_template ? 'items.template' : 'items.instance')}
            {i.owner && ` · ${snapshot?.characters.find((c) => c.id === i.owner)?.name ?? ''}`}
          </button>
        ))}
      </div>
      {item && (
        <div key={item.id}>
          <h4>{item.name}</h4>
          {gm && (
            <>
              <label>
                {t('items.name')}
                <input
                  defaultValue={item.name}
                  onBlur={(e) => {
                    if (e.target.value.trim() && e.target.value !== item.name)
                      submit.mutate({
                        intent: {
                          kind: 'update_item',
                          id: item.id,
                          name: e.target.value,
                          sheet_json: null,
                          visible_to_players: null,
                        },
                        expectedVersion: item.version,
                      });
                  }}
                />
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={item.visible_to_players}
                  onChange={(e) => {
                    submit.mutate({
                      intent: {
                        kind: 'update_item',
                        id: item.id,
                        name: null,
                        sheet_json: null,
                        visible_to_players: e.target.checked,
                      },
                      expectedVersion: item.version,
                    });
                  }}
                />
                {t('items.shared')}
              </label>
              <label>
                {t('items.owner')}
                <select
                  value={item.is_template ? recipient : (item.owner ?? '')}
                  onChange={(e) => {
                    if (item.is_template) {
                      setRecipient(e.target.value);
                    } else {
                      submit.mutate({
                        intent: {
                          kind: 'transfer_item',
                          id: item.id,
                          owner: e.target.value || null,
                        },
                        expectedVersion: item.version,
                      });
                    }
                  }}
                >
                  <option value="">{t('items.noOwner')}</option>
                  {snapshot?.characters.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.name}
                    </option>
                  ))}
                </select>
              </label>
              {item.is_template && (
                <button
                  disabled={submit.isPending}
                  onClick={() => {
                    const id = uuidv7();
                    submit.mutate(
                      {
                        intent: {
                          kind: 'instantiate_item',
                          id,
                          template: item.id,
                          owner: recipient || null,
                        },
                      },
                      {
                        onSuccess: () => {
                          setSelected(id);
                        },
                      },
                    );
                  }}
                >
                  {t('items.instantiate')}
                </button>
              )}
            </>
          )}
          {def ? (
            <CharacterSheet
              layout={def.sheet_layout as SheetNode}
              schema={def.sheet_schema as JsonSchema}
              data={data}
              computed={{}}
              resources={[]}
              rolls={[]}
              canEdit={canEdit && !submit.isPending}
              onRoll={() => undefined}
              onChange={(next) => {
                submit.mutate({
                  intent: {
                    kind: 'update_item',
                    id: item.id,
                    name: null,
                    sheet_json: JSON.stringify(next),
                    visible_to_players: null,
                  },
                  expectedVersion: item.version,
                });
              }}
            />
          ) : (
            <p>{t('items.missingType')}</p>
          )}
          {gm && (
            <button
              onClick={() => {
                submit.mutate({
                  intent: { kind: 'delete_item', id: item.id },
                  expectedVersion: item.version,
                });
              }}
            >
              {t('items.delete')}
            </button>
          )}
        </div>
      )}
      {!items.length && <p className="muted">{t('items.empty')}</p>}
      <ErrorText error={submit.error} />
    </section>
  );
}
