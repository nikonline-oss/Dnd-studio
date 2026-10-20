import { useTranslation } from 'react-i18next';
import { useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';

/** Tokens of the selected map (a list until the Pixi map view lands in stage 2). */
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
    <section aria-labelledby="tokens-title" className="list-panel">
      <header>
        <h2 id="tokens-title">{t('tokens.title')}</h2>
        <button
          type="button"
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
        >
          {t('tokens.add')}
        </button>
      </header>
      {tokens.length === 0 && <p className="muted">{t('tokens.empty')}</p>}
      <ul>
        {tokens.map((token) => (
          <li key={token.id}>
            <span>
              {token.name} ({token.x / 16}, {token.y / 16})
            </span>
            <button
              type="button"
              onClick={() => {
                submit.mutate({
                  intent: { kind: 'move_token', id: token.id, x: token.x + 70 * 16, y: token.y },
                });
              }}
            >
              {t('tokens.nudge')}
            </button>
            <button
              type="button"
              onClick={() => {
                submit.mutate({
                  intent: { kind: 'delete_token', id: token.id },
                  expectedVersion: token.version,
                });
              }}
            >
              {t('tokens.delete')}
            </button>
          </li>
        ))}
      </ul>
      <ErrorText error={submit.error} />
    </section>
  );
}
