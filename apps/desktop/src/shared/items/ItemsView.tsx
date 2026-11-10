import { useState, type DragEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import { uuidv7 } from '@/shared/ipc/uuid';
import type { ItemDto, ItemKind, ItemStateDef, Label } from '@/shared/ipc/client';
import { campaignItemStates } from './itemStates';
import {
  CreateItemDialog,
  EditItemDialog,
  ItemEffectsDialog,
  ItemOwnerDialog,
  ItemSheetDialog,
} from './ItemDialogs';
import { IconButton } from '@/shared/ui/IconButton';
import { useListCommands } from '@/shared/ui/useListCommands';
import type { MenuItem } from '@/shared/ui/Menu';
import { Icon } from '@/shared/ui/Icon';
import { useSelection } from '@/shared/state/selection';
import { cardFields } from './cardFields';
import { ErrorText } from '@/shared/ui/ErrorText';

/** Drag payload of an inventory row: the item id. */
const ITEM_DRAG = 'application/x-dndstudio-item';

type DialogKind = 'create' | 'edit' | 'effects' | 'owner' | 'data';

/**
 * Campaign inventory cards, or one entry's details in its own center tab (ADR-0062).
 * Plugin fields are read-only in cards; edits continue through checked intents in dialogs.
 */
export function ItemsView({
  owner,
  kind = 'item',
  itemId,
}: {
  owner?: string;
  kind?: ItemKind;
  itemId?: string;
}) {
  const { t, i18n } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();
  const commands = useListCommands();
  const [selected, setSelected] = useState('');
  const [filter, setFilter] = useState(owner ?? 'all');
  const [dialog, setDialog] = useState<DialogKind | null>(null);
  const gm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const system = snapshot?.plugins.find((p) => p.id === snapshot.system_plugin)?.system;
  const defs = system?.mechanics?.item_types ?? [];
  const states = campaignItemStates(snapshot?.campaign.item_states);
  const entries = (snapshot?.items ?? []).filter((entry) => {
    if (itemId) return entry.id === itemId;
    if ((entry.kind ?? 'item') !== kind) return false;
    if (owner) return entry.owner === owner;
    if (filter === 'all') return true;
    if (filter === 'none') return !entry.owner;
    return entry.owner === filter;
  });
  const item = entries.find((entry) => entry.id === selected) ?? entries[0];
  const def = defs.find((d) => d.id === item?.type_id);
  const character = snapshot?.characters.find((c) => c.id === item?.owner);
  const canEdit =
    gm ||
    (campaign?.role === 'player' && !item?.is_template && character?.owner === campaign.user_id);
  const effects = item?.effects ?? [];
  const values = item?.values ?? [];
  const grants = item?.grants ?? [];

  /** Hands an entry to a character, or leaves it unowned; the GM's right. */
  const transfer = (id: string, target: string | null) => {
    const entry = snapshot?.items?.find((candidate) => candidate.id === id);
    if (!entry || entry.is_template || (entry.owner ?? null) === target) return;
    submit.mutate({
      intent: { kind: 'transfer_item', id, owner: target },
      expectedVersion: entry.version,
    });
  };

  const commandItems = (entry: ItemDto): MenuItem[] => {
    const owner = snapshot?.characters.find((c) => c.id === entry.owner);
    const editable =
      gm ||
      (campaign?.role === 'player' && !entry.is_template && owner?.owner === campaign.user_id);
    const open = (next: DialogKind) => {
      setSelected(entry.id);
      setDialog(next);
    };
    const items: MenuItem[] = editable
      ? [
          {
            label: t('common.edit'),
            onSelect: () => {
              open('edit');
            },
          },
        ]
      : [];
    if (!gm) return items;
    if (defs.some((d) => d.id === entry.type_id))
      items.push({
        label: t('items.data'),
        onSelect: () => {
          open('data');
        },
      });
    items.push({
      label: t('items.effects'),
      onSelect: () => {
        open('effects');
      },
    });
    items.push({
      label: t('items.give'),
      onSelect: () => {
        open('owner');
      },
    });
    items.push(
      {
        label: t('items.duplicate'),
        disabled: submit.isPending,
        onSelect: () => {
          const id = uuidv7();
          submit.mutate(
            { intent: { kind: 'duplicate_item', id: entry.id, new_id: id } },
            {
              onSuccess: () => {
                setSelected(id);
              },
            },
          );
        },
      },
      {
        label: t('items.delete'),
        disabled: submit.isPending,
        onSelect: () => {
          commands.confirm(t('items.delete'), () => {
            submit.mutate({
              intent: { kind: 'delete_item', id: entry.id },
              expectedVersion: entry.version,
            });
          });
        },
      },
    );
    return items;
  };

  return (
    <section className="items-view">
      <header className="control-page-header">
        {!owner && (
          <h2>
            {itemId
              ? (item?.name ?? t('items.empty'))
              : kind === 'skill'
                ? t('items.skillsTitle')
                : t('items.title')}
          </h2>
        )}
        {gm && !itemId && (
          <button
            type="button"
            className="btn-primary"
            onClick={() => {
              setDialog('create');
            }}
          >
            {t(kind === 'skill' ? 'items.newSkill' : 'items.newItem')}
          </button>
        )}
      </header>
      {!owner && !itemId && (
        <div className="items-filters" role="group" aria-label={t('items.filterByOwner')}>
          {(
            [
              ['all', t('items.filterAll'), null],
              ['none', t('items.noOwner'), null],
              ...(snapshot?.characters ?? []).map((c): [string, string, string] => [
                c.id,
                c.name,
                c.id,
              ]),
            ] as [string, string, string | null][]
          ).map(([value, text, dropOwner]) => {
            const dropProps =
              dropOwner === null && value !== 'none'
                ? {}
                : {
                    onDragOver: (e: DragEvent) => {
                      if (e.dataTransfer.types.includes(ITEM_DRAG)) e.preventDefault();
                    },
                    onDrop: (e: DragEvent) => {
                      const id = e.dataTransfer.getData(ITEM_DRAG);
                      if (!id) return;
                      e.preventDefault();
                      transfer(id, value === 'none' ? null : dropOwner);
                    },
                  };
            return (
              <button
                key={value}
                type="button"
                aria-pressed={filter === value}
                className="chip"
                title={dropProps.onDrop ? t('items.dropHere') : undefined}
                onClick={() => {
                  setFilter(value);
                }}
                {...dropProps}
              >
                {text}
              </button>
            );
          })}
        </div>
      )}
      <div className="items-workspace">
        {!itemId && (
          <nav className="items-list" aria-label={t('items.library')}>
            {entries.map((entry) => (
              <article className="item-card" key={entry.id}>
                <button
                  type="button"
                  className="item-card-open"
                  onContextMenu={(e) => {
                    commands.context(e, commandItems(entry));
                  }}
                  // Handing an entry over is the GM's move: players do not transfer entries.
                  draggable={!entry.is_template && gm}
                  onDragStart={(e) => {
                    e.dataTransfer.setData(ITEM_DRAG, entry.id);
                    e.dataTransfer.effectAllowed = 'move';
                  }}
                  onClick={() => {
                    useSelection.getState().openItem(entry.id);
                  }}
                >
                  <Icon
                    name={(entry.kind ?? 'item') === 'skill' ? 'sparkle' : 'backpack'}
                    size={26}
                  />
                  <strong>{entry.name}</strong>
                  <span className="muted">
                    {stateLabel(entry, states, lang, t)}
                    {entry.is_template && ` · ${t('items.template')}`}
                    {entry.owner &&
                      ` · ${snapshot?.characters.find((c) => c.id === entry.owner)?.name ?? ''}`}
                  </span>
                  <dl className="item-card-fields">
                    {cardFields(
                      entry,
                      defs.find((d) => d.id === entry.type_id),
                    ).map((field, index) => (
                      <div key={index}>
                        <dt>{labelText(field.label, lang)}</dt>
                        <dd>{field.value}</dd>
                      </div>
                    ))}
                  </dl>
                </button>
                {commandItems(entry).length > 0 && (
                  <IconButton
                    icon="more"
                    label={t('items.commands')}
                    onClick={(e) => {
                      commands.context(e, commandItems(entry));
                    }}
                  />
                )}
              </article>
            ))}
            {entries.length === 0 && (
              <p className="muted">{t(kind === 'skill' ? 'items.skillsEmpty' : 'items.empty')}</p>
            )}
          </nav>
        )}
        {itemId && item && (
          <section
            key={item.id}
            className="items-detail control-section"
            aria-label={t('items.details')}
            onContextMenu={(e) => {
              commands.context(e, commandItems(item));
            }}
          >
            <div className="items-detail-head">
              <h3>{item.name}</h3>
              <span className="chip">{stateLabel(item, states, lang, t)}</span>
            </div>
            <dl className="properties">
              <dt>{t('items.owner')}</dt>
              <dd>{character?.name ?? t('items.noOwner')}</dd>
              {effects.length > 0 && (
                <>
                  <dt>{t('items.effects')}</dt>
                  <dd className="items-chips">
                    {effects.map((effect) => (
                      <span key={effect.path} className="chip">
                        {effect.path} {effect.add > 0 ? `+${String(effect.add)}` : effect.add}
                      </span>
                    ))}
                  </dd>
                </>
              )}
              {values.length > 0 && (
                <>
                  <dt>{t('items.values')}</dt>
                  <dd className="items-chips">
                    {values.map((entry) => (
                      <span key={entry.key} className="chip">
                        {`@item.${entry.key}`} = {entry.value}
                      </span>
                    ))}
                  </dd>
                </>
              )}
              {grants.length > 0 && (
                <>
                  <dt>{t('items.grants')}</dt>
                  <dd className="items-chips">
                    {grants.map((grant) => (
                      <span key={grant} className="chip">
                        {grantLabel(system?.conditions, grant, lang)}
                      </span>
                    ))}
                  </dd>
                </>
              )}
            </dl>
            {item.description && <p className="items-description">{item.description}</p>}
            <div className="items-actions">
              {canEdit && (
                <button
                  type="button"
                  onClick={() => {
                    setDialog('edit');
                  }}
                >
                  {t('common.edit')}
                </button>
              )}
              {gm && (
                <IconButton
                  icon="more"
                  label={t('items.commands')}
                  onClick={(e) => {
                    commands.context(e, commandItems(item));
                  }}
                />
              )}
            </div>
            {!def && (item.kind ?? 'item') === 'item' && (
              <p className="muted">{t('items.freeItem')}</p>
            )}
          </section>
        )}
      </div>
      {commands.ui}
      <ErrorText error={submit.error} />
      {dialog === 'create' && (
        <CreateItemDialog
          kind={kind}
          onClose={() => {
            setDialog(null);
          }}
          onCreated={(id) => {
            setSelected(id);
            if (owner) {
              const instance = uuidv7();
              submit.mutate(
                { intent: { kind: 'instantiate_item', id: instance, template: id, owner } },
                {
                  onSuccess: () => {
                    useSelection.getState().openItem(instance);
                  },
                },
              );
            } else useSelection.getState().openItem(id);
          }}
        />
      )}
      {dialog === 'edit' && item && (
        <EditItemDialog
          item={item}
          onClose={() => {
            setDialog(null);
          }}
        />
      )}
      {dialog === 'effects' && item && (
        <ItemEffectsDialog
          item={item}
          onClose={() => {
            setDialog(null);
          }}
        />
      )}
      {dialog === 'data' && item && def && (
        <ItemSheetDialog
          item={item}
          def={def}
          onClose={() => {
            setDialog(null);
          }}
        />
      )}
      {dialog === 'owner' && item && (
        <ItemOwnerDialog
          item={item}
          onClose={() => {
            setDialog(null);
          }}
        />
      )}
    </section>
  );
}

/** "No state" means the campaign default: the first active state of the list. */
function stateLabel(
  item: ItemDto,
  states: ItemStateDef[],
  language: string,
  t: (key: string) => string,
): string {
  const state = item.state
    ? states.find((s) => s.id === item.state)
    : (states.find((s) => s.active) ?? states[0]);
  if (!state) return t('items.stateDefault');
  return labelText(state.label, language);
}

/** A granted condition's label, or its id when the system does not define it. */
function grantLabel(
  conditions: { id: string; label: Label }[] | undefined,
  id: string,
  lang: string,
) {
  const condition = conditions?.find((entry) => entry.id === id);
  return condition ? labelText(condition.label, lang) : id;
}
