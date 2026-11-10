import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { Dialog } from './Dialog';

function renderDialog(props: Partial<Parameters<typeof Dialog>[0]> = {}) {
  const onClose = vi.fn();
  render(
    <Dialog title="Заголовок" closeLabel="Закрыть" onClose={onClose} {...props}>
      <input aria-label="Первое" />
      <input aria-label="Второе" />
    </Dialog>,
  );
  return { onClose };
}

describe('Dialog', () => {
  it('moves focus inside, returns it to the opener, and locks the page scroll', async () => {
    const opener = document.createElement('button');
    opener.textContent = 'open';
    document.body.append(opener);
    opener.focus();
    const { onClose } = renderDialog();
    expect(screen.getByLabelText('Первое')).toHaveFocus();
    expect(document.body.style.overflow).toBe('hidden');
    await userEvent.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByRole('button', { name: 'Закрыть' }));
    expect(onClose).toHaveBeenCalledTimes(2);
    opener.remove();
  });

  it('keeps Tab inside the dialog', async () => {
    renderDialog();
    const first = screen.getByLabelText('Первое');
    const last = screen.getByRole('button', { name: 'Закрыть' });
    expect(first).toHaveFocus();
    // Forward from the last control wraps to the first.
    last.focus();
    await userEvent.keyboard('{Tab}');
    expect(first).toHaveFocus();
    // Backwards from the first wraps to the last.
    await userEvent.keyboard('{Shift>}{Tab}{/Shift}');
    expect(last).toHaveFocus();
  });

  it('does not close while a request is in flight', async () => {
    const { onClose } = renderDialog({ dismissible: false });
    await userEvent.keyboard('{Escape}');
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Закрыть' })).toBeDisabled();
  });

  it('renders a pinned footer for its actions', () => {
    renderDialog({ footer: <button type="button">Сохранить</button> });
    const footer = screen.getByRole('button', { name: 'Сохранить' }).closest('footer');
    expect(footer).not.toBeNull();
    // Actions must not scroll away with a long body.
    expect(footer?.className).toBe('dialog-footer');
  });

  it('exposes its sizes for narrow confirmations and wide forms', () => {
    const { unmount } = render(
      <Dialog title="A" closeLabel="B" onClose={() => {}} size="lg">
        <p>wide</p>
      </Dialog>,
    );
    expect(screen.getByRole('dialog').className).toContain('dialog-lg');
    unmount();
    render(
      <Dialog title="A" closeLabel="B" onClose={() => {}} size="sm">
        <p>narrow</p>
      </Dialog>,
    );
    expect(screen.getByRole('dialog').className).toContain('dialog-sm');
  });
});
