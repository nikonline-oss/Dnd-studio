import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import type { CompendiumEntryDto, ItemKind } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { Dialog } from '@/shared/ui/Dialog';
import { ErrorText } from '@/shared/ui/ErrorText';

/**
 * A compendium entry becomes an inventory entry (ADR-0062): any entry can be turned into an item
 * or a skill — the name and text come from the entry, the GM may adjust them before creating.
 * Entries that carry a character template get their own creation form instead.
 */
export function CompendiumCreate({
  entry,
  text,
}: {
  entry: CompendiumEntryDto;
  /** The entry's readable text, used as the description of the created entry. */
  text: string;
}) {
  const { t, i18n } = useTranslation();
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();
  const [kind, setKind] = useState<ItemKind | null>(null);
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  if (!isGm) return null;

  const start = (next: ItemKind) => {
    setKind(next);
    setName(labelText(entry.name, i18n.resolvedLanguage ?? i18n.language));
    setDescription(text.slice(0, 20_000));
  };

  const create = () => {
    if (!kind) return;
    submit.mutate(
      {
        intent: {
          kind: 'create_item',
          id: uuidv7(),
          item_kind: kind,
          name: name.trim(),
          type_id: null,
          description,
          sheet_json: null,
          visible_to_players: false,
        },
      },
      {
        onSuccess: () => {
          setKind(null);
        },
      },
    );
  };

  return (
    <div className="compendium-create">
      <button
        type="button"
        onClick={() => {
          start('item');
        }}
      >
        {t('items.newItem')}
      </button>
      <button
        type="button"
        onClick={() => {
          start('skill');
        }}
      >
        {t('items.newSkill')}
      </button>
      {kind && (
        <Dialog
          title={t(kind === 'skill' ? 'items.newSkill' : 'items.newItem')}
          closeLabel={t('common.cancel')}
          dismissible={!submit.isPending}
          onClose={() => {
            setKind(null);
          }}
          footer={
            <>
              <button
                type="button"
                onClick={() => {
                  setKind(null);
                }}
              >
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
                maxLength={200}
                onChange={(e) => {
                  setName(e.target.value);
                }}
              />
            </label>
            <label className="control-field">
              {t('items.description')}
              <textarea
                rows={8}
                value={description}
                maxLength={20_000}
                onChange={(e) => {
                  setDescription(e.target.value);
                }}
              />
            </label>
            <ErrorText error={submit.error} />
          </div>
        </Dialog>
      )}
    </div>
  );
}
