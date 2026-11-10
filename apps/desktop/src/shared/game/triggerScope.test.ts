import { expect, it } from 'vitest';
import { triggerRunBlock } from './triggerScope';
import type { TriggerDto } from '@/shared/ipc/client';

const trigger: TriggerDto = {
  id: 'tr',
  version: 1,
  fired: false,
  fired_maps: [],
  definition: {
    name: 'Alarm',
    enabled: true,
    once: true,
    map_ids: ['a', 'b'],
    tokens: ['allowed'],
    event: { type: 'manual' },
    condition: null,
    steps: [],
  },
};
const base = {
  trigger,
  maps: ['a', 'b'],
  tokenId: 'allowed',
  token: { id: 'allowed', map_id: 'a' },
  dirty: false,
  busy: false,
};
it('keeps unspent bound maps available', () => {
  expect(triggerRunBlock(base)).toBeNull();
  const spent = { ...trigger, fired: true, fired_maps: ['a'] };
  expect(triggerRunBlock({ ...base, trigger: spent })).toBe('spent');
  expect(
    triggerRunBlock({ ...base, trigger: spent, token: { id: 'allowed', map_id: 'b' } }),
  ).toBeNull();
});
it('distinguishes token problems from an exhausted trigger', () => {
  expect(triggerRunBlock({ ...base, token: undefined })).toBe('token_missing');
  expect(triggerRunBlock({ ...base, token: { id: 'allowed', map_id: 'other' } })).toBe('wrong_map');
  expect(
    triggerRunBlock({ ...base, tokenId: 'forbidden', token: { id: 'forbidden', map_id: 'a' } }),
  ).toBe('token_not_allowed');
});
it('explains busy, unsaved, disabled and unavailable map states', () => {
  expect(triggerRunBlock({ ...base, busy: true, dirty: true })).toBe('busy');
  expect(triggerRunBlock({ ...base, dirty: true })).toBe('unsaved');
  expect(
    triggerRunBlock({
      ...base,
      trigger: { ...trigger, definition: { ...trigger.definition, enabled: false } },
    }),
  ).toBe('disabled');
  expect(triggerRunBlock({ ...base, maps: [] })).toBe('map_missing');
});
