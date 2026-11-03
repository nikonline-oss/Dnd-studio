import { beginActionAim } from '@/shared/state/actionAim';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { useRemote } from '@/shared/game/useRemote';
import { useSelection } from '@/shared/state/selection';
import { labelText } from '@/shared/i18n/label';
import { GameClockPanel } from '@/shared/game/GameClockPanel';
import { TurnBudgetPanel } from '@/shared/game/TurnBudgetPanel';
import { ActionWindows } from '@/features/plugins/ActionWindows';
import { MaintainedEffects } from '@/features/plugins/MaintainedEffects';
import { PluginReactions } from '@/features/plugins/PluginReactions';
import { InitiativePanel } from '@/features/initiative/InitiativePanel';
import { ErrorText } from '@/shared/ui/ErrorText';
import type { GmPanelSection } from '@/shared/ipc/client';

const DEFAULT_SECTIONS: GmPanelSection[] = [
  {
    id: 'clock',
    kind: 'clock',
    label: { ru: 'Время мира', en: 'World time' },
    description: '',
    items: [],
  },
  {
    id: 'overview',
    kind: 'overview',
    label: { ru: 'Сцена и бой', en: 'Scene and combat' },
    description: '',
    items: [],
  },
  {
    id: 'reactions',
    kind: 'reactions',
    label: { ru: 'Ожидают решения', en: 'Awaiting a decision' },
    description: '',
    items: [],
  },
  {
    id: 'characters',
    kind: 'characters',
    label: { ru: 'Персонажи', en: 'Characters' },
    description: '',
    items: [],
  },
];

export function GmPanel() {
  const { t, i18n } = useTranslation();
  const { data: campaign } = useCampaign();
  const { data: snapshot } = useSnapshot();
  const remote = useRemote();
  const submit = useSubmitIntent();
  const selection = useSelection();
  const [mapChoice, setMapChoice] = useState('');
  const [search, setSearch] = useState('');
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [actionId, setActionId] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const lang = i18n.resolvedLanguage ?? i18n.language;
  if (campaign?.role !== 'gm' && campaign?.role !== 'co_gm') return <p>{t('gm.onlyGm')}</p>;
  if (!snapshot) return null;
  const plugin = snapshot.plugins.find((p) => p.id === snapshot.system_plugin);
  const rules = plugin?.system;
  const sections = rules?.mechanics?.gm_panel?.length ? rules.mechanics.gm_panel : DEFAULT_SECTIONS;
  const mapId = mapChoice || selection.mapId;
  const map = snapshot.maps.find((m) => m.id === mapId) ?? snapshot.maps[0];
  const tokens = snapshot.tokens.filter((token) => token.map_id === map?.id);
  const actors = tokens.filter((token) => token.character_id !== null);
  const source = actors.find((token) => token.id === (sourceId || selection.tokenId));
  const action = rules?.mechanics?.actions?.find((a) => a.id === actionId && !a.reaction_only);
  const target = action?.target_self ? source : actors.find((token) => token.id === targetId);
  const canEdit = !submit.isPending && (remote === null || remote.status.state === 'online');
  const sourceCharacter = snapshot.characters.find((c) => c.id === source?.character_id);
  const targetCharacter = snapshot.characters.find((c) => c.id === target?.character_id);
  const signature = JSON.stringify([
    map?.id,
    source,
    sourceCharacter?.version,
    action,
    target,
    targetCharacter?.version,
    plugin?.feature_settings,
    action?.radius_cells ? tokens : null,
    action?.radius_cells ? snapshot.characters.map((c) => [c.id, c.version]) : null,
  ]);
  const affected =
    target && action?.radius_cells && map
      ? tokens.filter(
          (token) =>
            token.character_id &&
            Math.hypot(token.x - target.x, token.y - target.y) <=
              (action.radius_cells ?? 0) * map.grid.cell_size * 16,
        )
      : target
        ? [target]
        : [];
  const featureOn = (id: string) =>
    plugin?.feature_settings?.[id] ??
    rules?.features?.find((f) => f.id === id)?.default_enabled ??
    false;
  return (
    <section className="gm-dashboard" aria-label={t('gm.title')}>
      <header className="gm-dashboard-header">
        <div>
          <h1>{t('gm.title')}</h1>
          <p className="muted">{plugin ? labelText(plugin.name, lang) : t('gm.noSystem')}</p>
        </div>
        <label>
          {t('gm.scene')}
          <select
            value={map?.id ?? ''}
            onChange={(e) => {
              setMapChoice(e.target.value);
              setSourceId('');
              setTargetId('');
              setConfirmation('');
            }}
          >
            {snapshot.maps.map((m) => (
              <option key={m.id} value={m.id}>
                {m.name}
              </option>
            ))}
          </select>
        </label>
        {map && (
          <button
            onClick={() => {
              selection.openMap(map.id);
            }}
          >
            {t('gm.openScene')}
          </button>
        )}
      </header>
      {!canEdit && <p role="status">{t('gm.unavailable')}</p>}
      <div className="gm-dashboard-grid">
        {sections.map((section) => (
          <details open className="gm-card" key={section.id}>
            <summary>{labelText(section.label, lang)}</summary>
            <p className="muted">{labelText(section.description, lang)}</p>
            {section.kind === 'clock' && (
              <GameClockPanel steps={section.items ?? []} disabled={!canEdit} />
            )}
            {section.kind === 'overview' && (
              <>
                <div className="gm-stats">
                  <div>
                    <strong>{tokens.length}</strong>
                    <span>{t('gm.tokens')}</span>
                  </div>
                  <div>
                    <strong>
                      {(snapshot.plugin_runtime?.movements.length ?? 0) +
                        (snapshot.plugin_runtime?.action_stacks?.length ?? 0)}
                    </strong>
                    <span>{t('gm.waiting')}</span>
                  </div>
                </div>
                <fieldset disabled={!canEdit}>
                  <InitiativePanel scopeMapId={map?.id} />
                </fieldset>
              </>
            )}
            {section.kind === 'reactions' && (
              <>
                {!(
                  snapshot.plugin_runtime?.movements.length ||
                  snapshot.plugin_runtime?.action_stacks?.length ||
                  snapshot.plugin_runtime?.trigger_runs?.some((run) => run.rolls?.length)
                ) && <p>{t('gm.noPending')}</p>}
                <fieldset disabled={!canEdit}>
                  {(snapshot.plugin_runtime?.trigger_runs ?? [])
                    .filter((run) => run.rolls?.length)
                    .map((run) => (
                      <p key={run.action}>
                        {t('triggers.waitingRolls', {
                          name:
                            snapshot.triggers.find((trigger) => trigger.id === run.trigger)
                              ?.definition.name ?? '',
                          pending:
                            run.rolls?.filter(
                              (id) =>
                                snapshot.roll_requests?.find((r) => r.id === id)?.status ===
                                'pending',
                            ).length ?? 0,
                          count: run.rolls?.length ?? 0,
                        })}
                      </p>
                    ))}
                  <ActionWindows />
                  <PluginReactions />
                </fieldset>
              </>
            )}
            {section.kind === 'maintained' && <MaintainedEffects />}
            {section.kind === 'characters' && (
              <>
                <label>
                  {t('gm.search')}
                  <input
                    type="search"
                    value={search}
                    onChange={(e) => {
                      setSearch(e.target.value);
                    }}
                  />
                </label>
                <ul className="gm-character-list">
                  {actors
                    .filter((token) =>
                      token.name.toLocaleLowerCase().includes(search.toLocaleLowerCase()),
                    )
                    .map((token) => {
                      const character = snapshot.characters.find(
                        (c) => c.id === token.character_id,
                      );
                      return (
                        <li key={token.id}>
                          <button
                            onClick={() => {
                              if (token.character_id) selection.openCharacter(token.character_id);
                            }}
                          >
                            {token.name}
                          </button>
                          <span>
                            {[
                              ...new Set([
                                ...(character?.conditions ?? []),
                                ...(snapshot.effects ?? [])
                                  .filter((e) => e.token === token.id && e.condition)
                                  .map((e) => e.condition ?? ''),
                              ]),
                            ]
                              .map((id) =>
                                labelText(
                                  rules?.conditions?.find((c) => c.id === id)?.label ?? id,
                                  lang,
                                ),
                              )
                              .join(', ') || t('gm.noConditions')}
                          </span>
                          {!token.visible && <small>{t('gm.hidden')}</small>}
                        </li>
                      );
                    })}
                </ul>
                {!actors.length && <p>{t('gm.noCharacters')}</p>}
              </>
            )}
            {section.kind === 'rules' && plugin && (
              <>
                {section.items?.map((id) => {
                  const feature = rules?.features?.find((f) => f.id === id);
                  if (!feature) return null;
                  return (
                    <label className="gm-rule" key={id}>
                      <input
                        type="checkbox"
                        checked={featureOn(id)}
                        disabled={!canEdit}
                        onChange={(e) => {
                          submit.mutate({
                            intent: {
                              kind: 'configure_plugin_feature',
                              id: plugin.id,
                              feature: id,
                              enabled: e.target.checked,
                            },
                          });
                        }}
                      />
                      <span>
                        {labelText(feature.label, lang)}
                        <small>{labelText(feature.description, lang)}</small>
                      </span>
                    </label>
                  );
                })}
                <button
                  onClick={() => {
                    selection.openPlugin(plugin.id);
                  }}
                >
                  {t('gm.allSettings')}
                </button>
              </>
            )}
            {section.kind === 'actions' && (
              <div className="gm-action-form">
                {source?.character_id && !!rules?.mechanics?.turn_budgets?.length && (
                  <TurnBudgetPanel characterId={source.character_id} />
                )}
                <label>
                  {t('gm.actor')}
                  <select
                    value={source?.id ?? ''}
                    onChange={(e) => {
                      setSourceId(e.target.value);
                      setConfirmation('');
                    }}
                  >
                    <option value="">{t('gm.choose')}</option>
                    {actors.map((token) => (
                      <option key={token.id} value={token.id}>
                        {token.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  {t('gm.action')}
                  <select
                    value={actionId}
                    onChange={(e) => {
                      setActionId(e.target.value);
                      setConfirmation('');
                    }}
                  >
                    <option value="">{t('gm.choose')}</option>
                    {rules?.mechanics?.actions
                      ?.filter((a) => section.items?.includes(a.id) && !a.reaction_only)
                      .map((a) => (
                        <option
                          key={a.id}
                          value={a.id}
                          disabled={
                            !snapshot.rules_view?.tokens
                              ?.find((t) => t.token === source?.id)
                              ?.actions.find((v) => v.id === a.id)?.enabled
                          }
                        >
                          {labelText(a.label, lang)}
                        </option>
                      ))}
                  </select>
                </label>
                {source && action && (
                  <button
                    disabled={
                      !canEdit ||
                      !snapshot.rules_view?.tokens
                        ?.find((t) => t.token === source.id)
                        ?.actions.find((v) => v.id === action.id)?.enabled
                    }
                    onClick={() => {
                      beginActionAim(source, action);
                    }}
                  >
                    {t('actions.chooseOnMap')}
                  </button>
                )}
                {!action?.target_self && (
                  <label>
                    {action?.radius_cells ? t('gm.areaCenter') : t('gm.target')}
                    <select
                      value={targetId}
                      onChange={(e) => {
                        setTargetId(e.target.value);
                        setConfirmation('');
                      }}
                    >
                      <option value="">{t('gm.choose')}</option>
                      {actors.map((token) => (
                        <option key={token.id} value={token.id}>
                          {token.name}
                        </option>
                      ))}
                    </select>
                  </label>
                )}
                {action?.radius_cells && <p>{t('gm.areaHint', { radius: action.radius_cells })}</p>}
                <button
                  disabled={
                    !canEdit ||
                    !snapshot.rules_view?.tokens
                      ?.find((t) => t.token === source?.id)
                      ?.actions.find((v) => v.id === action?.id)?.enabled ||
                    !source ||
                    !target ||
                    !action ||
                    (!!action.feature && !featureOn(action.feature))
                  }
                  onClick={() => {
                    setConfirmation(signature);
                  }}
                >
                  {t('gm.review')}
                </button>
                {confirmation === signature && source && target && action && (
                  <div className="gm-confirm" role="group" aria-label={t('gm.confirm')}>
                    <strong>
                      {labelText(action.label, lang)}: {source.name} → {target.name}
                    </strong>
                    <p>
                      {t('gm.affected')}:{' '}
                      {[...new Set(affected.map((token) => token.name))].join(', ')}
                    </p>
                    <p className="muted">{t('gm.costHint')}</p>
                    <button
                      disabled={!canEdit || (!!action.feature && !featureOn(action.feature))}
                      onClick={() => {
                        submit.mutate(
                          {
                            intent: {
                              kind: 'execute_plugin_action',
                              source: source.id,
                              action: action.id,
                              target: action.radius_cells ? null : target.id,
                              x: target.x,
                              y: target.y,
                            },
                          },
                          {
                            onSuccess: () => {
                              setConfirmation('');
                            },
                          },
                        );
                      }}
                    >
                      {t('gm.apply')}
                    </button>
                    <button
                      onClick={() => {
                        setConfirmation('');
                      }}
                    >
                      {t('common.cancel')}
                    </button>
                  </div>
                )}
              </div>
            )}
          </details>
        ))}
      </div>
      <ErrorText error={submit.error} />
    </section>
  );
}
