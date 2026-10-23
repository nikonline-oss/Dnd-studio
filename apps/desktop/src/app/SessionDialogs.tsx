import { ConnectDialog } from '@/features/connect/ConnectDialog';
import { HostingDialog } from '@/features/hosting/HostingDialog';
import { useDialogs } from '@/shared/state/dialogs';

/** The app-wide dialogs of session hosting and joining (opened from the menu or start screen). */
export function SessionDialogs() {
  const open = useDialogs((s) => s.open);
  if (open === 'hosting') return <HostingDialog />;
  if (open === 'connect') return <ConnectDialog />;
  return null;
}
