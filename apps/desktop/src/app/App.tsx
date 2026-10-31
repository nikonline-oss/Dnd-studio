import { CampaignThemeSync } from '@/shared/theme/CampaignThemeSync';
import { AppShell } from './AppShell';
import { SessionDialogs } from './SessionDialogs';
import { UpdateBanner } from './UpdateBanner';
import { CampaignScreen } from '@/features/campaign/CampaignScreen';
import { useLanguageSync } from '@/shared/i18n/useLanguageSync';
import { useCampaign, useGameEventsSync } from '@/shared/game/useGame';
import { useRemoteSync } from '@/shared/game/useRemote';
import { useThemeSync } from '@/shared/theme/useThemeSync';
import { ErrorBoundary } from '@/shared/ui/ErrorBoundary';
import { Toaster } from '@/shared/ui/Toast';

export function App() {
  useThemeSync();
  useLanguageSync();
  useGameEventsSync();
  useRemoteSync();
  const { data: campaign, isPending } = useCampaign();

  if (isPending) return null;
  return (
    <ErrorBoundary name="DndStudio">
      <div className="app-root">
        <UpdateBanner />
        {campaign ? (
          <>
            <CampaignThemeSync key={campaign.path} />
            <AppShell campaign={campaign} />
          </>
        ) : (
          <CampaignScreen />
        )}
        <SessionDialogs />
        <Toaster />
      </div>
    </ErrorBoundary>
  );
}
