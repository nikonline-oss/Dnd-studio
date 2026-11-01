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
      case 'play_sting':
        incomplete = !audio(action.asset) || action.volume < 0 || action.volume > 100;
        break;
      case 'spawn_tokens':
        incomplete =
          !action.character ||
          action.count < 1 ||
          action.count > 20 ||
          (action.placement.type === 'zone' && !action.placement.id);
        break;
      case 'start_combat':
        break;
    }
    return incomplete ? [index + 1] : [];
  });
}
