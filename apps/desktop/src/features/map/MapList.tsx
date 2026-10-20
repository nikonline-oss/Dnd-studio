import { useTranslation } from 'react-i18next';
import { useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { InlineEdit } from '@/shared/ui/InlineEdit';

const DEFAULT_GRID = { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 } as const;

/** Navigator: maps of the campaign. Every change is an intent; the list updates from events. */
export function MapList() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const submit = useSubmitIntent();
  const selectedId = useSelection((s) => s.mapId);
  const selectMap = useSelection((s) => s.selectMap);
  const maps = snapshot?.maps ?? [];

  const addMap = () => {
    const id = uuidv7();
    submit.mutate(
      {
        intent: {
          kind: 'create_map',
          id,
          name: `${t('maps.newName')} ${maps.length + 1}`,
          background_asset_id: null,
          grid: DEFAULT_GRID,
          visible_to_players: false,
        },
      },
      {
        onSuccess: () => {
          selectMap(id);
        },
      },
    );
  };

  return (
    <section aria-labelledby="maps-title" className="list-panel">
      <header className="panel-header">
        <h2 id="maps-title">{t('maps.title')}</h2>
        <button type="button" onClick={addMap} disabled={submit.isPending}>
          {t('maps.add')}
        </button>
      </header>
      {maps.length === 0 && <p className="muted">{t('maps.empty')}</p>}
      <ul>
        {maps.map((map) => (
          <li
            key={map.id}
            aria-current={map.id === selectedId}
            onClick={() => {
              selectMap(map.id);
            }}
          >
            <InlineEdit
              value={map.name}
              label={t('maps.rename')}
              onCommit={(name) => {
                submit.mutate({
                  intent: {
                    kind: 'update_map',
                    id: map.id,
                    name,
                    grid: null,
                    visible_to_players: null,
                  },
                  expectedVersion: map.version,
                });
              }}
            />
            <label className="inline" title={t('maps.visibleHint')}>
              <input
                type="checkbox"
                checked={map.visible_to_players}
                onChange={(e) => {
                  submit.mutate({
                    intent: {
                      kind: 'update_map',
                      id: map.id,
                      name: null,
                      grid: null,
                      visible_to_players: e.target.checked,
                    },
                    expectedVersion: map.version,
                  });
                }}
              />
              {map.visible_to_players ? t('maps.visible') : t('maps.hidden')}
            </label>
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                submit.mutate({
                  intent: { kind: 'delete_map', id: map.id },
                  expectedVersion: map.version,
                });
                if (map.id === selectedId) selectMap(null);
              }}
            >
              {t('maps.delete')}
            </button>
          </li>
        ))}
      </ul>
      <ErrorText error={submit.error} />
    </section>
  );
}
