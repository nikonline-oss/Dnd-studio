import { useQuery } from '@tanstack/react-query';
import { useState, type MouseEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, usePluginRevision } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import { commands, unwrap } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { NavSection } from '@/shared/ui/NavSection';
import { ContextMenu } from '@/shared/ui/ContextMenu';
import type { MenuItem } from '@/shared/ui/Menu';

/** Navigator: the compendia of the campaign's plugins (CLAUDE.md §12, ADR-0025). */
export function CompendiumList() {
  const { t, i18n } = useTranslation();
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const { data: campaign } = useCampaign();
  const revision = usePluginRevision();
  const active = useSelection((s) => s.active);
  const openCompendium = useSelection((s) => s.openCompendium);
  const compendia = useQuery({
    queryKey: ['compendia', campaign?.path, revision],
    queryFn: () => unwrap(commands.compendiumList()),
    enabled: campaign !== undefined && campaign !== null && revision !== undefined,
  });
  const list = compendia.data ?? [];
  const [menu, setMenu] = useState<{ x: number; y: number; key: string | null } | null>(null);
  const openMenu = (e: MouseEvent, key: string | null) => {
    if ((e.target as HTMLElement).closest('[role="menu"]')) return;
    e.preventDefault();
    e.stopPropagation();
    setMenu({ x: e.clientX, y: e.clientY, key });
  };
  const items: (MenuItem | 'separator')[] = [];
  const selectedKey = menu?.key;
  if (selectedKey && list.some((c) => `${c.plugin_id}/${c.id}` === selectedKey)) {
    items.push({
      label: t('context.open', { defaultValue: 'Open' }),
      onSelect: () => {
        openCompendium(selectedKey);
      },
    });
  }
  items.push({
    label: t('context.refresh', { defaultValue: 'Refresh' }),
    disabled: compendia.isFetching || !campaign || revision === undefined,
    onSelect: () => {
      void compendia.refetch();
    },
  });

  return (
    <div
      style={{ display: 'contents' }}
      onContextMenu={(e) => {
        openMenu(e, null);
      }}
    >
      <NavSection icon="book" title={t('compendia.title')} count={list.length}>
        {compendia.isSuccess && list.length === 0 && (
          <p className="nav-empty">{t('compendia.empty')}</p>
        )}
        <ul className="nav-list" aria-label={t('compendia.title')}>
          {list.map((c) => {
            const key = `${c.plugin_id}/${c.id}`;
            return (
              <li
                key={key}
                className="nav-row"
                aria-current={active?.kind === 'compendium' && active.id === key}
                onClick={() => {
                  openCompendium(key);
                }}
                onContextMenu={(e) => {
                  openMenu(e, key);
                }}
              >
                <Icon name="book" className="row-icon" />
                <span className="nav-row-name">{labelText(c.name, lang)}</span>
                <span className="muted">{c.count}</span>
              </li>
            );
          })}
        </ul>
        <ErrorText error={compendia.error} />
        {menu && (
          <ContextMenu
            x={menu.x}
            y={menu.y}
            label={t('compendia.title')}
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
