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
