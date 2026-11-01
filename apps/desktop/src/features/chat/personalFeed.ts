import type { ChatMessageDto, CharacterDto } from '@/shared/ipc/client';

type Character = Pick<CharacterDto, 'id' | 'name' | 'owner'>;
const normalize = (value: string) => value.normalize('NFC').trim().toLowerCase();

/** Convenience filter over already-authorized messages; never a substitute for visibility rules. */
export function personalFeedFilter(
  userId: string | undefined,
  userName: string | undefined,
  characters: readonly Character[],
): (message: ChatMessageDto) => boolean {
  if (!userId) return () => false;
  const own = characters.filter((c) => c.owner === userId);
  const ids = new Set(own.map((c) => c.id));
  const uniqueNames = new Set(
    own
      .map((c) => normalize(c.name))
      .filter(
        (name) => name !== '' && characters.filter((c) => normalize(c.name) === name).length === 1,
      ),
  );
  const names = [...new Set([userName ? normalize(userName) : '', ...uniqueNames])].filter(Boolean);
  const matchers = names.map(
    (name) =>
      new RegExp(
        `(^|[^\\p{L}\\p{N}_])${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?=$|[^\\p{L}\\p{N}_])`,
        'u',
      ),
  );
  const mentions = (text: string) => matchers.some((pattern) => pattern.test(normalize(text)));
  return (message) => {
    if (message.sender === userId) return true;
    const related = message.related_characters ?? [];
    if (related.some((id) => ids.has(id))) return true;
    if (message.kind === 'text') return mentions(message.body);
    if (message.system?.code === 'narration') return mentions(message.system.text);
    // Explicit metadata wins over an ambiguous or outdated display name.
    if (related.length) return false;
    if (message.system && 'name' in message.system)
      return uniqueNames.has(normalize(message.system.name));
    return message.kind === 'roll' && uniqueNames.has(normalize(message.sender_name));
  };
}
