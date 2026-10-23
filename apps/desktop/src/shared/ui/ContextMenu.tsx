import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { MenuItem } from './Menu';

interface Props {
  /** Where the pointer was (client px). */
  x: number;
  y: number;
  items: (MenuItem | 'separator')[];
  label: string;
  onClose: () => void;
}

/**
 * A right-click menu at the pointer: closes on selection, Escape, a click outside, scrolling or
 * the window losing focus. ↑/↓ move between items; the first one is focused on open.
 */
export function ContextMenu({ x, y, items, label, onClose }: Props) {
  const ref = useRef<HTMLUListElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });

  // Keep the whole menu on screen.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    setPos({
      left: Math.max(0, Math.min(x, window.innerWidth - width - 4)),
      top: Math.max(0, Math.min(y, window.innerHeight - height - 4)),
    });
    el.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
  }, [x, y]);

  useEffect(() => {
    const onPointer = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('pointerdown', onPointer);
    document.addEventListener('keydown', onKey);
    window.addEventListener('blur', onClose);
    window.addEventListener('wheel', onClose, { passive: true });
    return () => {
      document.removeEventListener('pointerdown', onPointer);
      document.removeEventListener('keydown', onKey);
      window.removeEventListener('blur', onClose);
      window.removeEventListener('wheel', onClose);
    };
  }, [onClose]);

  const move = (step: number) => {
    const buttons = [
      ...(ref.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? []),
    ];
    const n = buttons.indexOf(document.activeElement as HTMLButtonElement);
    buttons[(n + step + buttons.length) % buttons.length]?.focus();
  };

  return (
    <ul
      ref={ref}
      role="menu"
      aria-label={label}
      className="menu-list context-menu"
      style={{ left: pos.left, top: pos.top }}
      onKeyDown={(e) => {
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
          e.preventDefault();
          move(e.key === 'ArrowDown' ? 1 : -1);
        }
      }}
      onContextMenu={(e) => {
        e.preventDefault();
      }}
    >
      {items.map((item, n) =>
        item === 'separator' ? (
          <li key={`sep-${String(n)}`} role="separator" />
        ) : (
          <li key={item.label} role="none">
            <button
              type="button"
              role="menuitem"
              disabled={item.disabled}
              onClick={() => {
                onClose();
                item.onSelect();
              }}
            >
              <span className="menu-check" />
              {item.label}
            </button>
          </li>
        ),
      )}
    </ul>
  );
}
