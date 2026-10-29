import { useMutation } from '@tanstack/react-query';
import { lazy, Suspense, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Group, Panel, Separator, useDefaultLayout, usePanelRef } from 'react-resizable-panels';
import { ActivityBar } from './ActivityBar';
import { CenterTabs } from './CenterTabs';
import { MenuBar } from './MenuBar';
import { StatusBar } from './StatusBar';
import { BoardList } from '@/features/boards/BoardList';
import { BoardView } from '@/features/boards/BoardView';
import { CompendiumList } from '@/features/compendium/CompendiumList';
import { CompendiumView } from '@/features/compendium/CompendiumView';
import { LibraryPanel } from './LibraryPanel';
import { OfflineEdits } from '@/features/connect/OfflineEdits';
import { TriggerEditor } from '@/features/triggers/TriggerEditor';
import { TriggerList } from '@/features/triggers/TriggerList';
import { CharacterEditor } from '@/features/characters/CharacterEditor';
import { CharacterList } from '@/features/characters/CharacterList';
import { ChatPanel } from '@/features/chat/ChatPanel';
import { JournalEditor } from '@/features/journal/JournalEditor';
import { PluginsView } from '@/features/plugins/PluginsView';
import { InitiativePanel } from '@/features/initiative/InitiativePanel';
import { LogPanel } from '@/features/logs/LogPanel';
import { MapInspector } from '@/features/map/MapInspector';
import { TokenInspector } from '@/features/tokens/TokenInspector';
import { TokenList } from '@/features/tokens/TokenList';

import { useSnapshotSelect } from '@/shared/game/useGame';
import { errorCode, ipc, type CampaignInfo } from '@/shared/ipc/client';
import { useSettings, type SideView } from '@/shared/settings/settings';
import { useSelection } from '@/shared/state/selection';
import { ErrorBoundary } from '@/shared/ui/ErrorBoundary';
import { Icon } from '@/shared/ui/Icon';
import { useToast } from '@/shared/ui/Toast';

const BOTTOM_TAB_ICONS = { chat: 'send', initiative: 'dice', logs: 'book' } as const;

// Pixi is large; it loads with the first map view, not with the app shell.
const MapView = lazy(() => import('@/features/map/MapView').then((m) => ({ default: m.MapView })));

/** Side panel views per preset; players get no map list (§5: the map is chosen for them). */
const GM_VIEWS: SideView[] = [
  'boards',
  'characters',
  'library',
  'plugins',
  'triggers',
  'compendia',
];
const PLAYER_VIEWS: SideView[] = ['characters', 'library', 'boards', 'compendia'];
const VIEW_ICONS = {
  characters: 'users',
  library: 'book',
  plugins: 'puzzle',
  boards: 'board',
  triggers: 'bolt',
  compendia: 'book',
} as const;

/**
 * IDE layout (CLAUDE.md §11): menu + breadcrumbs, Activity Bar with one side view at a time (as in
 * VS Code), center tabs, right inspector, bottom panel, status bar. The Player preset (§5) keeps
 * the map, chat and the character sheet; its side panel starts collapsed.
 */
export function AppShell({ campaign }: { campaign: CampaignInfo }) {
  const { t } = useTranslation();
  const preset = useSettings((s) => (campaign.role === 'player' ? 'player' : s.layoutPreset));
  const mapId = useSelection((s) => s.mapId);
  const inspect = useSelection((s) => s.inspect);
  const center = useSelection((s) => s.center);
  const journalId = useSelection((s) => s.journalId);
  const active = useSelection((s) => s.active);
  const sheetId = active?.kind === 'character' ? active.id : null;
  const sheetName = useSnapshotSelect((s) => s.characters.find((c) => c.id === sheetId)?.name);
  const boardId = active?.kind === 'board' ? active.id : null;
  const boardName = useSnapshotSelect((s) => s.boards.find((b) => b.id === boardId)?.name);
  const triggerId = active?.kind === 'trigger' ? active.id : null;
  const compendiumKey = active?.kind === 'compendium' ? active.id : null;
  const triggerName = useSnapshotSelect(
    (s) => s.triggers.find((tr) => tr.id === triggerId)?.definition.name,
  );
  const journalTitle = useSnapshotSelect((s) => s.journal.find((e) => e.id === journalId)?.title);
  const mapName = useSnapshotSelect((s) => s.maps.find((m) => m.id === mapId)?.name);
  // Breadcrumbs follow the tab in front.
  const crumbs = {
    journal: journalTitle,
    character: sheetName,
    board: boardName,
    trigger: triggerName,
    compendium: t('compendia.title'),
    map: mapName,
  };
  const crumb = crumbs[center];
  const isGm = preset === 'gm';
  // Logs are a GM/maintenance tool; the player preset keeps only chat (§5).
  const bottomTabs = isGm
    ? (['chat', 'initiative', 'logs'] as const)
    : (['chat', 'initiative'] as const);
  const [bottomTab, setBottomTab] = useState<'chat' | 'initiative' | 'logs'>('chat');

  // Activity Bar: which view the side panel shows, and whether the panel is collapsed.
  const views = isGm ? GM_VIEWS : PLAYER_VIEWS;
  const storedView = useSettings((s) => s.sideView);
  const updateSettings = useSettings((s) => s.update);
  const sideView = views.includes(storedView) ? storedView : (views[0] ?? 'boards');
  const sidePanel = usePanelRef();
  const [sideCollapsed, setSideCollapsed] = useState(!isGm);
  const pendingApproval = useSnapshotSelect(
    (s) => s.characters.filter((c) => c.status === 'pending_approval').length,
  );
  const selectView = (view: SideView) => {
    const panel = sidePanel.current;
    if (view === sideView && !sideCollapsed) {
      panel?.collapse();
      return;
    }
    updateSettings({ sideView: view });
    if (sideCollapsed) panel?.expand();
  };
  const toggleSide = () => {
    const panel = sidePanel.current;
    if (!panel) return;
    if (panel.isCollapsed()) panel.expand();
    else panel.collapse();
  };
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
      // Ctrl+B: collapse or expand the side panel; Alt+1..4: its views (as in VS Code).
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 'b') {
        e.preventDefault();
        toggleSide();
      }
      const n = Number(e.key);
      if (e.altKey && !e.ctrlKey && !e.metaKey && n >= 1) {
        const view = views[n - 1];
        if (view) {
          e.preventDefault();
          selectView(view);
        }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
    };
  });

  const horizontal = useDefaultLayout({
    id: `dndstudio.layout.${preset}.h`,
    storage: localStorage,
  });
  const vertical = useDefaultLayout({ id: `dndstudio.layout.${preset}.v`, storage: localStorage });

  return (
    <div className="app-shell">
      <MenuBar breadcrumbs={[campaign.name, ...(crumb ? [crumb] : [])]} />
      <div className="workbench-row">
        <ActivityBar
          views={views.map((id) => ({
            id,
            icon: VIEW_ICONS[id],
            ...(id === 'characters' && isGm ? { badge: pendingApproval ?? 0 } : {}),
          }))}
          active={sideCollapsed ? null : sideView}
          onSelect={selectView}
        />
        <Group
          orientation="horizontal"
          className="workbench"
          defaultLayout={horizontal.defaultLayout}
          onLayoutChanged={horizontal.onLayoutChanged}
        >
          <Panel
            id="left"
            panelRef={sidePanel}
            defaultSize={isGm ? '20' : '0'}
            minSize="12"
            collapsible
            className="side-panel"
            onResize={(size) => {
              setSideCollapsed(size.inPixels === 0);
            }}
          >
            {sideView === 'characters' ? (
              <ErrorBoundary name={t('characters.title')}>
                <CharacterList />
              </ErrorBoundary>
            ) : sideView === 'library' ? (
              <ErrorBoundary name={t('library.title')}>
                <LibraryPanel isGm={isGm} />
              </ErrorBoundary>
            ) : sideView === 'triggers' ? (
              <ErrorBoundary name={t('triggers.title')}>
                <TriggerList />
              </ErrorBoundary>
            ) : sideView === 'boards' ? (
              <ErrorBoundary name={t('boards.title')}>
                <BoardList />
              </ErrorBoundary>
            ) : sideView === 'plugins' ? (
              <ErrorBoundary name={t('plugins.title')}>
                <PluginsView />
              </ErrorBoundary>
            ) : (
              <ErrorBoundary name={t('compendia.title')}>
                <CompendiumList />
              </ErrorBoundary>
            )}
          </Panel>
          <Separator className="separator" />
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
                  ) : center === 'compendium' && compendiumKey !== null ? (
                    <ErrorBoundary name={t('compendia.title')}>
                      <CompendiumView key={compendiumKey} compendiumKey={compendiumKey} />
                    </ErrorBoundary>
                  ) : center === 'trigger' && triggerId !== null ? (
                    <ErrorBoundary name={t('triggers.title')}>
                      <TriggerEditor key={triggerId} triggerId={triggerId} />
                    </ErrorBoundary>
                  ) : center === 'board' && boardId !== null ? (
                    <ErrorBoundary name={t('boards.title')}>
                      <BoardView key={boardId} boardId={boardId} />
                    </ErrorBoundary>
                  ) : center === 'character' && sheetId !== null ? (
                    <ErrorBoundary name={t('characters.title')}>
                      <CharacterEditor key={sheetId} characterId={sheetId} />
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
      </div>
      <StatusBar campaign={campaign} />
      <OfflineEdits isGm={campaign.role === 'gm'} campaignPath={campaign.path} />
    </div>
  );
}
