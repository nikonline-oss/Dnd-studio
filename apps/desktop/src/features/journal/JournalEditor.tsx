import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useJournalEntry, useSubmitIntent } from '@/shared/game/useGame';
import { IntentRejected, type JournalEntryDto, type JournalTable } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { CodeEditor, type CodeEditorHandle } from '@/shared/ui/CodeEditor';
import { ElementPicker } from '@/shared/ui/ElementPicker';
import { ErrorText } from '@/shared/ui/ErrorText';
import { InlineEdit } from '@/shared/ui/InlineEdit';
import { linkMarkdown, Markdown } from '@/shared/ui/Markdown';
import { TableGrid } from './TableGrid';

/** UI-draft debounce (CLAUDE.md §10): the text is sent 2 s after the last keystroke. */
export const DRAFT_DEBOUNCE_MS = 2000;

/** What an entry holds: a Markdown text or a table (ADR-0026). */
type Content = string | JournalTable;

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
  /** Local text (or table) not yet confirmed by the core; `null` = showing the stored one. */
  const [draft, setDraft] = useState<Content | null>(null);
  const [conflict, setConflict] = useState(false);
  const [preview, setPreview] = useState(false);
  /** Which picker is open: a picture or a link to insert at the cursor. */
  const [picking, setPicking] = useState<'picture' | 'link' | null>(null);
  const editor = useRef<CodeEditorHandle>(null);

  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const canEditText =
    isGm ||
    (campaign?.role === 'player' && entry.visibility === 'players' && entry.players_can_edit);
  const content: Content = draft ?? entry.table ?? entry.markdown;
  const text = typeof content === 'string' ? content : '';

  const saveText = (saved: Content, version: number) => {
    submit.mutate(
      {
        intent: {
          kind: 'update_journal_entry',
          id: entry.id,
          title: null,
          markdown: typeof saved === 'string' ? saved : null,
          visibility: null,
          players_can_edit: null,
          table: typeof saved === 'string' ? null : saved,
        },
        expectedVersion: version,
      },
      {
        onSuccess: () => {
          // Keep typing that happened while the save was in flight.
          setDraft((current) => (current === saved ? null : current));
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
        <div className="inline">
          {canEditText && !preview && typeof content === 'string' && (
            <>
              <button
                type="button"
                onClick={() => {
                  setPicking('link');
                }}
              >
                {t('journal.insertLink')}
              </button>
              <button
                type="button"
                onClick={() => {
                  setPicking('picture');
                }}
              >
                {t('journal.insertPicture')}
              </button>
            </>
          )}
          {typeof content === 'string' && (
            <button
              type="button"
              aria-pressed={preview}
              onClick={() => {
                setPreview((p) => !p);
              }}
            >
              {preview ? t('journal.edit') : t('journal.preview')}
            </button>
          )}
        </div>
      </header>
      {picking === 'picture' && (
        <ElementPicker
          title={t('journal.insertPicture')}
          kinds={['asset']}
          onClose={() => {
            setPicking(null);
          }}
          onPick={({ id, name }) => {
            // Typed in at the cursor: saved with the text after the usual debounce.
            const alt = name.replace(/\.[a-z0-9]+$/i, '').replace(/[[\]]/g, '');
            editor.current?.insert(`![${alt}](asset:${id})`);
          }}
        />
      )}
      {picking === 'link' && (
        <ElementPicker
          title={t('journal.insertLink')}
          kinds={['journal', 'map', 'character', 'board', 'asset', 'audio']}
          onClose={() => {
            setPicking(null);
          }}
          onPick={({ kind, id, name }) => {
            if (kind === 'asset' || kind === 'audio') {
              const label = name.replace(/[[\]\\]/g, '');
              editor.current?.insert(`${kind === 'asset' ? '!' : ''}[${label}](asset:${id})`);
            }
            if (kind === 'journal' || kind === 'map' || kind === 'character' || kind === 'board') {
              editor.current?.insert(linkMarkdown(kind, id, name));
            }
          }}
        />
      )}

      {isGm && (
        <div className="inline actions">
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

      {typeof content !== 'string' ? (
        <TableGrid table={content} editable={canEditText} onChange={setDraft} />
      ) : preview || !canEditText ? (
        <Markdown source={text} />
      ) : (
        <CodeEditor
          ref={editor}
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
