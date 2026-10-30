import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { CompendiumView } from './CompendiumView';
import { queryKeys } from '@/shared/game/useGame';
import type { CampaignInfo, IntentEnvelope } from '@/shared/ipc/client';

describe('compendium character templates', () => {
  const template = {
    kind: 'npc',
    name: { en: 'Scout' },
    sheet_data: { attributes: { agility: 2 } },
  };
  async function setup(
    role: CampaignInfo['role'],
    reject = false,
    data: unknown = { character_template: template },
  ) {
    const sent: IntentEnvelope[] = [];
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: Infinity } },
    });
    client.setQueryData(queryKeys.campaign, { name: 'C', path: '/c', user_id: 'u1', role });
    client.setQueryData(queryKeys.snapshot, {
      plugins: [],
      maps: [{ id: 'map1', name: 'Forest', width: 100, height: 80 }],
    });
    mockIPC((cmd, args) => {
      if (cmd === 'compendium_entries')
        return {
          entries: [{ id: 'scout', name: 'Scout', data }],
          total: 1,
        };
      if (cmd === 'submit_intent') {
        sent.push((args as { envelope: IntentEnvelope }).envelope);
        if (reject) throw new Error('creation failed');
        return { status: 'accepted', seq: 2 };
      }
      throw new Error(`unexpected ${cmd}`);
    });
    render(
      <QueryClientProvider client={client}>
        <CompendiumView compendiumKey="generic-d6/scouts" />
      </QueryClientProvider>,
    );
    await userEvent.click(await screen.findByRole('option', { name: 'Scout' }));
    return sent;
  }
  it.each(['gm', 'co_gm'] as const)(
    'lets %s create a sheet then place its linked token',
    async (role) => {
      const sent = await setup(role);
      await userEvent.click(screen.getByRole('button', { name: 'Create character' }));
      expect(
        await screen.findByRole('button', { name: 'Open character sheet' }),
      ).toBeInTheDocument();
      expect(sent[0]?.intent).toMatchObject({
        kind: 'create_character',
        character_kind: 'npc',
        name: 'Scout',
        owner: null,
        sheet_json: JSON.stringify(template.sheet_data),
      });
      await userEvent.selectOptions(
        screen.getByRole('combobox', { name: 'Map for token' }),
        'map1',
      );
      await userEvent.click(screen.getByRole('button', { name: 'Place token at map centre' }));
      await waitFor(() => {
        expect(sent).toHaveLength(2);
      });
      const character = sent[0]?.intent;
      expect(sent[1]?.intent).toMatchObject({
        kind: 'create_token',
        character_id: character && 'id' in character ? character.id : null,
        map_id: 'map1',
        x: 800,
        y: 640,
      });
    },
  );
  it.each(['player', 'spectator'] as const)('does not offer creation to %s', async (role) => {
    await setup(role);
    expect(screen.queryByRole('button', { name: 'Create character' })).not.toBeInTheDocument();
  });
  it('does not offer placement after rejected character creation', async () => {
    const sent = await setup('gm', true);
    await userEvent.click(screen.getByRole('button', { name: 'Create character' }));
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(screen.queryByRole('combobox')).not.toBeInTheDocument();
    expect(sent).toHaveLength(1);
  });
  it('creates a default NPC from an ordinary entry without mutating its data', async () => {
    const data = Object.freeze({ description: 'A guide', agility: 2 });
    const sent = await setup('gm', false, data);
    expect(
      screen.getByText('No character template: creates an editable sheet with system defaults.'),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Create character' }));
    await screen.findByRole('button', { name: 'Open character sheet' });
    expect(sent[0]?.intent).toMatchObject({
      kind: 'create_character',
      name: 'Scout',
      character_kind: 'npc',
      sheet_json: '{}',
    });
    expect(data).toEqual({ description: 'A guide', agility: 2 });
  });
  it.each([null, { kind: 'monster', sheet_data: {} }, { kind: 'npc', sheet_data: [] }])(
    'blocks an invalid template rather than falling back: %j',
    async (bad) => {
      const sent = await setup('gm', false, { character_template: bad });
      expect(screen.getByRole('alert')).toHaveTextContent('Invalid character template.');
      expect(screen.getByRole('button', { name: 'Create character' })).toBeDisabled();
      expect(sent).toHaveLength(0);
    },
  );
});
