import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useJournalEntry, useSubmitIntent } from '@/shared/game/useGame';
import { IntentRejected, type JournalEntryDto } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { CodeEditor } from '@/shared/ui/CodeEditor';
import { ErrorText } from '@/shared/ui/ErrorText';
import { InlineEdit } from '@/shared/ui/InlineEdit';
import { Markdown } from '@/shared/ui/Markdown';

/** UI-draft debounce (CLAUDE.md §10): the text is sent 2 s after the last keystroke. */
export const DRAFT_DEBOUNCE_MS = 2000;

/** Center view of the selected journal entry. */
export function JournalEditor() {
  const { t } = useTranslation();
  const journalId = useSelection((s) => s.journalId);
  const { data: entry, isPending } = useJournalEntry(journalId);
  if (!journalId || isPending) return null;
  if (!entry) return <p className="muted">{t('journal.missing')}</p>;
  return <Editor key={entry.id} entry={entry} />;
}

function Editor({ entry }: { entry: JournalEntryDto }) {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();
  const closeTab = useSelection((s) => s.closeTab);
  /** Local text not yet confirmed by the core; `null` = showing the stored text. */
  const [draft, setDraft] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [preview, setPreview] = useState(false);

  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const canEditText =
    isGm ||
    (campaign?.role === 'player' && entry.visibility === 'players' && entry.players_can_edit);
  const text = draft ?? entry.markdown;

  const saveText = (markdown: string, version: number) => {
    submit.mutate(
      {
        intent: {
          kind: 'update_journal_entry',
          id: entry.id,
          title: null,
          markdown,
          visibility: null,
          players_can_edit: null,
        },
        expectedVersion: version,
      },
      {
        onSuccess: () => {
          // Keep typing that happened while the save was in flight.
          setDraft((current) => (current === markdown ? null : current));
        },
        onError: (error) => {
          if (error instanceof IntentRejected && error.reason.code === 'stale_version') {
            setConflict(true);
          }
        },
      },
    );
  };

  // Debounced save of the draft (never while a conflict waits for the user's decision).
  useEffect(() => {
    if (draft === null || conflict) return;
    const timer = setTimeout(() => {
      saveText(draft, entry.version);
    }, DRAFT_DEBOUNCE_MS);
    return () => {
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- saveText is recreated every render
  }, [draft, conflict, entry.version]);

  // Leaving the entry within the debounce window must not lose the draft.
  const pending = useRef({ draft, conflict, version: entry.version, save: saveText });
  useEffect(() => {
    pending.current = { draft, conflict, version: entry.version, save: saveText };
  });
  useEffect(
    () => () => {
      const p = pending.current;
      if (p.draft !== null && !p.conflict) p.save(p.draft, p.version);
    },
    [],
  );

  const update = (patch: {
    title?: string;
    visibility?: 'gm' | 'players';
    players_can_edit?: boolean;
  }) => {
    submit.mutate({
      intent: {
        kind: 'update_journal_entry',
        id: entry.id,
        title: patch.title ?? null,
        markdown: null,
        visibility: patch.visibility ?? null,
        players_can_edit: patch.players_can_edit ?? null,
      },
      expectedVersion: entry.version,
    });
  };

  return (
    <article className="journal-editor" aria-label={entry.title}>
      <header className="journal-header">
        <h2>
          {canEditText ? (
            <InlineEdit
              value={entry.title}
              label={t('journal.rename')}
              onCommit={(title) => {
                update({ title });
              }}
            />
          ) : (
            entry.title
          )}
        </h2>
        <button
          type="button"
          aria-pressed={preview}
          onClick={() => {
            setPreview((p) => !p);
          }}
        >
          {preview ? t('journal.edit') : t('journal.preview')}
        </button>
      </header>

      {isGm && (
        <div className="inline actions">
          <label className="inline">
            {t('journal.folder')}
            <FolderInput
              value={entry.folder}
              onCommit={(folder) => {
                submit.mutate({
                  intent: {
                    kind: 'move_journal_entry',
                    id: entry.id,
                    folder,
                    sort_order: entry.sort_order,
                  },
                  expectedVersion: entry.version,
                });
              }}
            />
          </label>
          <select
            aria-label={t('journal.visibilityLabel')}
            value={entry.visibility}
            onChange={(e) => {
              update({ visibility: e.target.value === 'players' ? 'players' : 'gm' });
            }}
          >
            <option value="gm">{t('journal.visibility.gm')}</option>
            <option value="players">{t('journal.visibility.players')}</option>
          </select>
          <label className="inline">
            <input
              type="checkbox"
              checked={entry.players_can_edit}
              disabled={entry.visibility !== 'players'}
              onChange={(e) => {
                update({ players_can_edit: e.target.checked });
              }}
            />
            {t('journal.playersCanEdit')}
          </label>
          <button
            type="button"
            onClick={() => {
              submit.mutate({
                intent: { kind: 'delete_journal_entry', id: entry.id },
                expectedVersion: entry.version,
              });
              closeTab({ kind: 'journal', id: entry.id });
            }}
          >
            {t('journal.delete')}
          </button>
        </div>
      )}

      {conflict && (
        <div role="alert" className="notice">
          <p>{t('journal.conflict')}</p>
          <button
            type="button"
            onClick={() => {
              setConflict(false);
              if (draft !== null) saveText(draft, entry.version);
            }}
          >
            {t('journal.applyMine')}
          </button>
          <button
            type="button"
            onClick={() => {
              setConflict(false);
              setDraft(null);
            }}
          >
            {t('journal.discardMine')}
          </button>
        </div>
      )}

      {preview || !canEditText ? (
        <Markdown source={text} />
      ) : (
        <CodeEditor
          value={text}
          onChange={setDraft}
          label={t('journal.text')}
          language="markdown"
        />
      )}
      {draft !== null && !conflict && <p className="muted">{t('journal.unsaved')}</p>}
      <ErrorText error={conflict ? null : submit.error} />
    </article>
  );
}

function FolderInput({ value, onCommit }: { value: string; onCommit: (folder: string) => void }) {
  const [draft, setDraft] = useState(value);
  const commit = () => {
    if (draft.trim() !== value) onCommit(draft);
  };
  return (
    <input
      value={draft}
      maxLength={200}
      placeholder="/"
      onChange={(e) => {
        setDraft(e.target.value);
      }}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === 'Enter') commit();
      }}
    />
  );
}
