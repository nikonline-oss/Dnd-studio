import { keepPreviousData, useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';
import { CharacterSheet } from '@/shared/sheet/CharacterSheet';
import type { JsonSchema, SheetData, SheetNode } from '@/shared/sheet/model';
import { useSubmitIntent } from '@/shared/game/useGame';
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
  const data = useMemo(() => parseSheet(character.sheet_json), [character.sheet_json]);
  const view = useQuery({
    queryKey: ['sheetView', character.sheet_json],
    queryFn: () => ipc.sheetView(character.sheet_json),
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

  return (
    <>
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
        rolls={rules.rolls}
        resources={resources}
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
