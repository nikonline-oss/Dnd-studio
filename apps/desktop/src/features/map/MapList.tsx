import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { useRemote } from '@/shared/game/useRemote';
import { ipc } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { ConfirmDelete, IconButton } from '@/shared/ui/IconButton';
import { InlineEdit } from '@/shared/ui/InlineEdit';
import { NavSection } from '@/shared/ui/NavSection';
import { NAV_ROW_HEIGHT, VirtualList } from '@/shared/ui/VirtualList';
import { useToast } from '@/shared/ui/Toast';

const DEFAULT_GRID = { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 } as const;

/** Navigator: maps of the campaign. Every change is an intent; the list updates from events. */
export function MapList() {
  const { t } = useTranslation();
  const assets = useSnapshotSelect((s) => s.assets);
  const submit = useSubmitIntent();
  const selectedId = useSelection((s) => s.mapId);
  const openMap = useSelection((s) => s.openMap);
  const maps = useSnapshotSelect((s) => s.maps) ?? [];
  const { data: campaign } = useCampaign();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  // Images are imported into the local campaign; uploading them to a session comes with stage 4.
  const remote = useRemote();
  const showToast = useToast((s) => s.show);

  // The file is picked in a native dialog on the Rust side; the map arrives as game events.
  const importMap = useMutation({
    mutationFn: ipc.assetImportMap,
    onSuccess: (imported) => {
      if (!imported) return;
      openMap(imported.map_id);
      if (imported.downscaled) {
        showToast(
          t('maps.downscaled', {
            width: imported.source_width,
            height: imported.source_height,
          }),
        );
      }
    },
  });

  const addMap = () => {
    const id = uuidv7();
    submit.mutate(
      {
        intent: {
          kind: 'create_map',
          id,
          name: `${t('maps.newName')} ${maps.length + 1}`,
          background_asset_id: null,
          // Logical size without a background image (ADR-0016); fixed after creation.
          width: 4000,
          height: 3000,
          grid: DEFAULT_GRID,
          visible_to_players: false,
        },
      },
      {
        onSuccess: () => {
          openMap(id);
        },
      },
    );
  };

  return (
    <NavSection
      icon="map"
      title={t('maps.title')}
      count={maps.length}
      actions={
        <>
          {isGm && remote === null && (
            <IconButton
              icon="image"
              label={t('maps.import')}
              disabled={importMap.isPending}
              onClick={() => {
                importMap.mutate();
              }}
            />
          )}
          <IconButton
            icon="plus"
            label={t('maps.add')}
            disabled={submit.isPending}
            onClick={addMap}
          />
        </>
      }
    >
      {maps.length === 0 && <p className="nav-empty">{t('maps.empty')}</p>}
      <VirtualList
        items={maps}
        rowHeight={NAV_ROW_HEIGHT}
        getKey={(map) => map.id}
        label={t('maps.title')}
        renderRow={(map) => {
          const background = assets?.find((a) => a.id === map.background_asset_id);
          return (
            <li
              key={map.id}
              className="nav-row"
              aria-current={map.id === selectedId}
              onClick={() => {
                openMap(map.id);
              }}
            >
              <Icon name={background ? 'image' : 'grid'} className="row-icon" />
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
              <label
                className="eye-toggle"
                data-on={map.visible_to_players}
                title={map.visible_to_players ? t('maps.visible') : t('maps.hidden')}
                onClick={(e) => {
                  e.stopPropagation();
                }}
              >
                <input
                  type="checkbox"
                  className="visually-hidden"
                  aria-label={t('maps.visibleHint')}
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
                <Icon name={map.visible_to_players ? 'eye' : 'eyeOff'} />
              </label>
              <span className="row-actions">
                <ConfirmDelete
                  label={t('maps.delete')}
                  onConfirm={() => {
                    submit.mutate({
                      intent: { kind: 'delete_map', id: map.id },
                      expectedVersion: map.version,
                    });
                  }}
                />
              </span>
            </li>
          );
        }}
      />
      <ErrorText error={importMap.error ?? submit.error} />
    </NavSection>
  );
}
