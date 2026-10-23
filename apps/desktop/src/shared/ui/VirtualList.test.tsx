import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { VirtualList } from './VirtualList';

const items = Array.from({ length: 5000 }, (_, n) => ({
  id: `e${String(n)}`,
  title: `Entry ${String(n)}`,
}));

function List({ scrollToIndex = null }: { scrollToIndex?: number | null }) {
  return (
    <VirtualList
      items={items}
      rowHeight={30}
      getKey={(item) => item.id}
      renderRow={(item) => <li>{item.title}</li>}
      label="Entries"
      scrollToIndex={scrollToIndex}
    />
  );
}

describe('VirtualList', () => {
  it('renders only the rows in view plus overscan, sized for the whole list', () => {
    render(<List />);
    const list = screen.getByRole('list', { name: 'Entries' });
    // 800 px fallback / 30 px rows = 27 visible + 8 overscan.
    expect(screen.getAllByRole('listitem')).toHaveLength(35);
    expect(list).toHaveStyle({ height: `${String(5000 * 30)}px` });
    expect(screen.getByText('Entry 0')).toBeInTheDocument();
    expect(screen.queryByText('Entry 100')).not.toBeInTheDocument();
  });

  it('renders the rows at the scroll position', () => {
    render(<List />);
    const box = screen.getByRole('list', { name: 'Entries' }).parentElement;
    if (!box) throw new Error('no scroll box');
    fireEvent.scroll(box, { target: { scrollTop: 3000 * 30 } });
    expect(screen.getByText('Entry 3000')).toBeInTheDocument();
    expect(screen.queryByText('Entry 0')).not.toBeInTheDocument();
    // Rows before the window are replaced by padding, so positions stay right.
    expect(screen.getByRole('list', { name: 'Entries' })).toHaveStyle({
      paddingTop: `${String((3000 - 8) * 30)}px`,
    });
  });

  it('scrolls a row into view for keyboard navigation', () => {
    const { rerender } = render(<List />);
    const box = screen.getByRole('list', { name: 'Entries' }).parentElement;
    if (!box) throw new Error('no scroll box');
    rerender(<List scrollToIndex={1000} />);
    // jsdom has no layout (clientHeight 0): the row's bottom edge becomes the scroll position.
    expect(box.scrollTop).toBe(1000 * 30 + 30);
  });
});
