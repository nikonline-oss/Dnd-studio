import type { Label } from '@/shared/ipc/client';

/** A plugin's text in `lang` (`ru…` or anything else as English), falling back to the other. */
export function labelText(label: Label, lang: string): string {
  if (typeof label === 'string') return label;
  const ru = lang.toLowerCase().startsWith('ru');
  return (ru ? (label.ru ?? label.en) : (label.en ?? label.ru)) ?? '';
}
