import { create } from 'zustand';

interface SelectionState {
  mapId: string | null;
  characterId: string | null;
  journalId: string | null;
  /** What the inspector shows. */
  inspect: 'map' | 'character';
  /** What the center area shows. */
  center: 'map' | 'journal';
  selectMap: (mapId: string | null) => void;
  selectCharacter: (characterId: string | null) => void;
  selectJournal: (journalId: string | null) => void;
}

/** Cross-feature UI selection (features communicate through `shared`, not each other). */
export const useSelection = create<SelectionState>()((set) => ({
  mapId: null,
  characterId: null,
  journalId: null,
  inspect: 'map',
  center: 'map',
  selectMap: (mapId) => {
    set({ mapId, inspect: 'map', center: 'map' });
  },
  selectCharacter: (characterId) => {
    set(
      characterId === null
        ? { characterId, inspect: 'map' }
        : { characterId, inspect: 'character' },
    );
  },
  selectJournal: (journalId) => {
    set(journalId === null ? { journalId, center: 'map' } : { journalId, center: 'journal' });
  },
}));
