import { useTranslation } from 'react-i18next';
import { useSnapshotSelect } from '@/shared/game/useGame';
import type { BoardLink } from '@/shared/ipc/client';
import { IconButton } from '@/shared/ui/IconButton';

/** GM tools of a board: new cards (note, link, picture), connecting two cards, resetting the view. */
export function BoardToolbar({
  onNote,
  onLink,
  onImage,
  connecting,
  canConnect,
  onConnect,
  onResetView,
  disabled,
}: {
  onNote: () => void;
  onLink: (target: BoardLink) => void;
  onImage: (assetId: string) => void;
  connecting: boolean;
  canConnect: boolean;
  onConnect: () => void;
  onResetView: () => void;
  disabled: boolean;
}) {
  const { t } = useTranslation();
  const maps = useSnapshotSelect((s) => s.maps.map((m) => ({ id: m.id, name: m.name }))) ?? [];
  const entries =
    useSnapshotSelect((s) => s.journal.map((e) => ({ id: e.id, name: e.title }))) ?? [];
  const characters =
    useSnapshotSelect((s) => s.characters.map((c) => ({ id: c.id, name: c.name }))) ?? [];
  const assets =
    useSnapshotSelect((s) =>
      s.assets.map((a) => ({ id: a.id, name: a.original_name ?? a.sha256.slice(0, 8) })),
    ) ?? [];

  return (
    <div className="board-toolbar" role="toolbar" aria-label={t('boards.tools')}>
      <IconButton icon="note" label={t('boards.addNote')} disabled={disabled} onClick={onNote}>
        {t('boards.addNote')}
      </IconButton>
      <select
        aria-label={t('boards.addLink')}
        value=""
        disabled={disabled}
        onChange={(e) => {
          const [kind, id] = e.target.value.split(':');
          if (!id) return;
          if (kind === 'map' || kind === 'journal' || kind === 'character') onLink({ kind, id });
        }}
      >
        <option value="">{t('boards.addLink')}</option>
        {(
          [
            ['map', t('maps.title'), maps],
            ['journal', t('journal.title'), entries],
            ['character', t('characters.title'), characters],
          ] as const
        ).map(([kind, label, items]) =>
          items.length === 0 ? null : (
            <optgroup key={kind} label={label}>
              {items.map((item) => (
                <option key={item.id} value={`${kind}:${item.id}`}>
                  {item.name}
                </option>
              ))}
            </optgroup>
          ),
        )}
      </select>
      <select
        aria-label={t('boards.addImage')}
        value=""
        disabled={disabled || assets.length === 0}
        onChange={(e) => {
          if (e.target.value) onImage(e.target.value);
        }}
      >
        <option value="">{t('boards.addImage')}</option>
        {assets.map((asset) => (
          <option key={asset.id} value={asset.id}>
            {asset.name}
          </option>
        ))}
      </select>
      <IconButton
        icon="link"
        label={t('boards.connect')}
        aria-pressed={connecting}
        disabled={!canConnect && !connecting}
        onClick={onConnect}
      >
        {t('boards.connect')}
      </IconButton>
      <IconButton icon="fit" label={t('boards.resetView')} onClick={onResetView} />
    </div>
  );
}
