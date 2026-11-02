import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { assetUrl, type AssetDto } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';
import './imagePresentation.css';

/** A synchronized presentation; Esc is local, only GM roles can stop the shared show. */
export function ImagePresentation() {
  const presentation = useSnapshotSelect((s) => s.presented_image);
  const asset = useSnapshotSelect((s) => s.assets.find((a) => a.id === presentation?.asset_id));
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();
  if (!presentation || !asset || !campaign) return null;
  const gm = campaign.role === 'gm' || campaign.role === 'co_gm';
  return (
    <PresentationView
      key={`${presentation.id}:${presentation.version}`}
      asset={asset}
      pending={submit.isPending}
      error={submit.error}
      stop={
        gm
          ? () => {
              if (!submit.isPending)
                submit.mutate({
                  intent: { kind: 'hide_image', id: presentation.id },
                  expectedVersion: presentation.version,
                });
            }
          : undefined
      }
    />
  );
}

function PresentationView({
  asset,
  stop,
  pending,
  error,
}: {
  asset: AssetDto;
  stop?: (() => void) | undefined;
  pending: boolean;
  error: unknown;
}) {
  const { t } = useTranslation();
  const [dismissed, setDismissed] = useState(false);
  const [failed, setFailed] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (dismissed) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    closeButton.current?.focus();
    return () => {
      previous?.focus();
    };
  }, [dismissed]);
  if (dismissed)
    return stop ? (
      <aside className="image-presentation-status" aria-label={t('presentation.active')}>
        <span>{t('presentation.active')}</span>
        <button
          type="button"
          onClick={() => {
            setDismissed(false);
          }}
        >
          {t('presentation.view')}
        </button>
        <button type="button" disabled={pending} onClick={stop}>
          {t('presentation.stop')}
        </button>
        <ErrorText error={error} />
      </aside>
    ) : null;
  const close = () => {
    setDismissed(true);
  };
  return (
    <div
      ref={box}
      className="image-presentation"
      role="dialog"
      aria-modal="true"
      aria-label={t('presentation.title')}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === 'Escape') {
          event.preventDefault();
          close();
        }
        if (event.key === 'Tab') {
          event.preventDefault();
          const buttons = Array.from(
            box.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [],
          );
          const current = buttons.findIndex((b) => b === document.activeElement);
          buttons[(current + (event.shiftKey ? -1 : 1) + buttons.length) % buttons.length]?.focus();
        }
      }}
    >
      <header>
        <strong>{t('presentation.title')}</strong>
        <div className="image-presentation-actions">
          {stop && (
            <button type="button" disabled={pending} onClick={stop}>
              {t('presentation.stop')}
            </button>
          )}
          <button ref={closeButton} type="button" onClick={close}>
            {t('presentation.close')}
          </button>
        </div>
      </header>
      <ErrorText error={error} />
      {failed ? (
        <p role="alert">{t('presentation.failed')}</p>
      ) : (
        <div
          className="presented-image-frame"
          role="img"
          aria-label={asset.original_name ?? t('presentation.title')}
          style={{
            aspectRatio: `${asset.width} / ${asset.height}`,
            width: `min(100%, calc((100vh - 100px) * ${asset.width / Math.max(1, asset.height)}))`,
          }}
        >
          {asset.usage === 'map' && !asset.display_sha256 && asset.tiles.length > 0 ? (
            asset.tiles.map((tile) => (
              <img
                className="presented-image-tile"
                key={`${tile.x}:${tile.y}`}
                alt=""
                src={assetUrl(tile.sha256)}
                onError={() => {
                  setFailed(true);
                }}
                style={{
                  left: `${(tile.x / asset.width) * 100}%`,
                  top: `${(tile.y / asset.height) * 100}%`,
                  width: `${(tile.width / asset.width) * 100}%`,
                  height: `${(tile.height / asset.height) * 100}%`,
                }}
              />
            ))
          ) : (
            <img
              className="presented-image-full"
              alt=""
              src={assetUrl(asset.display_sha256 ?? asset.thumbnail_sha256)}
              onError={() => {
                setFailed(true);
              }}
            />
          )}
        </div>
      )}
    </div>
  );
}
