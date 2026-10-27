// The declarative character sheet (CLAUDE.md §8): a layout tree from the system plugin, a JSON
// Schema subset describing the sheet data, and pure helpers the renderer is built on.

/** Plugin text: one string for every language, or one per language. */
/** As in the plugin format; a missing language may also come as `null` from the protocol. */
export type Label = string | { ru?: string | null; en?: string | null };

export type Widget = 'number' | 'text' | 'checkbox' | 'select' | 'textarea';

export interface TabsNode {
  type: 'tabs';
  tabs: { label: Label; children: SheetNode[] }[];
}
export interface SectionNode {
  type: 'section';
  label?: Label;
  children: SheetNode[];
}
export interface RowNode {
  type: 'row';
  children: SheetNode[];
}
export interface GridNode {
  type: 'grid';
  columns: number;
  children: SheetNode[];
}
export interface FieldNode {
  type: 'field';
  /** Dotted path into the sheet data (relative to the list item inside a `list`). */
  path: string;
  label: Label;
  widget?: Widget;
}
export interface ComputedNode {
  type: 'computed';
  name: string;
  label: Label;
}
export interface RollNode {
  type: 'roll';
  roll: string;
  label?: Label;
}
export interface ResourceNode {
  type: 'resource';
  resource: string;
}
export interface ListNode {
  type: 'list';
  path: string;
  label: Label;
  item: SheetNode[];
}
export interface TextNode {
  type: 'text';
  text: Label;
}

export type SheetNode =
  | TabsNode
  | SectionNode
  | RowNode
  | GridNode
  | FieldNode
  | ComputedNode
  | RollNode
  | ResourceNode
  | ListNode
  | TextNode;

/** The JSON Schema subset sheet data is described with. */
export interface JsonSchema {
  type?: 'object' | 'integer' | 'string' | 'boolean' | 'array';
  title?: string;
  properties?: Record<string, JsonSchema>;
  required?: string[];
  minimum?: number;
  maximum?: number;
  maxLength?: number;
  enum?: (string | number)[];
  items?: JsonSchema;
  maxItems?: number;
  default?: unknown;
}

export type SheetData = Record<string, unknown>;

export interface SheetRoll {
  id: string;
  label: Label;
  dice: string;
}

export interface SheetResource {
  id: string;
  label: Label;
  value: number;
  max: number;
}

/** `abilities.str` → `['abilities', 'str']`; empty segments are dropped. */
export function splitPath(path: unknown): string[] {
  if (typeof path !== 'string') return []; // plugin data is not trusted to be well-formed
  return path.split('.').filter((s) => s !== '');
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

/** The value at `path`, or `undefined` if any step is missing. Array steps are indices. */
export function getPath(data: unknown, path: string | string[]): unknown {
  const segments = typeof path === 'string' ? splitPath(path) : path;
  let current: unknown = data;
  for (const seg of segments) {
    if (Array.isArray(current)) current = current[Number(seg)];
    else if (isRecord(current)) current = current[seg];
    else return undefined;
  }
  return current;
}

/**
 * A copy of `data` with `value` at `path`; untouched branches keep their identity.
 * Missing steps become objects (or arrays, when the next step is an index).
 */
export function setPath<T>(data: T, path: string | string[], value: unknown): T {
  const segments = typeof path === 'string' ? splitPath(path) : path;
  const set = (node: unknown, i: number): unknown => {
    const seg = segments[i];
    if (seg === undefined) return value;
    if (Array.isArray(node)) {
      const copy = [...(node as unknown[])];
      const index = Number(seg);
      copy[index] = set(copy[index], i + 1);
      return copy;
    }
    const base = isRecord(node) ? node : /^\d+$/.test(seg) ? [] : {};
    if (Array.isArray(base)) return set(base, i);
    return { ...base, [seg]: set(base[seg], i + 1) };
  };
  return set(data, 0) as T;
}

/** The text for `lang`, falling back to the other language. */
export function resolveLabel(label: Label | undefined, lang: string): string {
  if (label === undefined) return '';
  if (typeof label === 'string') return label;
  const primary = lang.toLowerCase().startsWith('ru') ? 'ru' : 'en';
  const other = primary === 'ru' ? 'en' : 'ru';
  return label[primary] ?? label[other] ?? '';
}

/** The schema describing the value at `path` (array steps go through `items`). */
export function schemaAt(schema: JsonSchema | undefined, path: string | string[]) {
  const segments = typeof path === 'string' ? splitPath(path) : path;
  let current = schema;
  for (const seg of segments) {
    if (current === undefined) return undefined;
    current = current.type === 'array' ? current.items : current.properties?.[seg];
  }
  return current;
}

/** The widget for a field: the layout's choice if it fits, else derived from the schema or value. */
export function widgetFor(
  explicit: Widget | undefined,
  schema: JsonSchema | undefined,
  value: unknown,
): Widget {
  if (explicit === 'select') return schema?.enum?.length ? 'select' : 'text';
  if (explicit) return explicit;
  if (schema?.enum?.length) return 'select';
  switch (schema?.type) {
    case 'integer':
      return 'number';
    case 'boolean':
      return 'checkbox';
    case 'string':
      return 'text';
    default:
      if (typeof value === 'number') return 'number';
      if (typeof value === 'boolean') return 'checkbox';
      return 'text';
  }
}

/**
 * Parses and clamps an integer typed into a number field (ADR-0016: integers only).
 * `null` means the input is not an integer and must not be emitted.
 */
export function parseInteger(input: string, schema: JsonSchema | undefined): number | null {
  const text = input.trim();
  if (!/^[+-]?\d+$/.test(text)) return null;
  let n = Number(text);
  const min = Math.max(schema?.minimum ?? -Number.MAX_SAFE_INTEGER, -Number.MAX_SAFE_INTEGER);
  const max = Math.min(schema?.maximum ?? Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER);
  if (n < min) n = Math.ceil(min);
  if (n > max) n = Math.floor(max);
  return n;
}

/** Cuts text to the schema's `maxLength` (counted in characters, not UTF-16 units). */
export function clampText(input: string, schema: JsonSchema | undefined): string {
  const max = schema?.maxLength;
  if (max === undefined) return input;
  const chars = Array.from(input); // code points, as JSON Schema counts them
  return chars.length > max ? chars.slice(0, max).join('') : input;
}

/** The value a new item (list row) starts with: `default`, else an empty value of its type. */
export function defaultValue(schema: JsonSchema | undefined): unknown {
  if (schema === undefined) return {};
  if (schema.default !== undefined) return structuredClone(schema.default);
  if (schema.enum?.length) return schema.enum[0];
  switch (schema.type) {
    case 'integer':
      return parseInteger('0', schema) ?? 0;
    case 'string':
      return '';
    case 'boolean':
      return false;
    case 'array':
      return [];
    default: {
      const out: Record<string, unknown> = {};
      for (const [key, prop] of Object.entries(schema.properties ?? {})) {
        out[key] = defaultValue(prop);
      }
      return out;
    }
  }
}
