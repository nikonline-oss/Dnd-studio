import { ItemsView } from '@/shared/items/ItemsView';
import { CharacterActions, CharacterSheetAction } from './CharacterActions';
import { keepPreviousData, useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';
import { TurnBudgetSummary } from '@/shared/game/TurnBudgetPanel';
import { CharacterSheet } from '@/shared/sheet/CharacterSheet';
import type { JsonSchema, SheetData, SheetNode } from '@/shared/sheet/model';
import { useTranslation } from 'react-i18next';
import { labelText } from '@/shared/i18n/label';
import { useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { ipc, type CharacterDto, type SystemRulesDto } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { ErrorText } from '@/shared/ui/ErrorText';

function parseSheet(sheet: string): SheetData {
  try {
    const value: unknown = JSON.parse(sheet);
    return typeof value === 'object' && value !== null && !Array.isArray(value)
      ? (value as SheetData)
      : {};
  } catch {
    return {};
  }
}

/**
 * The character sheet of the campaign's system plugin (ADR-0023): the plugin's layout over the
 * character's data. Formula fields and resources come from the core's own evaluation
 * (`sheet_view`); edits and roll buttons become intents.
 */
export function SystemSheet({
  character,
  rules,
  canEdit,
}: {
  character: CharacterDto;
  rules: SystemRulesDto;
  canEdit: boolean;
}) {
  const submit = useSubmitIntent();
  const { t, i18n } = useTranslation();
  const effects = useSnapshotSelect((s) =>
    (s.effects ?? []).filter((e) =>
      s.tokens.some((token) => token.id === e.token && token.character_id === character.id),
    ),
  );
  const flags = useSnapshotSelect(
    (s) => s.plugins.find((p) => p.id === s.system_plugin)?.feature_settings,
  );
  const stacks = useSnapshotSelect((s) => s.plugin_runtime?.action_stacks);
  const combats = useSnapshotSelect((s) => s.combats);
  const data = useMemo(() => parseSheet(character.sheet_json), [character.sheet_json]);
  // Active items change effective fields (ADR-0062); their versions make the key move.
  const itemKey = useSnapshotSelect((s) =>
    (s.items ?? [])
      .filter((item) => item.owner === character.id)
      .map((item) => `${item.id}:${String(item.version)}`)
      .sort()
      .join('|'),
  );
  const view = useQuery({
    queryKey: [
      'sheetView',
      character.id,
      character.sheet_json,
      character.conditions,
      rules,
      flags,
      effects,
      itemKey,
      stacks,
      combats,
    ],
    queryFn: () => ipc.sheetView(character.sheet_json, character.id),
    placeholderData: keepPreviousData,
  });
  const resources = rules.resources.map((r) => {
    const value = view.data?.resources.find((v) => v.id === r.id);
    return { id: r.id, label: r.label, value: value?.value ?? 0, max: value?.max ?? 0 };
  });
  const computed: Record<string, number> = {};
  for (const [name, value] of Object.entries(view.data?.computed ?? {})) {
    if (typeof value === 'number') computed[name] = value;
  }
  // Identity follows the numbers, not every refetch: the sheet re-renders fields only when an
  // adjustment really changed (ADR-0062).
  const adjustedKey = JSON.stringify(view.data?.adjusted ?? {});
  const adjusted = useMemo(() => JSON.parse(adjustedKey) as Record<string, number>, [adjustedKey]);

  return (
    <>
      {(view.data?.modifiers ?? []).length > 0 && (
        <aside className="sheet-modifiers" aria-label={t('plugins.activeModifiers')}>
          <strong>{t('plugins.activeModifiers')}</strong>
          <ul>
            {view.data?.modifiers?.map((label, index) => (
              <li key={index}>{labelText(label, i18n.language)}</li>
            ))}
          </ul>
        </aside>
      )}
      <TurnBudgetSummary economy={view.data?.economy} />
      {!rules.mechanics?.sheet_actions?.length && <CharacterActions character={character} />}
      <details>
        <summary>{t('items.title')}</summary>
        <ItemsView owner={character.id} />
      </details>
      <CharacterSheet
        // Checked by the plugin loader and again by the core when the plugin was installed.
        layout={
          (character.kind === 'npc'
            ? (rules.creature_sheet_layout ?? rules.sheet_layout)
            : rules.sheet_layout) as SheetNode
        }
        schema={rules.sheet_schema as JsonSchema}
        data={data}
        computed={computed}
        adjusted={adjusted}
        rolls={rules.rolls}
        resources={resources}
        renderAction={(collection, path, change) => (
          <CharacterSheetAction
            character={character}
            collection={collection}
            path={path}
            canEdit={canEdit && !submit.isPending}
            onChange={change}
          />
        )}
        canEdit={canEdit}
        onChange={(next) => {
          submit.mutate({
            intent: {
              kind: 'update_character',
              id: character.id,
              name: null,
              sheet_json: JSON.stringify(next),
            },
            expectedVersion: character.version,
          });
        }}
        onRoll={(rollId) => {
          const roll = rules.rolls.find((r) => r.id === rollId);
          if (!roll) return;
          submit.mutate({
            intent: {
              kind: 'roll_dice',
              id: uuidv7(),
              expression: roll.dice,
              hidden: false,
              character_id: character.id,
              // The system's roll: the core uses its formula, and triggers know which it was.
              roll: roll.id,
            },
          });
        }}
      />
      <ErrorText error={submit.error} />
    </>
  );
}
