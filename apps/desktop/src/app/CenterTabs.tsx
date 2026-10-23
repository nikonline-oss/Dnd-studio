import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useSnapshotSelect } from '@/shared/game/useGame';
import { useSelection, type TabRef } from '@/shared/state/selection';
import { Icon } from '@/shared/ui/Icon';

/**
 * Tab bar of the center area: maps and journal entries opened from the navigator. A tab stays
 * open until closed (× or middle click) or until its map/entry is deleted.
 */
export function CenterTabs() {
  const { t } = useTranslation();
  // Names only: a tab title does not change when a token moves.
  const maps = useSnapshotSelect((s) => s.maps.map((m) => ({ id: m.id, name: m.name })));
  const entries = useSnapshotSelect((s) => s.journal.map((e) => ({ id: e.id, title: e.title })));
  const tabs = useSelection((s) => s.tabs);
  const active = useSelection((s) => s.active);
  const activate = useSelection((s) => s.activate);
  const closeTab = useSelection((s) => s.closeTab);
  const pruneTabs = useSelection((s) => s.pruneTabs);

  // Deleted here or by someone else: the tab goes away with the entity.
  useEffect(() => {
    if (!maps || !entries) return;
    pruneTabs((tab) =>
      tab.kind === 'map' ? maps.some((m) => m.id === tab.id) : entries.some((e) => e.id === tab.id),
    );
  }, [maps, entries, pruneTabs]);

  const title = (tab: TabRef) =>
    tab.kind === 'map'
      ? (maps?.find((m) => m.id === tab.id)?.name ?? t('panels.map'))
      : (entries?.find((e) => e.id === tab.id)?.title ?? t('journal.title'));

  if (tabs.length === 0) {
    // Nothing opened yet (and the player preset has no navigator): the map view on its own.
    return (
      <div role="tablist" className="tabs">
        <span role="tab" aria-selected="true" className="tab">
          <Icon name="map" />
          {t('panels.map')}
        </span>
      </div>
    );
  }

  return (
    <div role="tablist" className="tabs">
      {tabs.map((tab) => {
        const selected = active?.kind === tab.kind && active.id === tab.id;
        const name = title(tab);
        return (
          <div
            key={`${tab.kind}:${tab.id}`}
            role="tab"
            tabIndex={selected ? 0 : -1}
            aria-selected={selected}
            className="tab closable"
            title={name}
            onClick={() => {
              activate(tab);
            }}
            onAuxClick={(e) => {
              if (e.button === 1) closeTab(tab);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') activate(tab);
              if (e.key === 'Delete') closeTab(tab);
            }}
          >
            <Icon name={tab.kind === 'map' ? 'map' : 'book'} />
            <span className="tab-title">{name}</span>
            <button
              type="button"
              className="tab-close"
              tabIndex={-1}
              aria-label={t('tabs.close', { name })}
              title={t('tabs.close', { name })}
              onClick={(e) => {
                e.stopPropagation();
                closeTab(tab);
              }}
            >
              <Icon name="x" size={12} />
            </button>
          </div>
        );
      })}
    </div>
  );
}
