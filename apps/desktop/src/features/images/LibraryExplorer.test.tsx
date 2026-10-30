import { createEvent, fireEvent, render, screen } from '@testing-library/react';
import { mockConvertFileSrc } from '@tauri-apps/api/mocks';
import { describe, expect, it, vi } from 'vitest';
import { LibraryExplorer } from './LibraryExplorer';
import type { AssetDto, Library } from '@/shared/ipc/client';

const { mutate } = vi.hoisted(() => ({ mutate: vi.fn() }));
vi.mock('@/shared/game/useGame', () => ({
  useSnapshotSelect: () => [
    { id: 'folder', library: 'images', name: 'Pictures', parent: null, version: 2 },
    { id: 'audio-folder', library: 'audio', name: 'Sounds', parent: null, version: 2 },
  ],
  useSubmitIntent: () => ({ mutate }),
}));

function setup(library: Library) {
  mockConvertFileSrc('windows');
  mutate.mockClear();
  const asset: AssetDto = {
    id: 'asset',
    usage: library === 'audio' ? 'audio' : 'token',
    sha256: 'a'.repeat(64),
    mime: library === 'audio' ? 'audio/mpeg' : 'image/webp',
    size_bytes: 10,
    original_name: 'Source',
    width: 0,
    height: 0,
    display_sha256: null,
    thumbnail_sha256: 'a'.repeat(64),
    tiles: [],
    folder_id: null,
    version: 3,
  };
  const view = render(
    <LibraryExplorer library={library} assets={[asset]} selected={null} onSelect={vi.fn()} />,
  );
  const source = screen.getByRole('treeitem', { name: 'Source' });
  const target = screen.getByRole('treeitem', {
    name: library === 'audio' ? 'Sounds' : 'Pictures',
  });
  const root = view.container.querySelector('.library-tree-root');
  if (!root) throw new Error('missing library root');
  const dataTransfer = { setData: vi.fn(), effectAllowed: 'none', dropEffect: 'none' };
  fireEvent.dragStart(source, { dataTransfer });
  return { ...view, root, source, target, dataTransfer };
}

function leave(target: Element, relatedTarget: Element | null) {
  const event = createEvent.dragLeave(target);
  Object.defineProperty(event, 'relatedTarget', { value: relatedTarget });
  fireEvent(target, event);
}

it('opens a folder from its row while toggling expansion, but not from its context menu', () => {
  const onOpenFolder = vi.fn();
  render(
    <LibraryExplorer
      library="images"
      assets={[]}
      selected={null}
      onSelect={vi.fn()}
      onOpenFolder={onOpenFolder}
    />,
  );
  const folder = screen.getByRole('treeitem', { name: 'Pictures' });
  expect(folder).toHaveAttribute('aria-expanded', 'true');
  fireEvent.click(screen.getByText('Pictures'));
  expect(folder).toHaveAttribute('aria-expanded', 'false');
  expect(onOpenFolder).toHaveBeenCalledExactlyOnceWith('folder');
  fireEvent.contextMenu(folder);
  expect(onOpenFolder).toHaveBeenCalledTimes(1);
  expect(folder).toHaveAttribute('aria-expanded', 'false');
});

describe.each<Library>(['images', 'audio'])('%s library drag cleanup', (library) => {
  it('highlights only the nearest target and clears all outlines after a bubbling drop', () => {
    const { container, root, target, dataTransfer } = setup(library);
    fireEvent.dragOver(target, { dataTransfer });
    expect(target).toHaveAttribute('data-drop', 'true');
    expect(root).not.toHaveAttribute('data-drop', 'true');
    fireEvent.drop(target, { dataTransfer });
    expect(mutate).toHaveBeenCalledTimes(1);
    expect(mutate).toHaveBeenCalledWith({
      intent: {
        kind: 'move_asset',
        id: 'asset',
        folder_id: library === 'audio' ? 'audio-folder' : 'folder',
      },
      expectedVersion: 3,
    });
    expect(container.querySelector('[data-drop="true"]')).toBeNull();
  });

  it('moves the outline between targets and clears it when leaving an asset row', () => {
    const { container, root, source, target, dataTransfer } = setup(library);
    fireEvent.dragOver(target, { dataTransfer });
    fireEvent.dragOver(source, { dataTransfer });
    expect(target).not.toHaveAttribute('data-drop', 'true');
    expect(source).toHaveAttribute('data-drop', 'true');
    leave(source, root);
    expect(container.querySelector('[data-drop="true"]')).toBeNull();
  });

  it('keeps the target highlighted when crossing its children', () => {
    const { target, dataTransfer } = setup(library);
    fireEvent.dragOver(target, { dataTransfer });
    leave(target, target.querySelector('.row-name'));
    expect(target).toHaveAttribute('data-drop', 'true');
  });

  it('clears a folder drag on dragend and does not treat its own row as a root drop', () => {
    const { container, root, target, dataTransfer } = setup(library);
    fireEvent.dragEnd(screen.getByRole('treeitem', { name: 'Source' }));
    fireEvent.dragStart(target, { dataTransfer });
    fireEvent.dragOver(root, { dataTransfer });
    expect(root).toHaveAttribute('data-drop', 'true');
    fireEvent.dragOver(target, { dataTransfer });
    expect(container.querySelector('[data-drop="true"]')).toBeNull();
    fireEvent.drop(target, { dataTransfer });
    expect(mutate).not.toHaveBeenCalled();
    fireEvent.dragStart(target, { dataTransfer });
    fireEvent.dragOver(root, { dataTransfer });
    fireEvent.dragEnd(target, { dataTransfer });
    expect(container.querySelector('[data-drop="true"]')).toBeNull();
  });

  it.each(['dragend', 'escape', 'blur', 'outside-drop'])(
    'clears the outline and stale drag payload after %s',
    (ending) => {
      const { container, source, target, dataTransfer } = setup(library);
      fireEvent.dragOver(target, { dataTransfer });
      if (ending === 'dragend') fireEvent.dragEnd(source, { dataTransfer });
      if (ending === 'escape') fireEvent.keyDown(window, { key: 'Escape' });
      if (ending === 'blur') fireEvent.blur(window);
      if (ending === 'outside-drop') fireEvent.drop(document.body, { dataTransfer });
      expect(container.querySelector('[data-drop="true"]')).toBeNull();
      fireEvent.dragOver(target, { dataTransfer });
      fireEvent.drop(target, { dataTransfer });
      expect(mutate).not.toHaveBeenCalled();
    },
  );
});
