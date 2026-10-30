import { useState, type MouseEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { TemplateDialog } from './TemplateDialog';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import type { TriggerDto } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ContextMenu } from '@/shared/ui/ContextMenu';
import type { MenuItem } from '@/shared/ui/Menu';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { ConfirmDelete, IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';

/** Navigator: the campaign's triggers (ADR-0024), GM roles only. */
export function TriggerList() {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const submit = useSubmitIntent();
  const triggers = useSnapshotSelect((s) => s.triggers) ?? [];
  const active = useSelection((s) => s.active);
  const openTrigger = useSelection((s) => s.openTrigger);
  const [fromTemplate, setFromTemplate] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number; id: string | null } | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const sorted = [...triggers].sort((a, b) => a.definition.name.localeCompare(b.definition.name));
  const target = triggers.find((trigger) => trigger.id === menu?.id);
  const toDelete = triggers.find((trigger) => trigger.id === deleting);
  const toggle = (trigger: TriggerDto) => {
    if (!isGm || submit.isPending) return;
    submit.mutate({
      intent: {
        kind: 'update_trigger',
        id: trigger.id,
        definition: { ...trigger.definition, enabled: !trigger.definition.enabled },
      },
      expectedVersion: trigger.version,
    });
  };
  const remove = (trigger: TriggerDto) => {
    if (!isGm || submit.isPending) return;
    setDeleting(null);
    submit.mutate({
      intent: { kind: 'delete_trigger', id: trigger.id },
      expectedVersion: trigger.version,
    });
  };
  const create = () => {
    if (!isGm || submit.isPending) return;
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
              { when: null, action: { type: 'chat', text: t('triggers.newText'), gm_only: true } },
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
  };
  const openMenu = (e: MouseEvent, id: string | null) => {
    if ((e.target as HTMLElement).closest('[role="menu"], [role="alertdialog"], [role="dialog"]'))
      return;
    e.preventDefault();
    e.stopPropagation();
    if (isGm) setMenu({ x: e.clientX, y: e.clientY, id });
  };
  const items: (MenuItem | 'separator')[] = [];
  if (target) {
    items.push(
      {
        label: t('context.open', { defaultValue: 'Open' }),
        onSelect: () => {
          openTrigger(target.id);
        },
      },
      {
        label: target.definition.enabled
          ? t('context.disable', { defaultValue: 'Disable' })
          : t('context.enable', { defaultValue: 'Enable' }),
        disabled: submit.isPending,
        onSelect: () => {
          toggle(target);
        },
      },
      {
        label: t('triggers.run'),
        disabled: submit.isPending || target.definition.event.type !== 'manual' || target.fired,
        onSelect: () => {
          if (isGm && !submit.isPending)
            submit.mutate({ intent: { kind: 'fire_trigger', id: target.id, token: null } });
        },
      },
      {
        label: t('triggers.delete'),
        disabled: submit.isPending,
        onSelect: () => {
          setDeleting(target.id);
        },
      },
      'separator',
    );
  }
  items.push(
    { label: t('triggers.add'), disabled: submit.isPending, onSelect: create },
    {
      label: t('triggers.fromTemplate'),
      disabled: submit.isPending,
      onSelect: () => {
        setFromTemplate(true);
      },
    },
  );
  if (!isGm) return null;
  return (
    <div
      style={{ display: 'contents' }}
      onContextMenu={(e) => {
        openMenu(e, null);
      }}
    >
      <NavSection
        icon="bolt"
        title={t('triggers.title')}
        count={triggers.length}
        actions={
          <>
            <IconButton
              icon="puzzle"
              label={t('triggers.fromTemplate')}
              disabled={submit.isPending}
              onClick={() => {
                setFromTemplate(true);
              }}
            />
            <IconButton
              icon="plus"
              label={t('triggers.add')}
              disabled={submit.isPending}
              onClick={create}
            />
          </>
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
              onContextMenu={(e) => {
                openMenu(e, trigger.id);
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
                  disabled={submit.isPending}
                  onChange={() => {
                    toggle(trigger);
                  }}
                />
                <Icon name={trigger.definition.enabled ? 'check' : 'x'} />
              </label>
              <span className="row-actions">
                <ConfirmDelete
                  label={t('triggers.delete')}
                  disabled={submit.isPending}
                  onConfirm={() => {
                    remove(trigger);
                  }}
                />
              </span>
            </li>
          ))}
        </ul>
        <ErrorText error={submit.error} />
        {fromTemplate && (
          <TemplateDialog
            onClose={() => {
              setFromTemplate(false);
            }}
          />
        )}
        {toDelete && (
          <div
            className="confirm-bar"
            role="alertdialog"
            aria-label={t('triggers.delete')}
            onKeyDown={(e) => {
              if (e.key === 'Escape') setDeleting(null);
            }}
          >
            <p>
              {t('triggers.delete')}: {toDelete.definition.name}
            </p>
            <button
              type="button"
              className="confirm-delete"
              autoFocus
              disabled={submit.isPending}
              onClick={() => {
                remove(toDelete);
              }}
            >
              {t('common.confirmDelete')}
            </button>
            <button
              type="button"
              onClick={() => {
                setDeleting(null);
              }}
            >
              {t('journal.cancel')}
            </button>
          </div>
        )}
        {menu && (
          <ContextMenu
            x={menu.x}
            y={menu.y}
            label={t('triggers.title')}
            items={items}
            onClose={() => {
              setMenu(null);
            }}
          />
        )}
      </NavSection>
    </div>
  );
}
