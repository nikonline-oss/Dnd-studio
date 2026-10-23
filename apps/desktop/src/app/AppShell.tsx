import { useMutation } from '@tanstack/react-query';
import { lazy, Suspense, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Group, Panel, Separator, useDefaultLayout } from 'react-resizable-panels';
import { CenterTabs } from './CenterTabs';
import { MenuBar } from './MenuBar';
import { StatusBar } from './StatusBar';
import { CharacterEditor } from '@/features/characters/CharacterEditor';
import { CharacterList } from '@/features/characters/CharacterList';
import { ChatPanel } from '@/features/chat/ChatPanel';
import { JournalEditor } from '@/features/journal/JournalEditor';
import { JournalTree } from '@/features/journal/JournalTree';
import { InitiativePanel } from '@/features/initiative/InitiativePanel';
import { LogPanel } from '@/features/logs/LogPanel';
import { MapInspector } from '@/features/map/MapInspector';
import { MapList } from '@/features/map/MapList';
import { TokenInspector } from '@/features/tokens/TokenInspector';
import { TokenList } from '@/features/tokens/TokenList';

const BOTTOM_TAB_ICONS = { chat: 'send', initiative: 'dice', logs: 'book' } as const;

// Pixi is large; it loads with the first map view, not with the app shell.
const MapView = lazy(() => import('@/features/map/MapView').then((m) => ({ default: m.MapView })));
import { useSnapshot } from '@/shared/game/useGame';
import { errorCode, ipc, type CampaignInfo } from '@/shared/ipc/client';
import { useSettings } from '@/shared/settings/settings';
import { useSelection } from '@/shared/state/selection';
import { ErrorBoundary } from '@/shared/ui/ErrorBoundary';
import { Icon } from '@/shared/ui/Icon';
import { useToast } from '@/shared/ui/Toast';

/**
 * IDE layout (CLAUDE.md §11): menu + breadcrumbs, left navigator, center tabs, right inspector,
 * bottom panel, status bar. The Player preset (§5) keeps only the center and the bottom panel.
 */
export function AppShell({ campaign }: { campaign: CampaignInfo }) {
  const { t } = useTranslation();
  const preset = useSettings((s) => (campaign.role === 'player' ? 'player' : s.layoutPreset));
  const { data: snapshot } = useSnapshot();
  const mapId = useSelection((s) => s.mapId);
  const inspect = useSelection((s) => s.inspect);
  const center = useSelection((s) => s.center);
  const journalId = useSelection((s) => s.journalId);
  const journalTitle = snapshot?.journal.find((e) => e.id === journalId)?.title;
  const mapName = snapshot?.maps.find((m) => m.id === mapId)?.name;
  // Breadcrumbs follow the tab in front.
  const crumb = center === 'journal' ? journalTitle : mapName;
  const isGm = preset === 'gm';
  // Logs are a GM/maintenance tool; the player preset keeps only chat (§5).
  const bottomTabs = isGm
    ? (['chat', 'initiative', 'logs'] as const)
    : (['chat', 'initiative'] as const);
  const [bottomTab, setBottomTab] = useState<'chat' | 'initiative' | 'logs'>('chat');
  const showToast = useToast((s) => s.show);
  // Ctrl+S: force a WAL checkpoint. Data is already saved on every action (ADR-0014); this only
  // folds the WAL into campaign.sqlite and reassures the user.
  const { mutate: checkpoint } = useMutation({
    mutationFn: ipc.campaignCheckpoint,
    onSuccess: () => {
      showToast(t('toast.saved'));
    },
    onError: (error) => {
      showToast(t(`errors.${errorCode(error)}`, { defaultValue: t('errors.unknown') }), 'error');
    },
  });
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 's') {
        e.preventDefault();
        checkpoint();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
    };
  }, [checkpoint]);

  const horizontal = useDefaultLayout({
    id: `dndstudio.layout.${preset}.h`,
    storage: localStorage,
  });
  const vertical = useDefaultLayout({ id: `dndstudio.layout.${preset}.v`, storage: localStorage });

  return (
    <div className="app-shell">
      <MenuBar breadcrumbs={[campaign.name, ...(crumb ? [crumb] : [])]} />
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
              <ErrorBoundary name={t('characters.title')}>
                <CharacterList />
              </ErrorBoundary>
              <ErrorBoundary name={t('journal.title')}>
                <JournalTree />
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
              <CenterTabs />
              <div role="tabpanel" className="tab-body">
                {center === 'journal' ? (
                  <ErrorBoundary name={t('journal.title')}>
                    <JournalEditor />
                  </ErrorBoundary>
                ) : (
                  <ErrorBoundary name={t('panels.map')}>
                    <Suspense fallback={<p className="muted">{t('mapView.loading')}</p>}>
                      <MapView />
                    </Suspense>
                  </ErrorBoundary>
                )}
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
                    <Icon name={BOTTOM_TAB_ICONS[tab]} />
                    {t(`panels.${tab}`)}
                  </button>
                ))}
              </div>
              {bottomTab === 'logs' ? (
                <ErrorBoundary name={t('panels.logs')}>
                  <LogPanel />
                </ErrorBoundary>
              ) : bottomTab === 'initiative' ? (
                <ErrorBoundary name={t('panels.initiative')}>
                  <InitiativePanel />
                </ErrorBoundary>
              ) : (
                <ErrorBoundary name={t('panels.chat')}>
                  <ChatPanel />
                </ErrorBoundary>
              )}
            </Panel>
          </Group>
        </Panel>
        <Separator className="separator" />
        <Panel id="right" defaultSize="22" minSize="12" collapsible className="side-panel">
          {isGm ? (
            <>
              <h2 className="panel-title">{t('inspector.title')}</h2>
              <ErrorBoundary name={t('inspector.title')}>
                {inspect === 'character' ? (
                  <CharacterEditor />
                ) : inspect === 'token' ? (
                  <TokenInspector />
                ) : (
                  <>
                    <MapInspector />
                    <TokenList />
                  </>
                )}
              </ErrorBoundary>
            </>
          ) : (
            // Player preset (§5): the character sheet next to the map and chat.
            <ErrorBoundary name={t('characters.title')}>
              <CharacterList />
              <CharacterEditor />
            </ErrorBoundary>
          )}
        </Panel>
      </Group>
      <StatusBar campaign={campaign} />
    </div>
  );
}
