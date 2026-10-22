import { describe, expect, it } from 'vitest';
import { detectLanguage } from './index';
import en from './locales/en.json';
import ru from './locales/ru.json';

/** Flattened keys, with i18next plural suffixes collapsed (ru has more plural forms than en). */
function keys(value: unknown, prefix = ''): Set<string> {
  const out = new Set<string>();
  if (typeof value === 'object' && value !== null) {
    for (const [k, v] of Object.entries(value)) {
      const path = prefix ? `${prefix}.${k}` : k;
      if (typeof v === 'object' && v !== null) {
        for (const nested of keys(v, path)) out.add(nested);
      } else {
        out.add(path.replace(/_(zero|one|two|few|many|other)$/, ''));
      }
    }
  }
  return out;
}

describe('locales', () => {
  it('RU and EN have the same keys', () => {
    expect([...keys(ru)].sort()).toEqual([...keys(en)].sort());
  });

  it('has a message for every IPC and rejection code', () => {
    const codes = [
      'no_campaign_open',
      'campaign_corrupt',
      'campaign_too_new',
      'already_exists',
      'not_found',
      'not_a_campaign',
      'invalid_name',
      'storage_error',
      'io_error',
      'stale_version',
      'missing_expected_version',
      'forbidden',
      'invalid',
      'protocol_mismatch',
      'intent_id_reused',
      'limit_exceeded',
      'asset_empty',
      'asset_too_large',
      'asset_svg',
      'asset_unsupported',
      'asset_dimensions',
      'asset_decode',
      'import_failed',
    ];
    for (const code of codes) {
      expect(en.errors).toHaveProperty(code);
    }
  });

  it('detects the language from the locale', () => {
    expect(detectLanguage('ru-RU')).toBe('ru');
    expect(detectLanguage('en-US')).toBe('en');
    expect(detectLanguage('de-DE')).toBe('en');
    expect(detectLanguage(undefined)).toBe('en');
  });
});
