import {
  Fragment,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type HTMLAttributes,
  type ReactNode,
  type Ref,
} from 'react';

interface VirtualListProps<T> {
  items: readonly T[];
  /** Every row is exactly this tall (CSS px); rows get it through `--row-h`. */
  rowHeight: number;
  getKey: (item: T) => string;
  /** Renders one `<li>`. */
  renderRow: (item: T, index: number) => ReactNode;
  /** Accessible name of the list. */
  label?: string;
  className?: string;
  /** Rows rendered above and below the visible ones. */
  overscan?: number;
  /** Keeps this row in view when it changes (keyboard navigation). */
  scrollToIndex?: number | null;
  /** Extra attributes of the `<ul>` (a tree role, keyboard handling, focus). */
  listProps?: HTMLAttributes<HTMLUListElement> & { ref?: Ref<HTMLUListElement> };
}

/** Height of a navigator row (`.nav-row`). */
export const NAV_ROW_HEIGHT = 30;

/** Until the real size is known (and in tests, where nothing is laid out). */
const FALLBACK_HEIGHT = 800;

/**
 * A list that renders only the rows in view (plus `overscan`), so thousands of journal entries or
 * tokens cost what a screenful does. The list scrolls itself: its container must have a bounded
 * height (flex child with `min-height: 0`, or a `max-height`).
 */
export function VirtualList<T>({
  items,
  rowHeight,
  getKey,
  renderRow,
  label,
  className,
  overscan = 8,
  scrollToIndex = null,
  listProps,
}: VirtualListProps<T>) {
  const box = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(FALLBACK_HEIGHT);

  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => {
      if (el.clientHeight > 0) setHeight(el.clientHeight);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => {
      observer.disconnect();
    };
  }, []);

  useEffect(() => {
    const el = box.current;
    if (!el || scrollToIndex === null || scrollToIndex < 0) return;
    const top = scrollToIndex * rowHeight;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + rowHeight > el.scrollTop + el.clientHeight) {
      el.scrollTop = top + rowHeight - el.clientHeight;
    }
  }, [scrollToIndex, rowHeight]);

  const first = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const last = Math.min(items.length, Math.ceil((scrollTop + height) / rowHeight) + overscan);
  const style = { '--row-h': `${String(rowHeight)}px` } as CSSProperties;

  return (
    <div
      ref={box}
      className={className ? `virtual-list ${className}` : 'virtual-list'}
      style={style}
      onScroll={(e) => {
        setScrollTop(e.currentTarget.scrollTop);
      }}
    >
      <ul
        {...listProps}
        className="nav-list"
        aria-label={label}
        style={{ height: items.length * rowHeight, paddingTop: first * rowHeight }}
      >
        {items.slice(first, last).map((item, n) => (
          <Fragment key={getKey(item)}>{renderRow(item, first + n)}</Fragment>
        ))}
      </ul>
    </div>
  );
}
