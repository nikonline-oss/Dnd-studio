import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { ChatPanel, parseChatInput } from './ChatPanel';
import type {
  CampaignInfo,
  ChatMessageDto,
  IntentEnvelope,
  Role,
  Snapshot,
} from '@/shared/ipc/client';

describe('parseChatInput', () => {
  it('recognizes roll commands and plain text', () => {
    expect(parseChatInput('  hi  ')).toEqual({ kind: 'text', body: 'hi' });
    expect(parseChatInput('/r 1d20+5')).toEqual({
      kind: 'roll',
      expression: '1d20+5',
      hidden: false,
    });
    expect(parseChatInput('/ROLL 4d6kh3')).toEqual({
      kind: 'roll',
      expression: '4d6kh3',
      hidden: false,
    });
    expect(parseChatInput('/gr 1d20 adv')).toEqual({
      kind: 'roll',
      expression: '1d20 adv',
      hidden: true,
    });
    expect(parseChatInput('/gmroll d%')).toEqual({ kind: 'roll', expression: 'd%', hidden: true });
    expect(parseChatInput('/r   ')).toEqual({ kind: 'empty' });
    expect(parseChatInput('/rolling stones')).toEqual({ kind: 'text', body: '/rolling stones' });
    expect(parseChatInput('')).toEqual({ kind: 'empty' });
  });
});

const ME = '0190a000-0000-7000-8000-000000000001';

const rollMessage: ChatMessageDto = {
  id: '0190a000-0000-7000-8000-0000000000a1',
  kind: 'roll',
  sender: null,
  sender_name: 'Aria',
  body: '1d20 adv + 3',
  roll: {
    expression: 'd20 adv+3',
    terms: [
      {
        negative: false,
        notation: 'd20 adv',
        subtotal: 17,
        dice: [
          { value: 17, kept: true },
          { value: 4, kept: false },
        ],
      },
      { negative: false, notation: '3', subtotal: 3, dice: [] },
    ],
    total: 20,
  },
  system: null,
  visibility: 'gm',
  at_ms: 1_760_000_000_000,
};

const systemMessage: ChatMessageDto = {
  ...rollMessage,
  id: '0190a000-0000-7000-8000-0000000000a2',
  kind: 'system',
  sender_name: '',
  body: '',
  roll: null,
  visibility: 'all',
  system: { code: 'character_approved', name: 'Aria' },
};

function renderChat(role: Role, chat: ChatMessageDto[] = []) {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [],
    tokens: [],
    characters: [],
    users: [],
    journal: [],
    assets: [],
    fog: [],
    combats: [],
    combatants: [],
    chat,
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: ME, role };
  const sent: IntentEnvelope[] = [];
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    if (cmd === 'chat_history') return [];
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return { status: 'accepted', seq: 2 };
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ChatPanel />
    </QueryClientProvider>,
  );
  return sent;
}

describe('ChatPanel', () => {
  it('shows rolls with every die (dropped ones marked), hidden badge and system messages', async () => {
    renderChat('gm', [rollMessage, systemMessage]);
    expect(await screen.findByText('Aria was approved')).toBeInTheDocument();
    expect(screen.getByText('hidden')).toBeInTheDocument();
    expect(screen.getByLabelText('Total')).toHaveTextContent('20');
    expect(screen.getByText('4')).toHaveAttribute('data-kept', 'false');
    // The stored name is shown even though the sender was deleted (sender = null).
    expect(screen.getByText('Aria', { selector: 'strong' })).toBeInTheDocument();
  });

  it('sends text and rolls', async () => {
    const sent = renderChat('player');
    const input = await screen.findByLabelText('Message');
    await userEvent.type(input, 'Hello{Enter}');
    await userEvent.type(input, '/r 2d6+1{Enter}');
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });
    expect(sent[0]?.intent).toMatchObject({ kind: 'send_message', body: 'Hello' });
    expect(sent[1]?.intent).toMatchObject({
      kind: 'roll_dice',
      expression: '2d6+1',
      hidden: false,
      character_id: null,
    });
  });

  it('players cannot roll hidden', async () => {
    const sent = renderChat('player');
    await userEvent.type(await screen.findByLabelText('Message'), '/gr 1d20{Enter}');
    expect(await screen.findByRole('alert')).toHaveTextContent('Only the GM can roll hidden');
    expect(sent).toHaveLength(0);
  });
});
