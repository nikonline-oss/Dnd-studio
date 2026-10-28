import { useMemo, useState, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Dialog } from './Dialog';
import { Icon, type IconName } from './Icon';
import { useSnapshotSelect } from '@/shared/game/useGame';
import { assetUrl, type Snapshot } from '@/shared/ipc/client';

/** What can be picked. */
export type ElementKind =
  'map' | 'journal' | 'character' | 'token' | 'board' | 'board_node' | 'zone' | 'asset';

export interface PickedElement {
  kind: ElementKind;
  id: string;
  name: string;
}

interface Row extends PickedElement {
  /** Second column: where it is or what it is. */
  detail: string;
  /** A small picture, if the element has one. */
  thumbnail: string | null;
}

const ICONS: Record<ElementKind, IconName> = {
  map: 'map',
  journal: 'book',
  character: 'user',
  token: 'users',
  board: 'board',
  board_node: 'note',
  zone: 'zone',
  asset: 'image',
};

/** The elements of `kind` in the snapshot, by name. */
function rowsOf(snapshot: Snapshot, kind: ElementKind, t: (key: string) => string): Row[] {
  const mapName = (id: string) => snapshot.maps.find((m) => m.id === id)?.name ?? '';
  const thumb = (assetId: string | null) => {
    const asset = assetId ? snapshot.assets.find((a) => a.id === assetId) : undefined;
    return asset ? assetUrl(asset.thumbnail_sha256) : null;
  };
  const rows: Row[] = (() => {
    switch (kind) {
      case 'map':
        return snapshot.maps.map((m) => ({
          kind,
          id: m.id,
          name: m.name,
          detail: m.visible_to_players ? t('picker.shown') : t('picker.hidden'),
          thumbnail: thumb(m.background_asset_id),
        }));
      case 'journal': {
        const folder = (id: string | null) =>
          snapshot.journal_folders.find((f) => f.id === id)?.name ?? '';
        return snapshot.journal.map((e) => ({
          kind,
          id: e.id,
          name: e.title,
          detail: folder(e.folder_id),
          thumbnail: null,
        }));
      }
      case 'character':
        return snapshot.characters.map((c) => ({
          kind,
          id: c.id,
          name: c.name,
          detail: snapshot.users.find((u) => u.id === c.owner)?.name ?? t('picker.npc'),
          thumbnail: null,
        }));
      case 'token':
        return snapshot.tokens.map((tok) => ({
          kind,
          id: tok.id,
          name: tok.name || t('picker.unnamed'),
          detail: mapName(tok.map_id),
          thumbnail: thumb(tok.asset_id),
        }));
      case 'board':
        return snapshot.boards.map((b) => ({
          kind,
          id: b.id,
          name: b.name,
          detail: b.visible_to_players ? t('picker.shown') : t('picker.hidden'),
          thumbnail: null,
        }));
      case 'board_node':
        return snapshot.board_nodes.map((n) => {
          const board = snapshot.boards.find((b) => b.id === n.board_id)?.name ?? '';
          const content = n.content;
          const name =
            content.type === 'note'
              ? content.markdown
                  .replace(/[#*_`>]/g, '')
                  .trim()
                  .split('\n')[0] || t('picker.emptyNote')
              : content.type === 'image'
                ? t('picker.image')
                : t('picker.link');
          return { kind, id: n.id, name, detail: board, thumbnail: null };
        });
      case 'zone':
        return snapshot.zones.map((z) => ({
          kind,
          id: z.id,
          name: z.name,
          detail: mapName(z.map_id),
          thumbnail: null,
        }));
      case 'asset':
        return snapshot.assets.map((a) => ({
          kind,
          id: a.id,
          name: a.original_name ?? a.sha256.slice(0, 8),
          detail: `${a.width}×${a.height}`,
          thumbnail: assetUrl(a.thumbnail_sha256),
        }));
    }
  })();
  return rows.sort((a, b) => a.name.localeCompare(b.name));
}

/**
 * A selection form (as in 1C): element kinds on the left, a searchable list on the right; a
 * double click, Enter or "Select" picks the highlighted element.
 */
export function ElementPicker({
  title,
  kinds,
  onPick,
  onClose,
}: {
  title: string;
  kinds: ElementKind[];
  onPick: (element: PickedElement) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const snapshot = useSnapshotSelect((s) => s);
  const [kind, setKind] = useState<ElementKind>(kinds[0] ?? 'map');
  const [query, setQuery] = useState('');
  const [highlighted, setHighlighted] = useState<string | null>(null);

  const all = useMemo(() => (snapshot ? rowsOf(snapshot, kind, t) : []), [snapshot, kind, t]);
  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q
      ? all.filter((r) => r.name.toLowerCase().includes(q) || r.detail.toLowerCase().includes(q))
      : all;
  }, [all, query]);
  const current = rows.find((r) => r.id === highlighted) ?? null;

  const pick = (row: Row | null) => {
    if (!row) return;
    onPick({ kind: row.kind, id: row.id, name: row.name });
    onClose();
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const index = rows.findIndex((r) => r.id === highlighted);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const next =
        rows[e.key === 'ArrowDown' ? Math.min(index + 1, rows.length - 1) : Math.max(index - 1, 0)];
      if (next) setHighlighted(next.id);
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      pick(current ?? rows[0] ?? null);
    }
  };

  return (
    <Dialog title={title} closeLabel={t('picker.cancel')} onClose={onClose}>
      <div className="picker" onKeyDown={onKeyDown}>
        {kinds.length > 1 && (
          <nav className="picker-kinds" aria-label={t('picker.kinds')}>
            {kinds.map((k) => (
              <button
                key={k}
                type="button"
                aria-pressed={k === kind}
                onClick={() => {
                  setKind(k);
                  setHighlighted(null);
                }}
              >
                <Icon name={ICONS[k]} />
                {t(`picker.kind.${k}`)}
              </button>
            ))}
          </nav>
        )}
        <div className="picker-main">
          <input
            type="search"
            aria-label={t('picker.search')}
            placeholder={t('picker.search')}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
            }}
          />
          <ul className="picker-list" role="listbox" aria-label={t(`picker.kind.${kind}`)}>
            {rows.length === 0 && <li className="muted">{t('picker.nothing')}</li>}
            {rows.map((row) => (
              <li
                key={row.id}
                role="option"
                aria-selected={row.id === highlighted}
                onClick={() => {
                  setHighlighted(row.id);
                }}
                onDoubleClick={() => {
                  pick(row);
                }}
              >
                {row.thumbnail ? (
                  <img src={row.thumbnail} alt="" className="picker-thumb" />
                ) : (
                  <Icon name={ICONS[row.kind]} className="picker-thumb" />
                )}
                <span className="picker-name">{row.name}</span>
                <span className="picker-detail muted">{row.detail}</span>
              </li>
            ))}
          </ul>
        </div>
      </div>
      <footer className="dialog-actions">
        <button type="button" onClick={onClose}>
          {t('picker.cancel')}
        </button>
        <button
          type="button"
          className="btn-primary"
          disabled={!current}
          onClick={() => {
            pick(current);
          }}
        >
          {t('picker.select')}
        </button>
      </footer>
    </Dialog>
  );
}
