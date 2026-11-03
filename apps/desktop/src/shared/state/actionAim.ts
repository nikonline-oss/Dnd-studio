import { create } from 'zustand';
import type { PluginAction, TokenDto } from '@/shared/ipc/client';
import { useSelection } from './selection';
export interface ActionAim {
  source: string;
  mapId: string;
  action: PluginAction;
  point: { x: number; y: number; target: string | null; valid?: boolean } | null;
}
export const useActionAim = create<{
  aim: ActionAim | null;
  setAim: (aim: ActionAim | null) => void;
}>((set) => ({
  aim: null,
  setAim: (aim) => {
    set({ aim });
  },
}));
export function beginActionAim(source: TokenDto, action: PluginAction) {
  const selection = useSelection.getState();
  selection.splitTab({ kind: 'map', id: source.map_id });
  useActionAim.getState().setAim({
    source: source.id,
    mapId: source.map_id,
    action,
    point: action.target_self ? { x: source.x, y: source.y, target: source.id, valid: true } : null,
  });
}
