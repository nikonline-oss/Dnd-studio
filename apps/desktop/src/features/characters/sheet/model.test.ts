import { describe, expect, it } from 'vitest';
import {
  clampText,
  defaultValue,
  getPath,
  parseInteger,
  resolveLabel,
  schemaAt,
  setPath,
  widgetFor,
  type JsonSchema,
} from './model';

describe('getPath', () => {
  const data = { a: { b: 3 }, list: [{ name: 'x' }] };

  it('walks objects and arrays', () => {
    expect(getPath(data, 'a.b')).toBe(3);
    expect(getPath(data, 'list.0.name')).toBe('x');
    expect(getPath(data, ['list', '0'])).toEqual({ name: 'x' });
  });

  it('returns undefined for missing steps', () => {
    expect(getPath(data, 'a.c')).toBeUndefined();
    expect(getPath(data, 'a.b.c')).toBeUndefined();
    expect(getPath(data, 'list.5.name')).toBeUndefined();
  });
});

describe('setPath', () => {
  it('copies only the changed branch', () => {
    const data = { a: { b: 1 }, other: { x: 1 } };
    const next = setPath(data, 'a.b', 2);
    expect(next).toEqual({ a: { b: 2 }, other: { x: 1 } });
    expect(data.a.b).toBe(1);
    expect(next.other).toBe(data.other);
    expect(next.a).not.toBe(data.a);
  });

  it('creates missing objects and arrays', () => {
    expect(setPath({}, 'a.b', 1)).toEqual({ a: { b: 1 } });
    expect(setPath({}, 'list.0.name', 'x')).toEqual({ list: [{ name: 'x' }] });
  });

  it('updates an array item without touching its neighbours', () => {
    const data = { list: [{ n: 1 }, { n: 2 }] };
    const next = setPath(data, 'list.1.n', 5);
    expect(next.list).toEqual([{ n: 1 }, { n: 5 }]);
    expect(next.list[0]).toBe(data.list[0]);
    expect(data.list[1]?.n).toBe(2);
  });
});

describe('resolveLabel', () => {
  it('picks the language and falls back to the other', () => {
    expect(resolveLabel('Plain', 'ru')).toBe('Plain');
    expect(resolveLabel({ ru: 'Сила', en: 'Strength' }, 'ru')).toBe('Сила');
    expect(resolveLabel({ ru: 'Сила', en: 'Strength' }, 'en-US')).toBe('Strength');
    expect(resolveLabel({ en: 'Strength' }, 'ru')).toBe('Strength');
    expect(resolveLabel({ ru: 'Сила' }, 'en')).toBe('Сила');
    expect(resolveLabel(undefined, 'en')).toBe('');
  });
});

describe('schema helpers', () => {
  const schema: JsonSchema = {
    type: 'object',
    properties: {
      str: { type: 'integer', minimum: 1, maximum: 30 },
      alive: { type: 'boolean' },
      size: { type: 'string', enum: ['small', 'medium'] },
      skills: {
        type: 'array',
        items: { type: 'object', properties: { name: { type: 'string', maxLength: 3 } } },
      },
    },
  };

  it('finds the schema at a path, through array items', () => {
    expect(schemaAt(schema, 'str')?.maximum).toBe(30);
    expect(schemaAt(schema, 'skills.2.name')?.maxLength).toBe(3);
    expect(schemaAt(schema, 'nope.deeper')).toBeUndefined();
  });

  it('derives widgets from the schema, the value and the layout', () => {
    expect(widgetFor(undefined, schemaAt(schema, 'str'), 1)).toBe('number');
    expect(widgetFor(undefined, schemaAt(schema, 'alive'), true)).toBe('checkbox');
    expect(widgetFor(undefined, schemaAt(schema, 'size'), 'small')).toBe('select');
    expect(widgetFor(undefined, { type: 'string' }, '')).toBe('text');
    expect(widgetFor('textarea', { type: 'string' }, '')).toBe('textarea');
    expect(widgetFor(undefined, undefined, 3)).toBe('number');
    expect(widgetFor(undefined, undefined, false)).toBe('checkbox');
    // A select needs options to choose from.
    expect(widgetFor('select', { type: 'string' }, '')).toBe('text');
  });

  it('parses and clamps integers', () => {
    const str = schemaAt(schema, 'str');
    expect(parseInteger(' 12 ', str)).toBe(12);
    expect(parseInteger('99', str)).toBe(30);
    expect(parseInteger('-4', str)).toBe(1);
    expect(parseInteger('+7', undefined)).toBe(7);
    expect(parseInteger('1.5', str)).toBeNull();
    expect(parseInteger('abc', str)).toBeNull();
    expect(parseInteger('', str)).toBeNull();
    expect(parseInteger('99999999999999999999', undefined)).toBe(Number.MAX_SAFE_INTEGER);
  });

  it('cuts text to maxLength by characters', () => {
    expect(clampText('abcdef', { maxLength: 3 })).toBe('abc');
    expect(clampText('🎲🎲🎲🎲', { maxLength: 2 })).toBe('🎲🎲');
    expect(clampText('abc', undefined)).toBe('abc');
  });

  it('builds default values for new list rows', () => {
    expect(defaultValue(schemaAt(schema, 'skills.0'))).toEqual({ name: '' });
    expect(
      defaultValue({
        type: 'object',
        properties: {
          n: { type: 'integer', minimum: 2 },
          m: { type: 'integer', maximum: -1 },
          size: { type: 'string', enum: ['s', 'm'] },
          flag: { type: 'boolean', default: true },
          tags: { type: 'array' },
        },
      }),
    ).toEqual({ n: 2, m: -1, size: 's', flag: true, tags: [] });
    expect(defaultValue(undefined)).toEqual({});
  });
});
