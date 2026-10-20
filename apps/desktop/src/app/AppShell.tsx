import { useMutation } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Group, Panel, Separator, useDefaultLayout } from 'react-resizable-panels';
import { MenuBar } from './MenuBar';
import { StatusBar } from './StatusBar';
import { LogPanel } from '@/features/logs/LogPanel';
import { MapInspector } from '@/features/map/MapInspector';
import { MapList } from '@/features/map/MapList';
import { TokenList } from '@/features/tokens/TokenList';
import { useSnapshot } from '@/shared/game/useGame';
import { ipc, type CampaignInfo } from '@/shared/ipc/client';
import { useSettings } from '@/shared/settings/settings';
import { useSelection } from '@/shared/state/selection';
import { ErrorBoundary } from '@/shared/ui/ErrorBoundary';

/**
 * IDE layout (CLAUDE.md §11): menu + breadcrumbs, left navigator, center tabs, right inspector,
 * bottom panel, status bar. The Player preset (§5) keeps only the center and the bottom panel.
 */
export function AppShell({ campaign }: { campaign: CampaignInfo }) {
  const { t } = useTranslation();
  const preset = useSettings((s) => (campaign.role === 'player' ? 'player' : s.layoutPreset));
  const { data: snapshot } = useSnapshot();
  const mapId = useSelection((s) => s.mapId);
  const mapName = snapshot?.maps.find((m) => m.id === mapId)?.name;
  const isGm = preset === 'gm';
  // Logs are a GM/maintenance tool; the player preset keeps only chat (§5).
  const bottomTabs = isGm ? (['chat', 'logs'] as const) : (['chat'] as const);
  const [bottomTab, setBottomTab] = useState<'chat' | 'logs'>('chat');
  const { mutate: saveNow } = useMutation({ mutationFn: ipc.campaignSave });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
        e.preventDefault();
        saveNow();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
    };
  }, [saveNow]);

  const horizontal = useDefaultLayout({
    id: `dndstudio.layout.${preset}.h`,
    storage: localStorage,
  });
  const vertical = useDefaultLayout({ id: `dndstudio.layout.${preset}.v`, storage: localStorage });

  return (
    <div className="app-shell">
      <MenuBar breadcrumbs={[campaign.name, ...(mapName ? [mapName] : [])]} />
      <Group
        orientation="horizontal"
        className="workbench"
        defaultLayout={horizontal.defaultLayout}
        onLayoutChanged={horizontal.onLayoutChanged}
      >
        {isGm && (
          <>
            <Panel id="left" defaultSize="20" minSize="12" collapsible className="side-panel">
              <ErrorBoundary name={t('maps.title')}>
                <MapList />
              </ErrorBoundary>
            </Panel>
            <Separator className="separator" />
          </>
        )}
        <Panel id="center" minSize="30">
          <Group
            orientation="vertical"
            defaultLayout={vertical.defaultLayout}
            onLayoutChanged={vertical.onLayoutChanged}
          >
            <Panel id="main" minSize="20" className="center-panel">
              <div role="tablist" className="tabs">
                <button type="button" role="tab" aria-selected="true">
                  {mapName ?? t('panels.map')}
                </button>
              </div>
              <div role="tabpanel" className="tab-body">
                <ErrorBoundary name={t('tokens.title')}>
                  <TokenList />
                </ErrorBoundary>
              </div>
            </Panel>
            <Separator className="separator" />
            <Panel id="bottom" defaultSize="25" minSize="10" collapsible className="bottom-panel">
              <div role="tablist" className="tabs">
                {bottomTabs.map((tab) => (
                  <button
                    key={tab}
                    type="button"
                    role="tab"
                    aria-selected={bottomTab === tab}
                    onClick={() => {
                      setBottomTab(tab);
                    }}
                  >
                    {t(`panels.${tab}`)}
                  </button>
                ))}
              </div>
              {bottomTab === 'logs' ? (
                <ErrorBoundary name={t('panels.logs')}>
                  <LogPanel />
                </ErrorBoundary>
              ) : (
                <p className="muted">{t('panels.chatSoon')}</p>
              )}
            </Panel>
          </Group>
        </Panel>
        {isGm && (
          <>
            <Separator className="separator" />
            <Panel id="right" defaultSize="22" minSize="12" collapsible className="side-panel">
              <h2>{t('inspector.title')}</h2>
              <ErrorBoundary name={t('inspector.title')}>
                <MapInspector />
              </ErrorBoundary>
            </Panel>
          </>
        )}
      </Group>
      <StatusBar campaign={campaign} />
    </div>
  );
}
