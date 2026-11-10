import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { ErrorText } from './ErrorText';
import { IpcFailure } from '@/shared/ipc/client';

it('offers safe query retry, recovery advice and an expandable error code', () => {
  const retry = vi.fn();
  const error = new IpcFailure({
    kind: 'recoverable',
    code: 'server_unreachable',
    message: 'private transport detail',
  });
  render(<ErrorText error={error} onRetry={retry} />);
  expect(screen.getByText(/Check your connection/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
  expect(retry).toHaveBeenCalledOnce();
  expect(screen.queryByText('private transport detail')).not.toBeInTheDocument();
  fireEvent.click(screen.getByText('Details'));
  expect(screen.getByText('server_unreachable')).toBeInTheDocument();
});
