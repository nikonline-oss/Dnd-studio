import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { tokenCssColor, tokenInitial } from '@/shared/game/tokenLook';
import { labelText } from '@/shared/i18n/label';
import type { ConditionDef, TokenDto, TokenEffectDto, TokenLayer } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { ConfirmDelete, IconButton } from '@/shared/ui/IconButton';

const LAYERS: TokenLayer[] = ['background', 'tokens', 'gm'];
const DISPOSITIONS = ['neutral', 'ally', 'enemy'] as const;
/** Sizes offered in grid cells; the core accepts any positive scale. */
const SCALES = [0.5, 1, 2, 3, 4];

type Changes = Partial<
  Pick<TokenDto, 'name' | 'rotation' | 'scale' | 'layer' | 'visible' | 'disposition'>
>;

/** Inspector for the token selected on the map: GM roles change its properties (§5). */
export function TokenInspector() {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const tokenId = useSelection((s) => s.tokenId);
  const selectToken = useSelection((s) => s.selectToken);
  const openCharacter = useSelection((s) => s.openCharacter);
  const token = useSnapshotSelect((s) => s.tokens.find((tok) => tok.id === tokenId));
  const characters = useSnapshotSelect((s) => s.characters);
  const effects = (useSnapshotSelect((s) => s.effects) ?? []).filter((e) => e.token === tokenId);
  const conditions =
    useSnapshotSelect((s) => s.plugins.find((p) => p.id === s.system_plugin)?.system?.conditions) ??
    [];

  if (!token || !characters) return <p className="inspector-empty muted">{t('inspector.empty')}</p>;
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const character = characters.find((c) => c.id === token.character_id);

  return (
    <div className="inspector-stack">
      <section className="inspector-section">
        <div className="token-head">
          <span
            className="avatar large"
            style={{ background: tokenCssColor(token.character_id ?? token.id), color: '#fff' }}
            aria-hidden="true"
          >
            {tokenInitial(token.name)}
          </span>
          <div>
            <strong>{token.name || t('tokens.unnamed')}</strong>
            <p className="muted">
              {character ? t('tokens.linkedTo', { name: character.name }) : t('tokens.noCharacter')}
            </p>
            {character && (
              <button
                className="btn btn-ghost"
                onClick={() => {
                  openCharacter(character.id);
                }}
              >
                {t('tokens.openCharacterSheet')}
              </button>
            )}
          </div>
        </div>
        {isGm ? (
          // Remounted on every version so drafts never hide a newer value.
          <TokenForm
            key={`${token.id}:${token.version}`}
            token={token}
            onDeleted={() => {
              selectToken(null);
            }}
          />
        ) : (
          <dl className="properties">
            <dt>{t('tokens.size')}</dt>
            <dd>{t('tokens.cells', { count: token.scale })}</dd>
          </dl>
        )}
      </section>
      {effects.length > 0 && (
        <section className="inspector-section" aria-label={t('effects.title')}>
          <h3>{t('effects.title')}</h3>
          <ul className="token-effects">
            {effects.map((effect) => (
              <TokenEffect key={effect.id} effect={effect} conditions={conditions} isGm={isGm} />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

/** One effect on the token: label, condition badge, time left; GM roles may remove it. */
function TokenEffect({
  effect,
  conditions,
  isGm,
}: {
  effect: TokenEffectDto;
  conditions: ConditionDef[];
  isGm: boolean;
}) {
  const { t, i18n } = useTranslation();
  const submit = useSubmitIntent();
  const now = useNow();
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const duration = effect.duration;
  const left =
    duration.type === 'rounds'
      ? t('effects.roundsLeft', { count: duration.remaining })
      : duration.type === 'turns'
        ? t('effects.turnsLeft', { count: duration.remaining })
        : duration.type === 'turn_end'
          ? t('effects.untilTurnEnd')
          : now === null
            ? null
            : t('effects.minutesLeft', {
                count: Math.max(0, Math.ceil((duration.until_ts - now) / 60_000)),
              });
  const condition = effect.condition
    ? labelText(conditions.find((c) => c.id === effect.condition)?.label ?? effect.condition, lang)
    : null;

  return (
    <li className="token-effect">
      <span className="token-effect-label">{effect.label}</span>
      {condition && <span className="condition-chip">{condition}</span>}
      {left && <span className="muted">{left}</span>}
      {isGm && (
        <IconButton
          icon="trash"
          label={t('effects.remove')}
          disabled={submit.isPending}
          onClick={() => {
            submit.mutate({
              intent: { kind: 'remove_token_effect', id: effect.id },
              expectedVersion: effect.version,
            });
          }}
        />
      )}
    </li>
  );
}

/** Current time, refreshed twice a minute; `null` until the first tick (after mount). */
function useNow(): number | null {
  const [now, setNow] = useState<number | null>(null);
  useEffect(() => {
    const tick = () => {
      setNow(Date.now());
    };
    tick();
    const timer = setInterval(tick, 30_000);
    return () => {
      clearInterval(timer);
    };
  }, []);
  return now;
}

function TokenForm({ token, onDeleted }: { token: TokenDto; onDeleted: () => void }) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const [name, setName] = useState(token.name);
  const [rotation, setRotation] = useState(String(Math.round(token.rotation)));

  const update = (changes: Changes) => {
    submit.mutate({
      intent: {
        kind: 'update_token',
        id: token.id,
        name: changes.name ?? null,
        rotation: changes.rotation ?? null,
        scale: changes.scale ?? null,
        layer: changes.layer ?? null,
        visible: changes.visible ?? null,
        disposition: changes.disposition ?? null,
      },
      expectedVersion: token.version,
    });
  };

  const scales = SCALES.includes(token.scale)
    ? SCALES
    : [...SCALES, token.scale].sort((a, b) => a - b);

  return (
    <div className="grid-form token-form">
      <label htmlFor="token-name">{t('inspector.name')}</label>
      <input
        id="token-name"
        value={name}
        maxLength={200}
        onChange={(e) => {
          setName(e.target.value);
        }}
        onBlur={() => {
          if (name.trim() !== token.name) update({ name: name.trim() });
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur();
        }}
      />
      <label htmlFor="token-layer">{t('tokens.layer')}</label>
      <select
        id="token-layer"
        value={token.layer}
        onChange={(e) => {
          update({ layer: e.target.value as TokenLayer });
        }}
      >
        {LAYERS.map((layer) => (
          <option key={layer} value={layer}>
            {t(`tokens.layers.${layer}`)}
          </option>
        ))}
      </select>
      <label htmlFor="token-disposition">{t('tokens.disposition')}</label>
      <select
        id="token-disposition"
        value={token.disposition ?? 'neutral'}
        onChange={(e) => {
          update({ disposition: e.target.value as NonNullable<TokenDto['disposition']> });
        }}
      >
        {DISPOSITIONS.map((disposition) => (
          <option key={disposition} value={disposition}>
            {t(`tokens.dispositions.${disposition}`)}
          </option>
        ))}
      </select>
      <label htmlFor="token-size">{t('tokens.size')}</label>
      <select
        id="token-size"
        value={token.scale}
        onChange={(e) => {
          update({ scale: Number(e.target.value) });
        }}
      >
        {scales.map((scale) => (
          <option key={scale} value={scale}>
            {t('tokens.cells', { count: scale })}
          </option>
        ))}
      </select>
      <label htmlFor="token-rotation">{t('tokens.rotation')}</label>
      <input
        id="token-rotation"
        type="number"
        step={15}
        value={rotation}
        onChange={(e) => {
          setRotation(e.target.value);
        }}
        onBlur={() => {
          const value = Number(rotation);
          if (Number.isFinite(value) && value !== Math.round(token.rotation)) {
            update({ rotation: value });
          }
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur();
        }}
      />
      <span />
      <label className="inline">
        <input
          type="checkbox"
          checked={token.visible}
          onChange={(e) => {
            update({ visible: e.target.checked });
          }}
        />
        <Icon name={token.visible ? 'eye' : 'eyeOff'} />
        {t('tokens.visibleToPlayers')}
      </label>
      <span />
      <ConfirmDelete
        label={t('tokens.delete')}
        disabled={submit.isPending}
        onConfirm={() => {
          submit.mutate({
            intent: { kind: 'delete_token', id: token.id },
            expectedVersion: token.version,
          });
          onDeleted();
        }}
      />
      <ErrorText error={submit.error} />
    </div>
  );
}
