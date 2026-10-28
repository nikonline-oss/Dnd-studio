import { labelText } from '@/shared/i18n/label';
import type { CompendiumEntryDto, Label } from '@/shared/ipc/client';

/** `hit_dice` → `Hit dice`: field names as plugins write them, made readable. */
export function fieldName(key: string): string {
  const words = key.replace(/[_-]+/g, ' ').trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

function isLabel(value: unknown): value is Label {
  if (typeof value === 'string') return true;
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const keys = Object.keys(value);
  return keys.length > 0 && keys.every((k) => k === 'ru' || k === 'en');
}

/** Markdown can't break out of a value: no raw HTML is rendered anyway (rehype-sanitize). */
function inline(text: string): string {
  return text.replace(/([\\`*_[\]#|])/g, '\\$1');
}

function value(v: unknown, lang: string, depth: number): string {
  if (v === null || v === undefined) return '—';
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  if (isLabel(v)) return inline(labelText(v, lang));
  const indent = '  '.repeat(depth);
  if (Array.isArray(v)) {
    // A list of records (attacks, actions): one line each.
    return (v as unknown[])
      .map((item) => {
        if (typeof item === 'object' && item !== null && !Array.isArray(item) && !isLabel(item)) {
          const parts = Object.entries(item as Record<string, unknown>).map(
            ([k, x]) => `${fieldName(k)}: ${value(x, lang, depth + 1)}`,
          );
          return `\n${indent}- ${parts.join(', ')}`;
        }
        return `\n${indent}- ${value(item, lang, depth + 1)}`;
      })
      .join('');
  }
  // A nested record (ability scores): a compact line.
  return Object.entries(v as Record<string, unknown>)
    .map(([k, x]) => `${fieldName(k)} ${value(x, lang, depth + 1)}`)
    .join(' · ');
}

/**
 * A compendium entry as Markdown: its name, then each field. Any system's entries render the same
 * way (ADR-0005: the app knows no system's words); the journal keeps this text when the entry is
 * copied there.
 */
export function entryMarkdown(entry: CompendiumEntryDto, lang: string): string {
  const fields =
    typeof entry.data === 'object' && entry.data !== null && !Array.isArray(entry.data)
      ? Object.entries(entry.data as Record<string, unknown>)
      : [];
  const lines = fields.map(([k, v]) => `- **${fieldName(k)}:** ${value(v, lang, 1)}`);
  return `# ${inline(labelText(entry.name, lang))}\n\n${lines.join('\n')}\n`;
}
