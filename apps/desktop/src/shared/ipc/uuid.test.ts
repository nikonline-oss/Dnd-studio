import { describe, expect, it } from 'vitest';
import { isUuidV7, uuidv7 } from './uuid';

describe('uuidv7', () => {
  it('produces RFC 9562 version 7 ids', () => {
    for (let i = 0; i < 100; i++) {
      const id = uuidv7();
      expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
      expect(isUuidV7(id)).toBe(true);
    }
  });

  it('is unique and monotonic within one client', () => {
    const ids = Array.from({ length: 1000 }, () => uuidv7());
    expect(new Set(ids).size).toBe(ids.length);
    expect([...ids].sort()).toEqual(ids);
  });

  it('rejects other versions', () => {
    expect(isUuidV7('6e4c3f2a-9b1d-4c7e-8f00-123456789abc')).toBe(false); // v4
    expect(isUuidV7('00000000-0000-0000-0000-000000000000')).toBe(false);
    expect(isUuidV7('not-a-uuid')).toBe(false);
  });
});
