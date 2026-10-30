import { beforeEach, describe, expect, it } from 'vitest';
import { useSelection, type TabRef } from './selection';

const map = (id: string): TabRef => ({ kind: 'map', id });
const entry = (id: string): TabRef => ({ kind: 'journal', id });
const state = () => useSelection.getState();

beforeEach(() => {
  state().reset();
});

describe('center tabs', () => {
  it('opens one media tab per folder and changes the selected file inside it', () => {
    state().openMedia('images', 'folder', 'a');
    state().openMedia('images', 'folder', 'b');
    expect(state().tabs).toEqual([{ kind: 'images', id: 'folder' }]);
    expect(state().mediaSelection['images:folder']).toBe('b');
    state().openMedia('audio', null, 'song');
    expect(state().active).toEqual({ kind: 'audio', id: 'root' });
    state().reset();
    expect(state().mediaSelection).toEqual({});
  });
  it('opening adds a tab once and brings it to the front', () => {
    state().openMap('m1');
    state().openJournal('j1');
    state().openMap('m1');
    expect(state().tabs).toEqual([map('m1'), entry('j1')]);
    expect(state().active).toEqual(map('m1'));
    expect(state().center).toBe('map');
  });

  it('a journal tab in front keeps the last map for the map tools', () => {
    state().openMap('m1');
    state().openJournal('j1');
    expect(state()).toMatchObject({ center: 'journal', journalId: 'j1', mapId: 'm1' });
    // Switching back to the map does not close the journal tab.
    state().activate(map('m1'));
    expect(state().tabs).toEqual([map('m1'), entry('j1')]);
    expect(state()).toMatchObject({ center: 'map', journalId: null, mapId: 'm1' });
  });

  it('closing the front tab brings the right neighbour, else the left one', () => {
    state().openMap('m1');
    state().openMap('m2');
    state().openMap('m3');
    state().activate(map('m2'));
    state().closeTab(map('m2'));
    expect(state().active).toEqual(map('m3'));
    state().closeTab(map('m3'));
    expect(state().active).toEqual(map('m1'));
    state().closeTab(map('m1'));
    expect(state()).toMatchObject({ tabs: [], active: null, mapId: null, center: 'map' });
  });

  it('closing a background tab keeps the front one', () => {
    state().openMap('m1');
    state().openJournal('j1');
    state().closeTab(map('m1'));
    expect(state().active).toEqual(entry('j1'));
    // The closed map is no longer the map the tools work on.
    expect(state().mapId).toBeNull();
  });

  it('prunes tabs of deleted maps and entries', () => {
    state().openMap('m1');
    state().openJournal('j1');
    state().openMap('m2');
    state().pruneTabs((tab) => tab.id !== 'm2');
    expect(state().tabs).toEqual([map('m1'), entry('j1')]);
    expect(state().active).toEqual(entry('j1'));
    expect(state().mapId).toBe('m1');
    const before = state();
    state().pruneTabs(() => true);
    expect(state()).toBe(before);
  });

  it('opening a map shows it in the inspector; reset closes everything', () => {
    state().selectCharacter('c1');
    expect(state().inspect).toBe('character');
    state().openMap('m1');
    expect(state().inspect).toBe('map');
    state().reset();
    expect(state()).toMatchObject({ tabs: [], active: null, mapId: null, characterId: null });
  });
});
