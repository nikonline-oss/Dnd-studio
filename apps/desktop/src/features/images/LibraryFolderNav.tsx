import { useTranslation } from 'react-i18next';
import { useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import type { Library, LibraryFolderDto } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { IconButton } from '@/shared/ui/IconButton';

/** Shared folder tree for the image and audio libraries. */
export function LibraryFolderNav({
  library,
  selected,
  onSelect,
}: {
  library: Library;
  selected: string | null;
  onSelect: (id: string | null) => void;
}) {
  const { t } = useTranslation();
  const folders = (useSnapshotSelect((s) => s.library_folders ?? []) ?? []).filter(
    (folder) => folder.library === library,
  );
  const assets = useSnapshotSelect((s) => s.assets) ?? [];
  const submit = useSubmitIntent();
  const children = (parent: string | null): LibraryFolderDto[] =>
    folders
      .filter((folder) => folder.parent === parent)
      .sort((a, b) => a.name.localeCompare(b.name));
  const descendants = (id: string): Set<string> => {
    const found = new Set<string>([id]);
    for (const child of children(id)) for (const nested of descendants(child.id)) found.add(nested);
    return found;
  };
  const rows: { folder: LibraryFolderDto; depth: number }[] = [];
  const visit = (parent: string | null, depth: number) => {
    for (const folder of children(parent)) {
      rows.push({ folder, depth });
      visit(folder.id, depth + 1);
    }
  };
  visit(null, 0);
  const create = () => {
    const name = window.prompt(t('library.folderName'))?.trim();
    if (!name) return;
    submit.mutate({
      intent: {
        kind: 'create_library_folder',
        id: uuidv7(),
        library,
        parent: selected,
        name,
      },
    });
  };

  return (
    <div className="library-folders" aria-label={t('library.folders')}>
      <div className="library-folder-toolbar">
        <button
          type="button"
          aria-pressed={selected === null}
          onClick={() => {
            onSelect(null);
          }}
        >
          {t('library.root')}
        </button>
        <IconButton icon="folderPlus" label={t('library.addFolder')} onClick={create} />
      </div>
      {rows.map(({ folder, depth }) => (
        <div
          className="library-folder-row"
          key={folder.id}
          style={{ paddingInlineStart: `${8 + depth * 16}px` }}
        >
          <button
            type="button"
            className="library-folder-select"
            aria-pressed={selected === folder.id}
            onClick={() => {
              onSelect(folder.id);
            }}
          >
            {folder.name}
          </button>
          <IconButton
            icon="note"
            label={t('library.renameFolder', { name: folder.name })}
            onClick={() => {
              const name = window.prompt(t('library.folderName'), folder.name)?.trim();
              if (name && name !== folder.name)
                submit.mutate({
                  intent: { kind: 'rename_library_folder', id: folder.id, name },
                  expectedVersion: folder.version,
                });
            }}
          />
          <IconButton
            icon="trash"
            label={t('library.deleteFolder', { name: folder.name })}
            onClick={() => {
              const subtree = descendants(folder.id);
              const count = assets.filter(
                (asset) => asset.folder_id && subtree.has(asset.folder_id),
              ).length;
              if (window.confirm(t('library.confirmDeleteFolder', { name: folder.name, count })))
                submit.mutate({
                  intent: { kind: 'delete_library_folder', id: folder.id },
                  expectedVersion: folder.version,
                });
            }}
          />
        </div>
      ))}
    </div>
  );
}
