import { useEffect, useId, useRef, type ReactNode } from 'react';
import { IconButton } from './IconButton';

interface Props {
  title: string;
  closeLabel: string;
  onClose: () => void;
  children: ReactNode;
  /** Width: a short confirmation, a form, or a wide form with a preview. */
  size?: 'sm' | 'md' | 'lg';
  /** Action row pinned under the body; the body scrolls above it, the actions stay reachable. */
  footer?: ReactNode;
  /**
   * False while a request is in flight: the backdrop and Escape stop closing the dialog, so a
   * result cannot land in a component that is already gone.
   */
  dismissible?: boolean;
}

const SIZE_CLASS = { sm: 'dialog-sm', md: '', lg: 'dialog-lg' } as const;

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * A modal dialog: Escape or the close button closes it, the page behind does not scroll, focus moves
 * inside on open, Tab stays inside, and focus returns to the element that opened it.
 */
export function Dialog({
  title,
  closeLabel,
  onClose,
  children,
  size = 'md',
  footer,
  dismissible = true,
}: Props) {
  const titleId = useId();
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const body = document.body;
    const previousOverflow = body.style.overflow;
    body.style.overflow = 'hidden';
    box.current
      ?.querySelector<HTMLElement>(
        'input, textarea, select, button:not([data-close]), [tabindex="0"]',
      )
      ?.focus();
    return () => {
      body.style.overflow = previousOverflow;
      previous?.focus();
    };
  }, []);

  return (
    <div
      className="dialog-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && dismissible) onClose();
      }}
    >
      <div
        ref={box}
        className={`dialog ${SIZE_CLASS[size]}`.trim()}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onKeyDown={(e) => {
          if (e.key === 'Escape' && dismissible) {
            e.stopPropagation();
            onClose();
            return;
          }
          if (e.key !== 'Tab') return;
          // Focus trap: the rest of the application is inert while the dialog is open.
          const items = Array.from(box.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? []);
          const first = items[0];
          const last = items.at(-1);
          if (!first || !last) return;
          if (e.shiftKey && document.activeElement === first) {
            e.preventDefault();
            last.focus();
          } else if (!e.shiftKey && document.activeElement === last) {
            e.preventDefault();
            first.focus();
          }
        }}
      >
        <header className="dialog-header">
          <h2 id={titleId}>{title}</h2>
          <IconButton
            icon="x"
            label={closeLabel}
            onClick={onClose}
            disabled={!dismissible}
            data-close
          />
        </header>
        <div className="dialog-body">{children}</div>
        {footer && <footer className="dialog-footer">{footer}</footer>}
      </div>
    </div>
  );
}
