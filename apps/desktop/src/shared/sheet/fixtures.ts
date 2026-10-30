// Two deliberately different sheets (CLAUDE.md §13: universality is proven by a second system):
// a d20-like one and a d6-pool-like one. Computed values, rolls and resources are what the core
// would have evaluated for `data`.
import type { JsonSchema, SheetData, SheetNode, SheetResource, SheetRoll } from './model';

export interface SheetFixture {
  layout: SheetNode;
  schema: JsonSchema;
  data: SheetData;
  computed: Record<string, number>;
  rolls: SheetRoll[];
  resources: SheetResource[];
}

const ABILITIES = [
  ['str', 'Сила', 'Strength'],
  ['dex', 'Ловкость', 'Dexterity'],
  ['con', 'Телосложение', 'Constitution'],
  ['int', 'Интеллект', 'Intelligence'],
  ['wis', 'Мудрость', 'Wisdom'],
  ['cha', 'Харизма', 'Charisma'],
] as const;

const SCORES = { str: 8, dex: 14, con: 12, int: 10, wis: 13, cha: 15 };
const mod = (score: number) => Math.floor((score - 10) / 2);

const ability: JsonSchema = { type: 'integer', minimum: 1, maximum: 30, default: 10 };

export const d20Sheet: SheetFixture = {
  schema: {
    type: 'object',
    required: ['abilities'],
    properties: {
      name: { type: 'string', maxLength: 60, title: 'Name' },
      level: { type: 'integer', minimum: 1, maximum: 20, default: 1 },
      inspiration: { type: 'boolean' },
      alignment: { type: 'string', enum: ['lawful', 'neutral', 'chaotic'] },
      abilities: {
        type: 'object',
        properties: Object.fromEntries(ABILITIES.map(([id]) => [id, ability])),
      },
      saves: {
        type: 'object',
        properties: Object.fromEntries(ABILITIES.map(([id]) => [id, { type: 'boolean' }])),
      },
      skills: {
        type: 'array',
        maxItems: 30,
        items: {
          type: 'object',
          properties: {
            name: { type: 'string', maxLength: 40 },
            ability: { type: 'string', enum: ABILITIES.map(([id]) => id) },
            proficient: { type: 'boolean' },
          },
        },
      },
      notes: { type: 'string', maxLength: 10000 },
    },
  },
  data: {
    name: 'Aria',
    level: 3,
    inspiration: false,
    alignment: 'neutral',
    abilities: SCORES,
    saves: { str: false, dex: true, con: false, int: false, wis: false, cha: true },
    skills: [
      { name: 'Stealth', ability: 'dex', proficient: true },
      { name: 'Persuasion', ability: 'cha', proficient: false },
    ],
    notes: '',
  },
  computed: {
    proficiency: 2,
    ...Object.fromEntries(ABILITIES.map(([id]) => [`${id}_mod`, mod(SCORES[id])])),
  },
  rolls: [
    ...ABILITIES.map(([id, ru, en]) => ({
      id: `check_${id}`,
      label: { ru: `Проверка: ${ru}`, en: `${en} check` },
      dice: `1d20 + @${id}_mod`,
    })),
    { id: 'initiative', label: { ru: 'Инициатива', en: 'Initiative' }, dice: '1d20 + @dex_mod' },
  ],
  resources: [{ id: 'hp', label: { ru: 'Хиты', en: 'Hit points' }, value: 17, max: 24 }],
  layout: {
    type: 'tabs',
    tabs: [
      {
        label: { ru: 'Основное', en: 'Main' },
        children: [
          {
            type: 'row',
            children: [
              { type: 'field', path: 'name', label: { ru: 'Имя', en: 'Name' } },
              { type: 'field', path: 'level', label: { ru: 'Уровень', en: 'Level' } },
              { type: 'field', path: 'alignment', label: { ru: 'Мировоззрение', en: 'Alignment' } },
              {
                type: 'field',
                path: 'inspiration',
                label: { ru: 'Вдохновение', en: 'Inspiration' },
              },
            ],
          },
          { type: 'resource', resource: 'hp' },
          {
            type: 'section',
            label: { ru: 'Характеристики', en: 'Abilities' },
            children: [
              {
                type: 'grid',
                columns: 3,
                children: ABILITIES.map(([id, ru, en]) => ({
                  type: 'section',
                  children: [
                    { type: 'field', path: `abilities.${id}`, label: { ru, en } },
                    { type: 'computed', name: `${id}_mod`, label: { ru: 'Мод.', en: 'Mod' } },
                    { type: 'roll', roll: `check_${id}` },
                  ],
                })),
              },
            ],
          },
          {
            type: 'section',
            label: { ru: 'Спасброски', en: 'Saving throws' },
            children: [
              {
                type: 'grid',
                columns: 2,
                children: ABILITIES.map(([id, ru, en]) => ({
                  type: 'field',
                  path: `saves.${id}`,
                  label: { ru, en },
                })),
              },
            ],
          },
          { type: 'roll', roll: 'initiative' },
        ],
      },
      {
        label: { ru: 'Навыки', en: 'Skills' },
        children: [
          { type: 'computed', name: 'proficiency', label: { ru: 'Мастерство', en: 'Proficiency' } },
          {
            type: 'list',
            path: 'skills',
            label: { ru: 'Навыки', en: 'Skills' },
            item: [
              { type: 'field', path: 'name', label: { ru: 'Навык', en: 'Skill' } },
              { type: 'field', path: 'ability', label: { ru: 'Характеристика', en: 'Ability' } },
              { type: 'field', path: 'proficient', label: { ru: 'Владение', en: 'Proficient' } },
            ],
          },
        ],
      },
      {
        label: { ru: 'Заметки', en: 'Notes' },
        children: [
          { type: 'text', text: { ru: 'Предыстория и прочее.', en: 'Background and the rest.' } },
          {
            type: 'field',
            path: 'notes',
            label: { ru: 'Заметки', en: 'Notes' },
            widget: 'textarea',
          },
        ],
      },
    ],
  },
};

const dice: JsonSchema = { type: 'integer', minimum: 1, maximum: 6, default: 2 };

export const d6PoolSheet: SheetFixture = {
  schema: {
    type: 'object',
    properties: {
      concept: { type: 'string', maxLength: 80 },
      attributes: {
        type: 'object',
        properties: { body: dice, agility: dice, mind: dice },
      },
      talents: {
        type: 'array',
        maxItems: 3,
        items: {
          type: 'object',
          properties: {
            name: { type: 'string', maxLength: 30 },
            dice: { type: 'integer', minimum: 1, maximum: 4, default: 1 },
          },
        },
      },
      wounds: {
        type: 'object',
        properties: {
          bruised: { type: 'boolean' },
          hurt: { type: 'boolean' },
          down: { type: 'boolean' },
        },
      },
    },
  },
  data: {
    concept: 'Smuggler',
    attributes: { body: 2, agility: 4, mind: 3 },
    talents: [{ name: 'Pilot', dice: 2 }],
    wounds: { bruised: true, hurt: false, down: false },
  },
  computed: { pool_agility: 6, wound_penalty: -1 },
  rolls: [
    { id: 'pool_body', label: { ru: 'Тело', en: 'Body' }, dice: '2d6>=5' },
    { id: 'pool_agility', label: { ru: 'Ловкость', en: 'Agility' }, dice: '4d6>=5' },
    { id: 'pool_mind', label: { ru: 'Разум', en: 'Mind' }, dice: '3d6>=5' },
  ],
  resources: [{ id: 'edge', label: { ru: 'Удача', en: 'Edge' }, value: 2, max: 3 }],
  layout: {
    type: 'section',
    label: { ru: 'Персонаж', en: 'Character' },
    children: [
      { type: 'field', path: 'concept', label: { ru: 'Концепция', en: 'Concept' } },
      {
        type: 'grid',
        columns: 3,
        children: (
          [
            ['body', 'Тело', 'Body'],
            ['agility', 'Ловкость', 'Agility'],
            ['mind', 'Разум', 'Mind'],
          ] as const
        ).map(([id, ru, en]) => ({
          type: 'row',
          children: [
            { type: 'field', path: `attributes.${id}`, label: { ru, en } },
            { type: 'roll', roll: `pool_${id}`, label: { ru: 'Бросить', en: 'Roll' } },
          ],
        })),
      },
      {
        type: 'list',
        path: 'talents',
        label: { ru: 'Таланты', en: 'Talents' },
        item: [
          { type: 'field', path: 'name', label: { ru: 'Талант', en: 'Talent' } },
          { type: 'field', path: 'dice', label: { ru: 'Кубы', en: 'Dice' } },
        ],
      },
      {
        type: 'section',
        label: { ru: 'Раны', en: 'Wounds' },
        children: [
          { type: 'field', path: 'wounds.bruised', label: { ru: 'Ушиб', en: 'Bruised' } },
          { type: 'field', path: 'wounds.hurt', label: { ru: 'Ранен', en: 'Hurt' } },
          { type: 'field', path: 'wounds.down', label: { ru: 'Выбит', en: 'Down' } },
          { type: 'computed', name: 'wound_penalty', label: { ru: 'Штраф', en: 'Penalty' } },
        ],
      },
      { type: 'resource', resource: 'edge' },
    ],
  },
};
