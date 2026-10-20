import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { CampaignScreen } from '@/features/campaign/CampaignScreen';
import { MapList } from '@/features/map/MapList';
import { TokenList } from '@/features/tokens/TokenList';
import { queryKeys, useCampaign, useGameEventsSync } from '@/shared/game/useGame';
import { ipc } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

export function App() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: campaign, isPending } = useCampaign();
  useGameEventsSync();

  const close = useMutation({
    mutationFn: ipc.campaignClose,
    onSuccess: () => {
      useSelection.getState().selectMap(null);
      queryClient.setQueryData(queryKeys.campaign, null);
      queryClient.removeQueries({ queryKey: queryKeys.snapshot });
    },
  });

  if (isPending) return null;
  if (!campaign) return <CampaignScreen />;

  return (
    <div className="workspace">
      <header className="topbar">
        <strong>{campaign.name}</strong>
        <button
          type="button"
          onClick={() => {
            close.mutate();
          }}
        >
          {t('campaign.close')}
        </button>
      </header>
      <div className="columns">
        <MapList />
        <TokenList />
      </div>
    </div>
  );
}
