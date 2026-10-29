import { useMutation, useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { useRemote } from '@/shared/game/useRemote';
import { labelText } from '@/shared/i18n/label';
import { ipc, type PluginDto } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';
import { ConfirmButton, IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';

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

  const row = (plugin: PluginDto) => {
    const isSystem = plugin.id === system;
    return (
      <li key={plugin.id} className="plugin-row" aria-current={isSystem}>
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
                disabled={submit.isPending}
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
                disabled={submit.isPending}
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
    <NavSection
      icon="puzzle"
      title={t('plugins.title')}
      count={plugins.length}
      actions={
        canManage && (
          <IconButton
            icon="filePlus"
            label={t('plugins.installFile')}
            disabled={installFile.isPending}
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
              <li key={b.id} className="plugin-row">
                <div className="plugin-name">
                  <strong>{labelText(b.name, lang)}</strong>
                  <span className="muted">
                    {b.version} · {t(`plugins.kinds.${b.kind}`)}
                  </span>
                </div>
                <button
                  type="button"
                  disabled={installBundled.isPending}
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
    </NavSection>
  );
}
