import { useEffect } from 'react';
import { TabStrip } from './TabStrip';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect } from '@/shared/game/useGame';
import { useSelection, type TabRef } from '@/shared/state/selection';
import { labelText } from '@/shared/i18n/label';
import { Icon } from '@/shared/ui/Icon';

const TAB_ICONS = {
  map: 'map',
  journal: 'book',
  character: 'user',
  board: 'board',
  trigger: 'bolt',
  compendium: 'books',
  plugin: 'puzzle',
  gm_panel: 'grid',
  items: 'backpack',
  item: 'backpack',
  skills: 'sparkle',
  images: 'image',
  audio: 'music',
} as const;

/**
 * Tab bar of the center area: maps, journal entries and character sheets opened from the navigator. A tab stays
 * open until closed (× or middle click) or until its map/entry is deleted.
 */
export function CenterTabs() {
  const { t, i18n } = useTranslation();
  const { data: campaign } = useCampaign();
  const gm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const openGmPanel = useSelection((s) => s.openGmPanel);
  const plugins = useSnapshotSelect((s) => s.plugins);
  const items = useSnapshotSelect((s) => s.items ?? []);
  // Names only: a tab title does not change when a token moves.
  const maps = useSnapshotSelect((s) => s.maps.map((m) => ({ id: m.id, name: m.name })));
  const entries = useSnapshotSelect((s) => s.journal.map((e) => ({ id: e.id, title: e.title })));
  const characters = useSnapshotSelect((s) =>
    s.characters.map((c) => ({ id: c.id, name: c.name })),
  );
  const boards = useSnapshotSelect((s) => s.boards.map((b) => ({ id: b.id, name: b.name })));
  const triggers = useSnapshotSelect((s) =>
    s.triggers.map((tr) => ({ id: tr.id, name: tr.definition.name })),
  );
  const tabs = useSelection((s) => s.tabs);
  const folders = useSnapshotSelect((s) => s.library_folders ?? []);
  const active = useSelection((s) => s.active);
  const splitTab = useSelection((s) => s.splitTab);
  const activate = useSelection((s) => s.activate);
  const showMap = useSelection((s) => s.showMap);
  const closeTab = useSelection((s) => s.closeTab);
  const pruneTabs = useSelection((s) => s.pruneTabs);

  // Deleted here or by someone else: the tab goes away with the entity.
  useEffect(() => {
    if (!maps || !entries || !characters || !boards || !triggers) return;
    const lists = {
      map: maps,
      journal: entries,
      character: characters,
      board: boards,
      trigger: triggers,
    };
    // Compendia are not in the snapshot; their tabs stay until closed.
    const listed = (tab: TabRef) =>
      tab.kind === 'item'
        ? (items?.some((item) => item.id === tab.id) ?? true)
        : tab.kind === 'items' || tab.kind === 'skills'
          ? true
          : tab.kind === 'gm_panel'
            ? campaign === undefined || gm
            : tab.kind === 'plugin'
              ? (plugins ?? []).some((p) => p.id === tab.id)
              : tab.kind === 'compendium' ||
                (tab.kind === 'images' || tab.kind === 'audio'
                  ? tab.id === 'root' ||
                    (folders ?? []).some((f) => f.id === tab.id && f.library === tab.kind)
                  : lists[tab.kind].some((item) => item.id === tab.id));
    pruneTabs(listed);
  }, [
    maps,
    entries,
    characters,
    boards,
    triggers,
    folders,
    plugins,
    items,
    campaign,
    gm,
    pruneTabs,
  ]);

  const title = (tab: TabRef) =>
    tab.kind === 'item'
      ? (items?.find((item) => item.id === tab.id)?.name ?? t('items.title'))
      : tab.kind === 'items'
        ? t('items.title')
        : tab.kind === 'skills'
          ? t('items.skillsTitle')
          : tab.kind === 'gm_panel'
            ? t('gm.title')
            : tab.kind === 'plugin'
              ? labelText(plugins?.find((p) => p.id === tab.id)?.name ?? '', i18n.language)
              : tab.kind === 'images' || tab.kind === 'audio'
                ? (folders?.find((f) => f.id === tab.id)?.name ?? t(`${tab.kind}.title`))
                : tab.kind === 'map'
                  ? (maps?.find((m) => m.id === tab.id)?.name ?? t('panels.map'))
                  : tab.kind === 'journal'
                    ? (entries?.find((e) => e.id === tab.id)?.title ?? t('journal.title'))
                    : tab.kind === 'character'
                      ? (characters?.find((c) => c.id === tab.id)?.name ?? t('characters.title'))
                      : tab.kind === 'board'
                        ? (boards?.find((b) => b.id === tab.id)?.name ?? t('boards.title'))
                        : tab.kind === 'trigger'
                          ? (triggers?.find((tr) => tr.id === tab.id)?.name ?? t('triggers.title'))
                          : t('compendia.title');

  // No map opened in a tab (always so for players, who have no navigator): the map view keeps a
  // tab of its own, so opening a journal entry does not hide the map.
  const defaultMapTab = !tabs.some((tab) => tab.kind === 'map');

  return (
    <TabStrip activeKey={active ? `${active.kind}:${active.id}` : 'default-map'}>
      {gm && !tabs.some((tab) => tab.kind === 'gm_panel') && (
        <button type="button" className="tab tab-launcher" onClick={openGmPanel}>
          <Icon name="grid" />
          <span className="tab-title">{t('gm.title')}</span>
        </button>
      )}
      {defaultMapTab && (
        <span
          role="tab"
          tabIndex={active === null ? 0 : -1}
          aria-selected={active === null}
          className="tab"
          onClick={showMap}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') showMap();
          }}
        >
          <Icon name="map" />
          {t('panels.map')}
        </span>
      )}
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
            <Icon name={TAB_ICONS[tab.kind]} />
            <span className="tab-title">{name}</span>
            <button
              type="button"
              className="tab-close"
              aria-label={t('tabs.split', { name })}
              onClick={(e) => {
                e.stopPropagation();
                const selection = useSelection.getState();
                if (!selection.active && !selection.paneTabs.length && maps?.[0])
                  selection.openMap(maps[0].id);
                splitTab(tab);
              }}
            >
              <Icon name="plus" size={12} />
            </button>
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
    </TabStrip>
  );
}
