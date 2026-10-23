import { useTranslation } from 'react-i18next';
import { useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { ConfirmDelete, IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';

/** Tokens of the selected map as a list; clicking one selects it like a click on the map. */
export function TokenList() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const submit = useSubmitIntent();
  const mapId = useSelection((s) => s.mapId);
  const selectToken = useSelection((s) => s.selectToken);

  const map = snapshot?.maps.find((m) => m.id === mapId);
  if (!mapId || !snapshot || !map) {
    return <p className="muted">{t('tokens.noMap')}</p>;
  }
  const tokens = snapshot.tokens.filter((tok) => tok.map_id === mapId);

  return (
    <NavSection
      icon="user"
      title={t('tokens.title')}
      count={tokens.length}
      actions={
        <IconButton
          icon="plus"
          label={t('tokens.add')}
          disabled={submit.isPending}
          onClick={() => {
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
          }}
        />
      }
    >
      {tokens.length === 0 && <p className="nav-empty">{t('tokens.empty')}</p>}
      <ul className="nav-list">
        {tokens.map((token) => (
          <li
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
          </li>
        ))}
      </ul>
      <ErrorText error={submit.error} />
    </NavSection>
  );
}
