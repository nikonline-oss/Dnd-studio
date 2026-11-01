import { create } from 'zustand';
export const useAttention = create<{
  requestsOpen: boolean;
  soundEnabled: boolean;
  setRequestsOpen: (open: boolean) => void;
  setSoundEnabled: (enabled: boolean) => void;
}>()((set) => ({
  requestsOpen: false,
  soundEnabled: true,
  setRequestsOpen: (requestsOpen) => {
    set({ requestsOpen });
  },
  setSoundEnabled: (soundEnabled) => {
    set({ soundEnabled });
  },
}));
