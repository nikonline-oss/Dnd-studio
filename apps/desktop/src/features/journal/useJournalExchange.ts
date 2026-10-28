import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { planExport, planImport } from './exchange';
import { useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { commands, unwrap } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useToast } from '@/shared/ui/Toast';

/**
 * Journal import and export (ADR-0026). Files are read and written by the desktop layer in a
 * folder the user picks; the plan is made here and sent as ordinary intents, parents first, so it
 * works the same in a local campaign and in a remote session.
 */
export function useJournalExchange() {
  const { t } = useTranslation();
  const showToast = useToast((s) => s.show);
  const folders = useSnapshotSelect((s) => s.journal_folders) ?? [];
  const metas = useSnapshotSelect((s) => s.journal) ?? [];
  const submit = useSubmitIntent();

  const importMutation = useMutation({
    mutationFn: async () => {
      const imported = await unwrap(commands.journalImportRead());
      if (!imported) return;
      if (imported.files.length === 0) {
        showToast(t('journal.importNothing'));
        return;
      }
      const roots = folders.filter((f) => f.parent === null).map((f) => f.name);
      const plan = planImport(imported, roots, uuidv7);
      for (const intent of plan.intents) {
        await submit.mutateAsync({ intent });
      }
      const counts = { entries: plan.entries, folders: plan.folders, skipped: imported.skipped };
      showToast(t(imported.skipped > 0 ? 'journal.importedSkipped' : 'journal.imported', counts));
    },
  });

  const exportMutation = useMutation({
    mutationFn: async () => {
      // Texts are not in the snapshot: each entry is fetched (ADR-0018).
      const entries = await Promise.all(metas.map((m) => unwrap(commands.journalEntry(m.id))));
      const folder = await unwrap(commands.journalExportWrite(planExport(folders, entries)));
      if (folder !== null) showToast(t('journal.exported', { folder }));
    },
  });

  return {
    importJournal: () => {
      importMutation.mutate();
    },
    exportJournal: () => {
      exportMutation.mutate();
    },
    busy: importMutation.isPending || exportMutation.isPending,
    error: importMutation.error ?? exportMutation.error,
  };
}
