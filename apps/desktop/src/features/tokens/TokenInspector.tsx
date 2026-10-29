import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { tokenCssColor, tokenInitial } from '@/shared/game/tokenLook';
import type { TokenDto, TokenLayer } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { ConfirmDelete } from '@/shared/ui/IconButton';

const LAYERS: TokenLayer[] = ['background', 'tokens', 'gm'];
/** Sizes offered in grid cells; the core accepts any positive scale. */
const SCALES = [0.5, 1, 2, 3, 4];

type Changes = Partial<Pick<TokenDto, 'name' | 'rotation' | 'scale' | 'layer' | 'visible'>>;

/** Inspector for the token selected on the map: GM roles change its properties (§5). */
export function TokenInspector() {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const tokenId = useSelection((s) => s.tokenId);
  const selectToken = useSelection((s) => s.selectToken);
  const openCharacter = useSelection((s) => s.openCharacter);
  const token = useSnapshotSelect((s) => s.tokens.find((tok) => tok.id === tokenId));
  const characters = useSnapshotSelect((s) => s.characters);

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
    </div>
  );
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
