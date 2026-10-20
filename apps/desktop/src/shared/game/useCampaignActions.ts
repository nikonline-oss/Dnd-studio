import { useMutation, useQueryClient } from '@tanstack/react-query';
import { queryKeys } from './useGame';
import { ipc, type CampaignInfo } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

/** Open / create / close the campaign and keep the query cache in sync. */
export function useCampaignActions() {
  const queryClient = useQueryClient();

  const onOpened = (info: CampaignInfo | null) => {
    if (!info) return; // dialog cancelled
    useSelection.getState().selectMap(null);
    queryClient.setQueryData(queryKeys.campaign, info);
    void queryClient.invalidateQueries({ queryKey: queryKeys.snapshot });
  };

  const create = useMutation({ mutationFn: ipc.campaignCreate, onSuccess: onOpened });
  const open = useMutation({ mutationFn: ipc.campaignOpen, onSuccess: onOpened });
  const close = useMutation({
    mutationFn: ipc.campaignClose,
    onSuccess: () => {
      useSelection.getState().selectMap(null);
      queryClient.setQueryData(queryKeys.campaign, null);
      queryClient.removeQueries({ queryKey: queryKeys.snapshot });
    },
  });

  return { create, open, close };
}
