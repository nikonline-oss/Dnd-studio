import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSubmitIntent } from '@/shared/game/useGame';
import type { MapDto } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';

/** Bounds of the map description in the core (`MAX_DESCRIPTION_LEN`). */
export const MAX_MAP_DESCRIPTION = 20_000;

/**
 * The map's free text. Everyone at the table reads it in the inspector; GM roles edit it with one
 * `UpdateMap` carrying the map version (ADR-0003).
 */
export function MapDescription({ map }: { map: MapDto }) {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const text = map.description ?? '';

  if (!isGm) {
    if (!text) return null;
    return (
      <section className="inspector-section map-description">
        <h3 className="section-title">
          <Icon name="book" />
          {t('inspector.description')}
        </h3>
        <p className="map-description-text">{text}</p>
      </section>
    );
  }

  return <MapDescriptionForm key={`${map.id}:${map.version}`} map={map} />;
}

function MapDescriptionForm({ map }: { map: MapDto }) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const [text, setText] = useState(map.description ?? '');
  const trimmed = text.trim();
  const changed = trimmed !== (map.description ?? '');
  const tooLong = trimmed.length > MAX_MAP_DESCRIPTION;

  return (
    <section className="inspector-section map-description">
      <form
        className="description-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (!changed || tooLong) return;
          submit.mutate({
            intent: {
              kind: 'update_map',
              id: map.id,
              name: null,
              description: trimmed,
              grid: null,
              visible_to_players: null,
            },
            expectedVersion: map.version,
          });
        }}
      >
        <label htmlFor="map-description">{t('inspector.description')}</label>
        <textarea
          id="map-description"
          value={text}
          rows={4}
          maxLength={MAX_MAP_DESCRIPTION}
          placeholder={t('inspector.descriptionPlaceholder')}
          onChange={(e) => {
            setText(e.currentTarget.value);
          }}
        />
        <div className="description-actions">
          <span className="muted">{t('inspector.descriptionHint')}</span>
          <button type="submit" disabled={!changed || tooLong}>
            {t('inspector.descriptionSave')}
          </button>
        </div>
        <ErrorText error={submit.error} />
      </form>
    </section>
  );
}
