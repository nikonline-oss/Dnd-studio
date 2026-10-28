import { useTranslation } from 'react-i18next';
import { useSnapshotSelect } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import type { RevealTarget, SystemRulesDto, TriggerAction, TriggerStep } from '@/shared/ipc/client';
import { ElementField, type ElementKind } from '@/shared/ui/ElementPicker';
import { IconButton } from '@/shared/ui/IconButton';

export type ActionType = TriggerAction['type'];

export const ACTION_TYPES: ActionType[] = [
  'reveal',
  'set_node_visibility',
  'chat',
  'roll',
  'change_resource',
  'add_condition',
  'remove_condition',
  'give_item',
  'play_sound',
  'stop_sound',
];

/** A fresh action of `type`, with the first sensible choices filled in. */
export function newAction(type: ActionType, rules: SystemRulesDto | null): TriggerAction {
  switch (type) {
    case 'reveal':
      return { type, target: { kind: 'node', id: '' } };
    case 'set_node_visibility':
      return { type, node: '', visibility: 'players' };
    case 'chat':
      return { type, text: '', gm_only: false };
    case 'roll':
      return { type, dice: '1d20', label: '' };
    case 'change_resource':
      return { type, resource: rules?.resources[0]?.id ?? '', dice: '1d6', heal: false };
    case 'add_condition':
    case 'remove_condition':
      return { type, condition: rules?.conditions?.[0]?.id ?? '' };
    case 'give_item':
      return { type, name: '' };
    case 'play_sound':
      return { type, asset: '', volume: 80, looped: false };
    case 'stop_sound':
      return { type, asset: null };
  }
}

const REVEAL_KINDS: { kind: RevealTarget['kind']; picker: ElementKind }[] = [
  { kind: 'node', picker: 'board_node' },
  { kind: 'board', picker: 'board' },
  { kind: 'map', picker: 'map' },
  { kind: 'journal', picker: 'journal' },
  { kind: 'token', picker: 'token' },
];

/** One step of a trigger: the action, its parameters and its own condition. */
export function StepEditor({
  step,
  index,
  count,
  rules,
  onChange,
  onMove,
  onRemove,
}: {
  step: TriggerStep;
  index: number;
  count: number;
  rules: SystemRulesDto | null;
  onChange: (step: TriggerStep) => void;
  onMove: (delta: -1 | 1) => void;
  onRemove: () => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const action = step.action;
  const sounds = (useSnapshotSelect((s) => s.assets) ?? [])
    .filter((a) => a.usage === 'audio')
    .sort((a, b) => (a.original_name ?? '').localeCompare(b.original_name ?? ''));
  const set = (next: TriggerAction) => {
    onChange({ ...step, action: next });
  };

  return (
    <li className="trigger-step" aria-label={t('triggers.stepN', { n: index + 1 })}>
      <div className="trigger-step-head">
        <span className="trigger-step-n">{index + 1}</span>
        <select
          aria-label={t('triggers.action')}
          value={action.type}
          onChange={(e) => {
            set(newAction(e.target.value as ActionType, rules));
          }}
        >
          {ACTION_TYPES.map((type) => (
            <option key={type} value={type}>
              {t(`triggers.actions.${type}`)}
            </option>
          ))}
        </select>
        <IconButton
          icon="chevron"
          className="up"
          label={t('triggers.up')}
          disabled={index === 0}
          onClick={() => {
            onMove(-1);
          }}
        />
        <IconButton
          icon="chevron"
          className="down"
          label={t('triggers.down')}
          disabled={index === count - 1}
          onClick={() => {
            onMove(1);
          }}
        />
        <IconButton icon="trash" label={t('triggers.removeStep')} onClick={onRemove} />
      </div>

      <div className="trigger-step-body">
        {action.type === 'reveal' && (
          <>
            <label className="trigger-field">
              <span>{t('triggers.revealWhat')}</span>
              <select
                value={action.target.kind}
                onChange={(e) => {
                  set({
                    type: 'reveal',
                    target: { kind: e.target.value as RevealTarget['kind'], id: '' },
                  });
                }}
              >
                {REVEAL_KINDS.map(({ kind }) => (
                  <option key={kind} value={kind}>
                    {t(`triggers.reveal.${kind}`)}
                  </option>
                ))}
              </select>
            </label>
            <ElementField
              label={t(`triggers.reveal.${action.target.kind}`)}
              kinds={[
                REVEAL_KINDS.find((k) => k.kind === action.target.kind)?.picker ?? 'board_node',
              ]}
              value={
                action.target.id
                  ? {
                      kind:
                        REVEAL_KINDS.find((k) => k.kind === action.target.kind)?.picker ??
                        'board_node',
                      id: action.target.id,
                    }
                  : null
              }
              onChange={({ id }) => {
                set({ type: 'reveal', target: { kind: action.target.kind, id } });
              }}
            />
          </>
        )}
        {action.type === 'set_node_visibility' && (
          <>
            <ElementField
              label={t('triggers.reveal.node')}
              kinds={['board_node']}
              value={action.node ? { kind: 'board_node', id: action.node } : null}
              onChange={({ id }) => {
                set({ ...action, node: id });
              }}
            />
            <label className="trigger-field">
              <span>{t('triggers.visibility')}</span>
              <select
                value={action.visibility}
                onChange={(e) => {
                  set({ ...action, visibility: e.target.value === 'gm' ? 'gm' : 'players' });
                }}
              >
                <option value="players">{t('boards.shownToPlayers')}</option>
                <option value="gm">{t('boards.gmOnly')}</option>
              </select>
            </label>
          </>
        )}
        {action.type === 'chat' && (
          <>
            <label className="trigger-field">
              <span>{t('triggers.text')}</span>
              <textarea
                rows={2}
                maxLength={1000}
                value={action.text}
                onChange={(e) => {
                  set({ ...action, text: e.target.value });
                }}
              />
            </label>
            <label className="inline">
              <input
                type="checkbox"
                checked={action.gm_only}
                onChange={(e) => {
                  set({ ...action, gm_only: e.target.checked });
                }}
              />
              {t('triggers.gmOnly')}
            </label>
          </>
        )}
        {action.type === 'roll' && (
          <>
            <label className="trigger-field">
              <span>{t('triggers.dice')}</span>
              <input
                className="mono"
                value={action.dice}
                onChange={(e) => {
                  set({ ...action, dice: e.target.value });
                }}
              />
            </label>
            <label className="trigger-field">
              <span>{t('triggers.rollLabel')}</span>
              <input
                value={action.label}
                onChange={(e) => {
                  set({ ...action, label: e.target.value });
                }}
              />
            </label>
            <p className="muted trigger-hint">{t('triggers.rollHint')}</p>
          </>
        )}
        {action.type === 'change_resource' && (
          <>
            <label className="trigger-field">
              <span>{t('triggers.resource')}</span>
              <select
                value={action.resource}
                onChange={(e) => {
                  set({ ...action, resource: e.target.value });
                }}
              >
                {(rules?.resources ?? []).map((r) => (
                  <option key={r.id} value={r.id}>
                    {labelText(r.label, lang)}
                  </option>
                ))}
              </select>
            </label>
            <label className="trigger-field">
              <span>{t('triggers.change')}</span>
              <select
                value={action.heal ? 'heal' : 'damage'}
                onChange={(e) => {
                  set({ ...action, heal: e.target.value === 'heal' });
                }}
              >
                <option value="damage">{t('triggers.damage')}</option>
                <option value="heal">{t('triggers.heal')}</option>
              </select>
            </label>
            <label className="trigger-field">
              <span>{t('triggers.dice')}</span>
              <input
                className="mono"
                value={action.dice}
                onChange={(e) => {
                  set({ ...action, dice: e.target.value });
                }}
              />
            </label>
          </>
        )}
        {(action.type === 'add_condition' || action.type === 'remove_condition') && (
          <label className="trigger-field">
            <span>{t('conditions.title')}</span>
            <select
              value={action.condition}
              onChange={(e) => {
                set({ ...action, condition: e.target.value });
              }}
            >
              {(rules?.conditions ?? []).map((c) => (
                <option key={c.id} value={c.id}>
                  {labelText(c.label, lang)}
                </option>
              ))}
            </select>
          </label>
        )}
        {action.type === 'give_item' && (
          <label className="trigger-field">
            <span>{t('items.name')}</span>
            <input
              value={action.name}
              maxLength={200}
              onChange={(e) => {
                set({ ...action, name: e.target.value });
              }}
            />
          </label>
        )}
        {action.type === 'play_sound' && (
          <>
            <label className="trigger-field">
              <span>{t('sounds.sound')}</span>
              <select
                value={action.asset}
                onChange={(e) => {
                  set({ ...action, asset: e.target.value });
                }}
              >
                <option value="" disabled>
                  {t('sounds.choose')}
                </option>
                {sounds.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.original_name ?? a.sha256.slice(0, 8)}
                  </option>
                ))}
              </select>
            </label>
            <label className="trigger-field">
              <span>{t('sounds.volume')}</span>
              <input
                type="range"
                min={0}
                max={100}
                value={action.volume}
                onChange={(e) => {
                  set({ ...action, volume: Number(e.target.value) });
                }}
              />
            </label>
            <label className="inline">
              <input
                type="checkbox"
                checked={action.looped}
                onChange={(e) => {
                  set({ ...action, looped: e.target.checked });
                }}
              />
              {t('sounds.loop')}
            </label>
          </>
        )}
        {action.type === 'stop_sound' && (
          <label className="trigger-field">
            <span>{t('sounds.sound')}</span>
            <select
              value={action.asset ?? ''}
              onChange={(e) => {
                set({ ...action, asset: e.target.value || null });
              }}
            >
              <option value="">{t('sounds.allSounds')}</option>
              {sounds.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.original_name ?? a.sha256.slice(0, 8)}
                </option>
              ))}
            </select>
          </label>
        )}
        <label className="trigger-field">
          <span>{t('triggers.when')}</span>
          <input
            className="mono"
            placeholder={t('triggers.alwaysPlaceholder')}
            value={step.when ?? ''}
            onChange={(e) => {
              onChange({ ...step, when: e.target.value.trim() ? e.target.value : null });
            }}
          />
        </label>
      </div>
    </li>
  );
}
