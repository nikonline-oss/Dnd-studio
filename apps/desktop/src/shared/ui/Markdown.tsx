import type { ReactNode } from 'react';
import ReactMarkdown, { defaultUrlTransform, type Components } from 'react-markdown';
import rehypeSanitize, { defaultSchema } from 'rehype-sanitize';
import remarkGfm from 'remark-gfm';
import { InlineAudioPlayer } from './InlineAudioPlayer';
import { markdownLineBreaks } from './markdownLineBreaks';
import { useSnapshotSelect } from '@/shared/game/useGame';
import { assetUrl, type Snapshot } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

/** Pictures of the campaign's library: `![alt](asset:<asset id>)` (ADR-0024). */
const ASSET_PREFIX = 'asset:';

/** What a text may link to inside the campaign: `[text](journal:<id>)` and the like (ADR-0026). */
export type LinkKind = 'journal' | 'map' | 'character' | 'board';
const LINK_KINDS: readonly LinkKind[] = ['journal', 'map', 'character', 'board'];
const ELEMENT_LINK = /^(journal|map|character|board):([0-9a-f-]{36})$/;

/** `journal:<id>` → its kind and id; `null` for any other link. */
export function elementLink(href: string): { kind: LinkKind; id: string } | null {
  const match = ELEMENT_LINK.exec(href);
  if (!match) return null;
  const [, kind = '', id = ''] = match;
  return LINK_KINDS.includes(kind as LinkKind) ? { kind: kind as LinkKind, id } : null;
}

/** How a link to an element is written into a text. */
export function linkMarkdown(kind: LinkKind, id: string, text: string): string {
  return `[${text.replace(/[[\]\\]/g, '')}](${kind}:${id})`;
}

/** The GitHub-like schema, plus `asset:` image sources and links to the campaign's elements. */
const schema = {
  ...defaultSchema,
  protocols: {
    ...defaultSchema.protocols,
    src: [...(defaultSchema.protocols?.src ?? []), 'asset'],
    href: [...(defaultSchema.protocols?.href ?? []), ...LINK_KINDS, 'asset'],
  },
};

function urlTransform(url: string): string {
  return url.startsWith(ASSET_PREFIX) || elementLink(url) ? url : defaultUrlTransform(url);
}

/** A library picture; its alt text when the viewer may not see it (or it is gone). */
function AssetImage({ id, alt }: { id: string; alt: string }) {
  const asset = useSnapshotSelect((s) => s.assets.find((a) => a.id === id));
  if (!asset) return <span className="md-image-missing">{alt}</span>;
  if (asset.usage === 'audio')
    return (
      <InlineAudioPlayer
        key={asset.sha256}
        title={alt || asset.original_name || ''}
        src={assetUrl(asset.sha256)}
      />
    );
  return (
    <img
      className="md-image"
      src={assetUrl(asset.display_sha256 ?? asset.thumbnail_sha256)}
      alt={alt}
      loading="lazy"
    />
  );
}

function exists(s: Snapshot, kind: LinkKind, id: string): boolean {
  switch (kind) {
    case 'journal':
      return s.journal.some((e) => e.id === id);
    case 'map':
      return s.maps.some((m) => m.id === id);
    case 'character':
      return s.characters.some((c) => c.id === id);
    case 'board':
      return s.boards.some((b) => b.id === id);
  }
}

/**
 * A link to another journal entry, map, character or board: opens it in a tab. Something the
 * viewer may not see (or that is gone) stays plain text, so a link does not reveal it.
 */
function ElementLink({ kind, id, children }: { kind: LinkKind; id: string; children: ReactNode }) {
  const visible = useSnapshotSelect((s) => exists(s, kind, id)) ?? false;
  if (!visible) return <span className="md-link-missing">{children}</span>;
  const open = () => {
    const selection = useSelection.getState();
    if (kind === 'journal') selection.openJournal(id);
    else if (kind === 'map') selection.openMap(id);
    else if (kind === 'character') selection.openCharacter(id);
    else selection.openBoard(id);
  };
  return (
    <a
      className="md-element-link"
      href={`#${kind}-${id}`}
      onClick={(e) => {
        e.preventDefault();
        open();
      }}
    >
      {children}
    </a>
  );
}

/**
 * Renders untrusted Markdown (CLAUDE.md §9): raw HTML is not parsed, the HAST is sanitized with
 * rehype-sanitize's GitHub-like schema, and links never navigate the app's webview — links to the
 * campaign's own elements open them in a tab, other links are shown with their target as a
 * tooltip (opening in the system browser comes later, ADR-0018). Remote images are blocked by the
 * CSP anyway; only the campaign's own pictures are shown.
 */
const components: Components = {
  a: ({ href, children }) => {
    if (href?.startsWith(ASSET_PREFIX)) {
      return (
        <span className="md-media-attachment">
          <span>{children}</span>
          <AssetImage
            id={href.slice(ASSET_PREFIX.length)}
            alt={typeof children === 'string' ? children : ''}
          />
        </span>
      );
    }
    const link = typeof href === 'string' ? elementLink(href) : null;
    return link ? (
      <ElementLink kind={link.kind} id={link.id}>
        {children}
      </ElementLink>
    ) : (
      <span className="md-link" title={href}>
        {children}
      </span>
    );
  },
  img: ({ src, alt }) =>
    typeof src === 'string' && src.startsWith(ASSET_PREFIX) ? (
      <AssetImage id={src.slice(ASSET_PREFIX.length)} alt={alt ?? ''} />
    ) : (
      <span className="md-image-missing">{alt}</span>
    ),
};

export function Markdown({ source }: { source: string }) {
  return (
    <div className="markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, markdownLineBreaks]}
        rehypePlugins={[[rehypeSanitize, schema]]}
        components={components}
        urlTransform={urlTransform}
        skipHtml
      >
        {source}
      </ReactMarkdown>
    </div>
  );
}
