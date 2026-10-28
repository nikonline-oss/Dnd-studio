import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { ElementPicker, type ElementKind } from './ElementPicker';
import type { AssetDto, CampaignInfo, Snapshot } from '@/shared/ipc/client';

const id = (n: number) => `0190a000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;

const picture = (n: number, name: string): AssetDto => ({
  id: id(n),
  usage: 'token',
  sha256: `${n}`.repeat(64).slice(0, 64),
  mime: 'image/webp',
  size_bytes: 300 * 1024,
  original_name: name,
  width: 512,
  height: 256,
  display_sha256: 'd'.repeat(63) + `${n}`,
  thumbnail_sha256: 't'.repeat(63) + `${n}`,
  tiles: [],
  version: 1,
});

function setup(kinds: ElementKind[]) {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [
      {
        id: id(20),
        name: 'Sunken Crypt',
        background_asset_id: id(30),
        width: 4000,
        height: 3000,
        grid: { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 },
        visible_to_players: false,
        version: 1,
      },
    ],
    tokens: [
      {
        id: id(40),
        map_id: id(20),
        character_id: null,
        asset_id: id(31),
        name: 'Ghoul',
        x: 0,
        y: 0,
        rotation: 0,
        scale: 1,
        layer: 'gm',
        visible: false,
        version: 1,
      },
    ],
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
    triggers: [
      {
        id: id(50),
        definition: {
          name: 'Collapse',
          enabled: true,
          once: false,
          event: { type: 'token_entered_zone', zone: id(60) },
          tokens: [],
          condition: null,
          steps: [
            { when: null, action: { type: 'chat', text: 'Rumble', gm_only: false } },
            { when: null, action: { type: 'add_condition', condition: 'prone' } },
          ],
        },
        fired: false,
        version: 1,
      },
    ],
    assets: [picture(30, 'crypt.png'), picture(31, 'ghoul.png')],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: id(99), role: 'gm' };
  mockIPC((cmd) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    throw new Error(`unexpected command ${cmd}`);
  });
  // Asset URLs: the hash is all the tests look at.
  (
    window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }
  ).__TAURI_INTERNALS__.convertFileSrc = (hash: string) => `asset://${hash}`;
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ElementPicker
        title="Pick"
        kinds={kinds}
        onPick={() => undefined}
        onClose={() => undefined}
      />
    </QueryClientProvider>,
  );
}

describe('ElementPicker details', () => {
  it('asks to highlight an element first', async () => {
    setup(['map']);
    await screen.findByRole('option', { name: /Sunken Crypt/ });
    expect(screen.getByRole('complementary', { name: 'Details' })).toHaveTextContent(
      'Highlight an element',
    );
  });

  it('shows a map with its background, size, grid and visibility', async () => {
    setup(['map']);
    await userEvent.click(await screen.findByRole('option', { name: /Sunken Crypt/ }));
    const details = screen.getByRole('complementary', { name: 'Details' });
    expect(within(details).getByRole('heading', { name: 'Sunken Crypt' })).toBeInTheDocument();
    expect(details.querySelector('img')?.getAttribute('src')).toContain('d'.repeat(63) + '30');
    expect(details).toHaveTextContent('Size4000×3000');
    expect(details).toHaveTextContent('GridSquare, 70');
    expect(details).toHaveTextContent('Playersno');
  });

  it('shows a token with its picture, map and layer', async () => {
    setup(['token']);
    await userEvent.click(await screen.findByRole('option', { name: /Ghoul/ }));
    const details = screen.getByRole('complementary', { name: 'Details' });
    expect(details.querySelector('img')?.getAttribute('src')).toContain('d'.repeat(63) + '31');
    expect(details).toHaveTextContent('MapSunken Crypt');
    expect(details).toHaveTextContent('LayerGM only');
  });

  it('shows a trigger with its event and steps', async () => {
    setup(['trigger']);
    await userEvent.click(await screen.findByRole('option', { name: /Collapse/ }));
    const details = screen.getByRole('complementary', { name: 'Details' });
    expect(details).toHaveTextContent('EventToken entered');
    expect(details).toHaveTextContent(/Steps.+→.+/);
  });

  it('follows the keyboard highlight and shows an asset with its file name', async () => {
    setup(['asset']);
    const first = await screen.findByRole('option', { name: /crypt\.png/ });
    await userEvent.click(first);
    await userEvent.click(screen.getByRole('searchbox'));
    await userEvent.keyboard('{ArrowDown}');
    const details = screen.getByRole('complementary', { name: 'Details' });
    expect(details).toHaveTextContent('Fileghoul.png');
    expect(details).toHaveTextContent('512×256');
  });
});
