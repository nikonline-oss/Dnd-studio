import { expect, it } from 'vitest';
import { personalFeedFilter } from './personalFeed';
import type { ChatMessageDto } from '@/shared/ipc/client';
const base: ChatMessageDto = {
  id: 'm',
  kind: 'text',
  sender: 'other',
  sender_name: 'Other',
  body: 'Hello',
  roll: null,
  system: null,
  visibility: 'all',
  at_ms: null,
};
const ownCharacter = { id: 'mine', name: 'Ария', owner: 'me' };
const characters = [ownCharacter, { id: 'npc', name: 'Ария', owner: null }];
it('includes own messages but not unrelated messages', () => {
  const filter = personalFeedFilter('me', 'Анна', characters);
  expect(filter({ ...base, sender: 'me' })).toBe(true);
  expect(filter(base)).toBe(false);
});
it('uses stable IDs after rename and does not confuse identical character names', () => {
  const filter = personalFeedFilter('me', 'Анна', characters);
  expect(
    filter({
      ...base,
      kind: 'system',
      system: { code: 'resource_changed', name: 'Old name', resource: 'hp', delta: -2 },
      related_characters: ['mine'],
    }),
  ).toBe(true);
  expect(
    filter({
      ...base,
      kind: 'system',
      system: { code: 'resource_changed', name: 'Ария', resource: 'hp', delta: -2 },
      related_characters: ['npc'],
    }),
  ).toBe(false);
  expect(
    filter({
      ...base,
      kind: 'system',
      system: { code: 'resource_changed', name: 'Ария', resource: 'hp', delta: -2 },
    }),
  ).toBe(false);
});
it('matches complete Unicode names in text and narration, not substrings', () => {
  const filter = personalFeedFilter('me', 'Анна', [ownCharacter]);
  expect(filter({ ...base, body: '@Анна, ваш ход!' })).toBe(true);
  expect(filter({ ...base, body: 'Жанна идёт первой' })).toBe(false);
  expect(filter({ ...base, body: 'С Арияной всё хорошо' })).toBe(false);
  expect(
    filter({
      ...base,
      kind: 'system',
      system: { code: 'narration', text: 'Ария получает письмо' },
    }),
  ).toBe(true);
});
it('falls back to unique current character names for legacy system events', () => {
  const filter = personalFeedFilter('me', 'Анна', [ownCharacter]);
  expect(
    filter({ ...base, kind: 'system', system: { code: 'character_approved', name: 'Ария' } }),
  ).toBe(true);
  expect(personalFeedFilter(undefined, undefined, characters)({ ...base, sender: 'me' })).toBe(
    false,
  );
});
it('escapes punctuation in user names rather than interpreting regular expressions', () => {
  const filter = personalFeedFilter('me', 'A+B (Scout)', []);
  expect(filter({ ...base, body: '@A+B (Scout), come here' })).toBe(true);
  expect(filter({ ...base, body: 'AAAB Scout, come here' })).toBe(false);
});
