import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { BoardLink } from '@/shared/ipc/client';
import { ElementPicker } from '@/shared/ui/ElementPicker';
import { ContextMenu } from '@/shared/ui/ContextMenu';
import { IconButton } from '@/shared/ui/IconButton';

/** GM tools of a board: new cards (note, link, picture), connecting two cards, resetting the view. */
export function BoardToolbar({
  contextRequest,
  onCloseContext,
  onNote,
  onLink,
  onImage,
  onTrigger,
  onMap,
  onImportMap,
  connecting,
  canConnect,
  onConnect,
  onResetView,
  disabled,
}: {
  contextRequest?: { x: number; y: number } | null;
  onCloseContext?: () => void;
  onNote: () => void;
  onLink: (target: BoardLink) => void;
  onImage: (assetId: string) => void;
  /** Places a trigger as a block; `null`: a new one. */
  onTrigger: (triggerId: string | null) => void;
  onMap: () => void;
  onImportMap: (() => void) | undefined;
  connecting: boolean;
  canConnect: boolean;
  onConnect: () => void;
  onResetView: () => void;
  disabled: boolean;
}) {
  const { t } = useTranslation();
  const [picking, setPicking] = useState<'link' | 'image' | 'trigger' | null>(null);

  return (
    <div className="board-toolbar" role="toolbar" aria-label={t('boards.tools')}>
      <IconButton icon="note" label={t('boards.addNote')} disabled={disabled} onClick={onNote}>
        {t('boards.addNote')}
      </IconButton>
      <IconButton icon="map" label={t('boards.addMap')} disabled={disabled} onClick={onMap}>
        {t('boards.addMap')}
      </IconButton>
      {onImportMap && (
        <IconButton
          icon="image"
          label={t('boards.importMap')}
          disabled={disabled}
          onClick={onImportMap}
        >
          {t('boards.importMap')}
        </IconButton>
      )}
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
        icon="bolt"
        label={t('boards.addTrigger')}
        disabled={disabled}
        onClick={() => {
          setPicking('trigger');
        }}
      >
        {t('boards.addTrigger')}
      </IconButton>
      <IconButton
        icon="plus"
        label={t('boards.newTrigger')}
        disabled={disabled}
        onClick={() => {
          onTrigger(null);
        }}
      />
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
      {contextRequest && (
        <ContextMenu
          {...contextRequest}
          label={t('boards.tools')}
          onClose={() => onCloseContext?.()}
          items={[
            { label: t('boards.addNote'), onSelect: onNote, disabled },
            { label: t('boards.addMap'), onSelect: onMap, disabled },
            ...(onImportMap
              ? [{ label: t('boards.importMap'), onSelect: onImportMap, disabled }]
              : []),
            {
              label: t('boards.addLink'),
              onSelect: () => {
                setPicking('link');
              },
              disabled,
            },
            {
              label: t('boards.addImage'),
              onSelect: () => {
                setPicking('image');
              },
              disabled,
            },
            {
              label: t('boards.addTrigger'),
              onSelect: () => {
                setPicking('trigger');
              },
              disabled,
            },
            {
              label: t('boards.newTrigger'),
              onSelect: () => {
                onTrigger(null);
              },
              disabled,
            },
            { label: t('boards.resetView'), onSelect: onResetView },
          ]}
        />
      )}
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
      {picking === 'trigger' && (
        <ElementPicker
          title={t('boards.addTrigger')}
          kinds={['trigger']}
          onClose={() => {
            setPicking(null);
          }}
          onPick={({ id }) => {
            onTrigger(id);
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
