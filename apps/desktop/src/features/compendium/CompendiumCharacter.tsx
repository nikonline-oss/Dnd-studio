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

/** Independent instances; the authoritative core validates the campaign's current sheet schema. */
export function CompendiumCharacter({ entry }: { entry: CompendiumEntryDto }) {
  const { t, i18n } = useTranslation();
  const { data: campaign } = useCampaign();
  const maps = useSnapshotSelect((s) => s.maps) ?? [];
  const rules = useSnapshotSelect((s) => s.plugins.find((p) => p.id === s.system_plugin)?.system);
  const activeMapId = useSelection((s) => s.mapId);
  const openCharacter = useSelection((s) => s.openCharacter);
  const openMap = useSelection((s) => s.openMap);
  const selectToken = useSelection((s) => s.selectToken);
  const [mapId, setMapId] = useState(activeMapId ?? '');
  const [created, setCreated] = useState<{ id: string; name: string } | null>(null);
  const [customName, setCustomName] = useState<string | null>(null);
  const submit = useSubmitIntent();
  const hasTemplate =
    typeof entry.data === 'object' &&
    entry.data !== null &&
    Object.hasOwn(entry.data, 'character_template');
  const parsed = entryTemplate.safeParse(
    hasTemplate ? entry.data : { character_template: { kind: 'npc', sheet_data: {} } },
  );
  if (!campaign || !['gm', 'co_gm'].includes(campaign.role)) return null;
  if (!parsed.success)
    return (
      <div>
        <p role="alert">
          {t('compendia.invalidTemplate', { defaultValue: 'Invalid character template.' })}
        </p>
        <button type="button" disabled>
          {t('compendia.createCharacter', { defaultValue: 'Create character' })}
        </button>
      </div>
    );
  const template = parsed.data.character_template;
  const templateName = template.name;
  const name =
    customName ??
    labelText(
      typeof templateName === 'object'
        ? { ru: templateName.ru ?? null, en: templateName.en ?? null }
        : (templateName ?? entry.name),
      i18n.resolvedLanguage ?? i18n.language,
    );
  const map = maps.find((m) => m.id === mapId);
  return (
    <div className="compendium-character">
      <h3>{t('compendia.characterInstance', { defaultValue: 'Create an instance' })}</h3>
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
        <CompendiumSheetPreview rules={rules} kind={template.kind} data={template.sheet_data} />
      )}
      {!hasTemplate && (
        <p className="muted">
          {t('compendia.defaultSheetHint', {
            defaultValue: 'No character template: creates an editable sheet with system defaults.',
          })}
        </p>
      )}
      <button
        type="button"
        disabled={submit.isPending || !name.trim()}
        onClick={() => {
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
        }}
      >
        {t('compendia.createCharacter', { defaultValue: 'Create character' })}
      </button>
      {created && (
        <div className="compendium-character-placement">
          <p role="status">
            {t('compendia.characterCreated', {
              defaultValue: 'Character created',
            })}
            {': '}
            {created.name}
          </p>
          <button
            type="button"
            onClick={() => {
              openCharacter(created.id);
            }}
          >
            {t('compendia.openCharacter', { defaultValue: 'Open character sheet' })}
          </button>
          <label>
            {t('compendia.tokenMap', { defaultValue: 'Map for token' })}
            <select
              value={map ? mapId : ''}
              onChange={(event) => {
                setMapId(event.target.value);
              }}
            >
              <option value="">{t('compendia.chooseMap', { defaultValue: 'Choose a map' })}</option>
              {maps.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            disabled={!map || submit.isPending}
            onClick={() => {
              if (!map) return;
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
                  },
                },
                {
                  onSuccess: () => {
                    openMap(map.id);
                    selectToken(id);
                  },
                },
              );
            }}
          >
            {t('compendia.placeToken', { defaultValue: 'Place token at map centre' })}
          </button>
        </div>
      )}
      <ErrorText error={submit.error} />
    </div>
  );
}
