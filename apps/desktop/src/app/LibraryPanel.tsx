import {
  useRef,
  useState,
  type KeyboardEvent,
  type KeyboardEventHandler,
  type PointerEventHandler,
} from 'react';
import { useTranslation } from 'react-i18next';
import { useSnapshotSelect } from '@/shared/game/useGame';
import { AudioLibrary } from '@/features/images/AudioLibrary';
import { ImageLibrary } from '@/features/images/ImageLibrary';
import { JournalTree } from '@/features/journal/JournalTree';

/** Unified Activity Bar destination for journal entries, images and audio. */
export function LibraryPanel({ isGm }: { isGm: boolean }) {
  const { t } = useTranslation();
  const panel = useRef<HTMLElement>(null);
  const [sizes, setSizes] = useState({ journal: 1, images: 1, audio: 1 });
  const [open, setOpen] = useState({ journal: true, images: true, audio: true });
  const drag = useRef<{
    upper: 'journal' | 'images';
    lower: 'images' | 'audio';
    y: number;
    height: number;
    upperSize: number;
    lowerSize: number;
  } | null>(null);
  const empty = useSnapshotSelect(
    (snapshot) =>
      snapshot.journal.length === 0 &&
      snapshot.assets.length === 0 &&
      (snapshot.sounds?.length ?? 0) === 0,
  );
  return (
    <section ref={panel} className="library-panel" aria-label={t('library.title')}>
      {empty && (
        <div className="library-empty-art">
          <img src="/assets/library-arcane-map.png" alt={t('library.emptyAlt')} />
          <p>{t('library.emptyMessage')}</p>
        </div>
      )}
      <details
        className="library-section"
        open={open.journal}
        style={{ flexGrow: sizes.journal }}
        onToggle={(event) => {
          const isOpen = event.currentTarget.open;
          setOpen((value) => ({ ...value, journal: isOpen }));
        }}
      >
        <summary>{t('journal.title')}</summary>
        <JournalTree />
      </details>
      {isGm && (
        <>
          <ResizeGrip
            label={t('library.resizeSections', {
              first: t('journal.title'),
              second: t('images.title'),
            })}
            disabled={!open.journal || !open.images}
            onStart={(event) => {
              event.currentTarget.setPointerCapture(event.pointerId);
              drag.current = {
                upper: 'journal',
                lower: 'images',
                y: event.clientY,
                height: panel.current?.clientHeight ?? 1,
                upperSize: sizes.journal,
                lowerSize: sizes.images,
              };
            }}
            onMove={(event) => {
              resize(event.clientY);
            }}
            onEnd={() => {
              drag.current = null;
            }}
            onKeyDown={(event) => {
              resizeByKey(event, 'journal', 'images');
            }}
          />
          <details
            className="library-section"
            open={open.images}
            style={{ flexGrow: sizes.images }}
            onToggle={(event) => {
              const isOpen = event.currentTarget.open;
              setOpen((value) => ({ ...value, images: isOpen }));
            }}
          >
            <summary>{t('images.title')}</summary>
            <ImageLibrary />
          </details>
          <ResizeGrip
            label={t('library.resizeSections', {
              first: t('images.title'),
              second: t('audio.title'),
            })}
            disabled={!open.images || !open.audio}
            onStart={(event) => {
              event.currentTarget.setPointerCapture(event.pointerId);
              drag.current = {
                upper: 'images',
                lower: 'audio',
                y: event.clientY,
                height: panel.current?.clientHeight ?? 1,
                upperSize: sizes.images,
                lowerSize: sizes.audio,
              };
            }}
            onMove={(event) => {
              resize(event.clientY);
            }}
            onEnd={() => {
              drag.current = null;
            }}
            onKeyDown={(event) => {
              resizeByKey(event, 'images', 'audio');
            }}
          />
          <details
            className="library-section"
            open={open.audio}
            style={{ flexGrow: sizes.audio }}
            onToggle={(event) => {
              const isOpen = event.currentTarget.open;
              setOpen((value) => ({ ...value, audio: isOpen }));
            }}
          >
            <summary>{t('audio.title')}</summary>
            <AudioLibrary />
          </details>
        </>
      )}
    </section>
  );

  function resize(y: number) {
    const current = drag.current;
    if (!current) return;
    const total = current.upperSize + current.lowerSize;
    const delta = ((y - current.y) / current.height) * total;
    const upperSize = Math.max(0.25, Math.min(total - 0.25, current.upperSize + delta));
    setSizes((value) => ({
      ...value,
      [current.upper]: upperSize,
      [current.lower]: total - upperSize,
    }));
  }

  function resizeByKey(
    event: KeyboardEvent<HTMLDivElement>,
    upper: 'journal' | 'images',
    lower: 'images' | 'audio',
  ) {
    if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
    event.preventDefault();
    const delta = event.key === 'ArrowDown' ? 0.1 : -0.1;
    setSizes((value) => {
      const total = value[upper] + value[lower];
      const upperSize = Math.max(0.25, Math.min(total - 0.25, value[upper] + delta));
      return { ...value, [upper]: upperSize, [lower]: total - upperSize };
    });
  }
}

function ResizeGrip({
  label,
  disabled,
  onStart,
  onMove,
  onEnd,
  onKeyDown,
}: {
  label: string;
  disabled: boolean;
  onStart: PointerEventHandler<HTMLDivElement>;
  onMove: PointerEventHandler<HTMLDivElement>;
  onEnd: PointerEventHandler<HTMLDivElement>;
  onKeyDown: KeyboardEventHandler<HTMLDivElement>;
}) {
  return (
    <div
      className="library-section-resize"
      role="separator"
      aria-label={label}
      aria-orientation="horizontal"
      aria-disabled={disabled}
      tabIndex={disabled ? -1 : 0}
      onPointerDown={disabled ? undefined : onStart}
      onPointerMove={disabled ? undefined : onMove}
      onPointerUp={disabled ? undefined : onEnd}
      onPointerCancel={disabled ? undefined : onEnd}
      onKeyDown={disabled ? undefined : onKeyDown}
    >
      <span />
    </div>
  );
}
