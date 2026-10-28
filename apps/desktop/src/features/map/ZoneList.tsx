import { useTranslation } from 'react-i18next';
import { useZoneTool } from './zoneTool';
import { useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { ConfirmDelete, IconButton } from '@/shared/ui/IconButton';
import { InlineEdit } from '@/shared/ui/InlineEdit';

/**
 * The zones of a map (ADR-0024), GM roles only: rename, redraw, add a part, merge ticked zones,
 * delete. New zones are drawn with the map's zone tool.
 */
export function ZoneList({ mapId }: { mapId: string }) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const zones = useSnapshotSelect((s) => s.zones.filter((z) => z.map_id === mapId)) ?? [];
  const drawing = useZoneTool((s) => s.drawing);
  const draw = useZoneTool((s) => s.draw);
  const ticked = useZoneTool((s) => s.ticked).filter((id) => zones.some((z) => z.id === id));
  const toggle = useZoneTool((s) => s.toggle);
  const clearTicks = useZoneTool((s) => s.clearTicks);
  const [into, ...from] = ticked;
  const target = zones.find((z) => z.id === into);

  return (
    <section className="inspector-section">
      <h3 className="section-title">
        <Icon name="zone" />
        {t('zones.title')}
      </h3>
      {zones.length === 0 && <p className="muted">{t('zones.empty')}</p>}
      {drawing.mode !== 'new' && (
        <p className="notice" role="status">
          {drawing.mode === 'replace' ? t('zones.drawReplace') : t('zones.drawAdd')}{' '}
          <button
            type="button"
            onClick={() => {
              draw({ mode: 'new' });
            }}
          >
            {t('picker.cancel')}
          </button>
        </p>
      )}
      <ul className="zone-list" aria-label={t('zones.title')}>
        {zones.map((zone) => (
          <li key={zone.id} className="nav-row">
            <input
              type="checkbox"
              aria-label={t('zones.tick', { name: zone.name })}
              checked={ticked.includes(zone.id)}
              onChange={() => {
                toggle(zone.id);
              }}
            />
            <InlineEdit
              value={zone.name}
              label={t('zones.rename')}
              onCommit={(name) => {
                submit.mutate({
                  intent: { kind: 'update_zone', id: zone.id, name, shape: null },
                  expectedVersion: zone.version,
                });
              }}
            />
            <span className="row-actions">
              <IconButton
                icon="brush"
                label={t('zones.redraw')}
                onClick={() => {
                  draw({ mode: 'replace', zoneId: zone.id });
                }}
              />
              <IconButton
                icon="plus"
                label={t('zones.addPart')}
                onClick={() => {
                  draw({ mode: 'add', zoneId: zone.id });
                }}
              />
              <ConfirmDelete
                label={t('zones.delete')}
                onConfirm={() => {
                  submit.mutate({
                    intent: { kind: 'delete_zone', id: zone.id },
                    expectedVersion: zone.version,
                  });
                }}
              />
            </span>
          </li>
        ))}
      </ul>
      {ticked.length > 0 && (
        <div className="inline actions">
          <button
            type="button"
            disabled={!target || from.length === 0 || submit.isPending}
            onClick={() => {
              if (!target) return;
              submit.mutate(
                {
                  intent: { kind: 'merge_zones', into: target.id, from },
                  expectedVersion: target.version,
                },
                { onSuccess: clearTicks },
              );
            }}
          >
            {t('zones.merge', { name: target?.name ?? '' })}
          </button>
          <span className="muted">{t('zones.mergeHint')}</span>
        </div>
      )}
      <ErrorText error={submit.error} />
    </section>
  );
}
