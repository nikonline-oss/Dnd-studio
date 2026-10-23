import { useTranslation } from 'react-i18next';
import { CHARACTER_DRAG_TYPE } from '@/shared/dragTypes';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import type { CharacterKind } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';
import { NAV_ROW_HEIGHT, VirtualList } from '@/shared/ui/VirtualList';

/** Navigator: characters with their status. GM roles create PCs and NPCs, players their own PC. */
export function CharacterList() {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();
  const selectedId = useSelection((s) => (s.inspect === 'character' ? s.characterId : null));
  const selectCharacter = useSelection((s) => s.selectCharacter);
  const characters = useSnapshotSelect((s) => s.characters) ?? [];
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
    <NavSection
      icon="users"
      title={t('characters.title')}
      count={characters.length}
      actions={
        canCreate && (
          <>
            <IconButton
              icon="user"
              label={t('characters.addPc')}
              disabled={submit.isPending}
              onClick={() => {
                create('pc');
              }}
            />
            {isGm && (
              <IconButton
                icon="plus"
                label={t('characters.addNpc')}
                disabled={submit.isPending}
                onClick={() => {
                  create('npc');
                }}
              />
            )}
          </>
        )
      }
    >
      {characters.length === 0 && <p className="nav-empty">{t('characters.empty')}</p>}
      <VirtualList
        items={characters}
        rowHeight={NAV_ROW_HEIGHT}
        getKey={(c) => c.id}
        label={t('characters.title')}
        renderRow={(c) => (
          <li
            key={c.id}
            className="nav-row"
            aria-current={c.id === selectedId}
            // GM roles drag a character onto the map to place its token.
            draggable={isGm}
            title={isGm ? t('characters.dragHint') : undefined}
            onDragStart={(e) => {
              e.dataTransfer.setData(CHARACTER_DRAG_TYPE, c.id);
              e.dataTransfer.effectAllowed = 'copy';
            }}
            onClick={() => {
              selectCharacter(c.id);
            }}
          >
            <span className="avatar" data-kind={c.kind} aria-hidden="true">
              {c.name.trim().charAt(0).toUpperCase() || '?'}
            </span>
            <span className="row-name">{c.name}</span>
            <span className="badge" data-status={c.status}>
              {t(`characters.status.${c.status}`)}
            </span>
          </li>
        )}
      />
      <ErrorText error={submit.error} />
    </NavSection>
  );
}
