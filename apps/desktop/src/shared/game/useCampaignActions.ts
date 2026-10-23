import { useMutation, useQueryClient } from '@tanstack/react-query';
import { queryKeys } from './useGame';
import { remoteKey } from './useRemote';
import { ipc, type CampaignInfo } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

/** Open / create / import / export / close the campaign and keep the query cache in sync. */
export function useCampaignActions() {
  const queryClient = useQueryClient();

  const onOpened = (info: CampaignInfo | null) => {
    if (!info) return; // dialog cancelled
    useSelection.getState().reset();
    queryClient.setQueryData(queryKeys.campaign, info);
    void queryClient.invalidateQueries({ queryKey: queryKeys.snapshot });
    void queryClient.invalidateQueries({ queryKey: remoteKey });
  };

  const create = useMutation({ mutationFn: ipc.campaignCreate, onSuccess: onOpened });
  const open = useMutation({ mutationFn: ipc.campaignOpen, onSuccess: onOpened });
  const importArchive = useMutation({ mutationFn: ipc.campaignImport, onSuccess: onOpened });
  const exportArchive = useMutation({ mutationFn: ipc.campaignExport });
  const close = useMutation({
    mutationFn: ipc.campaignClose,
    onSuccess: () => {
      useSelection.getState().reset();
      queryClient.setQueryData(queryKeys.campaign, null);
      queryClient.setQueryData(remoteKey, null);
      queryClient.removeQueries({ queryKey: queryKeys.snapshot });
    },
  });

  return { create, open, close, importArchive, exportArchive, onOpened };
}
