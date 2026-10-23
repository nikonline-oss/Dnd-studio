import { useEffect, useId, useRef, type ReactNode } from 'react';
import { IconButton } from './IconButton';

interface Props {
  title: string;
  closeLabel: string;
  onClose: () => void;
  children: ReactNode;
}

/** A modal dialog: Escape or the close button closes it; focus moves inside on open. */
export function Dialog({ title, closeLabel, onClose, children }: Props) {
  const titleId = useId();
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    box.current
      ?.querySelector<HTMLElement>('input, button:not([data-close]), [tabindex="0"]')
      ?.focus();
    return () => {
      previous?.focus();
    };
  }, []);

  return (
    <div
      className="dialog-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={box}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.stopPropagation();
            onClose();
          }
        }}
      >
        <header className="dialog-header">
          <h2 id={titleId}>{title}</h2>
          <IconButton icon="x" label={closeLabel} onClick={onClose} data-close />
        </header>
        <div className="dialog-body">{children}</div>
      </div>
    </div>
  );
}
