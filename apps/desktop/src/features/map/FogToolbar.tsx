import { useTranslation } from 'react-i18next';
import { MAX_BRUSH_RADIUS, MIN_BRUSH_RADIUS } from './fogShapes';
import type { FogTool } from './MapScene';
import type { FogOpKind } from '@/shared/ipc/client';
import { Icon, type IconName } from '@/shared/ui/Icon';
import { ConfirmButton } from '@/shared/ui/IconButton';

/** What the toolbar selects: a fog tool, or panning. */
export interface FogToolState {
  shape: 'pan' | FogTool['shape'];
  kind: FogOpKind;
  radius: number;
}

export const DEFAULT_FOG_TOOL: FogToolState = { shape: 'pan', kind: 'reveal', radius: 40 };

/** The scene's tool for a toolbar state (`null` = pan). */
export function sceneTool(state: FogToolState): FogTool | null {
  return state.shape === 'pan'
    ? null
    : { shape: state.shape, kind: state.kind, radius: state.radius };
}

interface Props {
  state: FogToolState;
  onChange: (state: FogToolState) => void;
  onReset: (covered: boolean) => void;
  /** The map has fog at all (otherwise "clear" has nothing to clear). */
  hasFog: boolean;
  disabled: boolean;
}

/** GM fog tools (ADR-0019): pan / rectangle / brush, reveal or hide, brush size, reset. */
export function FogToolbar({ state, onChange, onReset, hasFog, disabled }: Props) {
  const { t } = useTranslation();
  const shapes: [FogToolState['shape'], IconName, string][] = [
    ['pan', 'hand', t('fog.pan')],
    ['rect', 'square', t('fog.rect')],
    ['brush', 'brush', t('fog.brush')],
    ['zone', 'zone', t('zones.draw')],
  ];
  const kinds: [FogOpKind, IconName, string][] = [
    ['reveal', 'eye', t('fog.reveal')],
    ['hide', 'eyeOff', t('fog.hide')],
  ];

  return (
    <div className="fog-toolbar" role="toolbar" aria-label={t('fog.title')}>
      <span className="fog-label">
        <Icon name="cloud" />
        {t('fog.title')}
      </span>
      <div className="segmented" role="radiogroup" aria-label={t('fog.tool')}>
        {shapes.map(([shape, icon, label]) => (
          <button
            key={shape}
            type="button"
            role="radio"
            aria-checked={state.shape === shape}
            aria-label={label}
            title={label}
            onClick={() => {
              onChange({ ...state, shape });
            }}
          >
            <Icon name={icon} />
          </button>
        ))}
      </div>
      {state.shape !== 'pan' && state.shape !== 'zone' && (
        <div className="segmented" role="radiogroup" aria-label={t('fog.mode')}>
          {kinds.map(([kind, icon, label]) => (
            <button
              key={kind}
              type="button"
              role="radio"
              aria-checked={state.kind === kind}
              title={label}
              onClick={() => {
                onChange({ ...state, kind });
              }}
            >
              <Icon name={icon} />
              {label}
            </button>
          ))}
        </div>
      )}
      {state.shape === 'brush' && (
        <label className="brush-size" title={t('fog.brushSize')}>
          <span className="visually-hidden">{t('fog.brushSize')}</span>
          <input
            type="range"
            min={MIN_BRUSH_RADIUS}
            max={MAX_BRUSH_RADIUS}
            value={state.radius}
            onChange={(e) => {
              onChange({ ...state, radius: Number(e.target.value) });
            }}
          />
          <output>{state.radius}</output>
        </label>
      )}
      <span className="toolbar-divider" aria-hidden="true" />
      <ConfirmButton
        icon="cloud"
        label={t('fog.coverAll')}
        confirmLabel={t('fog.confirmCover')}
        disabled={disabled}
        onConfirm={() => {
          onReset(true);
        }}
      />
      <ConfirmButton
        icon="sparkle"
        label={t('fog.clearAll')}
        confirmLabel={t('fog.confirmClear')}
        disabled={disabled || !hasFog}
        onConfirm={() => {
          onReset(false);
        }}
      />
    </div>
  );
}
