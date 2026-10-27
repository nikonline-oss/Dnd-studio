import { useTranslation } from 'react-i18next';
import { useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { ConfirmDelete } from '@/shared/ui/IconButton';
import { InlineEdit } from '@/shared/ui/InlineEdit';

/** The zones of a map (ADR-0024), GM roles only: rename and delete; drawn with the map tool. */
export function ZoneList({ mapId }: { mapId: string }) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const zones = useSnapshotSelect((s) => s.zones.filter((z) => z.map_id === mapId)) ?? [];
  return (
    <section className="inspector-section">
      <h3 className="section-title">
        <Icon name="zone" />
        {t('zones.title')}
      </h3>
      {zones.length === 0 && <p className="muted">{t('zones.empty')}</p>}
      <ul className="zone-list" aria-label={t('zones.title')}>
        {zones.map((zone) => (
          <li key={zone.id} className="nav-row">
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
      <ErrorText error={submit.error} />
    </section>
  );
}
