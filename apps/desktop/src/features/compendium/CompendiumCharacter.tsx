import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { z } from 'zod';
import { CompendiumSheetPreview } from './CompendiumSheetPreview';
import './compendium-character.css';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import type { CompendiumEntryDto } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { Dialog } from '@/shared/ui/Dialog';
import { ErrorText } from '@/shared/ui/ErrorText';

const label = z.union([
  z.string().min(1).max(200),
  z
    .strictObject({
      ru: z.string().min(1).max(200).optional(),
      en: z.string().min(1).max(200).optional(),
    })
    .refine((v) => v.ru !== undefined || v.en !== undefined),
]);
const entryTemplate = z.object({
  character_template: z.strictObject({
    name: label.optional(),
    kind: z.enum(['pc', 'npc']),
    sheet_data: z.record(z.string(), z.unknown()),
  }),
});

/**
 * Creating a character from a compendium entry happens in its own modal: the GM reviews the form,
 * names the instance, creates it and may then place its token. The entry is never modified; the
 * authoritative core validates the campaign's current sheet schema.
 */
export function CompendiumCharacter({ entry }: { entry: CompendiumEntryDto }) {
  const { t, i18n } = useTranslation();
  const { data: campaign } = useCampaign();
  const maps = useSnapshotSelect((s) => s.maps) ?? [];
  const rules = useSnapshotSelect((s) => s.plugins.find((p) => p.id === s.system_plugin)?.system);
  const activeMapId = useSelection((s) => s.mapId);
  const openCharacter = useSelection((s) => s.openCharacter);
  const openMap = useSelection((s) => s.openMap);
  const selectToken = useSelection((s) => s.selectToken);
  const submit = useSubmitIntent();
  const [open, setOpen] = useState(false);
  const [mapId, setMapId] = useState(activeMapId ?? '');
  const [created, setCreated] = useState<{ id: string; name: string } | null>(null);
  const [customName, setCustomName] = useState<string | null>(null);

  const hasTemplate =
    typeof entry.data === 'object' &&
    entry.data !== null &&
    Object.hasOwn(entry.data, 'character_template');
  // Only an entry that carries a character template may create a character; every other entry
  // offered a character form with an empty sheet, which read as a stray dialog.
  if (!hasTemplate) return null;
  const parsed = entryTemplate.safeParse(entry.data);
  if (!campaign || !['gm', 'co_gm'].includes(campaign.role)) return null;

  const invalid = !parsed.success;
  const template = parsed.success ? parsed.data.character_template : null;
  const templateName = template?.name;
  const name =
    customName ??
    labelText(
      typeof templateName === 'object'
        ? { ru: templateName.ru ?? null, en: templateName.en ?? null }
        : (templateName ?? entry.name),
      i18n.resolvedLanguage ?? i18n.language,
    );
  const map = maps.find((m) => m.id === mapId);

  const close = () => {
    setOpen(false);
    // The next opening starts clean; an instance that was created stays created.
    setCreated(null);
    setCustomName(null);
  };

  const create = () => {
    if (!template) return;
    const id = uuidv7();
    submit.mutate(
      {
        intent: {
          kind: 'create_character',
          id,
          character_kind: template.kind,
          name: name.trim(),
          owner: template.kind === 'pc' ? campaign.user_id : null,
          sheet_json: JSON.stringify(template.sheet_data),
        },
      },
      {
        onSuccess: () => {
          setCreated({ id, name: name.trim() });
        },
      },
    );
  };

  const place = () => {
    if (!map || !created) return;
    const id = uuidv7();
    submit.mutate(
      {
        intent: {
          kind: 'create_token',
          id,
          map_id: map.id,
          character_id: created.id,
          asset_id: null,
          name: created.name,
          x: map.width * 8,
          y: map.height * 8,
          rotation: 0,
          scale: 1,
          layer: 'tokens',
          visible: true,
          disposition: 'neutral',
        },
      },
      {
        onSuccess: () => {
          openMap(map.id);
          selectToken(id);
          close();
        },
      },
    );
  };

  return (
    <div className="compendium-character">
      {invalid && (
        <p role="alert">
          {t('compendia.invalidTemplate', { defaultValue: 'Invalid character template.' })}
        </p>
      )}
      <button
        type="button"
        disabled={invalid}
        onClick={() => {
          setOpen(true);
        }}
      >
        {t('compendia.createCharacter', { defaultValue: 'Create character' })}
      </button>

      {open && template && (
        <Dialog
          title={t('compendia.characterInstance', { defaultValue: 'Create an instance' })}
          closeLabel={t('session.close')}
          size="lg"
          dismissible={!submit.isPending}
          onClose={close}
          footer={
            created ? (
              <>
                <span className="dialog-hint">{t('compendia.characterCreated')}</span>
                <button
                  type="button"
                  onClick={() => {
                    openCharacter(created.id);
                    close();
                  }}
                >
                  {t('compendia.openCharacter', { defaultValue: 'Open character sheet' })}
                </button>
                <button type="button" disabled={!map || submit.isPending} onClick={place}>
                  {t('compendia.placeToken', { defaultValue: 'Place token at map centre' })}
                </button>
                <button type="button" className="btn-primary" onClick={close}>
                  {t('common.close')}
                </button>
              </>
            ) : (
              <>
                {rules && <span className="dialog-hint">{t('compendia.sheetPreview')}</span>}
                <button type="button" onClick={close}>
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
            )
          }
        >
          {created ? (
            <div className="compendium-character-placement">
              <p role="status">
                {t('compendia.characterCreated', { defaultValue: 'Character created' })}
                {': '}
                {created.name}
              </p>
              <label>
                {t('compendia.tokenMap', { defaultValue: 'Map for token' })}
                <select
                  value={map ? mapId : ''}
                  onChange={(event) => {
                    setMapId(event.target.value);
                  }}
                >
                  <option value="">
                    {t('compendia.chooseMap', { defaultValue: 'Choose a map' })}
                  </option>
                  {maps.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.name}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          ) : (
            <>
              <label className="compendium-character-name">
                {t('compendia.instanceName', { defaultValue: 'Character name' })}
                <input
                  value={name}
                  maxLength={200}
                  onChange={(event) => {
                    setCustomName(event.target.value);
                  }}
                />
              </label>
              {rules && (
                <CompendiumSheetPreview
                  rules={rules}
                  kind={template.kind}
                  data={template.sheet_data}
                />
              )}
            </>
          )}
          <ErrorText error={submit.error} />
        </Dialog>
      )}
    </div>
  );
}
