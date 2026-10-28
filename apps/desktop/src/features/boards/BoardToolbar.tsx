import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { BoardLink } from '@/shared/ipc/client';
import { ElementPicker } from '@/shared/ui/ElementPicker';
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
  const [picking, setPicking] = useState<'link' | 'image' | null>(null);

  return (
    <div className="board-toolbar" role="toolbar" aria-label={t('boards.tools')}>
      <IconButton icon="note" label={t('boards.addNote')} disabled={disabled} onClick={onNote}>
        {t('boards.addNote')}
      </IconButton>
      <IconButton
        icon="link"
        label={t('boards.addLink')}
        disabled={disabled}
        onClick={() => {
          setPicking('link');
        }}
      >
        {t('boards.addLink')}
      </IconButton>
      <IconButton
        icon="image"
        label={t('boards.addImage')}
        disabled={disabled}
        onClick={() => {
          setPicking('image');
        }}
      >
        {t('boards.addImage')}
      </IconButton>
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
      {picking === 'link' && (
        <ElementPicker
          title={t('boards.pickLink')}
          kinds={['map', 'journal', 'character']}
          onClose={() => {
            setPicking(null);
          }}
          onPick={({ kind, id }) => {
            if (kind === 'map' || kind === 'journal' || kind === 'character') onLink({ kind, id });
          }}
        />
      )}
      {picking === 'image' && (
        <ElementPicker
          title={t('boards.pickImage')}
          kinds={['asset']}
          onClose={() => {
            setPicking(null);
          }}
          onPick={({ id }) => {
            onImage(id);
          }}
        />
      )}
    </div>
  );
}
