import { getPath } from '@/shared/sheet/model';
import { useSelection } from '@/shared/state/selection';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshot } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import { beginActionAim } from '@/shared/state/actionAim';
import type { CharacterDto } from '@/shared/ipc/client';

export function CharacterActions({ character }: { character: CharacterDto }) {
  const { t, i18n } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: campaign } = useCampaign();
  const [group, setGroup] = useState('');
  const [tokenId, setTokenId] = useState('');
  const plugin = snapshot?.plugins.find((p) => p.id === snapshot.system_plugin);
  const mechanics = plugin?.system?.mechanics;
  const tokens = snapshot?.tokens.filter((t) => t.character_id === character.id) ?? [];
  const token = tokens.find((t) => t.id === tokenId) ?? tokens[0];
  const allowed =
    campaign?.role === 'gm' ||
    campaign?.role === 'co_gm' ||
    (campaign?.role === 'player' && character.owner === campaign.user_id);
  const groups = mechanics?.action_groups ?? [];
  const chosen = groups.some((g) => g.id === group) ? group : groups[0]?.id;
  const available = snapshot?.rules_view?.tokens?.find((t) => t.token === token?.id)?.actions ?? [];
  const actions =
    mechanics?.actions?.filter((a) => !a.reaction_only && (!chosen || a.sheet_group === chosen)) ??
    [];
  if (!mechanics?.actions?.length) return null;
  return (
    <section className="character-actions">
      <div className="sheet-tabs" role="tablist" aria-label={t('actions.title')}>
        {groups.map((g) => (
          <button
            key={g.id}
            role="tab"
            aria-selected={chosen === g.id}
            onClick={() => {
              setGroup(g.id);
            }}
          >
            {labelText(g.label, i18n.language)}
          </button>
        ))}
      </div>
      {tokens.length > 1 && (
        <label>
          {t('actions.token')}
          <select
            value={token?.id}
            onChange={(e) => {
              setTokenId(e.target.value);
            }}
          >
            {tokens.map((v) => (
              <option key={v.id} value={v.id}>
                {snapshot?.maps.find((m) => m.id === v.map_id)?.name}: {v.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {!token && <p className="muted">{t('actions.placeToken')}</p>}
      <div className="action-buttons">
        {actions.map((action) => {
          const eligibility = available.find((a) => a.id === action.id);
          return (
            <button
              key={action.id}
              disabled={!allowed || !token || !eligibility?.enabled}
              title={t(`actions.reason.${eligibility?.reason ?? 'available'}`)}
              onClick={() => {
                if (token) beginActionAim(token, action);
              }}
            >
              {labelText(action.label, i18n.language)}
              {eligibility && (
                <small>
                  {t('actions.range', { range: eligibility.range_cells })}
                  {!eligibility.enabled && ` · ${t(`actions.reason.${eligibility.reason}`)}`}
                </small>
              )}
            </button>
          );
        })}
      </div>
    </section>
  );
}

/** The button belongs to the actual list row, and uses the authority's resolved action. */
export function CharacterSheetAction({
  character,
  collection,
  path,
  canEdit,
  onChange,
}: {
  character: CharacterDto;
  collection: string;
  path: string[];
  canEdit: boolean;
  onChange: (path: string, value: unknown) => void;
}) {
  const { t, i18n } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: campaign } = useCampaign();
  const selectedToken = useSelection((s) => s.tokenId);
  const [choice, setChoice] = useState('');
  const mechanics = snapshot?.plugins.find((p) => p.id === snapshot.system_plugin)?.system
    ?.mechanics;
  const def = mechanics?.sheet_actions?.find((c) => c.id === collection);
  const tokens = snapshot?.tokens.filter((t) => t.character_id === character.id) ?? [];
  const token =
    tokens.find((t) => t.id === choice) ?? tokens.find((t) => t.id === selectedToken) ?? tokens[0];
  const index = Number(path.at(-1));
  const row = getPath(JSON.parse(character.sheet_json) as unknown, path);
  const storedAction = def?.action_field ? getPath(row, def.action_field) : undefined;
  const selectedAction =
    typeof storedAction === 'string' && storedAction ? storedAction : def?.actions[0];
  const entry = snapshot?.rules_view?.tokens
    ?.find((t) => t.token === token?.id)
    ?.actions.find(
      (a) => a.sheet_action?.collection === collection && a.sheet_action.index === index,
    );
  const allowed =
    campaign?.role === 'gm' ||
    campaign?.role === 'co_gm' ||
    (campaign?.role === 'player' && character.owner === campaign.user_id);
  if (!def || path.slice(0, -1).join('.') !== def.path) return null;
  const reason = !token
    ? t('actions.placeToken')
    : !entry
      ? t('actions.invalidEntry')
      : !entry.enabled
        ? t(`actions.reason.${entry.reason}`)
        : t('actions.range', { range: entry.range_cells });
  return (
    <div className="sheet-action-control">
      {def.action_field && def.actions.length > 1 && (
        <label className="sheet-field">
          <span className="sheet-field-label">{t('actions.rule')}</span>
          <select
            value={selectedAction}
            disabled={!canEdit}
            onChange={(e) => {
              if (def.action_field) onChange(def.action_field, e.target.value);
            }}
          >
            {def.actions.map((id) => (
              <option key={id} value={id}>
                {labelText(
                  mechanics?.actions?.find((a) => a.id === id)?.label ?? id,
                  i18n.language,
                )}
              </option>
            ))}
          </select>
        </label>
      )}
      {tokens.length > 1 && (
        <label className="sheet-field">
          <span className="sheet-field-label">{t('actions.token')}</span>
          <select
            value={token?.id}
            onChange={(e) => {
              setChoice(e.target.value);
            }}
          >
            {tokens.map((v) => (
              <option key={v.id} value={v.id}>
                {snapshot?.maps.find((m) => m.id === v.map_id)?.name}: {v.name}
              </option>
            ))}
          </select>
        </label>
      )}
      <button
        type="button"
        className="primary"
        disabled={!allowed || !token || !entry?.enabled}
        onClick={() => {
          if (token && entry?.sheet_action) beginActionAim(token, entry.sheet_action.action);
        }}
      >
        {t('actions.useEntry')}
      </button>
      <small className="muted">{reason}</small>
    </div>
  );
}
