import type { Snapshot, TriggerDef } from '@/shared/ipc/client';

/** Required inputs only. The authoritative parser and reference checks remain in Rust. */
export function incompleteSteps(def: TriggerDef, snapshot: Snapshot): number[] {
  const audio = (id: string) => snapshot.assets.some((a) => a.id === id && a.usage === 'audio');
  return def.steps.flatMap(({ action }, index) => {
    let incomplete = false;
    switch (action.type) {
      case 'chat':
        incomplete = !action.text.trim();
        break;
      case 'roll':
        incomplete = !action.dice.trim();
        break;
      case 'change_resource':
        incomplete = !action.resource || !action.dice.trim();
        break;
      case 'add_condition':
      case 'remove_condition':
        incomplete = !action.condition;
        break;
      case 'give_item':
        incomplete = !action.name.trim();
        break;
      case 'reveal':
        incomplete = !action.target.id;
        break;
      case 'set_node_visibility':
        incomplete = !action.node;
        break;
      case 'play_sound':
        incomplete = !audio(action.asset) || action.volume < 0 || action.volume > 100;
        break;
      case 'stop_sound':
        incomplete = action.asset !== null && !audio(action.asset);
        break;
    }
    return incomplete ? [index + 1] : [];
  });
}
