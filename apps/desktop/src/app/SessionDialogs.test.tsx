import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { StatusBar } from './StatusBar';
import { ConnectDialog } from '@/features/connect/ConnectDialog';
import { HostingDialog } from '@/features/hosting/HostingDialog';
import type {
  CampaignInfo,
  HostedSession,
  IpcError,
  RemoteEntry,
  RemoteInfo,
} from '@/shared/ipc/client';
import { useDialogs } from '@/shared/state/dialogs';

const SESSION = '0190a000-0000-7000-8000-000000000090';
const info: CampaignInfo = {
  name: 'Lost Mine',
  path: `https://dnd.example.org/sessions/${SESSION}`,
  user_id: '0190a000-0000-7000-8000-000000000002',
  role: 'player',
};
const saved: RemoteEntry = {
  server: 'https://dnd.example.org',
  session_id: SESSION,
  session_name: 'Lost Mine',
  hosted: true,
  name: 'Game Master',
  role: 'gm',
  user_id: null,
  invite_url: null,
  access_code: 'K7PQX-2MWRT',
  saved_at_ms: 1,
};

let calls: { cmd: string; args: Record<string, unknown> }[];

function setup(
  ui: React.ReactElement,
  answers: Record<string, (args: Record<string, unknown>) => unknown>,
) {
  calls = [];
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args: payload });
    const answer = answers[cmd];
    if (!answer) throw new Error(`unexpected command ${cmd}`);
    return answer(payload);
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
  return client;
}

beforeEach(() => {
  useDialogs.setState({ open: 'connect' });
});

afterEach(() => {
  useDialogs.setState({ open: null });
});

describe('ConnectDialog', () => {
  it('joins with the invite, the code and a name, then plays there', async () => {
    let wrongCode = true;
    const client = setup(<ConnectDialog />, {
      remote_sessions: () => [],
      remote_current: () => null,
      remote_join: () => {
        if (wrongCode) {
          const error: IpcError = { kind: 'user', code: 'unauthorized', message: 'no' };
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw error;
        }
        return info;
      },
    });
    const join = screen.getByRole('button', { name: 'Join' });
    expect(join).toBeDisabled();
    await userEvent.type(
      screen.getByLabelText('Invite link'),
      `https://dnd.example.org/api/sessions/${SESSION}`,
    );
    await userEvent.type(screen.getByLabelText('Access code'), 'k7pqx2mwrt');
    await userEvent.type(screen.getByLabelText('Your name'), ' Mira ');
    await userEvent.click(join);
    expect(await screen.findByRole('alert')).toHaveTextContent('Wrong access code');
    expect(useDialogs.getState().open).toBe('connect');

    wrongCode = false;
    await userEvent.click(join);
    await waitFor(() => {
      expect(useDialogs.getState().open).toBeNull();
    });
    expect(calls.filter((c) => c.cmd === 'remote_join').at(-1)?.args).toEqual({
      invite: `https://dnd.example.org/api/sessions/${SESSION}`,
      accessCode: 'k7pqx2mwrt',
      name: 'Mira',
    });
    expect(client.getQueryData(['campaign'])).toEqual(info);
  });

  it('reconnects to or forgets a saved session', async () => {
    setup(<ConnectDialog />, {
      remote_sessions: () => [saved],
      remote_current: () => null,
      remote_connect: () => ({ ...info, role: 'gm' }),
      remote_forget: () => null,
    });
    expect(await screen.findByText('Lost Mine')).toBeInTheDocument();
    expect(screen.getByText(/K7PQX-2MWRT/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Forget the session' }));
    await userEvent.click(screen.getByRole('button', { name: 'Connect' }));
    await waitFor(() => {
      expect(useDialogs.getState().open).toBeNull();
    });
    expect(calls.map((c) => c.cmd)).toEqual(
      expect.arrayContaining(['remote_forget', 'remote_connect']),
    );
    expect(calls.find((c) => c.cmd === 'remote_connect')?.args).toEqual({
      server: 'https://dnd.example.org',
      sessionId: SESSION,
    });
  });
});

describe('HostingDialog', () => {
  it('uploads, shows the code and the link, and joins as the GM', async () => {
    useDialogs.setState({ open: 'hosting' });
    const hosted: HostedSession = {
      server: 'https://dnd.example.org',
      session_id: SESSION,
      name: 'Lost Mine',
      access_code: 'K7PQX-2MWRT',
      invite_url: `https://dnd.example.org/api/sessions/${SESSION}`,
    };
    setup(<HostingDialog />, {
      remote_host: () => hosted,
      remote_connect: () => ({ ...info, role: 'gm' }),
      remote_current: () => null,
    });
    await userEvent.type(screen.getByLabelText('Server address'), 'https://dnd.example.org');
    await userEvent.click(screen.getByRole('button', { name: 'Upload the campaign' }));
    expect(await screen.findByLabelText('Access code')).toHaveTextContent('K7PQX-2MWRT');
    expect(screen.getByLabelText('Invite link')).toHaveTextContent(hosted.invite_url);
    // An empty host token means "the saved one".
    expect(calls.find((c) => c.cmd === 'remote_host')?.args).toEqual({
      server: 'https://dnd.example.org',
      hostToken: '',
    });
    await userEvent.click(screen.getByRole('button', { name: 'Join as the GM' }));
    await waitFor(() => {
      expect(useDialogs.getState().open).toBeNull();
    });
  });
});

describe('StatusBar on a session server', () => {
  const remote = (status: RemoteInfo['status']): RemoteInfo => ({
    server: 'https://dnd.example.org',
    session_id: SESSION,
    session_name: 'Lost Mine',
    role: 'player',
    hosted: false,
    status,
  });

  it.each([
    [{ state: 'online' } as const, 'On the server'],
    [{ state: 'reconnecting', attempt: 2, retry_in_ms: 4000 } as const, 'retrying in 4 s'],
    [{ state: 'closed', reason: 'session_closed' } as const, 'The session has ended'],
  ])('shows %o', async (status, text) => {
    setup(<StatusBar campaign={info} />, {
      app_info: () => ({ version: '0.0.1' }),
      remote_current: () => remote(status),
    });
    expect(await screen.findByRole('status')).toHaveTextContent(text);
  });

  it('says local play without a server', async () => {
    setup(<StatusBar campaign={info} />, {
      app_info: () => ({ version: '0.0.1' }),
      remote_current: () => null,
    });
    expect(await screen.findByText('Local game')).toBeInTheDocument();
  });
});
