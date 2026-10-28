import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { CompendiumView } from './CompendiumView';
import { entryMarkdown } from './entryMarkdown';
import type { CampaignInfo, CompendiumEntryDto, IntentEnvelope } from '@/shared/ipc/client';

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
  it('searches, shows an entry and copies it to the journal', async () => {
    const calls: { cmd: string; args: unknown }[] = [];
    const sent: IntentEnvelope[] = [];
    const info: CampaignInfo = { name: 'C', path: '/c', user_id: 'u1', role: 'gm' };
    mockIPC((cmd, args) => {
      calls.push({ cmd, args });
      if (cmd === 'campaign_current') return info;
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
