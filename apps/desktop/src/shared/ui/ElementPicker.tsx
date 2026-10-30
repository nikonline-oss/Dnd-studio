import { useMemo, useState, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Dialog } from './Dialog';
import { Icon, type IconName } from './Icon';
import { useSnapshotSelect } from '@/shared/game/useGame';
import { assetUrl, type Snapshot } from '@/shared/ipc/client';

/** What can be picked. */
export type ElementKind =
  | 'map'
  | 'journal'
  | 'character'
  | 'token'
  | 'board'
  | 'board_node'
  | 'zone'
  | 'asset'
  | 'audio'
  | 'trigger';

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
  audio: 'music',
  trigger: 'bolt',
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
      case 'trigger':
        return snapshot.triggers.map((tr) => ({
          kind,
          id: tr.id,
          name: tr.definition.name,
          detail: t(`triggers.events.${tr.definition.event.type}`),
          thumbnail: null,
        }));
      case 'asset':
      case 'audio':
        return snapshot.assets
          .filter((a) => (kind === 'audio' ? a.usage === 'audio' : a.usage !== 'audio'))
          .map((a) => ({
            kind,
            id: a.id,
            name: a.original_name ?? a.sha256.slice(0, 8),
            detail: a.usage === 'audio' ? a.mime : `${a.width}×${a.height}`,
            thumbnail: a.usage === 'audio' ? null : assetUrl(a.thumbnail_sha256),
          }));
    }
  })();
  return rows.sort((a, b) => a.name.localeCompare(b.name));
}

/** What the details pane shows about the highlighted element. */
interface Details {
  /** A large picture (map background, token, asset), if the element has one. */
  preview: string | null;
  facts: { label: string; value: string }[];
}

/** The facts that matter about one element, per kind. */
function detailsOf(
  snapshot: Snapshot,
  row: Row,
  t: (key: string) => string,
  language: string,
): Details {
  const picture = (assetId: string | null) => {
    const asset = assetId ? snapshot.assets.find((a) => a.id === assetId) : undefined;
    return asset ? assetUrl(asset.display_sha256 ?? asset.thumbnail_sha256) : null;
  };
  const mapName = (id: string) => snapshot.maps.find((m) => m.id === id)?.name ?? '';
  const yesNo = (value: boolean) => (value ? t('picker.yes') : t('picker.no'));
  const fact = (key: string, value: string) => ({ label: t(`picker.fact.${key}`), value });
  switch (row.kind) {
    case 'map': {
      const m = snapshot.maps.find((x) => x.id === row.id);
      if (!m) break;
      const grid =
        m.grid.kind === 'none'
          ? t('inspector.gridKind.none')
          : `${t(`inspector.gridKind.${m.grid.kind}`)}, ${m.grid.cell_size}`;
      return {
        preview: picture(m.background_asset_id),
        facts: [
          fact('size', `${m.width}×${m.height}`),
          fact('grid', grid),
          fact('players', yesNo(m.visible_to_players)),
        ],
      };
    }
    case 'journal': {
      const e = snapshot.journal.find((x) => x.id === row.id);
      if (!e) break;
      const folder = snapshot.journal_folders.find((f) => f.id === e.folder_id)?.name;
      return {
        preview: null,
        facts: [
          fact('folder', folder ?? t('picker.fact.root')),
          fact('visibility', t(`journal.visibility.${e.visibility}`)),
        ],
      };
    }
    case 'character': {
      const c = snapshot.characters.find((x) => x.id === row.id);
      if (!c) break;
      return {
        preview: null,
        facts: [
          fact('type', t(`characters.kinds.${c.kind}`)),
          fact('owner', snapshot.users.find((u) => u.id === c.owner)?.name ?? t('picker.npc')),
          fact('status', t(`characters.status.${c.status}`)),
        ],
      };
    }
    case 'token': {
      const tok = snapshot.tokens.find((x) => x.id === row.id);
      if (!tok) break;
      return {
        preview: picture(tok.asset_id),
        facts: [
          fact('map', mapName(tok.map_id)),
          fact('layer', t(`tokens.layers.${tok.layer}`)),
          fact('players', yesNo(tok.visible)),
        ],
      };
    }
    case 'board': {
      const b = snapshot.boards.find((x) => x.id === row.id);
      if (!b) break;
      return { preview: null, facts: [fact('players', yesNo(b.visible_to_players))] };
    }
    case 'board_node': {
      const n = snapshot.board_nodes.find((x) => x.id === row.id);
      if (!n) break;
      return {
        preview: n.content.type === 'image' ? picture(n.content.asset_id) : null,
        facts: [
          fact('board', snapshot.boards.find((b) => b.id === n.board_id)?.name ?? ''),
          fact('type', t(`picker.cardType.${n.content.type}`)),
          fact('players', yesNo(n.visibility === 'players')),
        ],
      };
    }
    case 'zone': {
      const z = snapshot.zones.find((x) => x.id === row.id);
      if (!z) break;
      const facts = [fact('map', mapName(z.map_id))];
      if (z.shape.type === 'union') facts.push(fact('parts', String(z.shape.parts.length)));
      return { preview: null, facts };
    }
    case 'trigger': {
      const tr = snapshot.triggers.find((x) => x.id === row.id);
      if (!tr) break;
      const steps = tr.definition.steps.map((s) => t(`triggers.actions.${s.action.type}`));
      return {
        preview: null,
        facts: [
          fact('event', t(`triggers.events.${tr.definition.event.type}`)),
          fact('steps', steps.length > 0 ? steps.join(' → ') : t('picker.fact.noSteps')),
        ],
      };
    }
    case 'asset':
    case 'audio': {
      const a = snapshot.assets.find((x) => x.id === row.id);
      if (!a) break;
      return {
        preview: a.usage === 'audio' ? null : picture(a.id),
        facts: [
          fact('size', `${a.width}×${a.height}, ${formatBytes(a.size_bytes, language)}`),
          fact('file', a.original_name ?? a.sha256.slice(0, 12)),
        ],
      };
    }
  }
  return { preview: null, facts: [] };
}

function formatBytes(bytes: number, language: string): string {
  const [value, unit] =
    bytes < 1024 * 1024 ? [bytes / 1024, 'kilobyte'] : [bytes / (1024 * 1024), 'megabyte'];
  return new Intl.NumberFormat(language, {
    style: 'unit',
    unit,
    maximumFractionDigits: unit === 'kilobyte' ? 0 : 1,
  }).format(value);
}

/** The highlighted element: a large picture, its name and kind, and what matters about it. */
function DetailsPane({ snapshot, row }: { snapshot: Snapshot | undefined; row: Row | null }) {
  const { t, i18n } = useTranslation();
  if (!snapshot || !row) {
    return (
      <aside className="picker-details muted" aria-label={t('picker.details')}>
        {t('picker.noSelection')}
      </aside>
    );
  }
  const { preview, facts } = detailsOf(snapshot, row, t, i18n.language);
  const audio = row.kind === 'audio' ? snapshot.assets.find((a) => a.id === row.id) : undefined;
  return (
    <aside className="picker-details" aria-label={t('picker.details')}>
      <div className="picker-preview">
        {preview ? <img src={preview} alt="" /> : <Icon name={ICONS[row.kind]} size={48} />}
      </div>
      <h3 className="picker-details-name">{row.name}</h3>
      {audio && (
        <audio
          key={audio.id}
          aria-label={row.name}
          controls
          preload="metadata"
          src={assetUrl(audio.sha256)}
        />
      )}
      <p className="muted">{t(`picker.kind.${row.kind}`)}</p>
      <dl>
        {facts.map((f) => (
          <div key={f.label}>
            <dt>{f.label}</dt>
            <dd>{f.value}</dd>
          </div>
        ))}
      </dl>
    </aside>
  );
}

/**
 * A selection form (as in 1C): element kinds on the left, a searchable list in the middle and
 * the highlighted element's details on the right; a
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
        <DetailsPane snapshot={snapshot} row={current} />
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

/** The name of an element, or `null` when it is gone (or hidden from this viewer). */
export function useElementName(kind: ElementKind, id: string | null): string | null {
  const { t } = useTranslation();
  const snapshot = useSnapshotSelect((s) => s);
  if (!snapshot || id === null) return null;
  return rowsOf(snapshot, kind, t).find((r) => r.id === id)?.name ?? null;
}

/** A form field holding one element: its name and a "Pick…" button opening the selection form. */
export function ElementField({
  label,
  kinds,
  value,
  onChange,
}: {
  label: string;
  kinds: ElementKind[];
  value: { kind: ElementKind; id: string } | null;
  onChange: (element: PickedElement) => void;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const name = useElementName(value?.kind ?? kinds[0] ?? 'map', value?.id ?? null);
  return (
    <div className="element-field">
      <span className="element-field-label">{label}</span>
      <span className="element-field-value" data-empty={name === null}>
        {value && <Icon name={ICONS[value.kind]} size={14} />}
        {name ?? (value ? t('picker.gone') : t('picker.none'))}
      </span>
      <button
        type="button"
        aria-label={t('picker.pickFor', { label })}
        onClick={() => {
          setOpen(true);
        }}
      >
        {t('picker.pick')}
      </button>
      {open && (
        <ElementPicker
          title={label}
          kinds={kinds}
          onPick={onChange}
          onClose={() => {
            setOpen(false);
          }}
        />
      )}
    </div>
  );
}
