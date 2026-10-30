import { useListCommands } from '@/shared/ui/useListCommands';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { ConfirmDelete, IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';
import { NAV_ROW_HEIGHT, VirtualList } from '@/shared/ui/VirtualList';

/** Tokens of the selected map as a list; clicking one selects it like a click on the map. */
export function TokenList() {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const commands = useListCommands();
  const { data: campaign } = useCampaign();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const openCharacter = useSelection((s) => s.openCharacter);
  const mapId = useSelection((s) => s.mapId);
  const selectToken = useSelection((s) => s.selectToken);

  const map = useSnapshotSelect((s) => s.maps.find((m) => m.id === mapId));
  const tokens = useSnapshotSelect((s) => s.tokens.filter((tok) => tok.map_id === mapId));
  if (!mapId || !map || !tokens) {
    return <p className="muted">{t('tokens.noMap')}</p>;
  }

  const addToken = () => {
    submit.mutate({
      intent: {
        kind: 'create_token',
        id: uuidv7(),
        map_id: mapId,
        character_id: null,
        asset_id: null,
        name: `${t('tokens.newName')} ${tokens.length + 1}`,
        // The centre of the map (Units16); drag it into place from there.
        x: map.width * 8,
        y: map.height * 8,
        rotation: 0,
        scale: 1,
        layer: 'tokens',
        visible: true,
      },
    });
  };
  const backgroundItems = isGm ? [{ label: t('tokens.add'), onSelect: addToken }] : [];
  return (
    <div
      onContextMenu={(e) => {
        commands.context(e, backgroundItems);
      }}
    >
      <NavSection
        icon="user"
        title={t('tokens.title')}
        count={tokens.length}
        actions={
          isGm && (
            <IconButton
              icon="plus"
              label={t('tokens.add')}
              disabled={submit.isPending}
              onClick={addToken}
            />
          )
        }
      >
        {tokens.length === 0 && <p className="nav-empty">{t('tokens.empty')}</p>}
        <VirtualList
          items={tokens}
          rowHeight={NAV_ROW_HEIGHT}
          getKey={(token) => token.id}
          label={t('tokens.title')}
          // Under the map inspector: a bounded box of its own.
          className="bounded"
          renderRow={(token) => (
            <li
              onContextMenu={(e) => {
                commands.context(e, [
                  {
                    label: t('tokens.select', { defaultValue: 'Select token' }),
                    onSelect: () => {
                      selectToken(token.id);
                    },
                  },
                  ...(token.character_id
                    ? [
                        {
                          label: t('compendia.openCharacter'),
                          onSelect: () => {
                            if (token.character_id) openCharacter(token.character_id);
                          },
                        },
                      ]
                    : []),
                  ...(isGm
                    ? [
                        {
                          label: t('tokens.rename', { defaultValue: 'Rename token' }),
                          onSelect: () => {
                            commands.rename(
                              t('tokens.rename', { defaultValue: 'Rename token' }),
                              token.name,
                              (name) => {
                                submit.mutate({
                                  intent: {
                                    kind: 'update_token',
                                    id: token.id,
                                    name,
                                    rotation: null,
                                    scale: null,
                                    layer: null,
                                    visible: null,
                                  },
                                  expectedVersion: token.version,
                                });
                              },
                            );
                          },
                        },
                        {
                          label: t(token.visible ? 'context.hidePlayers' : 'context.showPlayers'),
                          onSelect: () => {
                            submit.mutate({
                              intent: {
                                kind: 'update_token',
                                id: token.id,
                                name: null,
                                rotation: null,
                                scale: null,
                                layer: null,
                                visible: !token.visible,
                              },
                              expectedVersion: token.version,
                            });
                          },
                        },
                        {
                          label: t('tokens.delete'),
                          onSelect: () => {
                            commands.confirm(t('tokens.delete') + ': ' + token.name, () => {
                              submit.mutate({
                                intent: { kind: 'delete_token', id: token.id },
                                expectedVersion: token.version,
                              });
                            });
                          },
                        },
                      ]
                    : []),
                  ...backgroundItems,
                ]);
              }}
              key={token.id}
              className="nav-row"
              onClick={() => {
                selectToken(token.id);
              }}
            >
              <span className="avatar" aria-hidden="true">
                {token.name.trim().charAt(0).toUpperCase() || '?'}
              </span>
              <span className="row-name">{token.name}</span>
              <span className="row-meta">
                {token.x / 16}, {token.y / 16}
              </span>
              {isGm && (
                <span className="row-actions">
                  <ConfirmDelete
                    label={t('tokens.delete')}
                    onConfirm={() => {
                      submit.mutate({
                        intent: { kind: 'delete_token', id: token.id },
                        expectedVersion: token.version,
                      });
                    }}
                  />
                </span>
              )}
            </li>
          )}
        />
        <ErrorText error={submit.error} />
      </NavSection>
      {commands.ui}
    </div>
  );
}
