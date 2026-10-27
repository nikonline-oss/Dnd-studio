// The SRD 5.2 system plugin for the browser mock: its real rules, with a tiny evaluator standing in
// for the core's `sheet_view` (enough for the plugin's own formulas). Dev only, never shipped.
import layout from '../../../../../plugins/system-srd5/sheet/layout.json';
import schema from '../../../../../plugins/system-srd5/sheet/schema.json';
import system from '../../../../../plugins/system-srd5/system.json';
import type { PluginDto, SheetView, SystemRulesDto } from '@/shared/ipc/client';

const rules = {
  sheet_schema: schema,
  sheet_layout: layout,
  computed: system.computed,
  initiative: system.initiative,
  resources: system.resources,
  health_thresholds: system.health_thresholds,
  rolls: system.rolls,
} as unknown as SystemRulesDto;

export const SRD_PLUGIN: PluginDto = {
  id: 'srd5',
  version: '0.1.0',
  kind: 'system',
  name: { ru: 'SRD 5.2', en: 'SRD 5.2' },
  license: 'CC-BY-4.0',
  attribution: null,
  archive_sha256: '0'.repeat(64),
  system: rules,
};

export const SRD_SHEET = {
  level: 3,
  class: 'Rogue',
  species: 'Halfling',
  background: 'Criminal',
  hp: { current: 17, max: 24, temp: 0 },
  ac: 15,
  speed: 25,
  abilities: { str: 8, dex: 17, con: 12, int: 13, wis: 10, cha: 14 },
  saves: { dex: true, int: true },
  skills: { stealth: true, sleight_of_hand: true, perception: true, deception: true },
};

type Value = number | boolean;

/** Evaluates one formula of the plugin over `fields` (sheet paths and computed names). */
function evaluate(source: string, field: (path: string) => Value): Value {
  const tokens = source.match(/@[\w.]+|\d+|[a-z_]+|[-+*/()]/g) ?? [];
  let at = 0;
  const next = () => tokens[at++];
  const peek = () => tokens[at];
  const num = (v: Value) => (typeof v === 'number' ? v : v ? 1 : 0);

  function primary(): Value {
    const token = next();
    if (token === undefined) return 0;
    if (token === '(') {
      const v = expr();
      next();
      return v;
    }
    if (token === '-') return -num(primary());
    if (token.startsWith('@')) return field(token.slice(1));
    if (token === 'floor') {
      next();
      const v = num(expr());
      next();
      return Math.floor(v);
    }
    if (token === 'if') {
      const cond = expr();
      next(); // then
      const a = expr();
      next(); // else
      const b = expr();
      return cond === true || cond === 1 ? a : b;
    }
    return Number(token);
  }
  function term(): Value {
    let v = primary();
    while (peek() === '*' || peek() === '/') {
      const op = next();
      const r = num(primary());
      v = op === '*' ? num(v) * r : num(v) / r;
    }
    return v;
  }
  function expr(): Value {
    let v = term();
    while (peek() === '+' || peek() === '-') {
      const op = next();
      const r = num(term());
      v = op === '+' ? num(v) + r : num(v) - r;
    }
    return v;
  }
  return expr();
}

/** What the core's `sheet_view` would return for `sheetJson` under the SRD rules. */
export function srdSheetView(sheetJson: string): SheetView {
  let sheet: unknown = {};
  try {
    sheet = JSON.parse(sheetJson);
  } catch {
    // An empty sheet.
  }
  const computed: Record<string, number> = {};
  const field = (path: string): Value => {
    if (path in computed) return computed[path] ?? 0;
    let value: unknown = sheet;
    for (const key of path.split('.')) {
      value =
        typeof value === 'object' && value !== null
          ? (value as Record<string, unknown>)[key]
          : undefined;
    }
    return typeof value === 'number' || typeof value === 'boolean' ? value : 0;
  };
  for (const [name, formula] of Object.entries(system.computed)) {
    const v = evaluate(formula, field);
    computed[name] = typeof v === 'number' ? v : Number(v);
  }
  return {
    computed,
    resources: system.resources.map((r) => ({
      id: r.id,
      value: Number(field(r.value.slice(1))),
      max: Number(field(r.max.slice(1))),
    })),
  };
}
