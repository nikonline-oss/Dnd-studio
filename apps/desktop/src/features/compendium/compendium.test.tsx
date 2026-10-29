import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { CompendiumView } from './CompendiumView';
import { entryMarkdown } from './entryMarkdown';
import { IpcFailure } from '@/shared/ipc/client';
import type {
  CampaignInfo,
  CompendiumEntryDto,
  IntentEnvelope,
  Snapshot,
} from '@/shared/ipc/client';
import { queryKeys } from '@/shared/game/useGame';

const snapshot: Snapshot = {
  seq: 0,
  campaign: { name: 'C', description: '', version: 1 },
  maps: [],
  tokens: [],
  characters: [],
  users: [],
  journal: [],
  journal_folders: [],
  plugins: [],
  system_plugin: null,
  boards: [],
  board_nodes: [],
  board_edges: [],
  zones: [],
  triggers: [],
  assets: [],
  fog: [],
  combats: [],
  combatants: [],
  chat: [],
};

const rat: CompendiumEntryDto = {
  id: 'giant-rat',
  name: { ru: 'Гигантская крыса', en: 'Giant Rat' },
  data: {
    hit_dice: '2d6',
    ac: 13,
    abilities: { str: 7, dex: 16 },
    attacks: [{ name: 'Bite', bonus: 5 }],
    summary: { ru: 'Кусается.', en: 'It bites.' },
  },
};

describe('entryMarkdown', () => {
  it('writes any system’s entry as readable fields in the reader’s language', () => {
    expect(entryMarkdown(rat, 'en')).toBe(
      [
        '# Giant Rat',
        '',
        '- **Hit dice:** 2d6',
        '- **Ac:** 13',
        '- **Abilities:** Str 7 · Dex 16',
        '- **Attacks:** ',
        '  - Name: Bite, Bonus: 5',
        '- **Summary:** It bites.',
        '',
      ].join('\n'),
    );
    expect(entryMarkdown(rat, 'ru')).toContain('# Гигантская крыса');
  });
});

describe('CompendiumView', () => {
  it('ignores a first-load response for a plugin removed while it was downloading', async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: Infinity } },
    });
    client.setQueryData(queryKeys.snapshot, {
      ...snapshot,
      plugins: [
        {
          id: 'srd5',
          archive_sha256: 'a'.repeat(64),
          name: 'SRD',
          version: '0.1.0',
          kind: 'content',
          license: 'MIT',
          attribution: null,
          system: null,
        },
      ],
    } satisfies Snapshot);
    client.setQueryData(queryKeys.campaign, { name: 'C', path: '/c', user_id: 'u1', role: 'gm' });
    let release: (page: { entries: CompendiumEntryDto[]; total: number }) => void = () => {};
    const initial = new Promise<{ entries: CompendiumEntryDto[]; total: number }>((resolve) => {
      release = resolve;
    });
    let requests = 0;
    mockIPC((cmd) => {
      if (cmd === 'get_snapshot') return snapshot;
      if (cmd === 'compendium_entries') {
        if (++requests === 1) return initial;
        throw new IpcFailure({ kind: 'user', code: 'not_found', message: 'removed' });
      }
      throw new Error(`unexpected ${cmd}`);
    });
    render(
      <QueryClientProvider client={client}>
        <CompendiumView compendiumKey="srd5/srd5-monsters" />
      </QueryClientProvider>,
    );
    await waitFor(() => {
      expect(requests).toBe(1);
    });
    act(() => {
      client.setQueryData(queryKeys.snapshot, { ...snapshot, seq: 1 });
    });
    await waitFor(() => {
      expect(requests).toBe(2);
    });
    await act(async () => {
      release({ entries: [rat], total: 1 });
      await initial;
    });
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(screen.queryByRole('option', { name: 'Giant Rat' })).not.toBeInTheDocument();
  });

  it('clears an open entry when its plugin is no longer available', async () => {
    let removed = false;
    mockIPC((cmd) => {
      if (cmd === 'campaign_current') return { name: 'C', path: '/c', user_id: 'u1', role: 'gm' };
      if (cmd === 'get_snapshot') return snapshot;
      if (cmd === 'compendium_entries') {
        if (removed)
          throw new IpcFailure({ kind: 'user', code: 'not_found', message: 'plugin removed' });
        return { entries: [rat], total: 1 };
      }
      throw new Error(`unexpected ${cmd}`);
    });
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <CompendiumView compendiumKey="srd5/srd5-monsters" />
      </QueryClientProvider>,
    );
    await userEvent.click(await screen.findByRole('option', { name: 'Giant Rat' }));
    expect(screen.getByRole('heading', { name: 'Giant Rat' })).toBeInTheDocument();
    removed = true;
    await act(async () => {
      await client.invalidateQueries({ queryKey: ['compendium'] });
    });
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Giant Rat' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Copy to the journal' })).not.toBeInTheDocument();
  });

  it('searches, shows an entry and copies it to the journal', async () => {
    const calls: { cmd: string; args: unknown }[] = [];
    const sent: IntentEnvelope[] = [];
    const info: CampaignInfo = { name: 'C', path: '/c', user_id: 'u1', role: 'gm' };
    mockIPC((cmd, args) => {
      calls.push({ cmd, args });
      if (cmd === 'campaign_current') return info;
      if (cmd === 'get_snapshot') return snapshot;
      if (cmd === 'compendium_entries') return { entries: [rat], total: 1 };
      if (cmd === 'submit_intent') {
        sent.push((args as { envelope: IntentEnvelope }).envelope);
        return { status: 'accepted', seq: 2 };
      }
      throw new Error(`unexpected command ${cmd}`);
    });
    render(
      <QueryClientProvider client={new QueryClient()}>
        <CompendiumView compendiumKey="srd5/srd5-monsters" />
      </QueryClientProvider>,
    );
    await userEvent.type(screen.getByRole('searchbox'), 'rat');
    await waitFor(() => {
      expect(calls.at(-1)?.args).toMatchObject({
        pluginId: 'srd5',
        compendiumId: 'srd5-monsters',
        query: 'rat',
      });
    });
    await userEvent.click(await screen.findByRole('option', { name: 'Giant Rat' }));
    expect(screen.getByRole('heading', { name: 'Giant Rat' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Copy to the journal' }));
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({
        kind: 'create_journal_entry',
        title: 'Giant Rat',
        visibility: 'gm',
      });
    });
  });
});
