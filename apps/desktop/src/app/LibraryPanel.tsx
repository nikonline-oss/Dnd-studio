import { useTranslation } from 'react-i18next';
import { useSnapshotSelect } from '@/shared/game/useGame';
import { UnifiedLibrary } from '@/features/library/UnifiedLibrary';

/** Activity Bar destination: one explorer for journal entries, pictures, sounds and playlists. */
export function LibraryPanel() {
  const { t } = useTranslation();
  const empty = useSnapshotSelect(
    (snapshot) =>
      snapshot.journal.length === 0 &&
      snapshot.assets.length === 0 &&
      (snapshot.sounds?.length ?? 0) === 0,
  );
  return (
    <section className="library-panel" aria-label={t('library.title')}>
      {empty && (
        <div className="library-empty-art">
          <img src="/assets/library-arcane-map.png" alt={t('library.emptyAlt')} />
          <p>{t('library.emptyMessage')}</p>
        </div>
      )}
      <UnifiedLibrary />
    </section>
  );
}
