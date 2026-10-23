import { create } from 'zustand';

/** App-wide dialogs that several places open (menu, start screen). */
export type DialogName = 'hosting' | 'connect';

interface DialogsState {
  open: DialogName | null;
  show: (name: DialogName) => void;
  close: () => void;
}

export const useDialogs = create<DialogsState>()((set) => ({
  open: null,
  show: (name) => {
    set({ open: name });
  },
  close: () => {
    set({ open: null });
  },
}));
