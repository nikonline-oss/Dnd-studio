import { useState } from 'react';

interface Props {
  value: string;
  label: string;
  onCommit: (value: string) => void;
}

/**
 * Text that turns into an input on double click. Enter/blur commits, Escape cancels.
 * (Replaces `window.prompt`, which some webviews do not implement.)
 */
export function InlineEdit({ value, label, onCommit }: Props) {
  const [draft, setDraft] = useState<string | null>(null);

  if (draft === null) {
    return (
      <span
        className="inline-edit"
        title={label}
        onDoubleClick={() => {
          setDraft(value);
        }}
      >
        {value}
      </span>
    );
  }

  const commit = () => {
    const next = draft.trim();
    setDraft(null);
    if (next !== '' && next !== value) onCommit(next);
  };

  return (
    <input
      aria-label={label}
      autoFocus
      value={draft}
      maxLength={200}
      onChange={(e) => {
        setDraft(e.target.value);
      }}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === 'Enter') commit();
        if (e.key === 'Escape') setDraft(null);
      }}
    />
  );
}
