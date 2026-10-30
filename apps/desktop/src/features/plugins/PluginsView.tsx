import { useMutation, useQuery } from '@tanstack/react-query';
import { useState, type MouseEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { useRemote } from '@/shared/game/useRemote';
import { labelText } from '@/shared/i18n/label';
import { ipc, type PluginDto } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';
import { ConfirmButton, IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';
import { ContextMenu } from '@/shared/ui/ContextMenu';
import type { MenuItem } from '@/shared/ui/Menu';

/**
 * Navigator: the campaign's plugins (ADR-0023). GM roles install bundled ones or a `.dndplugin`
 * file, choose the campaign's rule system and uninstall the rest. Every local plugin is shown as
 * unsigned until the catalog signs them (stage 5).
 */
export function PluginsView() {
  const { t, i18n } = useTranslation();
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const { data: campaign } = useCampaign();
  const remote = useRemote();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const canManage =
    isGm &&
    (remote === null ||
      remote.status.state === 'online' ||
      (remote.hosted && remote.status.state === 'reconnecting'));
  const plugins = useSnapshotSelect((s) => s.plugins) ?? [];
  const system = useSnapshotSelect((s) => s.system_plugin) ?? null;
  const bundled = useQuery({ queryKey: ['pluginBundled'], queryFn: ipc.pluginBundled });
  const submit = useSubmitIntent();
  const installBundled = useMutation({ mutationFn: ipc.pluginInstallBundled });
  const installFile = useMutation({ mutationFn: ipc.pluginInstallFile });
  const notInstalled = (bundled.data ?? []).filter((b) => !plugins.some((p) => p.id === b.id));
  const busy = submit.isPending || installBundled.isPending || installFile.isPending;
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    id: string | null;
    bundled: boolean;
  } | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const toDelete = plugins.find((p) => p.id === deleting && p.id !== system);
  const openMenu = (e: MouseEvent, id: string | null, bundled = false) => {
    if ((e.target as HTMLElement).closest('[role="menu"], [role="alertdialog"]')) return;
    e.preventDefault();
    e.stopPropagation();
    setMenu({ x: e.clientX, y: e.clientY, id, bundled });
  };
  const menuItems: (MenuItem | 'separator')[] = [];
  const target = plugins.find((p) => p.id === menu?.id);
  if (canManage) {
    if (menu?.bundled && notInstalled.some((p) => p.id === menu.id)) {
      menuItems.push({
        label: t('plugins.install'),
        disabled: busy,
        onSelect: () => {
          if (menu.id && !busy) installBundled.mutate(menu.id);
        },
      });
    } else if (target) {
      if (target.kind === 'system')
        menuItems.push({
          label: t('plugins.makeSystem'),
          disabled: busy || target.id === system,
          onSelect: () => {
            if (!busy) submit.mutate({ intent: { kind: 'set_system_plugin', id: target.id } });
          },
        });
      menuItems.push({
        label: t('plugins.uninstall'),
        disabled: busy || target.id === system,
        onSelect: () => {
          setDeleting(target.id);
        },
      });
    }
    menuItems.push({
      label: t('plugins.installFile'),
      disabled: busy,
      onSelect: () => {
        if (!busy) installFile.mutate();
      },
    });
    for (const plugin of notInstalled) {
      if (menu?.bundled && menu.id === plugin.id) continue;
      menuItems.push({
        label: `${t('plugins.install')}: ${labelText(plugin.name, lang)}`,
        disabled: busy,
        onSelect: () => {
          if (!busy) installBundled.mutate(plugin.id);
        },
      });
    }
  }
  menuItems.push({
    label: t('context.refresh', { defaultValue: 'Refresh' }),
    disabled: busy || bundled.isFetching,
    onSelect: () => {
      void bundled.refetch();
    },
  });

  const row = (plugin: PluginDto) => {
    const isSystem = plugin.id === system;
    return (
      <li
        key={plugin.id}
        className="plugin-row"
        aria-current={isSystem}
        onContextMenu={(e) => {
          openMenu(e, plugin.id);
        }}
      >
        <div className="plugin-name">
          <strong>{labelText(plugin.name, lang)}</strong>
          <span className="muted">
            {plugin.version} · {t(`plugins.kinds.${plugin.kind}`)} · {t('plugins.license')}{' '}
            {plugin.license}
          </span>
          <span className="plugin-badges">
            {isSystem && <span className="badge">{t('plugins.system')}</span>}
            <span className="badge" data-trust="unsigned" title={t('plugins.unsignedHint')}>
              {t('plugins.unsigned')}
            </span>
          </span>
        </div>
        {canManage && (
          <span className="row-actions">
            {plugin.kind === 'system' && !isSystem && (
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  submit.mutate({ intent: { kind: 'set_system_plugin', id: plugin.id } });
                }}
              >
                {t('plugins.makeSystem')}
              </button>
            )}
            {!isSystem && (
              <ConfirmButton
                icon="trash"
                label={t('plugins.uninstall')}
                confirmLabel={t('common.confirmDelete')}
                disabled={busy}
                onConfirm={() => {
                  submit.mutate({ intent: { kind: 'uninstall_plugin', id: plugin.id } });
                }}
              />
            )}
          </span>
        )}
      </li>
    );
  };

  return (
    <div
      style={{ display: 'contents' }}
      onContextMenu={(e) => {
        openMenu(e, null);
      }}
    >
      <NavSection
        icon="puzzle"
        title={t('plugins.title')}
        count={plugins.length}
        actions={
          canManage && (
            <IconButton
              icon="filePlus"
              label={t('plugins.installFile')}
              disabled={busy}
              onClick={() => {
                installFile.mutate();
              }}
            />
          )
        }
      >
        {plugins.length === 0 && <p className="nav-empty">{t('plugins.empty')}</p>}
        <ul className="plugin-list" aria-label={t('plugins.title')}>
          {plugins.map(row)}
        </ul>
        {canManage && notInstalled.length > 0 && (
          <section className="plugin-bundled" aria-label={t('plugins.bundled')}>
            <h3>{t('plugins.bundled')}</h3>
            <ul className="plugin-list">
              {notInstalled.map((b) => (
                <li
                  key={b.id}
                  className="plugin-row"
                  onContextMenu={(e) => {
                    openMenu(e, b.id, true);
                  }}
                >
                  <div className="plugin-name">
                    <strong>{labelText(b.name, lang)}</strong>
                    <span className="muted">
                      {b.version} · {t(`plugins.kinds.${b.kind}`)}
                    </span>
                  </div>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => {
                      installBundled.mutate(b.id);
                    }}
                  >
                    {t('plugins.install')}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}
        {isGm && !canManage && <p className="nav-empty">{t('plugins.remote')}</p>}
        <ErrorText error={installFile.error ?? installBundled.error ?? submit.error} />
        {canManage && toDelete && (
          <div
            role="alertdialog"
            aria-label={t('plugins.uninstall')}
            className="confirm-bar"
            onKeyDown={(e) => {
              if (e.key === 'Escape') setDeleting(null);
            }}
          >
            <p>
              {t('plugins.uninstall')}: {labelText(toDelete.name, lang)}
            </p>
            <button
              type="button"
              className="confirm-delete"
              autoFocus
              disabled={busy}
              onClick={() => {
                if (busy) return;
                submit.mutate({ intent: { kind: 'uninstall_plugin', id: toDelete.id } });
                setDeleting(null);
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
            label={t('plugins.title')}
            items={menuItems}
            onClose={() => {
              setMenu(null);
            }}
          />
        )}
      </NavSection>
    </div>
  );
}
