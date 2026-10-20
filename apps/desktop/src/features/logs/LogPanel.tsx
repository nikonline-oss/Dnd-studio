import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands, events, type LogLevel, type LogRecord } from '@/shared/ipc/client';

const LEVELS: LogLevel[] = ['error', 'warn', 'info', 'debug', 'trace'];
const RANK: Record<LogLevel, number> = { trace: 0, debug: 1, info: 2, warn: 3, error: 4 };
const MAX_SHOWN = 2000;

/** Matches the panel filter: records at `minLevel` or more severe, containing `query`. */
export function filterLogs(records: LogRecord[], minLevel: LogLevel, query: string): LogRecord[] {
  const q = query.trim().toLowerCase();
  return records.filter(
    (r) =>
      RANK[r.level] >= RANK[minLevel] &&
      (q === '' || r.message.toLowerCase().includes(q) || r.target.toLowerCase().includes(q)),
  );
}

function formatRecord(r: LogRecord): string {
  return `${new Date(r.ts_ms).toISOString()} ${r.level.toUpperCase()} ${r.target}: ${r.message}`;
}

/** Log panel (CLAUDE.md §10): warn+error by default, level filter, search, copy. */
export function LogPanel() {
  const { t } = useTranslation();
  const [records, setRecords] = useState<LogRecord[]>([]);
  const [minLevel, setMinLevel] = useState<LogLevel>('warn');
  const [query, setQuery] = useState('');

  useEffect(() => {
    let active = true;
    void commands.logsRecent().then((recent) => {
      if (active) setRecords(recent);
    });
    const unlisten = events.logRecord.listen(({ payload }) => {
      setRecords((prev) => [...prev.slice(-(MAX_SHOWN - 1)), payload]);
    });
    return () => {
      active = false;
      void unlisten.then((stop) => {
        stop();
      });
    };
  }, []);

  const shown = useMemo(() => filterLogs(records, minLevel, query), [records, minLevel, query]);

  return (
    <section className="log-panel" aria-label={t('logs.title')}>
      <div className="log-toolbar">
        <label className="inline">
          {t('logs.level')}
          <select
            value={minLevel}
            onChange={(e) => {
              setMinLevel(e.target.value as LogLevel);
            }}
          >
            {LEVELS.map((level) => (
              <option key={level} value={level}>
                {t(`logs.levels.${level}`)}
              </option>
            ))}
          </select>
        </label>
        <input
          type="search"
          aria-label={t('logs.search')}
          placeholder={t('logs.search')}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
          }}
        />
        <button
          type="button"
          disabled={shown.length === 0}
          onClick={() => {
            void navigator.clipboard.writeText(shown.map(formatRecord).join('\n'));
          }}
        >
          {t('logs.copy')}
        </button>
      </div>
      {shown.length === 0 ? (
        <p className="muted">{t('logs.empty')}</p>
      ) : (
        <ol className="log-list">
          {shown.map((r, n) => (
            <li key={`${r.ts_ms}-${n}`} data-level={r.level}>
              <time dateTime={new Date(r.ts_ms).toISOString()}>
                {new Date(r.ts_ms).toLocaleTimeString()}
              </time>
              <span className="log-level">{r.level}</span>
              <span className="log-target">{r.target}</span>
              <span className="log-message">{r.message}</span>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
