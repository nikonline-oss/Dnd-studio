import { create } from 'zustand';

interface SelectionState {
  mapId: string | null;
  selectMap: (mapId: string | null) => void;
}

/** Cross-feature UI selection (features communicate through `shared`, not each other). */
export const useSelection = create<SelectionState>()((set) => ({
  mapId: null,
  selectMap: (mapId) => {
    set({ mapId });
  },
}));
