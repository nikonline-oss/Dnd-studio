import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { SafeMode } from './SafeMode';
import type { CampaignInfo } from '@/shared/ipc/client';

const campaign: CampaignInfo = {
  name: 'Cave',
  path: '/campaigns/cave',
  user_id: '0190a000-0000-7000-8000-000000000001',
  role: 'gm',
};

function renderBanner() {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <SafeMode campaign={campaign} />
    </QueryClientProvider>,
  );
}

describe('Safe Mode', () => {
  it('says that rules are off while the campaign is open safely', async () => {
    mockIPC(
      (cmd) => {
        if (cmd === 'campaign_safe_mode') return true;
        if (cmd === 'app_info') return { version: '0.0.1', safe_mode_suggested: false };
        throw new Error(`unexpected ${cmd}`);
      },
      { shouldMockEvents: true },
    );
    renderBanner();
    await waitFor(() => {
      expect(screen.getByText('Safe Mode')).toBeInTheDocument();
    });
    expect(screen.getByText(/open without its rules/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Open normally' })).toBeInTheDocument();
  });

  it('suggests the mode after repeated unclean shutdowns and can be dismissed', async () => {
    mockIPC(
      (cmd) => {
        if (cmd === 'campaign_safe_mode') return false;
        if (cmd === 'app_info') return { version: '0.0.1', safe_mode_suggested: true };
        throw new Error(`unexpected ${cmd}`);
      },
      { shouldMockEvents: true },
    );
    renderBanner();
    await waitFor(() => {
      expect(
        screen.getByText('The app failed to shut down several times in a row'),
      ).toBeInTheDocument();
    });
    await userEvent.click(screen.getByRole('button', { name: 'Not now' }));
    expect(
      screen.queryByText('The app failed to shut down several times in a row'),
    ).not.toBeInTheDocument();
  });

  it('stays out of the way when nothing is wrong', async () => {
    mockIPC(
      (cmd) => {
        if (cmd === 'campaign_safe_mode') return false;
        if (cmd === 'app_info') return { version: '0.0.1', safe_mode_suggested: false };
        throw new Error(`unexpected ${cmd}`);
      },
      { shouldMockEvents: true },
    );
    const { container } = renderBanner();
    await waitFor(() => {
      expect(screen.queryByRole('status')).not.toBeInTheDocument();
    });
    expect(container.querySelector('.safe-mode')).toBeNull();
  });
});
