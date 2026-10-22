import { create } from 'zustand';

/** What a center tab shows. */
export interface TabRef {
  kind: 'map' | 'journal';
  id: string;
}

const same = (a: TabRef | null, b: TabRef | null) => a?.kind === b?.kind && a?.id === b?.id;

interface SelectionState {
  /** Open center tabs, in display order. */
  tabs: TabRef[];
  /** The tab in front, or `null` when none is open. */
  active: TabRef | null;
  /** The map the map view, inspector and token list work on: the last map tab in front. */
  mapId: string | null;
  /** The journal entry in front (only while a journal tab is active). */
  journalId: string | null;
  characterId: string | null;
  /** What the inspector shows. */
  inspect: 'map' | 'character';
  /** What the center area shows (follows the active tab). */
  center: 'map' | 'journal';
  /** Opens a map in a tab (or brings its tab to the front). */
  openMap: (mapId: string) => void;
  /** Opens a journal entry in a tab (or brings its tab to the front). */
  openJournal: (journalId: string) => void;
  activate: (tab: TabRef) => void;
  /** Closes a tab; the neighbour on the right (else left) comes to the front. */
  closeTab: (tab: TabRef) => void;
  /** Closes tabs whose entity no longer exists (deleted locally or by someone else). */
  pruneTabs: (exists: (tab: TabRef) => boolean) => void;
  selectCharacter: (characterId: string | null) => void;
  /** Nothing open or selected (another campaign was opened or this one closed). */
  reset: () => void;
}

const EMPTY = {
  tabs: [],
  active: null,
  mapId: null,
  journalId: null,
  characterId: null,
  inspect: 'map',
  center: 'map',
} as const satisfies Partial<SelectionState>;

/** Fields that follow from the tab list and the active tab. */
function derive(
  tabs: TabRef[],
  active: TabRef | null,
  previousMapId: string | null,
): Pick<SelectionState, 'tabs' | 'active' | 'mapId' | 'journalId' | 'center'> {
  const mapStillOpen = tabs.some((t) => t.kind === 'map' && t.id === previousMapId);
  const mapId =
    active?.kind === 'map'
      ? active.id
      : mapStillOpen
        ? previousMapId
        : (tabs.find((t) => t.kind === 'map')?.id ?? null);
  return {
    tabs,
    active,
    mapId,
    journalId: active?.kind === 'journal' ? active.id : null,
    center: active?.kind ?? 'map',
  };
}

/** Cross-feature UI selection and center tabs (features communicate through `shared`). */
export const useSelection = create<SelectionState>()((set) => {
  const open = (tab: TabRef) => {
    set((s) => {
      const tabs = s.tabs.some((t) => same(t, tab)) ? s.tabs : [...s.tabs, tab];
      return {
        ...derive(tabs, tab, s.mapId),
        ...(tab.kind === 'map' ? { inspect: 'map' as const } : {}),
      };
    });
  };
  return {
    ...EMPTY,
    tabs: [],
    reset: () => {
      set({ ...EMPTY, tabs: [] });
    },
    openMap: (mapId) => {
      open({ kind: 'map', id: mapId });
    },
    openJournal: (journalId) => {
      open({ kind: 'journal', id: journalId });
    },
    activate: (tab) => {
      set((s) =>
        s.tabs.some((t) => same(t, tab))
          ? {
              ...derive(s.tabs, tab, s.mapId),
              ...(tab.kind === 'map' ? { inspect: 'map' as const } : {}),
            }
          : s,
      );
    },
    closeTab: (tab) => {
      set((s) => {
        const index = s.tabs.findIndex((t) => same(t, tab));
        if (index === -1) return s;
        const tabs = s.tabs.filter((_, i) => i !== index);
        const active = same(s.active, tab) ? (tabs[index] ?? tabs[index - 1] ?? null) : s.active;
        return derive(tabs, active, s.mapId);
      });
    },
    pruneTabs: (exists) => {
      set((s) => {
        if (s.tabs.every(exists)) return s;
        const tabs = s.tabs.filter(exists);
        const index = s.tabs.findIndex((t) => same(t, s.active));
        const active =
          s.active && exists(s.active)
            ? s.active
            : (tabs[Math.min(index, tabs.length - 1)] ?? null);
        return derive(tabs, active, s.mapId);
      });
    },
    selectCharacter: (characterId) => {
      set(
        characterId === null
          ? { characterId, inspect: 'map' }
          : { characterId, inspect: 'character' },
      );
    },
  };
});
