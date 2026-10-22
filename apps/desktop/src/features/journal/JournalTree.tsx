import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import type { JournalEntryMeta } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';

/** Entries grouped by folder (folders sorted, root first), each sorted by `sort_order`. */
export function groupByFolder(entries: JournalEntryMeta[]): [string, JournalEntryMeta[]][] {
  const groups = new Map<string, JournalEntryMeta[]>();
  for (const e of entries) {
    const list = groups.get(e.folder) ?? [];
    list.push(e);
    groups.set(e.folder, list);
  }
  return [...groups.entries()]
    .sort(([a], [b]) => (a === '' ? -1 : b === '' ? 1 : a.localeCompare(b)))
    .map(([folder, list]) => [
      folder,
      [...list].sort((x, y) => x.sort_order - y.sort_order || x.title.localeCompare(y.title)),
    ]);
}

/** Navigator: the journal by folder. GM roles add entries; everyone opens them. */
export function JournalTree() {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const { data: snapshot } = useSnapshot();
  const submit = useSubmitIntent();
  const selectedId = useSelection((s) => (s.center === 'journal' ? s.journalId : null));
  const selectJournal = useSelection((s) => s.selectJournal);
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const groups = groupByFolder(snapshot?.journal ?? []);

  const add = () => {
    const id = uuidv7();
    submit.mutate(
      {
        intent: {
          kind: 'create_journal_entry',
          id,
          folder: '',
          title: t('journal.newTitle'),
          markdown: '',
          visibility: 'gm',
          players_can_edit: false,
        },
      },
      {
        onSuccess: () => {
          selectJournal(id);
        },
      },
    );
  };

  return (
    <NavSection
      icon="book"
      title={t('journal.title')}
      count={snapshot?.journal.length ?? 0}
      actions={
        isGm && (
          <IconButton
            icon="plus"
            label={t('journal.add')}
            onClick={add}
            disabled={submit.isPending}
          />
        )
      }
    >
      {groups.length === 0 && <p className="nav-empty">{t('journal.empty')}</p>}
      {groups.map(([folder, entries]) => (
        <div key={folder} className="journal-folder">
          {folder !== '' && (
            <h3 className="folder-name">
              <Icon name="folder" />
              {folder}
            </h3>
          )}
          <ul className="nav-list" aria-label={folder === '' ? t('journal.root') : folder}>
            {entries.map((e) => (
              <li
                key={e.id}
                className="nav-row"
                data-nested={folder !== ''}
                aria-current={e.id === selectedId}
                onClick={() => {
                  selectJournal(e.id);
                }}
              >
                <Icon name={e.visibility === 'gm' ? 'lock' : 'book'} className="row-icon" />
                <span className="row-name">{e.title}</span>
                <span className="badge" data-visibility={e.visibility}>
                  {t(`journal.visibility.${e.visibility}`)}
                </span>
              </li>
            ))}
          </ul>
        </div>
      ))}
      <ErrorText error={submit.error} />
    </NavSection>
  );
}
