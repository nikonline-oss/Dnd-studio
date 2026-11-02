import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { RequestRollsDialog } from './RequestRollsDialog';
import type { CharacterDto, IntentEnvelope, UserDto } from '@/shared/ipc/client';

const characters: CharacterDto[] = [
  {
    id: 'a',
    name: 'Aria',
    owner: 'p1',
    kind: 'pc',
    status: 'active',
    sheet_json: '{}',
    version: 1,
  },
  {
    id: 'b',
    name: 'Borin',
    owner: 'p2',
    kind: 'pc',
    status: 'active',
    sheet_json: '{}',
    version: 1,
  },
  {
    id: 'npc',
    name: 'Guard',
    owner: null,
    kind: 'npc',
    status: 'active',
    sheet_json: '{}',
    version: 1,
  },
  {
    id: 'old',
    name: 'Old hero',
    owner: 'p1',
    kind: 'pc',
    status: 'archived',
    sheet_json: '{}',
    version: 1,
  },
];
const users: UserDto[] = [
  { id: 'p1', name: 'One', role: 'player' },
  { id: 'p2', name: 'Two', role: 'player' },
];

function mount(reject = false) {
  const sent: IntentEnvelope[] = [];
  const close = vi.fn();
  mockIPC((cmd, args) => {
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return reject
        ? { status: 'rejected', reason: { code: 'invalid', field: 'dice' } }
        : { status: 'accepted', seq: 2 };
    }
    throw new Error(`Unexpected ${cmd}`);
  });
  render(
    <QueryClientProvider client={new QueryClient()}>
      <RequestRollsDialog characters={characters} users={users} onClose={close} />
    </QueryClientProvider>,
  );
  return { sent, close };
}

describe('request checks dialog', () => {
  it('explains missing input and sends one check group to all player characters', async () => {
    const { sent, close } = mount();
    expect(screen.getByRole('button', { name: 'Send requests' })).toBeDisabled();
    await userEvent.type(screen.getByRole('textbox', { name: 'Check name' }), 'Perception');
    await userEvent.click(screen.getByRole('button', { name: 'All players' }));
    expect(screen.getByRole('checkbox', { name: /Aria/ })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: /Borin/ })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: /Guard/ })).not.toBeChecked();
    expect(screen.queryByText('Old hero')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Send requests' }));
    await waitFor(() => {
      expect(close).toHaveBeenCalledOnce();
    });
    expect(sent[0]?.intent).toMatchObject({
      kind: 'request_rolls',
      label: 'Perception',
      dice: '1d20',
      characters: ['a', 'b'],
    });
  });
  it('keeps the form open and explains a rejected expression', async () => {
    const { close } = mount(true);
    await userEvent.type(screen.getByRole('textbox', { name: 'Check name' }), 'Notice');
    await userEvent.click(screen.getByRole('checkbox', { name: /Aria/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Send requests' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(/formula|field/i);
    expect(close).not.toHaveBeenCalled();
    expect(screen.getByRole('textbox', { name: 'Check name' })).toHaveValue('Notice');
  });
});
