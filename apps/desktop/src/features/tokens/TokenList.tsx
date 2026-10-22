import { useTranslation } from 'react-i18next';
import { useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { ConfirmDelete, IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';

/** Tokens of the selected map, as a list until they are drawn on the map (stage 2a, step 7). */
export function TokenList() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const submit = useSubmitIntent();
  const mapId = useSelection((s) => s.mapId);

  if (!mapId || !snapshot?.maps.some((m) => m.id === mapId)) {
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
                x: 0,
                y: 0,
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
          <li key={token.id} className="nav-row">
            <span className="avatar" aria-hidden="true">
              {token.name.trim().charAt(0).toUpperCase() || '?'}
            </span>
            <span className="row-name">{token.name}</span>
            <span className="row-meta">
              {token.x / 16}, {token.y / 16}
            </span>
            <span className="row-actions">
              <IconButton
                icon="send"
                label={t('tokens.nudge')}
                onClick={() => {
                  submit.mutate({
                    intent: { kind: 'move_token', id: token.id, x: token.x + 70 * 16, y: token.y },
                  });
                }}
              />
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
