import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import type { CharacterKind } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';

/** Navigator: characters with their status. GM roles create PCs and NPCs, players their own PC. */
export function CharacterList() {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const { data: snapshot } = useSnapshot();
  const submit = useSubmitIntent();
  const selectedId = useSelection((s) => (s.inspect === 'character' ? s.characterId : null));
  const selectCharacter = useSelection((s) => s.selectCharacter);
  const characters = snapshot?.characters ?? [];
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const canCreate = isGm || campaign?.role === 'player';

  const create = (kind: CharacterKind) => {
    if (!campaign) return;
    const id = uuidv7();
    submit.mutate(
      {
        intent: {
          kind: 'create_character',
          id,
          character_kind: kind,
          name: kind === 'npc' ? t('characters.newNpc') : t('characters.newPc'),
          // Players may only create characters they own; NPCs belong to nobody.
          owner: kind === 'npc' ? null : campaign.user_id,
          sheet_json: '{}',
        },
      },
      {
        onSuccess: () => {
          selectCharacter(id);
        },
      },
    );
  };

  return (
    <section aria-labelledby="characters-title" className="list-panel">
      <header className="panel-header">
        <h2 id="characters-title">{t('characters.title')}</h2>
        {canCreate && (
          <span className="inline">
            <button
              type="button"
              disabled={submit.isPending}
              onClick={() => {
                create('pc');
              }}
            >
              {t('characters.addPc')}
            </button>
            {isGm && (
              <button
                type="button"
                disabled={submit.isPending}
                onClick={() => {
                  create('npc');
                }}
              >
                {t('characters.addNpc')}
              </button>
            )}
          </span>
        )}
      </header>
      {characters.length === 0 && <p className="muted">{t('characters.empty')}</p>}
      <ul>
        {characters.map((c) => (
          <li
            key={c.id}
            aria-current={c.id === selectedId}
            onClick={() => {
              selectCharacter(c.id);
            }}
          >
            <span className="inline-edit">{c.name}</span>
            <span className="badge" data-status={c.status}>
              {t(`characters.status.${c.status}`)}
            </span>
          </li>
        ))}
      </ul>
      <ErrorText error={submit.error} />
    </section>
  );
}
