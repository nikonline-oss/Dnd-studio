import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DRAFT_DEBOUNCE_MS, JournalEditor } from './JournalEditor';
import type {
  CampaignInfo,
  IntentEnvelope,
  IntentOutcome,
  JournalEntryDto,
  JournalEntryMeta,
  Role,
} from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { Markdown } from '@/shared/ui/Markdown';

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

const meta = (patch: Partial<JournalEntryMeta>): JournalEntryMeta => ({
  id: 'x',
  folder_id: null,
  title: 'T',
  sort_order: 0,
  visibility: 'gm',
  players_can_edit: false,
  author: null,
  version: 1,
  ...patch,
});

const entry: JournalEntryDto = {
  ...meta({ id: '0190a000-0000-7000-8000-0000000000e1', title: 'Gods', version: 3 }),
  markdown: '# Pantheon',
};

describe('Markdown', () => {
  it('renders formatting but no HTML, scripts or navigable links', () => {
    render(
      <Markdown
        source={
          '# Title\n\n**bold** <script>alert(1)</script> <img src=x onerror=alert(1)>\n\n[click](javascript:alert(1)) | a |\n|---|\n| t |'
        }
      />,
    );
    expect(screen.getByRole('heading', { name: 'Title' })).toBeInTheDocument();
    expect(screen.getByText('bold').tagName).toBe('STRONG');
    expect(document.querySelector('script')).toBeNull();
    expect(document.querySelector('img')).toBeNull();
    expect(document.querySelector('a')).toBeNull();
    expect(screen.getByText('click')).toHaveClass('md-link');
  });
});

describe('JournalEditor', () => {
  let outcomes: IntentOutcome[];
  let sent: IntentEnvelope[];

  function renderEditor(role: Role, e: JournalEntryDto = entry) {
    const info: CampaignInfo = {
      name: 'C',
      path: '/c',
      user_id: '0190a000-0000-7000-8000-000000000001',
      role,
    };
    mockIPC((cmd, args) => {
      if (cmd === 'journal_entry') return e;
      if (cmd === 'campaign_current') return info;
      if (cmd === 'get_snapshot') throw new Error('not needed');
      if (cmd === 'submit_intent') {
        sent.push((args as { envelope: IntentEnvelope }).envelope);
        return outcomes.shift() ?? { status: 'accepted', seq: 9 };
      }
      throw new Error(`unexpected command ${cmd}`);
    });
    useSelection.setState({ journalId: e.id, center: 'journal' });
    render(
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <JournalEditor />
      </QueryClientProvider>,
    );
  }

  beforeEach(() => {
    outcomes = [];
    sent = [];
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  async function typeText(text: string) {
    const editor = await screen.findByLabelText('Entry text (Markdown)');
    const { fireEvent } = await import('@testing-library/react');
    fireEvent.change(editor, { target: { value: text } });
  }

  it('sends the draft once, 2 s after the last change, with the base version', async () => {
    renderEditor('gm');
    await typeText('# Pantheon\n\nZeus');
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DRAFT_DEBOUNCE_MS - 100);
    });
    await typeText('# Pantheon\n\nZeus and Hera');
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DRAFT_DEBOUNCE_MS - 100);
    });
    expect(sent).toHaveLength(0);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(200);
    });
    expect(sent).toHaveLength(1);
    expect(sent[0]).toMatchObject({
      expected_version: 3,
      intent: { kind: 'update_journal_entry', markdown: '# Pantheon\n\nZeus and Hera' },
    });
  });

  it('keeps the draft on a version conflict and lets the user choose', async () => {
    outcomes = [{ status: 'rejected', reason: { code: 'stale_version', current: 4 } }];
    renderEditor('gm');
    await typeText('mine');
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DRAFT_DEBOUNCE_MS + 10);
    });
    expect(await screen.findByRole('alert')).toHaveTextContent('Someone changed this entry');
    expect(screen.getByLabelText('Entry text (Markdown)')).toHaveValue('mine');
    // No automatic retries while the conflict is open.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DRAFT_DEBOUNCE_MS * 3);
    });
    expect(sent).toHaveLength(1);

    screen.getByRole('button', { name: 'Save mine over theirs' }).click();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(sent).toHaveLength(2);
    expect(sent[1]?.intent).toMatchObject({ markdown: 'mine' });
  });

  it('leaving the entry before the debounce still saves the draft', async () => {
    renderEditor('gm');
    await typeText('half-written');
    act(() => {
      useSelection.setState({ journalId: null, center: 'map' });
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(sent).toHaveLength(1);
    expect(sent[0]?.intent).toMatchObject({ markdown: 'half-written' });
  });

  it('players see read-only text unless the entry is shared and editable', async () => {
    renderEditor('player', { ...entry, visibility: 'players', players_can_edit: false });
    expect(await screen.findByRole('heading', { name: 'Pantheon' })).toBeInTheDocument();
    expect(screen.queryByLabelText('Entry text (Markdown)')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Visibility')).not.toBeInTheDocument();
  });
});
