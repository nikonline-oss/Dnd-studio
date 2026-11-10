import { useState } from 'react';
import { ContextMenu } from '@/shared/ui/ContextMenu';
import { Dialog } from '@/shared/ui/Dialog';
import type { CharacterDto } from '@/shared/ipc/client';
import { useTranslation } from 'react-i18next';
import { CHARACTER_DRAG_TYPE } from '@/shared/dragTypes';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import type { CharacterKind } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { CreateCharacterDialog } from './CreateCharacterDialog';
import { IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';
import { NAV_ROW_HEIGHT, VirtualList } from '@/shared/ui/VirtualList';

/** Navigator: characters with their status. GM roles create PCs and NPCs, players their own PC. */
export function CharacterList() {
  const { t } = useTranslation();
  const [menu, setMenu] = useState<{ x: number; y: number; character: CharacterDto | null } | null>(
    null,
  );
  const [deleting, setDeleting] = useState<CharacterDto | null>(null);
  const [creating, setCreating] = useState<CharacterKind | null>(null);
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();
  const selectedId = useSelection((s) => (s.inspect === 'character' ? s.characterId : null));
  const selectCharacter = useSelection((s) => s.selectCharacter);
  const characters = useSnapshotSelect((s) => s.characters) ?? [];
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const canCreate = isGm || campaign?.role === 'player';

  const create = (kind: CharacterKind) => {
    if (!campaign) return;
    setCreating(kind);
  };

  return (
    <div
      onContextMenu={(e) => {
        e.preventDefault();
        setMenu({ x: e.clientX, y: e.clientY, character: null });
      }}
    >
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
              onContextMenu={(e) => {
                e.preventDefault();
                e.stopPropagation();
                selectCharacter(c.id);
                setMenu({ x: e.clientX, y: e.clientY, character: c });
              }}
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
              <span className="row-name">{c.name || t('characters.hiddenName')}</span>
              <span className="badge" data-status={c.status}>
                {t(`characters.status.${c.status}`)}
              </span>
            </li>
          )}
        />
        <ErrorText error={submit.error} />
      </NavSection>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          label={t('characters.title')}
          onClose={() => {
            setMenu(null);
          }}
          items={[
            ...(menu.character
              ? [
                  {
                    label: t('characters.openSheet'),
                    onSelect: () => {
                      if (menu.character) useSelection.getState().openCharacter(menu.character.id);
                    },
                  },
                ]
              : []),
            ...(canCreate
              ? [
                  {
                    label: t('characters.addPc'),
                    disabled: submit.isPending,
                    onSelect: () => {
                      create('pc');
                    },
                  },
                ]
              : []),
            ...(isGm
              ? [
                  {
                    label: t('characters.addNpc'),
                    disabled: submit.isPending,
                    onSelect: () => {
                      create('npc');
                    },
                  },
                ]
              : []),
            ...(isGm && menu.character
              ? [
                  ...(menu.character.status === 'pending_approval'
                    ? (['approve_character', 'return_character'] as const)
                    : []),
                  ...(menu.character.status !== 'archived' ? (['archive_character'] as const) : []),
                ].map((kind) => ({
                  label: t(
                    `characters.${kind === 'approve_character' ? 'approve' : kind === 'return_character' ? 'return' : 'archive'}`,
                  ),
                  disabled: submit.isPending,
                  onSelect: () => {
                    if (menu.character)
                      submit.mutate({
                        intent: { kind, id: menu.character.id },
                        expectedVersion: menu.character.version,
                      });
                  },
                }))
              : []),
            ...(canCreate &&
            menu.character?.status === 'draft' &&
            (isGm || menu.character.owner === campaign.user_id)
              ? [
                  {
                    label: t('characters.submit'),
                    disabled: submit.isPending,
                    onSelect: () => {
                      if (menu.character)
                        submit.mutate({
                          intent: { kind: 'submit_character', id: menu.character.id },
                          expectedVersion: menu.character.version,
                        });
                    },
                  },
                ]
              : []),
            ...(isGm && menu.character
              ? [
                  {
                    label: t('characters.delete'),
                    disabled: submit.isPending,
                    onSelect: () => {
                      setDeleting(menu.character);
                    },
                  },
                ]
              : []),
          ]}
        />
      )}
      {creating && (
        <CreateCharacterDialog
          kind={creating}
          canChooseKind={isGm}
          onClose={() => {
            setCreating(null);
          }}
          onCreated={selectCharacter}
        />
      )}
      {deleting && (
        <Dialog
          title={t('characters.delete')}
          closeLabel={t('common.cancel')}
          size="sm"
          dismissible={!submit.isPending}
          onClose={() => {
            setDeleting(null);
          }}
          footer={
            <>
              <button
                type="button"
                onClick={() => {
                  setDeleting(null);
                }}
              >
                {t('common.cancel')}
              </button>
              <button
                type="button"
                className="btn-danger"
                disabled={submit.isPending}
                onClick={() => {
                  submit.mutate(
                    {
                      intent: { kind: 'delete_character', id: deleting.id },
                      expectedVersion: deleting.version,
                    },
                    {
                      onSuccess: () => {
                        setDeleting(null);
                      },
                    },
                  );
                }}
              >
                {t('characters.delete')}
              </button>
            </>
          }
        >
          <p>{t('characters.deleteConfirm', { name: deleting.name })}</p>
          <ErrorText error={submit.error} />
        </Dialog>
      )}
    </div>
  );
}
