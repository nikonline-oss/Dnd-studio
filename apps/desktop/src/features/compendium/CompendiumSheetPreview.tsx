import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useCampaign, usePluginRevision } from '@/shared/game/useGame';
import { ipc, type CharacterKind, type SystemRulesDto } from '@/shared/ipc/client';
import { CharacterSheet } from '@/shared/sheet/CharacterSheet';
import type { JsonSchema, SheetNode } from '@/shared/sheet/model';
import { ErrorText } from '@/shared/ui/ErrorText';

/** A read-only instance preview; all calculations still run through the authoritative rules. */
export function CompendiumSheetPreview({
  rules,
  kind,
  data,
}: {
  rules: SystemRulesDto;
  kind: CharacterKind;
  data: Record<string, unknown>;
}) {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const revision = usePluginRevision();
  const json = JSON.stringify(data);
  const view = useQuery({
    queryKey: ['compendium-sheet-preview', campaign?.path, revision, json],
    queryFn: () => ipc.sheetView(json),
  });
  const resources = (view.data?.resources ?? []).flatMap((value) => {
    const definition = rules.resources.find((resource) => resource.id === value.id);
    return definition && typeof value.value === 'number' && typeof value.max === 'number'
      ? [{ id: value.id, value: value.value, max: value.max, label: definition.label }]
      : [];
  });
  const computed: Record<string, number> = {};
  for (const [name, value] of Object.entries(view.data?.computed ?? {})) {
    if (typeof value === 'number') computed[name] = value;
  }
  return (
    <details className="compendium-character-preview" open>
      <summary>{t('compendia.sheetPreview', { defaultValue: 'Character sheet preview' })}</summary>
      <fieldset
        disabled
        aria-label={t('compendia.sheetPreview', { defaultValue: 'Character sheet preview' })}
      >
        <CharacterSheet
          layout={
            (kind === 'npc'
              ? (rules.creature_sheet_layout ?? rules.sheet_layout)
              : rules.sheet_layout) as SheetNode
          }
          schema={rules.sheet_schema as JsonSchema}
          data={data}
          computed={computed}
          resources={resources}
          rolls={rules.rolls}
          canEdit={false}
          onChange={() => {}}
          onRoll={() => {}}
        />
      </fieldset>
      <ErrorText error={view.error} />
    </details>
  );
}
