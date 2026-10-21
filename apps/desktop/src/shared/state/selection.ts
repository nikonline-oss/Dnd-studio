import { create } from 'zustand';

interface SelectionState {
  mapId: string | null;
  characterId: string | null;
  /** What the inspector shows. */
  inspect: 'map' | 'character';
  selectMap: (mapId: string | null) => void;
  selectCharacter: (characterId: string | null) => void;
}

/** Cross-feature UI selection (features communicate through `shared`, not each other). */
export const useSelection = create<SelectionState>()((set) => ({
  mapId: null,
  characterId: null,
  inspect: 'map',
  selectMap: (mapId) => {
    set({ mapId, inspect: 'map' });
  },
  selectCharacter: (characterId) => {
    set(
      characterId === null
        ? { characterId, inspect: 'map' }
        : { characterId, inspect: 'character' },
    );
  },
}));
