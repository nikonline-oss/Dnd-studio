import { useRef } from 'react';
import { useListCommands } from '@/shared/ui/useListCommands';
import { useTranslation } from 'react-i18next';
import { useZoneTool } from './zoneTool';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { ConfirmDelete, IconButton } from '@/shared/ui/IconButton';
import { InlineEdit } from '@/shared/ui/InlineEdit';

/**
 * The zones of a map (ADR-0024), GM roles only: rename, edit on the map, redraw, add a part, merge
 * ticked zones, delete. New zones are drawn with the map's zone tool.
 */
export function ZoneList({ mapId }: { mapId: string }) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const commands = useListCommands();
  const { data: campaign } = useCampaign();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const colorInputs = useRef(new Map<string, HTMLInputElement>());
  const zones = useSnapshotSelect((s) => s.zones.filter((z) => z.map_id === mapId)) ?? [];
  const drawing = useZoneTool((s) => s.drawing);
  const draw = useZoneTool((s) => s.draw);
  const ticked = useZoneTool((s) => s.ticked).filter((id) => zones.some((z) => z.id === id));
  const toggle = useZoneTool((s) => s.toggle);
  const clearTicks = useZoneTool((s) => s.clearTicks);
  const editing = useZoneTool((s) => s.editing);
  const selected = useZoneTool((s) => s.selected);
  const [into, ...from] = ticked;
  const target = zones.find((z) => z.id === into);

  const backgroundItems = isGm
    ? [
        {
          label: t('zones.new', { defaultValue: 'Draw new zone' }),
          onSelect: () => {
            useZoneTool.getState().create();
          },
        },
        {
          label: t('zones.merge', { name: target?.name ?? '' }),
          disabled: !target || from.length === 0,
          onSelect: () => {
            if (target)
              submit.mutate(
                {
                  intent: { kind: 'merge_zones', into: target.id, from },
                  expectedVersion: target.version,
                },
                { onSuccess: clearTicks },
              );
          },
        },
      ]
    : [];
  if (!isGm) return null;
  return (
    <div
      onContextMenu={(e) => {
        commands.context(e, backgroundItems);
      }}
    >
      <section className="inspector-section">
        <h3 className="section-title">
          <Icon name="zone" />
          {t('zones.title')}
        </h3>
        {zones.length === 0 && <p className="muted">{t('zones.empty')}</p>}
        {drawing.mode !== 'new' && (
          <p className="notice" role="status">
            {drawing.mode === 'replace' ? t('zones.drawReplace') : t('zones.drawAdd')}{' '}
            <button
              type="button"
              onClick={() => {
                draw({ mode: 'new' });
              }}
            >
              {t('picker.cancel')}
            </button>
          </p>
        )}
        <ul className="zone-list" aria-label={t('zones.title')}>
          {zones.map((zone) => (
            <li
              onContextMenu={(e) => {
                commands.context(e, [
                  {
                    label: t('zones.editOne', { name: zone.name }),
                    onSelect: () => {
                      const tool = useZoneTool.getState();
                      tool.setEditing(true);
                      tool.select(zone.id);
                    },
                  },
                  {
                    label: t('zones.rename'),
                    onSelect: () => {
                      commands.rename(t('zones.rename'), zone.name, (name) => {
                        submit.mutate({
                          intent: { kind: 'update_zone', id: zone.id, name, shape: null },
                          expectedVersion: zone.version,
                        });
                      });
                    },
                  },
                  {
                    label: t('zones.color', { name: zone.name }),
                    onSelect: () => colorInputs.current.get(zone.id)?.click(),
                  },
                  {
                    label: t('zones.redraw'),
                    onSelect: () => {
                      draw({ mode: 'replace', zoneId: zone.id });
                    },
                  },
                  {
                    label: t('zones.addPart'),
                    onSelect: () => {
                      draw({ mode: 'add', zoneId: zone.id });
                    },
                  },
                  {
                    label: t('zones.tick', { name: zone.name }),
                    onSelect: () => {
                      toggle(zone.id);
                    },
                  },
                  {
                    label: t('zones.delete'),
                    onSelect: () => {
                      commands.confirm(t('zones.delete') + ': ' + zone.name, () => {
                        submit.mutate({
                          intent: { kind: 'delete_zone', id: zone.id },
                          expectedVersion: zone.version,
                        });
                      });
                    },
                  },
                  ...backgroundItems,
                ]);
              }}
              key={zone.id}
              className="nav-row"
              aria-current={editing && zone.id === selected ? 'true' : undefined}
            >
              <input
                type="checkbox"
                aria-label={t('zones.tick', { name: zone.name })}
                checked={ticked.includes(zone.id)}
                onChange={() => {
                  toggle(zone.id);
                }}
              />
              <input
                ref={(element) => {
                  if (element) colorInputs.current.set(zone.id, element);
                  else colorInputs.current.delete(zone.id);
                }}
                type="color"
                aria-label={t('zones.color', { name: zone.name })}
                title={t('zones.color', { name: zone.name })}
                value={`#${(zone.color ?? 0x9b7bff).toString(16).padStart(6, '0')}`}
                disabled={submit.isPending}
                onChange={(event) => {
                  submit.mutate({
                    intent: {
                      kind: 'set_zone_color',
                      id: zone.id,
                      color: Number.parseInt(event.target.value.slice(1), 16),
                    },
                    expectedVersion: zone.version,
                  });
                }}
              />
              <InlineEdit
                value={zone.name}
                label={t('zones.rename')}
                onCommit={(name) => {
                  submit.mutate({
                    intent: { kind: 'update_zone', id: zone.id, name, shape: null },
                    expectedVersion: zone.version,
                  });
                }}
              />
              <span className="row-actions">
                <IconButton
                  icon="pointer"
                  label={t('zones.editOne', { name: zone.name })}
                  onClick={() => {
                    const tool = useZoneTool.getState();
                    tool.setEditing(true);
                    tool.select(zone.id);
                  }}
                />
                <IconButton
                  icon="brush"
                  label={t('zones.redraw')}
                  onClick={() => {
                    draw({ mode: 'replace', zoneId: zone.id });
                  }}
                />
                <IconButton
                  icon="plus"
                  label={t('zones.addPart')}
                  onClick={() => {
                    draw({ mode: 'add', zoneId: zone.id });
                  }}
                />
                <ConfirmDelete
                  label={t('zones.delete')}
                  onConfirm={() => {
                    submit.mutate({
                      intent: { kind: 'delete_zone', id: zone.id },
                      expectedVersion: zone.version,
                    });
                  }}
                />
              </span>
            </li>
          ))}
        </ul>
        {ticked.length > 0 && (
          <div className="inline actions">
            <button
              type="button"
              disabled={!target || from.length === 0 || submit.isPending}
              onClick={() => {
                if (!target) return;
                submit.mutate(
                  {
                    intent: { kind: 'merge_zones', into: target.id, from },
                    expectedVersion: target.version,
                  },
                  { onSuccess: clearTicks },
                );
              }}
            >
              {t('zones.merge', { name: target?.name ?? '' })}
            </button>
            <span className="muted">{t('zones.mergeHint')}</span>
          </div>
        )}
        <ErrorText error={submit.error} />
      </section>
      {commands.ui}
    </div>
  );
}
