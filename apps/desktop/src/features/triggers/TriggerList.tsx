import { useTranslation } from 'react-i18next';
import { useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import type { TriggerDto } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { ConfirmDelete, IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';

/** Navigator: the campaign's triggers (ADR-0024), GM roles only. */
export function TriggerList() {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const triggers = useSnapshotSelect((s) => s.triggers) ?? [];
  const active = useSelection((s) => s.active);
  const openTrigger = useSelection((s) => s.openTrigger);
  const sorted = [...triggers].sort((a, b) => a.definition.name.localeCompare(b.definition.name));

  const toggle = (trigger: TriggerDto) => {
    submit.mutate({
      intent: {
        kind: 'update_trigger',
        id: trigger.id,
        definition: { ...trigger.definition, enabled: !trigger.definition.enabled },
      },
      expectedVersion: trigger.version,
    });
  };

  return (
    <NavSection
      icon="bolt"
      title={t('triggers.title')}
      count={triggers.length}
      actions={
        <IconButton
          icon="plus"
          label={t('triggers.add')}
          disabled={submit.isPending}
          onClick={() => {
            const id = uuidv7();
            submit.mutate(
              {
                intent: {
                  kind: 'create_trigger',
                  id,
                  definition: {
                    name: `${t('triggers.newName')} ${triggers.length + 1}`,
                    enabled: false,
                    once: false,
                    event: { type: 'manual' },
                    tokens: [],
                    condition: null,
                    steps: [
                      {
                        when: null,
                        action: { type: 'chat', text: t('triggers.newText'), gm_only: true },
                      },
                    ],
                  },
                },
              },
              {
                onSuccess: () => {
                  openTrigger(id);
                },
              },
            );
          }}
        />
      }
    >
      {triggers.length === 0 && <p className="nav-empty">{t('triggers.empty')}</p>}
      <ul className="nav-list" aria-label={t('triggers.title')}>
        {sorted.map((trigger) => (
          <li
            key={trigger.id}
            className="nav-row"
            data-disabled={!trigger.definition.enabled}
            aria-current={active?.kind === 'trigger' && active.id === trigger.id}
            onClick={() => {
              openTrigger(trigger.id);
            }}
          >
            <Icon name="bolt" className="row-icon" />
            <span className="nav-row-name">
              {trigger.definition.name}
              <span className="muted">
                {' '}
                · {t(`triggers.events.${trigger.definition.event.type}`)}
              </span>
            </span>
            {trigger.fired && <span className="badge">{t('triggers.fired')}</span>}
            <label
              className="eye-toggle"
              data-on={trigger.definition.enabled}
              title={trigger.definition.enabled ? t('triggers.on') : t('triggers.off')}
              onClick={(e) => {
                e.stopPropagation();
              }}
            >
              <input
                type="checkbox"
                className="visually-hidden"
                aria-label={t('triggers.enabled')}
                checked={trigger.definition.enabled}
                onChange={() => {
                  toggle(trigger);
                }}
              />
              <Icon name={trigger.definition.enabled ? 'check' : 'x'} />
            </label>
            <span className="row-actions">
              <ConfirmDelete
                label={t('triggers.delete')}
                onConfirm={() => {
                  submit.mutate({
                    intent: { kind: 'delete_trigger', id: trigger.id },
                    expectedVersion: trigger.version,
                  });
                }}
              />
            </span>
          </li>
        ))}
      </ul>
      <ErrorText error={submit.error} />
    </NavSection>
  );
}
