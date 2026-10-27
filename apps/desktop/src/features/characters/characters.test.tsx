import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { CharacterEditor, sheetProblem } from './CharacterEditor';
import { CharacterList } from './CharacterList';
import type {
  CampaignInfo,
  CharacterDto,
  IntentEnvelope,
  Role,
  Snapshot,
} from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

// CodeMirror needs layout APIs jsdom lacks; a textarea stands in for it here.
vi.mock('@/shared/ui/CodeEditor', () => ({
  CodeEditor: ({
    value,
    onChange,
    label,
  }: {
    value: string;
    onChange: (v: string) => void;
    label: string;
  }) => (
    <textarea
      aria-label={label}
      value={value}
      onChange={(e) => {
        onChange(e.target.value);
      }}
    />
  ),
}));

const ME = '0190a000-0000-7000-8000-000000000001';
const OTHER = '0190a000-0000-7000-8000-000000000002';

const character = (patch: Partial<CharacterDto>): CharacterDto => ({
  id: '0190a000-0000-7000-8000-0000000000c1',
  kind: 'pc',
  name: 'Aria',
  owner: ME,
  status: 'draft',
  sheet_json: '{"hp": 10}',
  version: 4,
  ...patch,
});

function renderWith(role: Role, characters: CharacterDto[], node: React.ReactNode) {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [],
    tokens: [],
    characters,
    users: [
      { id: ME, name: 'Me', role },
      { id: OTHER, name: 'Bob', role: 'player' },
    ],
    journal: [],
    journal_folders: [],
    plugins: [],
    system_plugin: null,
    boards: [],
    board_nodes: [],
    board_edges: [],
    zones: [],
    assets: [],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: ME, role };
  const sent: IntentEnvelope[] = [];
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return { status: 'accepted', seq: 2 };
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  render(<QueryClientProvider client={new QueryClient()}>{node}</QueryClientProvider>);
  return sent;
}

afterEach(() => {
  useSelection.setState({ characterId: null, mapId: null, inspect: 'map' });
});

describe('sheetProblem', () => {
  it('mirrors the core rules (ADR-0016)', () => {
    expect(sheetProblem('{"hp": 3, "tags": ["a", 1]}')).toBeNull();
    expect(sheetProblem('{')).toBe('syntax');
    expect(sheetProblem('[1]')).toBe('not_object');
    expect(sheetProblem('{"w": 0.5}')).toBe('fraction');
    expect(sheetProblem('{"big": 9007199254740992}')).toBe('fraction');
  });
});

describe('CharacterList', () => {
  it('GM can add PCs and NPCs; NPCs have no owner', async () => {
    const sent = renderWith('gm', [], <CharacterList />);
    await userEvent.click(await screen.findByRole('button', { name: 'Add NPC' }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]?.intent).toMatchObject({
      kind: 'create_character',
      character_kind: 'npc',
      owner: null,
    });
  });

  it('players add only their own PC', async () => {
    const sent = renderWith('player', [], <CharacterList />);
    await userEvent.click(await screen.findByRole('button', { name: 'Add PC' }));
    expect(screen.queryByRole('button', { name: 'Add NPC' })).not.toBeInTheDocument();
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]?.intent).toMatchObject({ character_kind: 'pc', owner: ME });
  });
});

describe('CharacterEditor', () => {
  it('owner edits the draft sheet and submits it with the current version', async () => {
    useSelection.setState({ characterId: character({}).id, inspect: 'character' });
    const sent = renderWith('player', [character({})], <CharacterEditor />);
    const sheet = await screen.findByLabelText('Sheet (JSON)');
    await userEvent.clear(sheet);
    await userEvent.type(sheet, '{{"hp": 0.5}');
    expect(screen.getByRole('alert')).toHaveTextContent('Numbers must be whole');
    expect(screen.getByRole('button', { name: 'Save sheet' })).toBeDisabled();

    await userEvent.click(screen.getByRole('button', { name: 'Submit for approval' }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 4,
      intent: { kind: 'submit_character' },
    });
    expect(screen.queryByRole('button', { name: 'Approve' })).not.toBeInTheDocument();
  });

  it('GM reviews a pending character', async () => {
    useSelection.setState({ characterId: character({}).id, inspect: 'character' });
    renderWith(
      'gm',
      [character({ status: 'pending_approval', owner: OTHER })],
      <CharacterEditor />,
    );
    expect(await screen.findByRole('button', { name: 'Approve' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Return for changes' })).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Owner' })).toHaveValue(OTHER);
  });

  it('a player cannot edit someone else’s character', async () => {
    useSelection.setState({ characterId: character({}).id, inspect: 'character' });
    renderWith('player', [character({ owner: OTHER, status: 'active' })], <CharacterEditor />);
    await screen.findByText('Bob');
    expect(screen.queryByRole('button', { name: 'Save sheet' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Archive' })).not.toBeInTheDocument();
  });
});
