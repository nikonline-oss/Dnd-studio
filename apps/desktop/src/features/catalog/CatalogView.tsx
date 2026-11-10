import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect } from '@/shared/game/useGame';
import { useRemote } from '@/shared/game/useRemote';
import { ipc, type CatalogEntryView } from '@/shared/ipc/client';
import { useSettings } from '@/shared/settings/settings';
import { ErrorText } from '@/shared/ui/ErrorText';
import { NavSection } from '@/shared/ui/NavSection';
import { IconButton } from '@/shared/ui/IconButton';
import './catalog.css';

/**
 * The plugin catalog (ADR-0006, ADR-0060, roadmap R3.4): the signed static index, its packages and
 * the trust they carry. Installs and updates go through the same verified path as the file dialog;
 * a revoked or incompatible version is shown but cannot be installed.
 */
export function CatalogView() {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const remote = useRemote();
  const settings = useSettings();
  const update = useSettings((state) => state.update);
  const client = useQueryClient();
  const [query, setQuery] = useState('');
  const installed = useSnapshotSelect((s) => s.plugins) ?? [];
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const canManage =
    isGm &&
    (remote === null ||
      remote.status.state === 'online' ||
      (remote.hosted && remote.status.state === 'reconnecting'));
  const configured = settings.catalogUrl.trim() !== '' && settings.catalogKey.trim() !== '';

  const catalog = useQuery({
    queryKey: ['catalog', settings.catalogUrl, settings.catalogKey],
    queryFn: () => ipc.catalogFetch(settings.catalogUrl, settings.catalogKey),
    enabled: configured,
    retry: false,
    staleTime: 5 * 60 * 1000,
  });
  const install = useMutation({
    mutationFn: (entry: CatalogEntryView) =>
      ipc.catalogInstall(settings.catalogUrl, settings.catalogKey, entry.id, entry.version),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ['catalog'] });
    },
  });

  const rows = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return (catalog.data?.plugins ?? [])
      .filter(
        (entry) =>
          needle === '' ||
          entry.id.toLowerCase().includes(needle) ||
          entry.license.toLowerCase().includes(needle),
      )
      .sort((a, b) => a.id.localeCompare(b.id) || a.version.localeCompare(b.version));
  }, [catalog.data, query]);

  if (!isGm) {
    return (
      <NavSection icon="download" title={t('catalog.title')} count={0}>
        <p className="nav-empty">{t('catalog.gmOnly')}</p>
      </NavSection>
    );
  }

  return (
    <NavSection
      icon="download"
      title={t('catalog.title')}
      count={rows.length}
      actions={
        <IconButton
          icon="refresh"
          label={t('context.refresh')}
          disabled={!configured || catalog.isFetching}
          onClick={() => {
            void catalog.refetch();
          }}
        />
      }
    >
      {!configured && (
        <section className="catalog-setup" aria-label={t('catalog.setupTitle')}>
          <p>{t('catalog.setupBody')}</p>
          <label>
            {t('catalog.address')}
            <input
              value={settings.catalogUrl}
              placeholder="https://example.org/plugins"
              onChange={(e) => {
                update({ catalogUrl: e.target.value.trim() });
              }}
            />
          </label>
          <label>
            {t('catalog.publicKey')}
            <input
              value={settings.catalogKey}
              placeholder="64 hex characters"
              onChange={(e) => {
                update({ catalogKey: e.target.value.trim().toLowerCase() });
              }}
            />
          </label>
        </section>
      )}
      {configured && (
        <input
          type="search"
          className="catalog-search"
          aria-label={t('catalog.search')}
          placeholder={t('catalog.search')}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
          }}
        />
      )}
      {catalog.error && <ErrorText error={catalog.error} />}
      {install.error && <ErrorText error={install.error} />}
      {configured && catalog.isSuccess && rows.length === 0 && (
        <p className="nav-empty">{t('catalog.empty')}</p>
      )}
      <ul className="catalog-list" aria-label={t('catalog.title')}>
        {rows.map((entry) => {
          const current = installed.find((plugin) => plugin.id === entry.id)?.version;
          return (
            <li key={`${entry.id}@${entry.version}`} className="catalog-row">
              <div className="catalog-name">
                <strong>{entry.id}</strong>
                <span className="muted">
                  {entry.version} · {t(`plugins.kinds.${entry.kind}`)} · {entry.license}
                </span>
                <span className="plugin-badges">
                  <span className="badge" data-trust="signed">
                    {t('catalog.signed', { key: catalog.data?.key_id })}
                  </span>
                  {entry.revoked && <span className="badge">{t('catalog.revoked')}</span>}
                  {!entry.compatible && <span className="badge">{t('catalog.incompatible')}</span>}
                  {current && (
                    <span className="badge">{t('catalog.installed', { version: current })}</span>
                  )}
                </span>
                {entry.dependencies.length > 0 && (
                  <span className="muted">
                    {t('catalog.dependsOn', { list: entry.dependencies.join(', ') })}
                  </span>
                )}
              </div>
              {canManage && entry.can_install && (
                <button
                  type="button"
                  disabled={install.isPending}
                  onClick={() => {
                    install.mutate(entry);
                  }}
                >
                  {current ? t('catalog.update') : t('plugins.install')}
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </NavSection>
  );
}
