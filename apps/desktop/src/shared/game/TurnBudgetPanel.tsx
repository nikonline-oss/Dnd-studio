import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useSnapshotSelect } from './useGame';
import { ipc, type TurnEconomyView } from '@/shared/ipc/client';
import { labelText } from '@/shared/i18n/label';

export function TurnBudgetSummary({ economy }: { economy: TurnEconomyView | null | undefined }) {
  const { t, i18n } = useTranslation();
  if (!economy?.budgets.length) return null;
  return (
    <aside className="turn-budget-panel" aria-label={t('economy.title')}>
      <strong>{t('economy.title')}</strong>
      <p>
        {t(
          !economy.in_combat
            ? 'economy.outsideCombat'
            : !economy.turn_started
              ? 'economy.notStarted'
              : economy.own_turn
                ? 'economy.ownTurn'
                : 'economy.otherTurn',
        )}
      </p>
      {economy.in_combat && (
        <ul>
          {economy.budgets.map((budget) => (
            <li key={budget.resource}>
              <div>
                <span>{labelText(budget.label, i18n.language)}</span>
                <small>
                  {t(budget.refresh === 'own_turn' ? 'economy.resetOwn' : 'economy.resetEvery')}
                </small>
              </div>
              <strong>{budget.remaining}</strong>
              {budget.reserved > 0 && (
                <small>{t('economy.reserved', { count: budget.reserved })}</small>
              )}
            </li>
          ))}
        </ul>
      )}
    </aside>
  );
}

/** Formula evaluation stays in the kernel; this view only displays IPC-derived balances. */
export function TurnBudgetPanel({ characterId }: { characterId: string }) {
  const character = useSnapshotSelect((s) => s.characters.find((c) => c.id === characterId));
  const plugin = useSnapshotSelect((s) => s.plugins.find((p) => p.id === s.system_plugin));
  const stacks = useSnapshotSelect((s) => s.plugin_runtime?.action_stacks);
  const combats = useSnapshotSelect((s) => s.combats);
  const effects = useSnapshotSelect((s) => s.effects);
  const view = useQuery({
    queryKey: [
      'turnBudgetView',
      characterId,
      character?.sheet_json,
      character?.conditions,
      plugin,
      stacks,
      combats,
      effects,
    ],
    queryFn: () =>
      character ? ipc.sheetView(character.sheet_json, characterId) : Promise.resolve(null),
    enabled: !!character,
  });
  return <TurnBudgetSummary economy={view.data?.economy} />;
}
