import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { filterLogs, LogPanel } from './LogPanel';
import type { LogRecord } from '@/shared/ipc/client';

const records: LogRecord[] = [
  { ts_ms: 1, level: 'info', target: 'dnd_db', message: 'campaign opened' },
  { ts_ms: 2, level: 'warn', target: 'dnd_desktop_lib', message: 'slow save' },
  { ts_ms: 3, level: 'error', target: 'frontend', message: 'Maps crashed' },
  { ts_ms: 4, level: 'debug', target: 'dnd_db', message: 'campaign saved' },
];

describe('filterLogs', () => {
  it('shows warn and error by default level', () => {
    expect(filterLogs(records, 'warn', '').map((r) => r.ts_ms)).toEqual([2, 3]);
  });

  it('lower levels include higher severities', () => {
    expect(filterLogs(records, 'debug', '')).toHaveLength(4);
  });

  it('searches message and target, case-insensitively', () => {
    expect(filterLogs(records, 'trace', 'CAMPAIGN').map((r) => r.ts_ms)).toEqual([1, 4]);
    expect(filterLogs(records, 'trace', 'frontend').map((r) => r.ts_ms)).toEqual([3]);
  });
});

describe('LogPanel', () => {
  it('loads recent records and filters by level', async () => {
    mockIPC(
      (cmd) => {
        if (cmd === 'logs_recent') return records;
        throw new Error(`unexpected command ${cmd}`);
      },
      { shouldMockEvents: true },
    );
    render(<LogPanel />);
    expect(await screen.findByText('slow save')).toBeInTheDocument();
    expect(screen.queryByText('campaign opened')).not.toBeInTheDocument();

    await userEvent.selectOptions(screen.getByRole('combobox'), 'info');
    expect(screen.getByText('campaign opened')).toBeInTheDocument();
  });
});
