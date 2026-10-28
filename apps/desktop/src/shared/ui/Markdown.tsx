import ReactMarkdown, { defaultUrlTransform, type Components } from 'react-markdown';
import rehypeSanitize, { defaultSchema } from 'rehype-sanitize';
import remarkGfm from 'remark-gfm';
import { useSnapshotSelect } from '@/shared/game/useGame';
import { assetUrl } from '@/shared/ipc/client';

/** Pictures of the campaign's library: `![alt](asset:<asset id>)` (ADR-0024). */
const ASSET_PREFIX = 'asset:';

/** The GitHub-like schema, plus `asset:` image sources (served by hash from the campaign). */
const schema = {
  ...defaultSchema,
  protocols: {
    ...defaultSchema.protocols,
    src: [...(defaultSchema.protocols?.src ?? []), 'asset'],
  },
};

function urlTransform(url: string): string {
  return url.startsWith(ASSET_PREFIX) ? url : defaultUrlTransform(url);
}

/** A library picture; its alt text when the viewer may not see it (or it is gone). */
function AssetImage({ id, alt }: { id: string; alt: string }) {
  const asset = useSnapshotSelect((s) => s.assets.find((a) => a.id === id));
  if (!asset) return <span className="md-image-missing">{alt}</span>;
  return (
    <img
      className="md-image"
      src={assetUrl(asset.display_sha256 ?? asset.thumbnail_sha256)}
      alt={alt}
      loading="lazy"
    />
  );
}

/**
 * Renders untrusted Markdown (CLAUDE.md §9): raw HTML is not parsed, the HAST is sanitized with
 * rehype-sanitize's GitHub-like schema, and links never navigate the app's webview — they are
 * shown with their target as a tooltip (opening in the system browser comes later, ADR-0018).
 * Remote images are blocked by the CSP anyway; only the campaign's own pictures are shown.
 */
const components: Components = {
  a: ({ href, children }) => (
    <span className="md-link" title={href}>
      {children}
    </span>
  ),
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
        remarkPlugins={[remarkGfm]}
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
