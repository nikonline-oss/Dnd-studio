import { useActionAim } from '@/shared/state/actionAim';
import { ItemsView } from '@/shared/items/ItemsView';
import { ActionSaves } from '@/shared/game/ActionSaves';
import { useMutation } from '@tanstack/react-query';
import { lazy, Suspense, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Group, Panel, Separator, useDefaultLayout, usePanelRef } from 'react-resizable-panels';
import { ActivityBar } from './ActivityBar';
import { CenterTabs } from './CenterTabs';
import { MediaFolderView } from '@/features/images/MediaFolderView';
import { CompactAudioPlayers } from '@/shared/ui/SessionAudioPlayer';
import { MenuBar } from './MenuBar';
import { StatusBar } from './StatusBar';
import { BoardList } from '@/features/boards/BoardList';
import { PlayerInspector } from './PlayerInspector';
import { BoardView } from '@/features/boards/BoardView';
import { CompendiumList } from '@/features/compendium/CompendiumList';
import { CompendiumView } from '@/features/compendium/CompendiumView';
import { LibraryPanel } from './LibraryPanel';
import { OfflineEdits } from '@/features/connect/OfflineEdits';
import { SafeMode } from '@/features/safety/SafeMode';
import { ActionWindows } from '@/features/plugins/ActionWindows';
import { MaintainedEffects } from '@/features/plugins/MaintainedEffects';
import { PluginReactions } from '@/features/plugins/PluginReactions';
import { RollRequests } from '@/features/triggers/RollRequests';
import { AttentionNotice } from '@/features/notifications/AttentionNotice';
import { ImagePresentation } from '@/features/images/ImagePresentation';
import { TemplateDialog } from '@/features/triggers/TemplateDialog';
import { TriggerEditor } from '@/features/triggers/TriggerEditor';
import { CharacterEditor } from '@/features/characters/CharacterEditor';
import { CharacterList } from '@/features/characters/CharacterList';
import { ChatPanel } from '@/features/chat/ChatPanel';
import { JournalEditor } from '@/features/journal/JournalEditor';
import { GmPanel } from './GmPanel';
import { PluginSettingsView } from '@/features/plugins/PluginSettingsView';
import { PluginsView } from '@/features/plugins/PluginsView';
import { CatalogView } from '@/features/catalog/CatalogView';
import { InitiativePanel } from '@/features/initiative/InitiativePanel';
import { LogPanel } from '@/features/logs/LogPanel';
import { MapInspector } from '@/features/map/MapInspector';
import { TokenInspector } from '@/features/tokens/TokenInspector';
import { TokenList } from '@/features/tokens/TokenList';

import { useSnapshotSelect } from '@/shared/game/useGame';
import { errorCode, ipc, type CampaignInfo } from '@/shared/ipc/client';
import { useSettings, type SideView } from '@/shared/settings/settings';
import { useSelection, type TabRef } from '@/shared/state/selection';
import { ErrorBoundary } from '@/shared/ui/ErrorBoundary';
import { Icon } from '@/shared/ui/Icon';
import { IconButton } from '@/shared/ui/IconButton';
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
  'catalog',
  'compendia',
  'items',
  'skills',
];
const PLAYER_VIEWS: SideView[] = [
  'characters',
  'library',
  'boards',
  'compendia',
  'items',
  'skills',
];
const VIEW_ICONS = {
  items: 'book',
  skills: 'bolt',
  characters: 'users',
  library: 'book',
  plugins: 'puzzle',
  catalog: 'download',
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
  useEffect(() => {
    useActionAim.getState().setAim(null);
    return () => {
      useActionAim.getState().setAim(null);
    };
  }, [campaign.path]);
  const [triggerTemplate, setTriggerTemplate] = useState<{
    boardId: string;
    x: number;
    y: number;
  } | null>(null);
  const preset = useSettings((s) => (campaign.role === 'player' ? 'player' : s.layoutPreset));
  const mapId = useSelection((s) => s.mapId);
  const inspect = useSelection((s) => s.inspect);
  const pendingActions = useSnapshotSelect((s) => s.plugin_runtime?.action_stacks?.length ?? 0);
  const maintainedCount = useSnapshotSelect((s) => s.plugin_runtime?.maintained?.length ?? 0);
  const paneTabs = useSelection((s) => s.paneTabs);
  const activatePane = useSelection((s) => s.activate);
  const center = useSelection((s) => s.center);
  const journalId = useSelection((s) => s.journalId);
  const active = useSelection((s) => s.active);
  const sheetId = active?.kind === 'character' ? active.id : null;
  const sheetName = useSnapshotSelect((s) => s.characters.find((c) => c.id === sheetId)?.name);
  const boardId = active?.kind === 'board' ? active.id : null;
  const boardName = useSnapshotSelect((s) => s.boards.find((b) => b.id === boardId)?.name);
  const triggerId = active?.kind === 'trigger' ? active.id : null;
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
    plugin: t('plugins.title'),
    gm_panel: t('gm.title'),
    items: t('items.title'),
    skills: t('items.skillsTitle'),
    compendium: t('compendia.title'),
    images: t('images.title'),
    audio: t('audio.title'),
    map: mapName,
  };
  const crumb = crumbs[center];
  const isGm = preset === 'gm';
  // Logs are a GM/maintenance tool; the player preset keeps only chat (§5).
  const bottomTabs = isGm
    ? (['chat', 'initiative', 'logs'] as const)
    : (['chat', 'initiative'] as const);
  const [bottomTab, setBottomTab] = useState<'chat' | 'initiative' | 'logs'>('chat');
  const [bottomVisible, setBottomVisible] = useState(true);

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

  const renderPane = (pane: TabRef | null) => {
    const paneKind = pane?.kind ?? 'map';
    const paneId = pane?.id ?? null;
    return (paneKind === 'images' || paneKind === 'audio') && pane ? (
      <ErrorBoundary name={t(`${paneKind}.title`)}>
        <MediaFolderView
          key={`${paneKind}:${pane.id}`}
          library={paneKind}
          folderId={pane.id === 'root' ? null : pane.id}
        />
      </ErrorBoundary>
    ) : paneKind === 'items' ? (
      <ItemsView />
    ) : paneKind === 'skills' ? (
      <ItemsView kind="skill" />
    ) : paneKind === 'gm_panel' ? (
      <ErrorBoundary name={t('gm.title')}>
        <GmPanel />
      </ErrorBoundary>
    ) : paneKind === 'plugin' && pane ? (
      <PluginSettingsView pluginId={pane.id} />
    ) : paneKind === 'journal' ? (
      <ErrorBoundary name={t('journal.title')}>
        <JournalEditor {...(paneId ? { entryId: paneId } : {})} />
      </ErrorBoundary>
    ) : paneKind === 'compendium' && paneId !== null ? (
      <ErrorBoundary name={t('compendia.title')}>
        <CompendiumView key={paneId} compendiumKey={paneId} />
      </ErrorBoundary>
    ) : paneKind === 'trigger' && paneId !== null ? (
      <ErrorBoundary name={t('triggers.title')}>
        <TriggerEditor key={paneId} triggerId={paneId} />
      </ErrorBoundary>
    ) : paneKind === 'board' && paneId !== null ? (
      <ErrorBoundary name={t('boards.title')}>
        <BoardView
          key={paneId}
          boardId={paneId}
          onTriggerTemplate={(position) => {
            setTriggerTemplate({ boardId: paneId, ...position });
          }}
        />
      </ErrorBoundary>
    ) : paneKind === 'character' && paneId !== null ? (
      <ErrorBoundary name={t('characters.title')}>
        <CharacterEditor key={paneId} characterId={paneId} />
      </ErrorBoundary>
    ) : (
      <ErrorBoundary name={t('panels.map')}>
        <Suspense fallback={<p className="muted">{t('mapView.loading')}</p>}>
          <MapView {...(pane?.kind === 'map' ? { mapId: pane.id } : {})} />
        </Suspense>
      </ErrorBoundary>
    );
  };
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
                <LibraryPanel />
              </ErrorBoundary>
            ) : sideView === 'boards' ? (
              <ErrorBoundary name={t('boards.title')}>
                <BoardList />
              </ErrorBoundary>
            ) : sideView === 'items' ? (
              <ItemsView />
            ) : sideView === 'skills' ? (
              <ItemsView kind="skill" />
            ) : sideView === 'plugins' ? (
              <ErrorBoundary name={t('plugins.title')}>
                <PluginsView />
              </ErrorBoundary>
            ) : sideView === 'catalog' ? (
              <ErrorBoundary name={t('catalog.title')}>
                <CatalogView />
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
                  <div className="center-panes">
                    {(paneTabs.length ? paneTabs : active ? [active] : [null]).map((pane) => (
                      <div
                        key={pane ? `${pane.kind}:${pane.id}` : 'default-map'}
                        className="center-pane"
                        onPointerDownCapture={() => {
                          if (pane && (pane.kind !== active?.kind || pane.id !== active.id))
                            activatePane(pane);
                        }}
                      >
                        {renderPane(pane)}
                      </div>
                    ))}
                  </div>
                </div>
              </Panel>
              {bottomVisible && <Separator className="separator" />}
              {bottomVisible && (
                <Panel
                  id="bottom"
                  defaultSize="25"
                  minSize="10"
                  collapsible
                  className="bottom-panel"
                >
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
                    <IconButton
                      icon="chevron"
                      className="panel-toggle panel-toggle-hide"
                      label={t('panels.hideBottom')}
                      onClick={() => {
                        setBottomVisible(false);
                      }}
                    />
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
              )}
            </Group>
          </Panel>
          <Separator className="separator" />
          <Panel id="right" defaultSize="22" minSize="12" collapsible className="side-panel">
            {isGm ? (
              <>
                <h2 className="panel-title">{t('inspector.title')}</h2>
                <button
                  onClick={() => {
                    useSelection.getState().inspectGmPanel();
                  }}
                >
                  {t('gm.title')}
                </button>
                <ErrorBoundary name={t('inspector.title')}>
                  {inspect === 'gm_panel' ? (
                    <GmPanel />
                  ) : inspect === 'character' ? (
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
              // Player preset (§5): the character sheet next to the map and chat, plus the
              // selected token or open map (ADR-0061).
              <ErrorBoundary name={t('characters.title')}>
                <PlayerInspector />
              </ErrorBoundary>
            )}
          </Panel>
        </Group>
      </div>
      <div className="app-footer">
        {!bottomVisible && (
          <IconButton
            icon="chevron"
            className="panel-toggle panel-toggle-show"
            label={t('panels.showBottom')}
            onClick={() => {
              setBottomVisible(true);
            }}
          />
        )}
        <CompactAudioPlayers />
        <div className="plugin-notices">
          {!!pendingActions && <ActionWindows />}
          {!!maintainedCount && <MaintainedEffects floating />}
          <PluginReactions />
          <ActionSaves />
        </div>
        <RollRequests />
        <AttentionNotice key={campaign.path} />
        <StatusBar campaign={campaign} />
      </div>
      {triggerTemplate && (
        <TemplateDialog
          boardId={triggerTemplate.boardId}
          position={triggerTemplate}
          onClose={() => {
            setTriggerTemplate(null);
          }}
        />
      )}
      <SafeMode campaign={campaign} />
      <OfflineEdits isGm={campaign.role === 'gm'} campaignPath={campaign.path} />
      <ErrorBoundary name={t('presentation.title')}>
        <ImagePresentation key={campaign.path} />
      </ErrorBoundary>
    </div>
  );
}
