import ReactMarkdown, { type Components } from 'react-markdown';
import rehypeSanitize from 'rehype-sanitize';
import remarkGfm from 'remark-gfm';

/**
 * Renders untrusted Markdown (CLAUDE.md §9): raw HTML is not parsed, the HAST is sanitized with
 * rehype-sanitize's GitHub-like schema, and links never navigate the app's webview — they are
 * shown with their target as a tooltip (opening in the system browser comes later, ADR-0018).
 * Remote images are blocked by the CSP anyway.
 */
const components: Components = {
  a: ({ href, children }) => (
    <span className="md-link" title={href}>
      {children}
    </span>
  ),
};

export function Markdown({ source }: { source: string }) {
  return (
    <div className="markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeSanitize]}
        components={components}
        skipHtml
      >
        {source}
      </ReactMarkdown>
    </div>
  );
}
