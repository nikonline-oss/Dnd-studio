import { AppShell } from './AppShell';
import { UpdateBanner } from './UpdateBanner';
import { CampaignScreen } from '@/features/campaign/CampaignScreen';
import { useLanguageSync } from '@/shared/i18n/useLanguageSync';
import { useCampaign, useGameEventsSync } from '@/shared/game/useGame';
import { useThemeSync } from '@/shared/theme/useThemeSync';
import { ErrorBoundary } from '@/shared/ui/ErrorBoundary';

export function App() {
  useThemeSync();
  useLanguageSync();
  useGameEventsSync();
  const { data: campaign, isPending } = useCampaign();

  if (isPending) return null;
  return (
    <ErrorBoundary name="DndStudio">
      <div className="app-root">
        <UpdateBanner />
        {campaign ? <AppShell campaign={campaign} /> : <CampaignScreen />}
      </div>
    </ErrorBoundary>
  );
}
